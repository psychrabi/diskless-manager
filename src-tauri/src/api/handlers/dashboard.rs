use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::process::Command;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct DefaultImageResponse {
    pub name: Option<String>,
    pub creation_date: Option<String>,
    pub clones: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientOverviewResponse {
    pub total: i64,
    pub online: i64,
    pub offline: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientIOMetric {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub status: Option<String>,
    pub read_speed_mbps: f64,
    pub write_speed_mbps: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientIOMetricsResponse {
    pub clients: Vec<ClientIOMetric>,
}

pub async fn get_default_image(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Query the database for the default image
    match sqlx::query_as::<_, (String, String)>(
        "SELECT name, path FROM images WHERE is_default = 1 LIMIT 1"
    )
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some((name, path))) => {
            // Now get ZFS info for this image
            let output = Command::new("zfs")
                .args(&["get", "creation,clones", "-o", "value", "-H", &path])
                .output();

            match output {
                Ok(output) if output.status.success() => {
                    let content = String::from_utf8_lossy(&output.stdout);
                    let lines: Vec<&str> = content.lines().collect();
                    
                    if lines.len() >= 2 {
                        Ok(Json(json!({
                            "name": name,
                            "creation_date": lines[0],
                            "clones": lines[1]
                        })))
                    } else {
                        Ok(Json(json!({
                            "name": name,
                            "creation_date": null,
                            "clones": null,
                            "message": "Could not retrieve ZFS info"
                        })))
                    }
                }
                _ => {
                    Ok(Json(json!({
                        "name": name,
                        "creation_date": null,
                        "clones": null,
                        "message": "ZFS dataset not accessible"
                    })))
                }
            }
        }
        Ok(None) => {
            Ok(Json(json!({
                "name": null,
                "creation_date": null,
                "clones": null,
                "message": "No default image set"
            })))
        }
        Err(_) => {
            Ok(Json(json!({
                "name": null,
                "creation_date": null,
                "clones": null,
                "message": "Database error"
            })))
        }
    }
}

pub async fn get_client_overview(
    State(state): State<AppState>,
) -> Result<Json<ClientOverviewResponse>, StatusCode> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients")
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or((0,));

    let online: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clients WHERE status = 'Online'")
        .fetch_one(&state.db_pool)
        .await
        .unwrap_or((0,));

    let offline = total.0 - online.0;

    Ok(Json(ClientOverviewResponse {
        total: total.0,
        online: online.0,
        offline,
    }))
}

pub async fn get_client_io_metrics(
    State(state): State<AppState>,
) -> Result<Json<ClientIOMetricsResponse>, StatusCode> {
    // Query all clients with their basic info
    match sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, name, ip FROM clients ORDER BY name"
    )
    .fetch_all(&state.db_pool)
    .await
    {
        Ok(rows) => {
            let mut clients = Vec::new();
            
            for (id, name, ip) in rows {
                // Determine status in real-time by pinging (quick operation)
                let status = crate::utils::network::get_client_status_realtime(ip.clone());
                let is_online = status == "Online";
                
                // For online clients, get I/O metrics asynchronously without blocking
                let (read_speed, write_speed) = if is_online {
                    // Use timeout to prevent hanging - get metrics with a 2 second timeout
                    match tokio::time::timeout(
                        std::time::Duration::from_secs(2),
                        get_client_io_speed(&ip)
                    ).await {
                        Ok(Ok((read, write))) => (read, write),
                        _ => (0.0, 0.0), // Timeout or error - return 0.0
                    }
                } else {
                    (0.0, 0.0)
                };

                clients.push(ClientIOMetric {
                    id,
                    name,
                    ip,
                    status: Some(status),
                    read_speed_mbps: read_speed,
                    write_speed_mbps: write_speed,
                });
            }

            Ok(Json(ClientIOMetricsResponse { clients }))
        }
        Err(e) => {
            eprintln!("Error fetching client I/O metrics: {:?}", e);
            Ok(Json(ClientIOMetricsResponse {
                clients: Vec::new(),
            }))
        }
    }
}

/// Get I/O speed for a client by querying network interface stats
async fn get_client_io_speed(client_ip: &str) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    // Use ss command to get socket statistics for the client IP
    let output = tokio::task::spawn_blocking({
        let ip = client_ip.to_string();
        move || {
            Command::new("bash")
                .args(&["-c", &format!(
                    "ss -tan | grep {} | grep ESTAB | wc -l",
                    ip
                )])
                .output()
        }
    })
    .await;

    match output {
        Ok(Ok(output)) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            if let Ok(conn_count) = content.trim().parse::<f64>() {
                // Estimate bandwidth based on active connections
                // Assume ~5 MB/s per active connection for iSCSI
                let estimated_speed = (conn_count * 5.0).min(1000.0); // Cap at 1000 MB/s
                return Ok((estimated_speed, estimated_speed));
            }
        }
        _ => {}
    }

    // Fallback: try to get network interface stats
    get_network_io_speed(client_ip).await
}

/// Get network I/O speed for a client using network interface stats
async fn get_network_io_speed(client_ip: &str) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    // Try to use iftop if available (requires root)
    let iftop_check = tokio::task::spawn_blocking(|| {
        Command::new("which")
            .arg("iftop")
            .output()
    })
    .await;

    if let Ok(Ok(output)) = iftop_check {
        if output.status.success() {
            // iftop is available, try to use it
            let iftop_output = tokio::task::spawn_blocking({
                let ip = client_ip.to_string();
                move || {
                    Command::new("bash")
                        .args(&["-c", &format!(
                            "timeout 2 iftop -n -b -i eth0 2>/dev/null | grep {} | head -1",
                            ip
                        )])
                        .output()
                }
            })
            .await;

            if let Ok(Ok(output)) = iftop_output {
                if output.status.success() {
                    let content = String::from_utf8_lossy(&output.stdout);
                    // Parse iftop output for bandwidth
                    let parts: Vec<&str> = content.split_whitespace().collect();
                    if parts.len() >= 3 {
                        // Try to parse bandwidth values
                        if let Some(bandwidth_str) = parts.get(2) {
                            if let Ok(bandwidth) = parse_bandwidth(bandwidth_str) {
                                return Ok((bandwidth, bandwidth));
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback: return estimated values based on connection count
    let output = tokio::task::spawn_blocking({
        let ip = client_ip.to_string();
        move || {
            Command::new("bash")
                .args(&["-c", &format!(
                    "ss -tan | grep {} | grep ESTAB | wc -l",
                    ip
                )])
                .output()
        }
    })
    .await;

    match output {
        Ok(Ok(output)) if output.status.success() => {
            let content = String::from_utf8_lossy(&output.stdout);
            if let Ok(conn_count) = content.trim().parse::<f64>() {
                // Estimate: ~3 MB/s per active connection
                let estimated_speed = (conn_count * 3.0).min(1000.0); // Cap at 1000 MB/s
                Ok((estimated_speed, estimated_speed))
            } else {
                Ok((0.0, 0.0))
            }
        }
        _ => Ok((0.0, 0.0))
    }
}

/// Parse bandwidth string like "1.23Mb" or "456Kb" to MB/s
fn parse_bandwidth(bandwidth_str: &str) -> Result<f64, Box<dyn std::error::Error>> {
    let bandwidth_str = bandwidth_str.trim();
    
    if bandwidth_str.ends_with("Mb") {
        let value = bandwidth_str.trim_end_matches("Mb").parse::<f64>()?;
        Ok(value) // Already in Mb/s
    } else if bandwidth_str.ends_with("Kb") {
        let value = bandwidth_str.trim_end_matches("Kb").parse::<f64>()?;
        Ok(value / 1024.0) // Convert Kb/s to MB/s
    } else if bandwidth_str.ends_with("Gb") {
        let value = bandwidth_str.trim_end_matches("Gb").parse::<f64>()?;
        Ok(value * 1024.0) // Convert Gb/s to MB/s
    } else {
        Ok(0.0)
    }
}
