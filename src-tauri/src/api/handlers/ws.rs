use axum::{
    extract::{
        ws::{WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::interval;

use crate::{
    metrics::{StorageTrafficMetrics, Throughput},
    state::AppState,
};

// Track when clients established their current iSCSI session.
lazy_static::lazy_static! {
    static ref CLIENT_SESSION_START_TIMES: Arc<Mutex<HashMap<String, i64>>> = Arc::new(Mutex::new(HashMap::new()));
}

fn session_uptime(
    session_starts: &mut HashMap<String, i64>,
    client_ip: &str,
    connected: bool,
    now: i64,
) -> i64 {
    if connected {
        let started = session_starts.entry(client_ip.to_string()).or_insert(now);
        now.saturating_sub(*started)
    } else {
        session_starts.remove(client_ip);
        0
    }
}

#[derive(Debug, Serialize)]
pub struct ClientMetric {
    pub ip: String,
    pub status: String,
    /// Compatibility fields: actual iSCSI read and write throughput when available.
    pub read_speed_mbps: Option<f64>,
    /// Compatibility fields: actual iSCSI read and write throughput when available.
    pub write_speed_mbps: Option<f64>,
    /// All conntrack-accounted traffic involving this client IP.
    pub network: Option<Throughput>,
    /// Conntrack-accounted traffic for this client on the configured iSCSI port.
    pub iscsi: Option<Throughput>,
    /// Indicates a readable source without a preceding sample for a real rate.
    pub warming_up: bool,
    pub uptime_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct MetricsUpdate {
    pub clients: Vec<ClientMetric>,
    /// Aggregate ZFS throughput across discovered pools.
    pub storage: StorageTrafficMetrics,
    /// Read errors from source counters; no estimates are returned for them.
    pub warnings: Vec<String>,
    pub timestamp: i64,
}

pub async fn ws_metrics_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!("WebSocket upgrade request received");
    ws.protocols(["diskless-auth"]).on_upgrade(|socket| {
        log::info!("WebSocket connection established");
        handle_metrics_socket(socket, state)
    })
}

async fn handle_metrics_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let state_clone = state.clone();
    let mut interval = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            // Send metrics every 1 second
            _ = interval.tick() => {
                match fetch_metrics(&state_clone).await {
                    Ok(metrics) => {
                        match serde_json::to_string(&metrics) {
                            Ok(msg) => {
                                if let Err(e) = sender.send(axum::extract::ws::Message::Text(msg.into())).await {
                                    log::debug!("WebSocket client disconnected: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                log::error!("Failed to serialize metrics: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to fetch metrics: {}", e);
                    }
                }
            }

            // Handle incoming messages
            msg = receiver.next() => {
                match msg {
                    Some(Ok(axum::extract::ws::Message::Close(_))) => {
                        log::debug!("WebSocket close message received from client");
                        break;
                    }
                    Some(Ok(axum::extract::ws::Message::Ping(_))) => {
                        log::debug!("WebSocket ping received");
                    }
                    Some(Ok(_)) => {
                        // Ignore other message types
                    }
                    Some(Err(e)) => {
                        log::debug!("WebSocket receive error: {}", e);
                        break;
                    }
                    None => {
                        log::debug!("WebSocket receiver closed");
                        break;
                    }
                }
            }
        }
    }
}

pub(crate) async fn fetch_metrics(state: &AppState) -> Result<MetricsUpdate, String> {
    let client_ips = state.client_ips.read().await.clone();
    let settings = state.settings.read().await.clone();
    let iscsi_port = settings.iscsi.portal_port;
    let registered_clients = crate::core::client::ClientManager::new(state.db_pool.clone())
        .list()
        .await
        .map_err(|error| format!("failed to load clients for metrics: {error}"))?;
    let mut target_by_ip = HashMap::new();
    let mut lio_sources = Vec::new();
    for client in registered_clients {
        let normalized_name = client.name.trim().to_lowercase();
        let target_iqn = client.target_iqn.unwrap_or_else(|| {
            format!(
                "{}:client.{}",
                settings.iscsi.target_prefix, normalized_name
            )
        });
        lio_sources.push((client.ip.clone(), format!("block_{normalized_name}")));
        target_by_ip.insert(client.ip, target_iqn);
    }
    let collector = Arc::clone(&state.metrics_collector);
    let (snapshot, lio_rates) = tokio::task::spawn_blocking(move || {
        let snapshot = collector.collect(&client_ips, iscsi_port);
        let lio_rates = collector.collect_lio(&lio_sources);
        (snapshot, lio_rates)
    })
    .await
    .map_err(|error| format!("metrics collection task failed: {error}"))?;

    let mut clients = Vec::new();
    let mut session_starts = CLIENT_SESSION_START_TIMES.lock().await;
    let now = chrono::Utc::now().timestamp();

    for mut sample in snapshot.clients {
        let ip = sample.ip;
        if let Some(lio) = lio_rates.get(&ip).cloned() {
            sample.iscsi = Some(lio.clone());
            // When conntrack accounting is unavailable, LIO still provides a
            // measured lower bound for this client's network traffic.
            sample.network.get_or_insert(lio);
        }
        let connected = target_by_ip
            .get(&ip)
            .map(|target_iqn| {
                crate::infrastructure::iscsi::target_has_active_sessions(target_iqn).unwrap_or_else(
                    |error| {
                        tracing::warn!(%target_iqn, %error, "failed to inspect iSCSI session");
                        false
                    },
                )
            })
            .unwrap_or(false);
        let status = if connected { "Online" } else { "Offline" }.to_string();
        let uptime_seconds = session_uptime(&mut session_starts, &ip, connected, now);

        clients.push(ClientMetric {
            read_speed_mbps: sample.iscsi.as_ref().map(|metric| metric.read_speed_mbps),
            write_speed_mbps: sample.iscsi.as_ref().map(|metric| metric.write_speed_mbps),
            network: sample.network,
            iscsi: sample.iscsi,
            warming_up: sample.warming_up,
            ip: ip.clone(),
            status,
            uptime_seconds,
        });
    }

    Ok(MetricsUpdate {
        clients,
        storage: snapshot.storage,
        warnings: snapshot.warnings,
        timestamp: snapshot.timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::session_uptime;
    use std::collections::HashMap;

    #[test]
    fn uptime_tracks_the_current_iscsi_connection_only() {
        let mut starts = HashMap::new();

        assert_eq!(session_uptime(&mut starts, "192.168.1.101", true, 100), 0);
        assert_eq!(session_uptime(&mut starts, "192.168.1.101", true, 115), 15);
        assert_eq!(session_uptime(&mut starts, "192.168.1.101", false, 120), 0);
        assert_eq!(session_uptime(&mut starts, "192.168.1.101", true, 130), 0);
    }
}
