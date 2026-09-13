use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Persisted client boot event exposed through the client application service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootLogEntry {
    pub id: String,
    pub client_id: String,
    pub image_id: Option<String>,
    pub boot_time: DateTime<Utc>,
    pub success: bool,
    pub duration_ms: Option<i64>,
    pub message: Option<String>,
}
