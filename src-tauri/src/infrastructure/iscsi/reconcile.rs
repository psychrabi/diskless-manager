use anyhow::{Context, Result};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::path::PathBuf;

/// Return the configfs file that tracks dynamic iSCSI sessions for a target.
fn dynamic_sessions_path(target_iqn: &str) -> PathBuf {
    PathBuf::from(format!(
        "/sys/kernel/config/target/iscsi/{target_iqn}/tpgt_1/dynamic_sessions"
    ))
}

/// Active TCP peers for the client's named initiator ACL. Missing or unreadable state
/// is an error so a report cannot authenticate from an unknown session.
pub fn active_session_peer_ips(target_iqn: &str, client_name: &str) -> Result<Vec<IpAddr>> {
    anyhow::ensure!(!client_name.trim().is_empty(), "invalid client name");
    anyhow::ensure!(
        !target_iqn.is_empty() && !target_iqn.contains('/') && !target_iqn.contains(".."),
        "invalid target identifier"
    );
    active_session_peer_ips_at(
        dynamic_sessions_path(target_iqn)
            .parent()
            .context("session path has no parent")?,
        &crate::infrastructure::pxe::client_initiator_iqn(client_name),
    )
}

fn active_session_peer_ips_at(tpg: &Path, initiator_iqn: &str) -> Result<Vec<IpAddr>> {
    anyhow::ensure!(
        initiator_iqn.starts_with("iqn.")
            && !initiator_iqn.contains('/')
            && !initiator_iqn.contains(".."),
        "invalid initiator identifier"
    );
    let info = fs::read_to_string(tpg.join("acls").join(initiator_iqn).join("info"))
        .context("named iSCSI ACL session state unavailable")?;
    if info.trim() == format!("No active iSCSI Session for Initiator Endpoint: {initiator_iqn}") {
        return Ok(Vec::new());
    }
    anyhow::ensure!(
        info.lines()
            .any(|line| line == format!("InitiatorName: {initiator_iqn}"))
            && info
                .lines()
                .any(|line| line == "Session State: TARG_SESS_STATE_LOGGED_IN"),
        "malformed or inactive named iSCSI ACL session"
    );
    let mut peers = Vec::new();
    let mut expecting_address = false;
    for line in info.lines() {
        if let Some(state) = line
            .strip_prefix("CID: ")
            .and_then(|line| line.split_once(" Connection State: "))
        {
            anyhow::ensure!(!expecting_address, "missing iSCSI connection address");
            let _: u16 = state.0.parse().context("malformed iSCSI connection ID")?;
            anyhow::ensure!(
                matches!(
                    state.1,
                    "TARG_CONN_STATE_FREE"
                        | "TARG_CONN_STATE_XPT_UP"
                        | "TARG_CONN_STATE_IN_LOGIN"
                        | "TARG_CONN_STATE_LOGGED_IN"
                        | "TARG_CONN_STATE_IN_LOGOUT"
                        | "TARG_CONN_STATE_LOGOUT_REQUESTED"
                        | "TARG_CONN_STATE_CLEANUP_WAIT"
                ),
                "malformed iSCSI connection state"
            );
            expecting_address = state.1 == "TARG_CONN_STATE_LOGGED_IN";
        } else if let Some(rest) = line.trim_start().strip_prefix("Address ") {
            let address = rest
                .split_whitespace()
                .next()
                .context("missing iSCSI peer address")?;
            let peer: IpAddr = address.parse().context("malformed iSCSI peer address")?;
            if expecting_address {
                let ip = match peer {
                    IpAddr::V6(ip) => ip
                        .to_ipv4_mapped()
                        .map(IpAddr::V4)
                        .unwrap_or(IpAddr::V6(ip)),
                    ip => ip,
                };
                peers.push(ip);
                expecting_address = false;
            }
        }
    }
    anyhow::ensure!(!expecting_address, "missing iSCSI connection address");
    anyhow::ensure!(
        !peers.is_empty(),
        "logged-in iSCSI session has no active connection"
    );
    Ok(peers)
}

/// Interpret the contents of LIO's dynamic-session file.
///
/// An empty file means that the target has no active dynamic sessions.
/// Non-empty session entries mean that an initiator is connected.
fn has_active_sessions_content(content: &str) -> bool {
    content.lines().any(|line| !line.trim().is_empty())
}

/// Conservative session observation for automatic destructive operations.
/// Missing/unreadable files are unknown. Explicit ACL sessions are inspected
/// as well as dynamic sessions, since either can hold the client disk open.
pub fn confirmed_target_connected(target_iqn: &str) -> Result<bool> {
    anyhow::ensure!(
        !target_iqn.is_empty() && !target_iqn.contains('/') && !target_iqn.contains(".."),
        "invalid target identifier"
    );
    confirmed_sessions_at(&dynamic_sessions_path(target_iqn))
}

fn confirmed_sessions_at(path: &std::path::Path) -> Result<bool> {
    let content = fs::read_to_string(path).context("iSCSI session state unavailable")?;
    if has_active_sessions_content(&content) {
        return Ok(true);
    }
    let parent = path.parent().context("session path has no parent")?;
    for acl in fs::read_dir(parent.join("acls")).context("iSCSI ACL state unavailable")? {
        let info = fs::read_to_string(acl?.path().join("info"))
            .context("iSCSI ACL session state unavailable")?;
        if !info.trim_start().starts_with("No active iSCSI Session") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Check whether an iSCSI target currently has active initiator sessions.
///
/// The configfs path is absent when the target does not exist. That is
/// treated as no active sessions rather than as an infrastructure error.
pub fn target_has_active_sessions(target_iqn: &str) -> Result<bool> {
    if target_iqn.trim().is_empty() {
        return Ok(false);
    }

    match fs::read_to_string(dynamic_sessions_path(target_iqn)) {
        Ok(content) if has_active_sessions_content(&content) => Ok(true),
        Ok(_) => confirmed_sessions_at(&dynamic_sessions_path(target_iqn)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("failed to inspect active iSCSI sessions"),
    }
}

#[cfg(test)]
mod tests {
    use super::has_active_sessions_content;

    #[test]
    fn named_acl_peers_include_only_logged_in_connections() {
        let root = tempfile::tempdir().unwrap();
        let acl = root.path().join("acls/iqn.2026-01.client:client.pc_001");
        std::fs::create_dir_all(&acl).unwrap();
        std::fs::write(
            acl.join("info"),
            "InitiatorName: iqn.2026-01.client:client.pc_001\nSession State: TARG_SESS_STATE_LOGGED_IN\nCID: 0 Connection State: TARG_CONN_STATE_LOGGED_IN\n Address 192.0.2.10 TCP StatSN: 0x01\nCID: 1 Connection State: TARG_CONN_STATE_LOGGED_IN\n Address ::ffff:192.0.2.11 TCP StatSN: 0x02\nCID: 2 Connection State: TARG_CONN_STATE_IN_LOGOUT\n Address 192.0.2.12 TCP StatSN: 0x03\n",
        ).unwrap();
        assert_eq!(
            super::active_session_peer_ips_at(root.path(), "iqn.2026-01.client:client.pc_001")
                .unwrap(),
            [
                "192.0.2.10".parse::<std::net::IpAddr>().unwrap(),
                "192.0.2.11".parse().unwrap()
            ]
        );
    }

    #[test]
    fn named_acl_inactive_and_unavailable_states_are_distinct() {
        let root = tempfile::tempdir().unwrap();
        let acl = root.path().join("acls/iqn.2026-01.client:client.pc001");
        std::fs::create_dir_all(&acl).unwrap();
        assert!(
            super::active_session_peer_ips_at(root.path(), "iqn.2026-01.client:client.pc001")
                .is_err()
        );
        std::fs::write(
            acl.join("info"),
            "No active iSCSI Session for Initiator Endpoint: iqn.2026-01.client:client.pc001\n",
        )
        .unwrap();
        assert!(
            super::active_session_peer_ips_at(root.path(), "iqn.2026-01.client:client.pc001")
                .unwrap()
                .is_empty()
        );
        std::fs::write(
            acl.join("info"),
            "Session State: TARG_SESS_STATE_LOGGED_IN\nAddress surprise\n",
        )
        .unwrap();
        assert!(
            super::active_session_peer_ips_at(root.path(), "iqn.2026-01.client:client.pc001")
                .is_err()
        );
    }

    #[test]
    fn logged_in_session_without_logged_in_connection_is_unavailable() {
        let root = tempfile::tempdir().unwrap();
        let acl = root.path().join("acls/iqn.2026-01.client:client.pc001");
        std::fs::create_dir_all(&acl).unwrap();
        std::fs::write(acl.join("info"), "InitiatorName: iqn.2026-01.client:client.pc001\nSession State: TARG_SESS_STATE_LOGGED_IN\n----------------------[iSCSI Connections]-------------------------\n").unwrap();
        assert!(
            super::active_session_peer_ips_at(root.path(), "iqn.2026-01.client:client.pc001")
                .is_err()
        );
    }

    #[test]
    fn automatic_reset_requires_readable_session_and_acl_state() {
        let root = std::env::temp_dir().join(format!("diskless-session-{}", uuid::Uuid::new_v4()));
        let path = root.join("dynamic_sessions");
        assert!(super::confirmed_sessions_at(&path).is_err());
        std::fs::create_dir_all(root.join("acls/client")).unwrap();
        std::fs::write(&path, "").unwrap();
        assert!(super::confirmed_sessions_at(&path).is_err());
        std::fs::write(
            root.join("acls/client/info"),
            "No active iSCSI Session for Initiator Endpoint: client\n",
        )
        .unwrap();
        assert!(!super::confirmed_sessions_at(&path).unwrap());
        std::fs::write(
            root.join("acls/client/info"),
            "InitiatorName: client\nSession State: TARG_SESS_STATE_LOGGED_IN\n",
        )
        .unwrap();
        assert!(super::confirmed_sessions_at(&path).unwrap());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_dynamic_session_content_means_no_active_sessions() {
        assert!(!has_active_sessions_content(""));
        assert!(!has_active_sessions_content("\n  \n"));
    }

    #[test]
    fn dynamic_session_content_detects_active_sessions() {
        let content = "iqn.1993-08.org.debian:01:client-a\n";

        assert!(has_active_sessions_content(content));
    }

    #[test]
    fn dynamic_session_content_ignores_blank_lines() {
        let content = "\n\t\n iqn.1993-08.org.debian:01:client-b \n";

        assert!(has_active_sessions_content(content));
    }
}
