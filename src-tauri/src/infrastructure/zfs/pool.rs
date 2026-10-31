use anyhow::{Context, Result};

use crate::infrastructure::command::run_command_output_no_sudo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZpoolHealth {
    pub name: String,
    pub health: String,
}

/// Read a pool's free capacity in bytes.
///
/// Callers that gate additional storage on capacity must treat an error as
/// "capacity unknown" and skip the operation rather than assume free space.
pub fn pool_free_bytes(pool: &str) -> Result<u64> {
    let output = run_command_output_no_sudo(["zpool", "list", "-H", "-p", "-o", "free", pool])
        .map_err(anyhow::Error::from)
        .with_context(|| format!("failed to query free space for pool '{pool}'"))?;
    parse_pool_free_bytes(&output)
        .with_context(|| format!("unrecognized free space for pool '{pool}'"))
}

fn parse_pool_free_bytes(output: &str) -> Result<u64> {
    let value = output
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .context("zpool reported no free space")?;
    Ok(value.parse::<u64>()?)
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
    use super::{parse_pool_free_bytes, parse_pool_health};

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

    #[test]
    fn parses_exact_free_bytes_and_rejects_unknown_output() {
        assert_eq!(
            parse_pool_free_bytes("53687091200\n").unwrap(),
            53_687_091_200
        );
        for output in ["", "-\n", "12.5G\n"] {
            assert!(parse_pool_free_bytes(output).is_err());
        }
    }
}
