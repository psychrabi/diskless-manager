use anyhow::{bail, Context, Result};
use regex::Regex;
use std::{collections::BTreeSet, net::Ipv4Addr};

pub(crate) fn reconcile_dynamic_pool(primary: &str, clients: &str) -> Result<String> {
    // Persist the original bounds in the generated file, not the progressively
    // smaller ranges. This also allows a completely exhausted pool to recover.
    let marked = Regex::new(
        r"(?ms)^[ \t]*# diskless-pool-base ([0-9.]+) ([0-9.]+)\n.*?^[ \t]*# diskless-pool-end[ \t]*",
    )?;
    let legacy = Regex::new(
        r#"(?m)^[ \t]*pool\s*\{\s*allow members of "pxeclients";\s*range ([0-9.]+) ([0-9.]+);\s*\}"#,
    )?;
    let matches: Vec<_> = if marked.is_match(primary) {
        marked.captures_iter(primary).collect()
    } else {
        legacy.captures_iter(primary).collect()
    };
    if matches.len() != 1 {
        bail!("Cannot identify the manager-owned DHCP pool; regenerate DHCP configuration before changing reservations");
    }
    let matched = &matches[0];
    let start = u32::from(
        matched[1]
            .parse::<Ipv4Addr>()
            .context("invalid pool start")?,
    );
    let end = u32::from(matched[2].parse::<Ipv4Addr>().context("invalid pool end")?);
    if start > end {
        bail!("DHCP pool start exceeds end");
    }

    let mut excluded = BTreeSet::new();
    let addresses = Regex::new(r"\bfixed-address\s+([^;]+);")?;
    for content in [primary, clients] {
        let unquoted = without_comments_and_strings(content);
        for capture in addresses.captures_iter(&unquoted) {
            for address in capture[1].split(',') {
                let ip = u32::from(address.trim().parse::<Ipv4Addr>().with_context(|| {
                    format!(
                        "Reservation must use an explicit IPv4 address: {}",
                        address.trim()
                    )
                })?);
                if (start..=end).contains(&ip) {
                    excluded.insert(ip);
                }
            }
        }
    }
    let mut ranges = Vec::new();
    let mut next = u64::from(start);
    for ip in excluded {
        let ip = u64::from(ip);
        if next < ip {
            ranges.push((next as u32, (ip - 1) as u32));
        }
        next = ip + 1;
    }
    if next <= u64::from(end) {
        ranges.push((next as u32, end));
    }
    let mut replacement = format!(
        "    # diskless-pool-base {} {}\n",
        Ipv4Addr::from(start),
        Ipv4Addr::from(end)
    );
    if !ranges.is_empty() {
        replacement.push_str("    pool {\n        allow members of \"pxeclients\";\n");
        for (start, end) in ranges {
            replacement.push_str(&format!(
                "        range {} {};\n",
                Ipv4Addr::from(start),
                Ipv4Addr::from(end)
            ));
        }
        replacement.push_str("    }\n");
    }
    replacement.push_str("    # diskless-pool-end");
    let span = matched.get(0).context("missing DHCP pool match")?;
    Ok(format!(
        "{}{}{}",
        &primary[..span.start()],
        replacement,
        &primary[span.end()..]
    ))
}

fn without_comments_and_strings(content: &str) -> String {
    let mut quoted = false;
    let mut comment = false;
    let mut escaped = false;
    content
        .chars()
        .map(|c| {
            if comment {
                if c == '\n' {
                    comment = false;
                    return '\n';
                }
                return ' ';
            }
            if quoted {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    quoted = false;
                }
                return ' ';
            }
            if c == '#' {
                comment = true;
                return ' ';
            }
            if c == '"' {
                quoted = true;
                return ' ';
            }
            c
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = "# operator comment\nsubnet 192.168.1.0 netmask 255.255.255.0 {\n    pool {\n        allow members of \"pxeclients\";\n        range 192.168.1.100 192.168.1.105;\n    }\n    next-server 192.168.1.250;\n}\n";

    fn ranges(config: &str) -> Vec<(String, String)> {
        regex::Regex::new(r"range\s+(\S+)\s+(\S+);")
            .unwrap()
            .captures_iter(config)
            .map(|c| (c[1].into(), c[2].into()))
            .collect()
    }

    #[test]
    fn reservations_split_pool_without_losing_available_addresses() {
        for (ips, expected) in [
            (vec![102], vec![(100, 101), (103, 105)]),
            (vec![100, 105], vec![(101, 104)]),
            (vec![101, 102, 102, 99, 106], vec![(100, 100), (103, 105)]),
            (vec![100, 101, 102, 103, 104, 105], vec![]),
            (vec![], vec![(100, 105)]),
        ] {
            let clients = ips
                .iter()
                .map(|ip| format!("host PC{ip} {{ fixed-address 192.168.1.{ip}; }}\n"))
                .collect::<String>();
            let output = reconcile_dynamic_pool(CONFIG, &clients).unwrap();
            let expected: Vec<_> = expected
                .into_iter()
                .map(|(a, b)| (format!("192.168.1.{a}"), format!("192.168.1.{b}")))
                .collect();
            assert_eq!(ranges(&output), expected, "reservations: {ips:?}");
            assert!(output.contains("# operator comment"));
            assert!(output.contains("next-server 192.168.1.250;"));
        }
    }

    #[test]
    fn removing_reservations_restores_base_pool_even_after_exhaustion() {
        let clients = (100..=105)
            .map(|ip| format!("fixed-address 192.168.1.{ip};\n"))
            .collect::<String>();
        let exhausted = reconcile_dynamic_pool(CONFIG, &clients).unwrap();
        assert!(ranges(&exhausted).is_empty());
        let restored = reconcile_dynamic_pool(&exhausted, "").unwrap();
        assert_eq!(
            ranges(&restored),
            vec![("192.168.1.100".into(), "192.168.1.105".into())]
        );
        assert_eq!(reconcile_dynamic_pool(&restored, "").unwrap(), restored);
    }

    #[test]
    fn changing_a_reservation_releases_only_its_previous_address() {
        let first = reconcile_dynamic_pool(CONFIG, "fixed-address 192.168.1.102;").unwrap();
        let changed = reconcile_dynamic_pool(&first, "fixed-address 192.168.1.104;").unwrap();
        assert_eq!(
            ranges(&changed),
            vec![
                ("192.168.1.100".into(), "192.168.1.103".into()),
                ("192.168.1.105".into(), "192.168.1.105".into())
            ]
        );
        assert_eq!(
            reconcile_dynamic_pool(&changed, "fixed-address 192.168.1.104;").unwrap(),
            changed
        );
    }

    #[test]
    fn ambiguous_pools_and_unresolved_reservations_are_rejected() {
        assert!(reconcile_dynamic_pool(&format!("{CONFIG}{CONFIG}"), "").is_err());
        assert!(reconcile_dynamic_pool(CONFIG, "fixed-address pc.example;").is_err());
        assert!(reconcile_dynamic_pool("# no managed pool", "").is_err());
    }

    #[test]
    fn includes_primary_reservations_but_ignores_comments_and_strings() {
        let primary = format!("{CONFIG}\nhost ADMIN {{ fixed-address 192.168.1.101; }}\n# fixed-address 192.168.1.102;\noption host-name \"fixed-address 192.168.1.103;\";\n");
        let output = reconcile_dynamic_pool(
            &primary,
            "host OTHER { fixed-address 192.168.1.104, 192.168.1.105; }",
        )
        .unwrap();
        assert_eq!(
            ranges(&output),
            vec![
                ("192.168.1.100".into(), "192.168.1.100".into()),
                ("192.168.1.102".into(), "192.168.1.103".into())
            ]
        );
    }
}
