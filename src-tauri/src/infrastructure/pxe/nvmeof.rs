use crate::infrastructure::nvmeof::NVMET_TCP_PORT;

#[must_use]
pub fn nvme_tcp_uri(server_ip: &str, nqn: &str) -> String {
    format!("nvme://{}:{}/{}", server_ip.trim(), NVMET_TCP_PORT, nqn.trim())
}

#[must_use]
pub fn render_windows_nvmeof_boot(nqn: &str) -> String {
    format!(
        r##"# Experimental Windows Server NVMe/TCP boot path.
# Requires an iPXE build with NVMe/TCP initiator support.
set nvme-nqn {nqn}
set nvme-root nvme://${{next-server}}:{port}/${{nvme-nqn}}
set keep-san 1
set net0/gateway 0.0.0.0
echo NVMe/TCP target: ${{nvme-root}}
sanboot ${{nvme-root}} || goto failed
"##,
        port = NVMET_TCP_PORT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nvme_tcp_uri() {
        assert_eq!(
            nvme_tcp_uri(
                "192.168.1.250",
                "nqn.2026-09.local.diskless:client.pc001"
            ),
            "nvme://192.168.1.250:4420/nqn.2026-09.local.diskless:client.pc001"
        );
    }

    #[test]
    fn script_uses_next_server_and_keeps_san() {
        let script = render_windows_nvmeof_boot(
            "nqn.2026-09.local.diskless:client.pc001"
        );
        assert!(script.contains("nvme://${next-server}:4420/${nvme-nqn}"));
        assert!(script.contains("set keep-san 1"));
        assert!(script.contains("sanboot ${nvme-root}"));
    }
}
