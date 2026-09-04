use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

/// Traffic rates in megabytes per second, measured from kernel byte counters.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Throughput {
    /// Bytes sent by the server to the client (client read traffic).
    pub read_speed_mbps: f64,
    /// Bytes received by the server from the client (client write traffic).
    pub write_speed_mbps: f64,
}

/// Per-client measurements from the authoritative collector.
#[derive(Debug, Clone, Serialize)]
pub struct ClientTrafficMetrics {
    /// Registered client address.
    pub ip: String,
    /// All tracked network traffic involving this address.
    pub network: Option<Throughput>,
    /// Tracked TCP traffic involving this address and the configured iSCSI port.
    pub iscsi: Option<Throughput>,
    /// The first successful sample has no previous counter to calculate a rate.
    pub warming_up: bool,
}

/// Host-level ZFS traffic measurement.
#[derive(Debug, Clone, Serialize)]
pub struct StorageTrafficMetrics {
    /// Aggregate read/write rate across discovered ZFS pools.
    pub zfs: Option<ZfsThroughput>,
    /// The first successful sample has no previous counter to calculate a rate.
    pub warming_up: bool,
}

/// ZFS read/write rate in megabytes per second.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ZfsThroughput {
    /// Bytes per second read by ZFS.
    pub read_speed_mbps: f64,
    /// Bytes per second written by ZFS.
    pub write_speed_mbps: f64,
}

/// One read-only metrics sample shared by REST and WebSocket handlers.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    /// Measured client traffic, keyed by client IP in each row.
    pub clients: Vec<ClientTrafficMetrics>,
    /// Measured host storage traffic.
    pub storage: StorageTrafficMetrics,
    /// Sources that could not be read. No estimate is substituted for them.
    pub warnings: Vec<String>,
    /// UTC timestamp of the sample.
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DirectionalCounters {
    from_client_bytes: u64,
    to_client_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ClientCounters {
    network: DirectionalCounters,
    iscsi: DirectionalCounters,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ZfsCounters {
    read_bytes: u64,
    write_bytes: u64,
}

#[derive(Debug, Clone, Default)]
struct RawCounters {
    clients: HashMap<IpAddr, ClientCounters>,
    zfs: Option<ZfsCounters>,
    warnings: Vec<String>,
}

/// Reads Linux accounting sources without executing shell commands.
pub struct LinuxMetricsReader {
    proc_root: PathBuf,
    zfs_kstat_root: PathBuf,
    target_core_root: PathBuf,
}

impl Default for LinuxMetricsReader {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            zfs_kstat_root: PathBuf::from("/proc/spl/kstat/zfs"),
            target_core_root: PathBuf::from("/sys/kernel/config/target/core"),
        }
    }
}

impl LinuxMetricsReader {
    #[cfg(test)]
    fn with_roots(proc_root: PathBuf, zfs_kstat_root: PathBuf) -> Self {
        Self {
            target_core_root: proc_root.join("missing-target-core"),
            proc_root,
            zfs_kstat_root,
        }
    }

    fn read_lio_backstore(&self, backstore: &str) -> Result<DirectionalCounters, String> {
        let plugins = fs::read_dir(&self.target_core_root).map_err(|error| error.to_string())?;
        for plugin in plugins {
            let plugin = plugin.map_err(|error| error.to_string())?;
            let statistics = plugin.path().join(backstore).join("statistics/scsi_lu");
            if !statistics.is_dir() {
                continue;
            }
            let read_mbytes = fs::read_to_string(statistics.join("read_mbytes"))
                .map_err(|error| error.to_string())?
                .trim()
                .parse::<u64>()
                .map_err(|error| error.to_string())?;
            let write_mbytes = fs::read_to_string(statistics.join("write_mbytes"))
                .map_err(|error| error.to_string())?
                .trim()
                .parse::<u64>()
                .map_err(|error| error.to_string())?;
            return Ok(DirectionalCounters {
                from_client_bytes: write_mbytes.saturating_mul(1024 * 1024),
                to_client_bytes: read_mbytes.saturating_mul(1024 * 1024),
            });
        }
        Err(format!(
            "LIO backstore statistics not found for {backstore}"
        ))
    }

    fn read(&self, clients: &[IpAddr], iscsi_port: u16) -> RawCounters {
        let mut raw = RawCounters::default();
        match self.read_conntrack(clients, iscsi_port) {
            Ok(counters) => raw.clients = counters,
            Err(error) => raw
                .warnings
                .push(format!("network/iSCSI metrics unavailable: {error}")),
        }
        match self.read_zfs() {
            Ok(counters) => raw.zfs = Some(counters),
            Err(error) => raw
                .warnings
                .push(format!("ZFS metrics unavailable: {error}")),
        }
        raw
    }

    fn read_conntrack(
        &self,
        clients: &[IpAddr],
        iscsi_port: u16,
    ) -> Result<HashMap<IpAddr, ClientCounters>, String> {
        let path = [
            self.proc_root.join("net/nf_conntrack"),
            self.proc_root.join("net/ip_conntrack"),
        ]
        .into_iter()
        .find(|candidate| candidate.exists())
        .ok_or_else(|| "conntrack accounting file is not available".to_owned())?;
        let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let has_entries = content.lines().any(|line| line.contains("src="));
        let has_byte_counters = content.lines().any(|line| line.contains("bytes="));
        if has_entries && !has_byte_counters {
            return Err(
                "conntrack byte accounting is disabled; enable net.netfilter.nf_conntrack_acct=1"
                    .to_owned(),
            );
        }
        let mut counters = clients
            .iter()
            .copied()
            .map(|client| (client, ClientCounters::default()))
            .collect::<HashMap<_, _>>();
        for line in content.lines() {
            for client in clients {
                if let Some(entry) = parse_conntrack_counters(line, *client, iscsi_port) {
                    let aggregate = counters.entry(*client).or_default();
                    aggregate.network.from_client_bytes = aggregate
                        .network
                        .from_client_bytes
                        .saturating_add(entry.network.from_client_bytes);
                    aggregate.network.to_client_bytes = aggregate
                        .network
                        .to_client_bytes
                        .saturating_add(entry.network.to_client_bytes);
                    aggregate.iscsi.from_client_bytes = aggregate
                        .iscsi
                        .from_client_bytes
                        .saturating_add(entry.iscsi.from_client_bytes);
                    aggregate.iscsi.to_client_bytes = aggregate
                        .iscsi
                        .to_client_bytes
                        .saturating_add(entry.iscsi.to_client_bytes);
                }
            }
        }
        Ok(counters)
    }

    fn read_zfs(&self) -> Result<ZfsCounters, String> {
        let entries = fs::read_dir(&self.zfs_kstat_root).map_err(|error| error.to_string())?;
        let mut totals = ZfsCounters::default();
        let mut found_pool = false;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let pool_path = entry.path();
            let io_path = [pool_path.join("io"), pool_path.join("iostats")]
                .into_iter()
                .find(|candidate| candidate.is_file());
            let Some(io_path) = io_path else {
                continue;
            };
            let content = fs::read_to_string(&io_path).map_err(|error| error.to_string())?;
            let counters = parse_zfs_kstat(&content)
                .ok_or_else(|| format!("invalid ZFS kstat {}", io_path.display()))?;
            totals.read_bytes = totals.read_bytes.saturating_add(counters.read_bytes);
            totals.write_bytes = totals.write_bytes.saturating_add(counters.write_bytes);
            found_pool = true;
        }
        if found_pool {
            Ok(totals)
        } else {
            Err("no ZFS pool kstats found".to_owned())
        }
    }
}

/// Stateful delta calculator used by all metrics transports.
pub struct MetricsCollector {
    reader: LinuxMetricsReader,
    previous: Mutex<Option<(Instant, RawCounters)>>,
    previous_lio: Mutex<Option<(Instant, HashMap<String, DirectionalCounters>)>>,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            reader: LinuxMetricsReader::default(),
            previous: Mutex::new(None),
            previous_lio: Mutex::new(None),
        }
    }
}

impl MetricsCollector {
    /// Collects exactly one host sample and calculates rates from the preceding sample.
    pub fn collect(&self, client_ips: &[String], iscsi_port: u16) -> MetricsSnapshot {
        let valid_clients = client_ips
            .iter()
            .filter_map(|value| value.parse::<IpAddr>().ok())
            .collect::<Vec<_>>();
        let mut previous = self
            .previous
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let raw = self.reader.read(&valid_clients, iscsi_port);
        Self::finish_sample_with_previous(client_ips, raw, &mut previous)
    }

    /// Collect per-client iSCSI traffic from LIO's authoritative backstore counters.
    pub fn collect_lio(&self, clients: &[(String, String)]) -> HashMap<String, Throughput> {
        let now = Instant::now();
        let current = clients
            .iter()
            .filter_map(|(ip, backstore)| {
                self.reader
                    .read_lio_backstore(backstore)
                    .ok()
                    .map(|counters| (ip.clone(), counters))
            })
            .collect::<HashMap<_, _>>();
        let mut previous = self
            .previous_lio
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let rates = previous
            .as_ref()
            .map(|(previous_time, old)| {
                let elapsed = now.duration_since(*previous_time).as_secs_f64();
                current
                    .iter()
                    .filter_map(|(ip, counters)| {
                        old.get(ip)
                            .map(|old| (ip.clone(), throughput(*counters, *old, elapsed)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        *previous = Some((now, current));
        rates
    }

    #[cfg(test)]
    fn finish_sample(&self, client_ips: &[String], raw: RawCounters) -> MetricsSnapshot {
        let mut previous = self
            .previous
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self::finish_sample_with_previous(client_ips, raw, &mut previous)
    }

    fn finish_sample_with_previous(
        client_ips: &[String],
        raw: RawCounters,
        previous: &mut Option<(Instant, RawCounters)>,
    ) -> MetricsSnapshot {
        let now = Instant::now();
        let elapsed = previous
            .as_ref()
            .map(|(previous_time, _)| now.duration_since(*previous_time).as_secs_f64())
            .filter(|seconds| *seconds > 0.0);
        let previous_raw = previous.as_ref().map(|(_, values)| values);
        let clients = client_ips
            .iter()
            .map(|ip| {
                let parsed = ip.parse::<IpAddr>().ok();
                let current = parsed.and_then(|address| raw.clients.get(&address));
                let old = parsed.and_then(|address| {
                    previous_raw.and_then(|values| values.clients.get(&address))
                });
                let network = current
                    .zip(old)
                    .zip(elapsed)
                    .map(|((current, old), seconds)| {
                        throughput(current.network, old.network, seconds)
                    });
                let iscsi = current
                    .zip(old)
                    .zip(elapsed)
                    .map(|((current, old), seconds)| throughput(current.iscsi, old.iscsi, seconds));
                ClientTrafficMetrics {
                    ip: ip.clone(),
                    warming_up: current.is_some() && (network.is_none() || iscsi.is_none()),
                    network,
                    iscsi,
                }
            })
            .collect();
        let zfs = raw
            .zfs
            .zip(previous_raw.and_then(|values| values.zfs))
            .zip(elapsed)
            .map(|((current, old), seconds)| ZfsThroughput {
                read_speed_mbps: bytes_to_mbps(
                    current.read_bytes.saturating_sub(old.read_bytes),
                    seconds,
                ),
                write_speed_mbps: bytes_to_mbps(
                    current.write_bytes.saturating_sub(old.write_bytes),
                    seconds,
                ),
            });
        let storage = StorageTrafficMetrics {
            warming_up: raw.zfs.is_some() && zfs.is_none(),
            zfs,
        };
        *previous = Some((now, raw.clone()));
        MetricsSnapshot {
            clients,
            storage,
            warnings: raw.warnings,
            timestamp: Utc::now().timestamp(),
        }
    }
}

fn throughput(
    current: DirectionalCounters,
    previous: DirectionalCounters,
    seconds: f64,
) -> Throughput {
    Throughput {
        read_speed_mbps: bytes_to_mbps(
            current
                .to_client_bytes
                .saturating_sub(previous.to_client_bytes),
            seconds,
        ),
        write_speed_mbps: bytes_to_mbps(
            current
                .from_client_bytes
                .saturating_sub(previous.from_client_bytes),
            seconds,
        ),
    }
}

fn bytes_to_mbps(bytes: u64, seconds: f64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / seconds
}

fn parse_conntrack_counters(line: &str, client: IpAddr, iscsi_port: u16) -> Option<ClientCounters> {
    let tuples = parse_conntrack_tuples(line);
    if tuples.is_empty() {
        return None;
    }
    let mut counters = ClientCounters::default();
    for tuple in tuples {
        if tuple.source == client {
            counters.network.from_client_bytes = counters
                .network
                .from_client_bytes
                .saturating_add(tuple.bytes);
            if tuple.source_port == iscsi_port || tuple.destination_port == iscsi_port {
                counters.iscsi.from_client_bytes =
                    counters.iscsi.from_client_bytes.saturating_add(tuple.bytes);
            }
        }
        if tuple.destination == client {
            counters.network.to_client_bytes =
                counters.network.to_client_bytes.saturating_add(tuple.bytes);
            if tuple.source_port == iscsi_port || tuple.destination_port == iscsi_port {
                counters.iscsi.to_client_bytes =
                    counters.iscsi.to_client_bytes.saturating_add(tuple.bytes);
            }
        }
    }
    Some(counters)
}

#[derive(Debug)]
struct ConntrackTuple {
    source: IpAddr,
    destination: IpAddr,
    source_port: u16,
    destination_port: u16,
    bytes: u64,
}

fn parse_conntrack_tuples(line: &str) -> Vec<ConntrackTuple> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    let mut tuples = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        if !fields[index].starts_with("src=") {
            index += 1;
            continue;
        }
        let window = &fields[index..fields.len().min(index + 8)];
        let value = |name: &str| window.iter().find_map(|field| field.strip_prefix(name));
        if let (
            Some(source),
            Some(destination),
            Some(source_port),
            Some(destination_port),
            Some(bytes),
        ) = (
            value("src="),
            value("dst="),
            value("sport="),
            value("dport="),
            value("bytes="),
        ) {
            if let (Ok(source), Ok(destination), Ok(source_port), Ok(destination_port), Ok(bytes)) = (
                source.parse(),
                destination.parse(),
                source_port.parse(),
                destination_port.parse(),
                bytes.parse(),
            ) {
                tuples.push(ConntrackTuple {
                    source,
                    destination,
                    source_port,
                    destination_port,
                    bytes,
                });
            }
        }
        index += 1;
    }
    tuples
}

fn parse_zfs_kstat(content: &str) -> Option<ZfsCounters> {
    let mut result = ZfsCounters::default();
    let mut found_read = false;
    let mut found_write = false;
    for line in content.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let Some(name) = fields.first() else { continue };
        let value = fields.last().and_then(|value| value.parse::<u64>().ok());
        match *name {
            "nread" => {
                result.read_bytes = value?;
                found_read = true;
            }
            "nwritten" => {
                result.write_bytes = value?;
                found_write = true;
            }
            "arc_read_bytes" | "direct_read_bytes" => {
                result.read_bytes = result.read_bytes.saturating_add(value?);
                found_read = true;
            }
            "arc_write_bytes" | "direct_write_bytes" => {
                result.write_bytes = result.write_bytes.saturating_add(value?);
                found_write = true;
            }
            _ => {}
        }
    }
    (found_read && found_write).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_conntrack_counters, parse_zfs_kstat, ClientCounters, LinuxMetricsReader,
        MetricsCollector, RawCounters, ZfsCounters,
    };
    use std::net::IpAddr;

    #[test]
    fn attributes_conntrack_bytes_to_one_client_and_iscsi_session() {
        let client: IpAddr = "192.168.1.101".parse().unwrap();
        let line = "tcp 6 431999 ESTABLISHED src=192.168.1.101 dst=192.168.1.250 sport=49152 dport=3260 packets=10 bytes=4096 src=192.168.1.250 dst=192.168.1.101 sport=3260 dport=49152 packets=8 bytes=8192 [ASSURED] mark=0 use=1";

        let counters = parse_conntrack_counters(line, client, 3260).unwrap();

        assert_eq!(counters.network.from_client_bytes, 4096);
        assert_eq!(counters.network.to_client_bytes, 8192);
        assert_eq!(counters.iscsi.from_client_bytes, 4096);
        assert_eq!(counters.iscsi.to_client_bytes, 8192);
    }

    #[test]
    fn parses_zfs_read_and_write_counters() {
        let counters = parse_zfs_kstat("nread 4 0x01 100\nnwritten 4 0x01 250\n").unwrap();
        assert_eq!(
            counters,
            ZfsCounters {
                read_bytes: 100,
                write_bytes: 250
            }
        );
    }

    #[test]
    fn parses_current_three_column_zfs_kstats() {
        let counters = parse_zfs_kstat(
            "13 1 0x01 2 320 0 0\nname type data\nnread 4 1048576\nnwritten 4 2097152\n",
        )
        .unwrap();

        assert_eq!(counters.read_bytes, 1_048_576);
        assert_eq!(counters.write_bytes, 2_097_152);
    }

    #[test]
    fn parses_current_pool_iostats_counters() {
        let counters = parse_zfs_kstat(
            "name type data\narc_read_bytes 4 100\narc_write_bytes 4 200\ndirect_read_bytes 4 30\ndirect_write_bytes 4 40\n",
        )
        .unwrap();

        assert_eq!(counters.read_bytes, 130);
        assert_eq!(counters.write_bytes, 240);
    }

    #[test]
    fn collector_requires_two_samples_before_reporting_a_rate() {
        let collector = MetricsCollector::default();
        let first = RawCounters {
            clients: [("192.168.1.101".parse().unwrap(), ClientCounters::default())].into(),
            zfs: Some(ZfsCounters {
                read_bytes: 10,
                write_bytes: 20,
            }),
            warnings: Vec::new(),
        };
        let first_snapshot = collector.finish_sample(&["192.168.1.101".to_owned()], first);
        assert!(first_snapshot.clients[0].network.is_none());
        assert!(first_snapshot.storage.zfs.is_none());
    }

    #[test]
    fn fixture_reader_uses_conntrack_and_zfs_kstats_without_an_interface_name() {
        let root = std::env::temp_dir().join(format!("diskless-metrics-{}", std::process::id()));
        let proc_root = root.join("proc");
        let kstat_root = proc_root.join("spl/kstat/zfs");
        std::fs::create_dir_all(proc_root.join("net")).unwrap();
        std::fs::create_dir_all(kstat_root.join("tank")).unwrap();
        std::fs::write(proc_root.join("net/nf_conntrack"), "tcp 6 1 ESTABLISHED src=192.168.1.101 dst=192.168.1.250 sport=12 dport=3260 packets=1 bytes=1 src=192.168.1.250 dst=192.168.1.101 sport=3260 dport=12 packets=1 bytes=2\n").unwrap();
        std::fs::write(
            kstat_root.join("tank/io"),
            "nread 4 0x01 3\nnwritten 4 0x01 4\n",
        )
        .unwrap();
        let reader = LinuxMetricsReader::with_roots(proc_root, kstat_root);
        let raw = reader.read(&["192.168.1.101".parse().unwrap()], 3260);
        assert_eq!(
            raw.clients[&"192.168.1.101".parse::<IpAddr>().unwrap()]
                .iscsi
                .to_client_bytes,
            2
        );
        assert_eq!(raw.zfs.unwrap().write_bytes, 4);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn reads_lio_backstore_counters_with_client_directions() {
        let root =
            std::env::temp_dir().join(format!("diskless-lio-metrics-{}", uuid::Uuid::new_v4()));
        let statistics = root.join("core/iblock_0/block_pc001/statistics/scsi_lu");
        std::fs::create_dir_all(&statistics).unwrap();
        std::fs::write(statistics.join("read_mbytes"), "12\n").unwrap();
        std::fs::write(statistics.join("write_mbytes"), "3\n").unwrap();
        let reader = LinuxMetricsReader {
            proc_root: root.join("proc"),
            zfs_kstat_root: root.join("zfs"),
            target_core_root: root.join("core"),
        };

        let counters = reader.read_lio_backstore("block_pc001").unwrap();

        assert_eq!(counters.to_client_bytes, 12 * 1024 * 1024);
        assert_eq!(counters.from_client_bytes, 3 * 1024 * 1024);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_conntrack_without_byte_accounting_instead_of_reporting_an_estimate() {
        let root =
            std::env::temp_dir().join(format!("diskless-metrics-no-bytes-{}", std::process::id()));
        let proc_root = root.join("proc");
        std::fs::create_dir_all(proc_root.join("net")).unwrap();
        std::fs::write(
            proc_root.join("net/nf_conntrack"),
            "tcp 6 1 ESTABLISHED src=192.168.1.101 dst=192.168.1.250 sport=12 dport=3260\n",
        )
        .unwrap();
        let reader = LinuxMetricsReader::with_roots(proc_root, root.join("missing-zfs"));

        let error = reader
            .read_conntrack(&["192.168.1.101".parse().unwrap()], 3260)
            .unwrap_err();

        assert!(error.contains("nf_conntrack_acct=1"));
        let _ = std::fs::remove_dir_all(root);
    }
}
