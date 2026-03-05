use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::interval;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::state::AppState;

// Track when clients came online for uptime calculation
lazy_static::lazy_static! {
    static ref CLIENT_ONLINE_TIMES: Arc<Mutex<HashMap<String, i64>>> = Arc::new(Mutex::new(HashMap::new()));
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientMetric {
    pub ip: String,
    pub status: String,
    pub read_speed_mbps: f64,
    pub write_speed_mbps: f64,
    pub uptime_seconds: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsUpdate {
    pub clients: Vec<ClientMetric>,
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

async fn fetch_metrics(state: &AppState) -> Result<MetricsUpdate, String> {
    // Get cached client IPs
    let client_ips = state.client_ips.read().await;
    
    if client_ips.is_empty() {
        log::debug!("No clients in cache, returning empty metrics");
        return Ok(MetricsUpdate {
            clients: Vec::new(),
            timestamp: chrono::Utc::now().timestamp(),
        });
    }
    
    let mut clients = Vec::new();
    let mut online_times = CLIENT_ONLINE_TIMES.lock().await;
    let now = chrono::Utc::now().timestamp();

    for ip in client_ips.iter() {
        // Determine status in real-time by pinging
        let status = crate::utils::network::get_client_status_realtime(ip.clone());
        let is_online = status == "Online";

        // Track online time
        let uptime_seconds = if is_online {
            // If client just came online, record the time
            if !online_times.contains_key(ip) {
                online_times.insert(ip.clone(), now);
            }
            // Calculate uptime from when it came online
            now - online_times.get(ip).copied().unwrap_or(now)
        } else {
            // Client is offline, remove from tracking
            online_times.remove(ip);
            0
        };

        // Get I/O metrics with timeout
        let (read_speed, write_speed) = if is_online {
            match tokio::time::timeout(
                Duration::from_secs(3),
                get_client_io_speed(ip),
            )
            .await
            {
                Ok(Ok((read, write))) => (read, write),
                _ => (0.0, 0.0),
            }
        } else {
            (0.0, 0.0)
        };

        clients.push(ClientMetric {
            ip: ip.clone(),
            status,
            read_speed_mbps: read_speed,
            write_speed_mbps: write_speed,
            uptime_seconds,
        });
    }

    Ok(MetricsUpdate {
        clients,
        timestamp: chrono::Utc::now().timestamp(),
    })
}

/// Get I/O speed for a client by measuring actual throughput
async fn get_client_io_speed(client_ip: &str) -> Result<(f64, f64), String> {
    // Take first measurement
    let (recv1, sent1) = get_socket_bytes_for_client(client_ip).await?;
    
    // Wait 1 second for measurement interval
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
    
    // Take second measurement
    let (recv2, sent2) = get_socket_bytes_for_client(client_ip).await?;
    
    // Calculate throughput (bytes per second = MB/s)
    // recv = bytes received by server (client writing to server) = write speed
    // sent = bytes sent by server (client reading from server) = read speed
    let write_delta = recv2.saturating_sub(recv1);
    let read_delta = sent2.saturating_sub(sent1);
    
    let read_speed = (read_delta as f64) / (1024.0 * 1024.0);
    let write_speed = (write_delta as f64) / (1024.0 * 1024.0);
    
    Ok((read_speed, write_speed))
}

/// Get socket bytes transferred for a specific client IP using ss command
async fn get_socket_bytes_for_client(client_ip: &str) -> Result<(u64, u64), String> {
    // Get the network interface used to reach this client
    let interface = get_client_interface(client_ip).await?;
    
    // Read /proc/net/dev to get interface statistics
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new("cat")
            .arg("/proc/net/dev")
            .output()
    })
    .await
    .map_err(|e| e.to_string())?;

    let output = output.map_err(|e| e.to_string())?;
    
    if !output.status.success() {
        return Ok((0, 0));
    }

    let content = String::from_utf8_lossy(&output.stdout);
    
    // Parse /proc/net/dev to find the interface
    // Format: interface: bytes_recv packets_recv errors_recv drop_recv ... bytes_sent packets_sent ...
    for line in content.lines() {
        if line.contains(&interface) && !line.trim().starts_with("face") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                // Index 1 is bytes received, index 9 is bytes sent
                if let (Some(recv), Some(sent)) = (
                    parts.get(1).and_then(|s| s.parse::<u64>().ok()),
                    parts.get(9).and_then(|s| s.parse::<u64>().ok()),
                ) {
                    return Ok((recv, sent));
                }
            }
        }
    }

    Ok((0, 0))
}

/// Get the network interface used to reach a client
async fn get_client_interface(client_ip: &str) -> Result<String, String> {
    if client_ip.parse::<std::net::IpAddr>().is_err() {
        return Err("Invalid client IP".to_string());
    }

    let output = tokio::task::spawn_blocking({
        let ip = client_ip.to_string();
        move || {
            std::process::Command::new("ip")
                .args(["route", "get", &ip])
                .output()
        }
    })
    .await
    .map_err(|e| e.to_string())?;

    let output = output.map_err(|e| e.to_string())?;
    
    if output.status.success() {
        let content = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = content.split_whitespace().collect();
        if let Some(dev_idx) = parts.iter().position(|p| *p == "dev") {
            if let Some(interface) = parts.get(dev_idx + 1) {
                if !interface.is_empty() {
                    return Ok((*interface).to_string());
                }
            }
        }
    }

    // Fallback to eth0
    Ok("eth0".to_string())
}
