use std::path::Path;

pub(super) async fn install_files(
    files: &[(&Path, &str)],
    staging: &Path,
    move_file: impl Fn(&Path, &Path) -> Result<(), String>,
    remove_file: impl Fn(&Path) -> Result<(), String>,
    validate: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    tokio::fs::create_dir_all(staging)
        .await
        .map_err(|e| e.to_string())?;
    let mut backups = Vec::new();
    for (index, (destination, content)) in files.iter().enumerate() {
        let backup = staging.join(format!("old-{index}"));
        match tokio::fs::read(destination).await {
            Ok(original) => {
                tokio::fs::write(&backup, original)
                    .await
                    .map_err(|e| e.to_string())?;
                backups.push(Some(backup));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => backups.push(None),
            Err(error) => return Err(format!("Cannot back up {}: {error}", destination.display())),
        }
        let temporary = staging.join(format!("new-{index}"));
        tokio::fs::write(&temporary, content)
            .await
            .map_err(|e| e.to_string())?;
    }
    let mut attempted = 0;
    let mut result = Ok(());
    for (index, (destination, _)) in files.iter().enumerate() {
        attempted = index + 1;
        result = move_file(&staging.join(format!("new-{index}")), destination);
        if result.is_err() {
            break;
        }
    }
    if result.is_ok() {
        result = validate();
    }
    if let Err(mut error) = result {
        let mut restore_failed = false;
        for index in (0..attempted).rev() {
            let destination = files[index].0;
            let restore = match &backups[index] {
                Some(backup) => move_file(backup, destination),
                None => remove_file(destination),
            };
            if let Err(restore_error) = restore {
                restore_failed = true;
                error.push_str(&format!(
                    "; restoring {} failed: {restore_error}",
                    destination.display()
                ));
            }
        }
        if restore_failed {
            error.push_str(&format!(
                "; recovery files retained in {}",
                staging.display()
            ));
        } else {
            let _ = tokio::fs::remove_dir_all(staging).await;
        }
        return Err(error);
    }
    let _ = tokio::fs::remove_dir_all(staging).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(a: &Path, b: &Path) -> Result<(), String> {
        std::fs::rename(a, b).map_err(|e| e.to_string())
    }
    fn remove(path: &Path) -> Result<(), String> {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }

    #[tokio::test]
    async fn failed_validation_restores_both_files() {
        let root =
            std::env::temp_dir().join(format!("diskless-config-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let primary = root.join("dhcpd.conf");
        let clients = root.join("clients.conf");
        std::fs::write(&primary, "old ranges").unwrap();
        std::fs::write(&clients, "old reservations").unwrap();
        let result = install_files(
            &[(&primary, "new ranges"), (&clients, "new reservations")],
            &root.join("staging"),
            rename,
            remove,
            || {
                assert_eq!(std::fs::read_to_string(&primary).unwrap(), "new ranges");
                assert_eq!(
                    std::fs::read_to_string(&clients).unwrap(),
                    "new reservations"
                );
                Err("invalid DHCP configuration".into())
            },
        )
        .await;
        let actual = (
            std::fs::read_to_string(&primary).unwrap(),
            std::fs::read_to_string(&clients).unwrap(),
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.is_err());
        assert_eq!(actual, ("old ranges".into(), "old reservations".into()));
    }

    #[tokio::test]
    async fn successful_validation_keeps_both_files_and_cleans_backups() {
        let root =
            std::env::temp_dir().join(format!("diskless-config-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let primary = root.join("dhcpd.conf");
        let clients = root.join("clients.conf");
        let staging = root.join("staging");
        install_files(
            &[(&primary, "new ranges"), (&clients, "new reservations")],
            &staging,
            rename,
            remove,
            || Ok(()),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read_to_string(&primary).unwrap(), "new ranges");
        assert_eq!(
            std::fs::read_to_string(&clients).unwrap(),
            "new reservations"
        );
        assert!(!staging.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn failed_initial_validation_restores_absent_files() {
        let root =
            std::env::temp_dir().join(format!("diskless-config-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let primary = root.join("dhcpd.conf");
        let clients = root.join("clients.conf");
        let result = install_files(
            &[(&primary, "new ranges"), (&clients, "new reservations")],
            &root.join("staging"),
            rename,
            remove,
            || Err("invalid".into()),
        )
        .await;
        assert!(result.is_err());
        assert!(!primary.exists());
        assert!(!clients.exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn failed_second_install_restores_first_file() {
        let root =
            std::env::temp_dir().join(format!("diskless-config-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let primary = root.join("dhcpd.conf");
        let clients = root.join("clients.conf");
        std::fs::write(&primary, "old ranges").unwrap();
        std::fs::write(&clients, "old reservations").unwrap();
        let result = install_files(
            &[(&primary, "new ranges"), (&clients, "new reservations")],
            &root.join("staging"),
            |a, b| {
                if a.file_name().unwrap() == "new-1" {
                    return Err("cannot install clients file".into());
                }
                rename(a, b)
            },
            remove,
            || panic!("validation must not run after failed install"),
        )
        .await;
        let actual = (
            std::fs::read_to_string(&primary).unwrap(),
            std::fs::read_to_string(&clients).unwrap(),
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert!(result.is_err());
        assert_eq!(actual, ("old ranges".into(), "old reservations".into()));
    }
}
