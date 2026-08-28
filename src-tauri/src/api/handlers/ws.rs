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

// Track when clients came online for uptime calculation
lazy_static::lazy_static! {
    static ref CLIENT_ONLINE_TIMES: Arc<Mutex<HashMap<String, i64>>> = Arc::new(Mutex::new(HashMap::new()));
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
    ws.on_upgrade(|socket| {
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
    let iscsi_port = state.settings.read().await.iscsi.portal_port;
    let collector = Arc::clone(&state.metrics_collector);
    let snapshot = tokio::task::spawn_blocking(move || collector.collect(&client_ips, iscsi_port))
        .await
        .map_err(|error| format!("metrics collection task failed: {error}"))?;

    let mut clients = Vec::new();
    let mut online_times = CLIENT_ONLINE_TIMES.lock().await;
    let now = chrono::Utc::now().timestamp();

    for sample in snapshot.clients {
        let ip = sample.ip;
        // Determine status in real-time by pinging
        let status = crate::utils::network::get_client_status_realtime(ip.clone());
        let is_online = status == "Online";

        // Track online time
        let uptime_seconds = if is_online {
            // If client just came online, record the time
            if !online_times.contains_key(&ip) {
                online_times.insert(ip.clone(), now);
            }
            // Calculate uptime from when it came online
            now - online_times.get(&ip).copied().unwrap_or(now)
        } else {
            // Client is offline, remove from tracking
            online_times.remove(&ip);
            0
        };

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
