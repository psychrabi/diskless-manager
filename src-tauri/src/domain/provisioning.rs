#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetIqn(String);

impl TargetIqn {
    /// Build a target IQN from a configured prefix and client name.
    ///
    /// Existing persisted IQNs must be reused by callers rather than
    /// regenerated. This constructor is for new targets.
    pub fn for_client_name(prefix: &str, client_name: &str) -> Self {
        let prefix = prefix.trim().trim_end_matches(':');
        let client_name = client_name.trim().to_lowercase();

        Self(format!("{prefix}:client.{client_name}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::TargetIqn;

    #[test]
    fn target_iqn_uses_configured_prefix_and_client_name() {
        let iqn = TargetIqn::for_client_name("iqn.2024-01.com.diskless", "PC001");

        assert_eq!(iqn.as_str(), "iqn.2024-01.com.diskless:client.pc001");
    }

    #[test]
    fn target_iqn_trims_prefix_separator_and_client_whitespace() {
        let iqn = TargetIqn::for_client_name(" iqn.2024-01.com.diskless: ", " PC001 ");

        assert_eq!(iqn.as_str(), "iqn.2024-01.com.diskless:client.pc001");
    }
}
