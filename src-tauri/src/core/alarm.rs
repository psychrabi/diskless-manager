use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlarmKind {
    BootFailure,
    StorageRecovery,
    Service,
    Pool,
}

impl std::fmt::Display for AlarmKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BootFailure => write!(formatter, "boot_failure"),
            Self::StorageRecovery => write!(formatter, "storage_recovery"),
            Self::Service => write!(formatter, "service"),
            Self::Pool => write!(formatter, "pool"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlarmSeverity {
    Critical,
    Warning,
    Info,
}

impl AlarmSeverity {
    fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

/// One active operational problem derived from stored or live state.
///
/// `id` is a stable fingerprint for the current occurrence. A change that
/// deserves a fresh acknowledgement (another boot failure, a different pool
/// health) produces a different id. Acknowledgements are stored separately so
/// this type stays a read model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alarm {
    pub id: String,
    pub kind: AlarmKind,
    pub severity: AlarmSeverity,
    pub title: String,
    pub detail: String,
    /// The affected client, service, or pool name.
    pub source: String,
    pub client_id: Option<String>,
    pub observed_at: Option<DateTime<Utc>>,
    pub acknowledged_at: Option<DateTime<Utc>>,
    pub acknowledged_by: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlarmSummary {
    pub alarms: Vec<Alarm>,
    pub critical: usize,
    pub warning: usize,
    pub acknowledged: usize,
}

impl AlarmSummary {
    pub fn from_alarms(mut alarms: Vec<Alarm>) -> Self {
        alarms.sort_by(|left, right| {
            left.severity
                .rank()
                .cmp(&right.severity.rank())
                .then_with(|| right.observed_at.cmp(&left.observed_at))
                .then_with(|| left.title.cmp(&right.title))
        });

        let critical = alarms
            .iter()
            .filter(|alarm| {
                alarm.severity == AlarmSeverity::Critical && alarm.acknowledged_at.is_none()
            })
            .count();
        let warning = alarms
            .iter()
            .filter(|alarm| {
                alarm.severity == AlarmSeverity::Warning && alarm.acknowledged_at.is_none()
            })
            .count();
        let acknowledged = alarms
            .iter()
            .filter(|alarm| alarm.acknowledged_at.is_some())
            .count();

        Self {
            alarms,
            critical,
            warning,
            acknowledged,
        }
    }
}
