//! Application-only backups. Restoration is applied before opening application state.
use crate::{state::AppState, types::Claims};
use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    Extension, Json,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
pub const MAX_BACKUP_BYTES: usize = 64 * 1024 * 1024;
const PENDING: &str = "pending-restore.json";
const ROLLBACK: &str = ".restore-rollback";
const FILES: &[&str] = &["diskless.db", "config.toml", "config.json", "jwt-secret"];
const LIVE_FILES: &[&str] = &[
    "diskless.db",
    "diskless.db-wal",
    "diskless.db-shm",
    "config.toml",
    "config.json",
    "jwt-secret",
    "setup-completed",
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    format: String,
    version: u32,
    files: BTreeMap<String, String>,
}

pub async fn create_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<([(header::HeaderName, &'static str); 3], Vec<u8>), StatusCode> {
    if claims.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }
    let dir = state
        .config_path
        .parent()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let bytes = create_bundle(&state.db_pool, dir, crate::auth::jwt_secret())
        .await
        .map_err(|error| {
            log::error!("backup failed: {error}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"diskless-backup.json\"",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}

pub async fn restore_backup(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    bytes: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if claims.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Administrator required"})),
        ));
    }
    let dir = state.config_path.parent().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error":"Configuration directory unavailable"})),
        )
    })?;
    let configured = std::env::var("JWT_SECRET").ok();
    stage_restore(dir, &bytes, configured.as_deref())
        .await
        .map_err(|error| {
            let status = if dir.join(PENDING).exists() {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(serde_json::json!({"error":error.to_string()})))
        })?;
    Ok(Json(
        serde_json::json!({"message":"Restore staged. Restart the server to apply it.", "restart_required":true}),
    ))
}

async fn open_db(path: &Path, readonly: bool) -> anyhow::Result<sqlx::SqlitePool> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .read_only(readonly)
                .create_if_missing(!readonly),
        )
        .await?)
}

fn read_bounded(path: &Path) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        std::fs::symlink_metadata(path)?.file_type().is_file(),
        "Backup source must be a regular file"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take((MAX_BACKUP_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= MAX_BACKUP_BYTES,
        "Backup exceeds 64 MiB limit"
    );
    Ok(bytes)
}

async fn create_bundle(
    pool: &sqlx::SqlitePool,
    dir: &Path,
    secret: &[u8],
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(secret.len() >= 32, "Invalid signing secret");
    let snapshot = tempfile::tempdir()?;
    let db_path = snapshot.path().join("diskless.db");
    // SQLite includes committed WAL transactions in this consistent snapshot.
    sqlx::query("VACUUM INTO ?")
        .bind(db_path.to_string_lossy().as_ref())
        .execute(pool)
        .await?;
    let mut files = BTreeMap::new();
    files.insert("diskless.db".into(), BASE64.encode(read_bounded(&db_path)?));
    files.insert("jwt-secret".into(), BASE64.encode(secret));
    for name in ["config.toml", "config.json"] {
        if dir.join(name).exists() {
            files.insert(name.into(), BASE64.encode(read_bounded(&dir.join(name))?));
        }
    }
    let bytes = serde_json::to_vec(&Bundle {
        format: "diskless-manager-backup".into(),
        version: 1,
        files,
    })?;
    anyhow::ensure!(
        bytes.len() <= MAX_BACKUP_BYTES,
        "Backup exceeds 64 MiB limit"
    );
    Ok(bytes)
}

async fn stage_restore(dir: &Path, bytes: &[u8], configured: Option<&str>) -> anyhow::Result<()> {
    anyhow::ensure!(!dir.join(PENDING).exists(), "A restore is already pending");
    let _checked = validate_bundle(bytes, configured).await?;
    // Reject an installation that cannot be safely backed up before promising a restart.
    save_safety_snapshot(dir, configured).await?;
    let mut staged = tempfile::NamedTempFile::new_in(dir)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(dir.join(PENDING))?;
    sync_dir(dir)?;
    Ok(())
}

async fn validate_bundle(
    bytes: &[u8],
    configured: Option<&str>,
) -> anyhow::Result<tempfile::TempDir> {
    anyhow::ensure!(
        bytes.len() <= MAX_BACKUP_BYTES,
        "Backup exceeds 64 MiB limit"
    );
    let bundle: Bundle = serde_json::from_slice(bytes)?;
    anyhow::ensure!(
        bundle.format == "diskless-manager-backup" && bundle.version == 1,
        "Unsupported backup format or version"
    );
    anyhow::ensure!(
        bundle.files.contains_key("diskless.db") && bundle.files.contains_key("jwt-secret"),
        "Database and signing secret are required"
    );
    let checked = tempfile::tempdir()?;
    let mut total = 0usize;
    for (name, encoded) in &bundle.files {
        anyhow::ensure!(FILES.contains(&name.as_str()), "Unexpected backup file");
        let data = BASE64.decode(encoded)?;
        total += data.len();
        anyhow::ensure!(total <= MAX_BACKUP_BYTES, "Backup exceeds 64 MiB limit");
        if name == "jwt-secret" {
            anyhow::ensure!(data.len() >= 32, "Invalid signing secret");
            if let Some(configured) = configured {
                anyhow::ensure!(
                    data == configured.as_bytes(),
                    "Backup signing secret differs from configured JWT_SECRET"
                );
            }
        } else if name == "config.toml" {
            let settings: crate::core::config::Settings =
                toml::from_str(std::str::from_utf8(&data)?)?;
            settings.validate()?;
        } else if name == "config.json" {
            let _: serde_json::Value = serde_json::from_slice(&data)?;
        }
        write_private(&checked.path().join(name), &data)?;
    }
    let pool = open_db(&checked.path().join("diskless.db"), false).await?;
    let result = async {
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(integrity == "ok", "Backup database failed integrity check");
        // Checks migration checksums and rejects databases from a newer release.
        MIGRATOR.run(&pool).await?;
        // Migration history does not prove that required tables/columns still exist.
        let reference = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await?;
        MIGRATOR.run(&reference).await?;
        let schema = "SELECT t.name, c.name FROM sqlite_schema t JOIN pragma_table_info(t.name) c WHERE t.type = 'table'";
        let required: Vec<(String, String)> = sqlx::query_as(schema).fetch_all(&reference).await?;
        reference.close().await;
        let actual: Vec<(String, String)> = sqlx::query_as(schema).fetch_all(&pool).await?;
        anyhow::ensure!(required.iter().all(|column| actual.contains(column)), "Backup database is missing required tables or columns");
        let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(admins > 0, "Backup must contain an administrator");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    pool.close().await;
    result?;
    Ok(checked)
}

pub async fn apply_pending_restore(dir: &Path) -> anyhow::Result<()> {
    if let Err(error) = try_apply_pending_restore(dir).await {
        // Only boot the original installation after recovery has safely finished.
        recover_interrupted_restore(dir)?;
        if dir.join(PENDING).exists() {
            std::fs::rename(
                dir.join(PENDING),
                dir.join(format!("failed-restore-{}.json", uuid::Uuid::new_v4())),
            )?;
            sync_dir(dir)?;
            log::error!(
                "Restore failed; original installation retained and bundle quarantined: {error}"
            );
        } else {
            log::error!("Committed restore cleanup needed recovery: {error}");
        }
    }
    Ok(())
}

async fn save_safety_snapshot(dir: &Path, configured: Option<&str>) -> anyhow::Result<()> {
    if !dir.join("diskless.db").exists() {
        return Ok(());
    }
    let secret = match configured {
        Some(secret) => secret.as_bytes().to_vec(),
        None => read_bounded(&dir.join("jwt-secret"))?,
    };
    let live = open_db(&dir.join("diskless.db"), false).await?;
    let safety = create_bundle(&live, dir, &secret).await;
    live.close().await;
    let safety = safety?;
    validate_bundle(&safety, configured).await?;
    let backups = dir.join("backups");
    std::fs::create_dir_all(&backups)?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &backups,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    write_private(
        &backups.join(format!("before-restore-{}.json", uuid::Uuid::new_v4())),
        &safety,
    )?;
    sync_dir(&backups)?;
    Ok(())
}

async fn try_apply_pending_restore(dir: &Path) -> anyhow::Result<()> {
    recover_interrupted_restore(dir)?;
    if !dir.join(PENDING).exists() {
        return Ok(());
    }
    let configured = std::env::var("JWT_SECRET").ok();
    let checked =
        validate_bundle(&read_bounded(&dir.join(PENDING))?, configured.as_deref()).await?;
    let restored = open_db(&checked.path().join("diskless.db"), false).await?;
    // A new random generation prevents tokens from either installation surviving restore.
    let version = (u64::from_le_bytes(uuid::Uuid::new_v4().as_bytes()[..8].try_into()?)
        & ((1u64 << 62) - 1)) as i64;
    let result = sqlx::query("UPDATE users SET session_version = ?")
        .bind(version)
        .execute(&restored)
        .await;
    restored.close().await;
    result?;
    save_safety_snapshot(dir, configured.as_deref()).await?;
    let rollback = dir.join(ROLLBACK);
    std::fs::create_dir(&rollback)?;
    #[cfg(unix)]
    std::fs::set_permissions(
        &rollback,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )?;
    // The manifest becomes durable only after all original copies are durable.
    let originals: Vec<&str> = LIVE_FILES
        .iter()
        .copied()
        .filter(|name| dir.join(name).exists())
        .collect();
    for name in &originals {
        write_private(&rollback.join(name), &read_bounded(&dir.join(name))?)?;
    }
    write_private(
        &rollback.join("original-files.json"),
        &serde_json::to_vec(&originals)?,
    )?;
    sync_dir(&rollback)?;
    sync_dir(dir)?;
    let result = (|| -> anyhow::Result<()> {
        for name in LIVE_FILES {
            if dir.join(name).exists() {
                std::fs::remove_file(dir.join(name))?;
            }
        }
        for name in FILES {
            let source = checked.path().join(name);
            if source.exists() {
                write_private(&dir.join(name), &read_bounded(&source)?)?;
            }
        }
        sync_dir(dir)?;
        // Removing pending is the commit point; recovery rolls back only before it.
        std::fs::remove_file(dir.join(PENDING))?;
        sync_dir(dir)?;
        Ok(())
    })();
    if let Err(error) = result {
        recover_interrupted_restore(dir)?;
        return Err(error);
    }
    std::fs::remove_dir_all(&rollback)?;
    sync_dir(dir)?;
    Ok(())
}

fn recover_interrupted_restore(dir: &Path) -> anyhow::Result<()> {
    let rollback = dir.join(ROLLBACK);
    if !rollback.exists() {
        return Ok(());
    }
    let manifest = rollback.join("original-files.json");
    if dir.join(PENDING).exists() && manifest.exists() {
        let originals: Vec<String> = serde_json::from_slice(&read_bounded(&manifest)?)?;
        anyhow::ensure!(
            originals
                .iter()
                .all(|name| LIVE_FILES.contains(&name.as_str())),
            "Invalid restore recovery manifest"
        );
        for name in LIVE_FILES {
            if originals.iter().any(|original| original == name) {
                // Keep rollback copies until recovery finishes, so recovery is restartable.
                let bytes = read_bounded(&rollback.join(name))?;
                let mut staged = tempfile::NamedTempFile::new_in(dir)?;
                staged.write_all(&bytes)?;
                staged.as_file().sync_all()?;
                staged.persist(dir.join(name))?;
            } else if dir.join(name).exists() {
                std::fs::remove_file(dir.join(name))?;
            }
        }
        sync_dir(dir)?;
        // Mark recovery complete before recursive cleanup, which itself may be interrupted.
        std::fs::remove_file(&manifest)?;
        sync_dir(&rollback)?;
    }
    std::fs::remove_dir_all(rollback)?;
    sync_dir(dir)?;
    Ok(())
}

fn write_private(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("File parent unavailable"))?;
    // Tempfiles are private and publication is atomic without overwriting existing data.
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist_noclobber(path)?;
    sync_dir(parent)?;
    Ok(())
}

fn sync_dir(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    std::fs::File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    async fn fixture(dir: &Path) -> sqlx::SqlitePool {
        let pool = open_db(&dir.join("diskless.db"), false).await.unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users(id,username,password_hash,role,created_at,updated_at) VALUES ('admin','operator','hash','admin','now','now')").execute(&pool).await.unwrap();
        std::fs::write(
            dir.join("jwt-secret"),
            b"test-signing-secret-at-least-32-bytes",
        )
        .unwrap();
        pool
    }
    #[tokio::test]
    async fn restore_is_staged_then_applied_with_safety_copy_and_revoked_sessions() {
        let source = tempfile::tempdir().unwrap();
        let pool = fixture(source.path()).await;
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET username='from-backup'")
            .execute(&pool)
            .await
            .unwrap();
        let bytes = create_bundle(
            &pool,
            source.path(),
            b"test-signing-secret-at-least-32-bytes",
        )
        .await
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let live = fixture(destination.path()).await;
        stage_restore(destination.path(), &bytes, None)
            .await
            .unwrap();
        let name: String = sqlx::query_scalar("SELECT username FROM users")
            .fetch_one(&live)
            .await
            .unwrap();
        assert_eq!(name, "operator");
        assert!(stage_restore(destination.path(), &bytes, None)
            .await
            .is_err());
        live.close().await;
        apply_pending_restore(destination.path()).await.unwrap();
        let restored = open_db(&destination.path().join("diskless.db"), true)
            .await
            .unwrap();
        let (name, version): (String, i64) =
            sqlx::query_as("SELECT username,session_version FROM users")
                .fetch_one(&restored)
                .await
                .unwrap();
        assert_eq!(name, "from-backup");
        assert!(version > 1_000_000);
        assert!(!destination.path().join(PENDING).exists());
        let safety = std::fs::read_dir(destination.path().join("backups"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let bundle: Bundle = serde_json::from_slice(&std::fs::read(safety).unwrap()).unwrap();
        let checked = validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .unwrap();
        let old = open_db(&checked.path().join("diskless.db"), true)
            .await
            .unwrap();
        let old_name: String = sqlx::query_scalar("SELECT username FROM users")
            .fetch_one(&old)
            .await
            .unwrap();
        assert_eq!(old_name, "operator");
        old.close().await;
        restored.close().await;
        pool.close().await;
    }
    #[tokio::test]
    async fn rejects_unsafe_files_bad_database_no_admin_and_secret_override() {
        let dir = tempfile::tempdir().unwrap();
        let pool = fixture(dir.path()).await;
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        let mut bundle: Bundle = serde_json::from_slice(&bytes).unwrap();
        bundle.files.insert("../outside".into(), String::new());
        assert!(validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .is_err());
        assert!(
            validate_bundle(&bytes, Some("a-different-configured-secret-over-32-bytes"))
                .await
                .is_err()
        );
        bundle.files.remove("../outside");
        bundle
            .files
            .insert("diskless.db".into(), BASE64.encode(b"not sqlite"));
        assert!(validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .is_err());
        sqlx::query("DELETE FROM users")
            .execute(&pool)
            .await
            .unwrap();
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        assert!(validate_bundle(&bytes, None).await.is_err());
        pool.close().await;
    }
    #[tokio::test]
    async fn interrupted_apply_recovers_original_files_and_keeps_pending() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diskless.db"), b"original").unwrap();
        std::fs::write(dir.path().join(PENDING), b"pending").unwrap();
        let rollback = dir.path().join(ROLLBACK);
        std::fs::create_dir(&rollback).unwrap();
        write_private(
            &rollback.join("original-files.json"),
            &serde_json::to_vec(&vec!["diskless.db"]).unwrap(),
        )
        .unwrap();
        std::fs::rename(dir.path().join("diskless.db"), rollback.join("diskless.db")).unwrap();
        std::fs::write(dir.path().join("diskless.db"), b"replacement").unwrap();
        recover_interrupted_restore(dir.path()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join("diskless.db")).unwrap(),
            b"original"
        );
        assert!(dir.path().join(PENDING).exists());
    }
    #[tokio::test]
    async fn restore_removes_old_setup_completion_and_absent_configuration() {
        let source = tempfile::tempdir().unwrap();
        let pool = fixture(source.path()).await;
        let bytes = create_bundle(
            &pool,
            source.path(),
            b"test-signing-secret-at-least-32-bytes",
        )
        .await
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let old = fixture(destination.path()).await;
        old.close().await;
        std::fs::write(destination.path().join("setup-completed"), b"done").unwrap();
        std::fs::write(destination.path().join("config.json"), b"{}").unwrap();
        stage_restore(destination.path(), &bytes, None)
            .await
            .unwrap();
        apply_pending_restore(destination.path()).await.unwrap();
        assert!(!destination.path().join("setup-completed").exists());
        assert!(!destination.path().join("config.json").exists());
        pool.close().await;
    }

    #[tokio::test]
    async fn invalid_pending_restore_keeps_original_database_and_secret() {
        let dir = tempfile::tempdir().unwrap();
        let live = fixture(dir.path()).await;
        live.close().await;
        let db = std::fs::read(dir.path().join("diskless.db")).unwrap();
        let secret = std::fs::read(dir.path().join("jwt-secret")).unwrap();
        std::fs::write(dir.path().join(PENDING), b"invalid bundle").unwrap();
        apply_pending_restore(dir.path()).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("diskless.db")).unwrap(), db);
        assert_eq!(
            std::fs::read(dir.path().join("jwt-secret")).unwrap(),
            secret
        );
        assert!(!dir.path().join(PENDING).exists());
        assert!(std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("failed-restore-")));
        assert!(!dir.path().join(ROLLBACK).exists());
    }

    #[tokio::test]
    async fn rejects_size_format_missing_files_migration_mismatch_and_unsafe_settings() {
        assert!(validate_bundle(&vec![b' '; MAX_BACKUP_BYTES + 1], None)
            .await
            .is_err());
        let dir = tempfile::tempdir().unwrap();
        let pool = fixture(dir.path()).await;
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        let mut bundle: Bundle = serde_json::from_slice(&bytes).unwrap();
        bundle.version = 2;
        assert!(validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .is_err());
        bundle.version = 1;
        let secret = bundle.files.remove("jwt-secret").unwrap();
        assert!(validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .is_err());
        bundle.files.insert("jwt-secret".into(), secret);
        bundle.files.insert(
            "config.toml".into(),
            BASE64.encode(b"[http]\nroot_dir='/etc'"),
        );
        assert!(validate_bundle(&serde_json::to_vec(&bundle).unwrap(), None)
            .await
            .is_err());
        sqlx::query("UPDATE _sqlx_migrations SET checksum = x'00'")
            .execute(&pool)
            .await
            .unwrap();
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        assert!(validate_bundle(&bytes, None).await.is_err());
        pool.close().await;
    }

    #[tokio::test]
    async fn concurrent_staging_has_one_winner_and_private_files() {
        let dir = tempfile::tempdir().unwrap();
        let pool = fixture(dir.path()).await;
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        let (first, second) = tokio::join!(
            stage_restore(dir.path(), &bytes, None),
            stage_restore(dir.path(), &bytes, None)
        );
        assert_ne!(first.is_ok(), second.is_ok());
        assert_eq!(std::fs::read(dir.path().join(PENDING)).unwrap(), bytes);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join(PENDING))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        pool.close().await;
    }

    #[tokio::test]
    async fn failed_safety_backup_quarantines_pending_and_keeps_live_files() {
        let source = tempfile::tempdir().unwrap();
        let pool = fixture(source.path()).await;
        let bytes = create_bundle(
            &pool,
            source.path(),
            b"test-signing-secret-at-least-32-bytes",
        )
        .await
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let live = fixture(destination.path()).await;
        live.close().await;
        let original = std::fs::read(destination.path().join("diskless.db")).unwrap();
        stage_restore(destination.path(), &bytes, None)
            .await
            .unwrap();
        std::fs::remove_dir_all(destination.path().join("backups")).ok();
        std::fs::write(
            destination.path().join("backups"),
            b"cannot create directory",
        )
        .unwrap();
        apply_pending_restore(destination.path()).await.unwrap();
        assert_eq!(
            std::fs::read(destination.path().join("diskless.db")).unwrap(),
            original
        );
        assert!(!destination.path().join(PENDING).exists());
        assert!(std::fs::read_dir(destination.path())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("failed-restore-")));
        assert!(!destination.path().join(ROLLBACK).exists());
        pool.close().await;
    }
    #[tokio::test]
    async fn staging_rejects_unbackuppable_live_state_before_publishing_pending() {
        let source = tempfile::tempdir().unwrap();
        let pool = fixture(source.path()).await;
        let bytes = create_bundle(
            &pool,
            source.path(),
            b"test-signing-secret-at-least-32-bytes",
        )
        .await
        .unwrap();
        let destination = tempfile::tempdir().unwrap();
        let old = fixture(destination.path()).await;
        std::fs::write(
            destination.path().join("config.json"),
            vec![b' '; MAX_BACKUP_BYTES],
        )
        .unwrap();
        assert!(stage_restore(destination.path(), &bytes, None)
            .await
            .is_err());
        assert!(!destination.path().join(PENDING).exists());
        old.close().await;
        pool.close().await;
    }
    #[tokio::test]
    async fn handlers_deny_non_administrators_before_reading_or_staging() {
        use std::sync::Arc;
        use tokio::sync::{Mutex, RwLock};
        let dir = tempfile::tempdir().unwrap();
        let pool = fixture(dir.path()).await;
        let state = AppState {
            client_mutations: Arc::new(Mutex::new(())),
            settings: Arc::new(RwLock::new(crate::core::config::Settings::default())),
            db_pool: pool.clone(),
            config_path: dir.path().join("config.json"),
            client_ips: Arc::new(RwLock::new(Vec::new())),
            metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
            ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
            application: Arc::new(crate::application::ApplicationServices::new(pool.clone())),
        };
        let claims = Claims {
            sub: "user".into(),
            username: "viewer".into(),
            role: "user".into(),
            exp: 0,
            iat: 0,
            session_version: 0,
        };
        assert_eq!(
            create_backup(State(state.clone()), Extension(claims.clone()))
                .await
                .unwrap_err(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            restore_backup(
                State(state),
                Extension(claims),
                Bytes::from_static(b"invalid bundle")
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        assert!(!dir.path().join(PENDING).exists());
        assert!(!dir.path().join("backups").exists());
        pool.close().await;
    }

    #[tokio::test]
    async fn missing_rollback_original_refuses_to_boot_or_discard_pending() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diskless.db"), b"replacement").unwrap();
        std::fs::write(dir.path().join(PENDING), b"pending").unwrap();
        let rollback = dir.path().join(ROLLBACK);
        std::fs::create_dir(&rollback).unwrap();
        write_private(&rollback.join("original-files.json"), br#"["diskless.db"]"#).unwrap();
        assert!(apply_pending_restore(dir.path()).await.is_err());
        assert!(dir.path().join(PENDING).exists());
        assert!(rollback.exists());
        assert_eq!(
            std::fs::read(dir.path().join("diskless.db")).unwrap(),
            b"replacement"
        );
    }

    #[test]
    fn committed_restore_cleanup_keeps_replacement_and_is_restartable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("diskless.db"), b"replacement").unwrap();
        let rollback = dir.path().join(ROLLBACK);
        std::fs::create_dir(&rollback).unwrap();
        write_private(&rollback.join("original-files.json"), br#"["diskless.db"]"#).unwrap();
        // Original copies may already have been deleted when committed cleanup was interrupted.
        recover_interrupted_restore(dir.path()).unwrap();
        recover_interrupted_restore(dir.path()).unwrap();
        assert!(!rollback.exists());
        assert_eq!(
            std::fs::read(dir.path().join("diskless.db")).unwrap(),
            b"replacement"
        );
    }
    #[tokio::test]
    async fn rejects_missing_runtime_schema_even_with_valid_migration_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let pool = fixture(dir.path()).await;
        sqlx::query("DROP TABLE clients")
            .execute(&pool)
            .await
            .unwrap();
        let bytes = create_bundle(&pool, dir.path(), b"test-signing-secret-at-least-32-bytes")
            .await
            .unwrap();
        assert!(validate_bundle(&bytes, None).await.is_err());
        pool.close().await;
    }
}
