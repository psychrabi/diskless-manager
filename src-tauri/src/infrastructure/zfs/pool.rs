use anyhow::{Context, Result};

use crate::infrastructure::command::run_command_output_no_sudo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZpoolHealth {
    pub name: String,
    pub health: String,
}

/// Read pool health without changing pool state.
pub fn list_pool_health() -> Result<Vec<ZpoolHealth>> {
    let output = run_command_output_no_sudo(["zpool", "list", "-H", "-o", "name,health"])
        .map_err(anyhow::Error::from)
        .context("failed to query ZFS pool health")?;
    Ok(parse_pool_health(&output))
}

fn parse_pool_health(output: &str) -> Vec<ZpoolHealth> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let health = fields.next()?;
            Some(ZpoolHealth {
                name: name.to_string(),
                health: health.to_ascii_uppercase(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_pool_health;

    #[test]
    fn parses_names_and_health_states() {
        let pools = parse_pool_health("tank\tONLINE\nbackup DEGRADED\n");

        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0].name, "tank");
        assert_eq!(pools[0].health, "ONLINE");
        assert_eq!(pools[1].health, "DEGRADED");
    }

    #[test]
    fn ignores_lines_without_a_health_column() {
        assert!(parse_pool_health("tank\n\n").is_empty());
    }
}
