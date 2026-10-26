//! End-to-end installation-session flow: prepare, scoped boot payload, progress report.
//!
//! The flow runs through the real router and application services with injected
//! image/iSCSI backends, so no live ZFS pool, LIO target or client disk is touched.
use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
    Router,
};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use tower::ServiceExt;

use crate::{
    application::{installation_service::InstallationService, ApplicationServices},
    core::image::{Image, ImageFormat, ImageKind, OsType},
    domain::{
        client::{ClientStatus, CreateClient, PxeMode},
        Client,
    },
    infrastructure::{
        image::{ImageBackend, ImageBackendInfo},
        iscsi::{
            ChapCredentials, IscsiLunBacking, IscsiLunState, IscsiProvisioner, IscsiTargetSpec,
            IscsiTargetState,
        },
    },
    persistence::repositories::{
        image::ImageRepository, installer_media::InstallerMediaRepository, ClientRepository,
        InstallerMedia, InstallerMediaType, StructuralValidationState,
    },
    state::AppState,
};

const CLIENT_IP: &str = "192.0.2.10";
const TARGET_IQN: &str = "iqn.2026-10.test:client-a";
const DISK_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Default)]
struct FakeImageBackend {
    datasets: Mutex<HashSet<String>>,
    snapshots: Mutex<HashSet<String>>,
    origins: Mutex<HashMap<String, String>>,
    mounted: AtomicBool,
    /// Mutable so a test can simulate a resize under a prepared installation.
    virtual_size: AtomicU64,
}

impl ImageBackend for FakeImageBackend {
    fn validate_unmounted(&self, _: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.mounted.load(Ordering::SeqCst),
            "injected mounted client volume"
        );
        Ok(())
    }
    fn settle_device_changes(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn clone_origin(&self, name: &str) -> anyhow::Result<Option<String>> {
        Ok(self.origins.lock().unwrap().get(name).cloned())
    }
    fn exists(&self, name: &str) -> anyhow::Result<bool> {
        Ok(self.datasets.lock().unwrap().contains(name)
            || self.snapshots.lock().unwrap().contains(name))
    }
    fn create_volume(&self, name: &str, _: u64) -> anyhow::Result<()> {
        self.datasets.lock().unwrap().insert(name.to_owned());
        Ok(())
    }
    fn destroy(&self, name: &str) -> anyhow::Result<()> {
        self.datasets.lock().unwrap().remove(name);
        self.origins.lock().unwrap().remove(name);
        Ok(())
    }
    fn rename(&self, old: &str, new: &str) -> anyhow::Result<()> {
        if self.datasets.lock().unwrap().remove(old) {
            self.datasets.lock().unwrap().insert(new.into());
        }
        Ok(())
    }
    fn clone_image(&self, source: &str, destination: &str) -> anyhow::Result<()> {
        self.datasets.lock().unwrap().insert(destination.into());
        self.origins
            .lock()
            .unwrap()
            .insert(destination.into(), source.into());
        Ok(())
    }
    fn create_snapshot(&self, dataset: &str, snapshot: &str) -> anyhow::Result<()> {
        self.snapshots
            .lock()
            .unwrap()
            .insert(format!("{dataset}@{snapshot}"));
        Ok(())
    }
    fn destroy_snapshot(&self, dataset: &str, snapshot: &str) -> anyhow::Result<()> {
        self.snapshots
            .lock()
            .unwrap()
            .remove(&format!("{dataset}@{snapshot}"));
        Ok(())
    }
    fn rollback_snapshot(&self, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn resize(&self, _: &str, _: u64) -> anyhow::Result<()> {
        Ok(())
    }
    fn import_raw(&self, _: &Path, _: &str, _: u64) -> anyhow::Result<()> {
        Ok(())
    }
    fn verify(&self, name: &str) -> anyhow::Result<bool> {
        self.exists(name)
    }
    fn info(&self, _: &str) -> anyhow::Result<ImageBackendInfo> {
        let virtual_size = self.virtual_size.load(Ordering::SeqCst);
        Ok(ImageBackendInfo {
            virtual_size,
            actual_size: virtual_size,
            format: ImageFormat::Raw,
            backing_file: None,
            snapshots: Vec::new(),
        })
    }
    fn set_os_type(&self, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
    fn image_parent(&self) -> anyhow::Result<String> {
        Ok("diskless/images".into())
    }
}

#[derive(Default)]
struct FakeIscsi {
    target: Mutex<Option<IscsiTargetSpec>>,
    active_session: AtomicBool,
}

impl IscsiProvisioner for FakeIscsi {
    fn active_session_peer_ips(&self, _: &str, _: &str) -> anyhow::Result<Vec<std::net::IpAddr>> {
        Ok(if self.active_session.load(Ordering::SeqCst) {
            vec![CLIENT_IP.parse().unwrap()]
        } else {
            vec![]
        })
    }
    fn create_target(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
        *self.target.lock().unwrap() = Some(spec.clone());
        Ok(())
    }
    fn remove_target(&self, _: &str) -> anyhow::Result<()> {
        *self.target.lock().unwrap() = None;
        Ok(())
    }
    fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> anyhow::Result<()> {
        *self.target.lock().unwrap() = None;
        Ok(())
    }
    fn target_exists(&self, _: &str) -> anyhow::Result<bool> {
        Ok(self.target.lock().unwrap().is_some())
    }
    fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> anyhow::Result<()> {
        Ok(())
    }
    fn list_target_luns(&self, _: &str) -> anyhow::Result<Vec<IscsiLunState>> {
        Ok(self
            .target
            .lock()
            .unwrap()
            .as_ref()
            .map(|spec| {
                spec.luns
                    .iter()
                    .map(|lun| IscsiLunState {
                        lun: lun.lun,
                        backstore: lun.backstore.clone(),
                        exists: true,
                        backstore_exists: true,
                        block_device_matches: true,
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
    fn inspect_target(&self, spec: &IscsiTargetSpec) -> anyhow::Result<IscsiTargetState> {
        let ready = self.target.lock().unwrap().as_ref().is_some_and(|current| {
            spec.luns.iter().all(|wanted| {
                current
                    .luns
                    .iter()
                    .any(|actual| actual.lun == wanted.lun && actual.backstore == wanted.backstore)
            })
        });
        Ok(IscsiTargetState::from_luns(
            spec.target_iqn.clone(),
            ready,
            spec.luns
                .iter()
                .map(|lun| IscsiLunState {
                    lun: lun.lun,
                    backstore: lun.backstore.clone(),
                    exists: ready,
                    backstore_exists: ready,
                    block_device_matches: ready,
                })
                .collect(),
            ready,
        ))
    }
    fn verify_named_file_lun_mappings(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
        if spec
            .luns
            .iter()
            .any(|lun| lun.backing == IscsiLunBacking::File)
        {
            anyhow::ensure!(
                spec.initiator_iqn.is_some(),
                "named file LUN mapping is missing"
            );
        }
        Ok(())
    }
    fn reconcile(&self, spec: &IscsiTargetSpec) -> anyhow::Result<()> {
        self.create_target(spec)
    }
}

struct Fixture {
    state: AppState,
    app: Router,
    /// Preboot listener; boot payloads and reports never travel over the management API.
    boot_app: Router,
    admin: String,
    client_id: String,
    identity_client_id: String,
    backend: Arc<FakeImageBackend>,
    /// Keeps the managed media directory alive for the test's lifetime.
    _root: tempfile::TempDir,
}

impl Fixture {
    async fn management(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let text = self
            .send(
                &self.app,
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("authorization", format!("Bearer {}", self.admin)),
                body,
            )
            .await;
        (text.0, serde_json::from_str(&text.1).unwrap_or(Value::Null))
    }

    /// Boot-scoped endpoints are bound to the requesting client's address, not a JWT.
    async fn scoped(&self, uri: &str, peer: &str, body: Option<Value>) -> (StatusCode, String) {
        self.send(
            &self.boot_app,
            Request::builder()
                .method("POST")
                .uri(uri)
                .extension(ConnectInfo(
                    format!("{peer}:40000")
                        .parse::<SocketAddr>()
                        .expect("test peer address"),
                )),
            body,
        )
        .await
    }

    async fn scoped_get(&self, uri: &str, peer: &str) -> (StatusCode, String) {
        self.send(
            &self.boot_app,
            Request::builder()
                .method("GET")
                .uri(uri)
                .extension(ConnectInfo(
                    format!("{peer}:40000")
                        .parse::<SocketAddr>()
                        .expect("test peer address"),
                )),
            None,
        )
        .await
    }

    async fn send(
        &self,
        app: &Router,
        builder: axum::http::request::Builder,
        body: Option<Value>,
    ) -> (StatusCode, String) {
        let request = match body {
            Some(value) => builder
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&value).expect("json body")))
                .expect("request builds"),
            None => builder.body(Body::empty()).expect("request builds"),
        };
        let response = app
            .clone()
            .oneshot(request)
            .await
            .expect("in-process request");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body");
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn session(&self, id: &str) -> (String, i64, String) {
        sqlx::query_as("SELECT state, progress, token FROM installation_sessions WHERE id = ?")
            .bind(id)
            .fetch_one(&self.state.db_pool)
            .await
            .expect("installation session")
    }

    /// Submit the WinPE hardware inventory that produces a hardware profile.
    async fn inventory(&self, mac: &str, peer: &str) -> StatusCode {
        let body = json!({
            "boot_nic_ids": ["PCI\\VEN_8086&DEV_15F3"],
            "secondary_ids": ["PCI\\VEN_10EC&DEV_8168"],
            "architecture": "AMD64",
            "firmware": "uefi",
            "subvendor_id": "8086",
            "subdevice_id": "15F3",
        });
        self.send(
            &self.boot_app,
            Request::builder()
                .method("POST")
                .uri(format!("/inventory/{mac}"))
                .extension(ConnectInfo(
                    format!("{peer}:68")
                        .parse::<SocketAddr>()
                        .expect("test peer address"),
                )),
            Some(body),
        )
        .await
        .0
    }

    async fn client_disk(&self, id: &str) -> (Option<String>, Option<String>) {
        sqlx::query_as("SELECT writeback, block_device FROM clients WHERE id = ?")
            .bind(id)
            .fetch_one(&self.state.db_pool)
            .await
            .expect("client row")
    }

    /// Insert a session directly to exercise recipe payloads whose media inspection
    /// needs real ISO tooling and full installation media.
    async fn insert_session(&self, id: &str, media_id: &str, recipe: &str, token: &str) {
        // Bind the catalog checksum so the session survives media revision checks.
        let checksum: String =
            sqlx::query_scalar("SELECT checksum FROM installer_media WHERE id = ?")
                .bind(media_id)
                .fetch_one(&self.state.db_pool)
                .await
                .expect("catalog checksum");
        sqlx::query("INSERT INTO installation_sessions (id, client_id, media_id, image_id, master, media_checksum, target_iqn, expected_bytes, source_ip, firmware, recipe, token, config_json, state, progress, created_at, updated_at, expires_at) VALUES (?, ?, ?, 'image-a', 'diskless/master', ?, ?, ?, ?, 'uefi', ?, ?, ?, 'booting', 0, ?, ?, ?)")
            .bind(id)
            .bind(&self.client_id)
            .bind(media_id)
            .bind(&checksum)
            .bind(TARGET_IQN)
            .bind(DISK_BYTES as i64)
            .bind(CLIENT_IP)
            .bind(recipe)
            .bind(token)
            .bind(json!({"username":"operator","password_hash":format!("$6$salt${}", "a".repeat(86))}).to_string())
            .bind(Utc::now().to_rfc3339())
            .bind(Utc::now().to_rfc3339())
            .bind((Utc::now() + chrono::Duration::hours(1)).to_rfc3339())
            .execute(&self.state.db_pool)
            .await
            .expect("inserted session");
    }
}

async fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("media root");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("database");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("schema");

    let password_hash = bcrypt::hash("StrongPass1", 4).expect("hash");
    sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES ('admin', 'admin', ?, 'admin', '', '')")
        .bind(&password_hash)
        .execute(&pool)
        .await
        .expect("admin user");
    let admin = crate::auth::authenticate_user(&pool, "admin", "StrongPass1")
        .await
        .expect("login")
        .token;

    let backend = Arc::new(FakeImageBackend {
        virtual_size: AtomicU64::new(DISK_BYTES),
        ..FakeImageBackend::default()
    });
    backend
        .datasets
        .lock()
        .unwrap()
        .insert("diskless/master".into());
    // Snapshot-backed client disk used by the hardware-identity lifecycle.
    backend
        .datasets
        .lock()
        .unwrap()
        .insert("diskless/clients/client-b".into());
    backend.origins.lock().unwrap().insert(
        "diskless/clients/client-b".into(),
        "diskless/windows-b@rev-1".into(),
    );
    backend
        .snapshots
        .lock()
        .unwrap()
        .insert("diskless/windows-b@rev-1".into());
    let iscsi = Arc::new(FakeIscsi::default());
    let application = Arc::new(ApplicationServices::with_backends(
        pool.clone(),
        crate::config::RuntimeConfig::default(),
        backend.clone(),
        iscsi.clone(),
        crate::core::config::IscsiBackend::Targetcli,
    ));

    // Installer media must be a real hashed file at the managed path.
    let media_bytes = b"synthetic installer media";
    let media_dir = root.path().join("installer-media");
    std::fs::create_dir_all(&media_dir).expect("media directory");
    let media_path = media_dir.join("media-a.iso");
    std::fs::write(&media_path, media_bytes).expect("media file");
    let checksum = format!("{:x}", Sha256::digest(media_bytes));
    let now = Utc::now();
    let media_repository = InstallerMediaRepository::new(pool.clone());
    media_repository
        .insert(&InstallerMedia {
            id: "media-a".into(),
            display_name: "Synthetic ISO".into(),
            media_type: InstallerMediaType::WindowsIso,
            managed_path: media_path.clone(),
            byte_size: media_bytes.len() as u64,
            checksum: checksum.clone(),
            validation_state: StructuralValidationState::Valid,
            is_default: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("catalog media");
    // A Linux ISO used by the scoped-payload tests for unattended recipes.
    let linux_path = media_dir.join("media-linux.iso");
    std::fs::write(&linux_path, media_bytes).expect("linux media file");
    media_repository
        .insert(&InstallerMedia {
            id: "media-linux".into(),
            display_name: "Synthetic Linux ISO".into(),
            media_type: InstallerMediaType::LinuxIso,
            managed_path: linux_path,
            byte_size: media_bytes.len() as u64,
            checksum: checksum.clone(),
            validation_state: StructuralValidationState::Valid,
            is_default: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .expect("linux catalog media");
    // Interactive boot requires a checksum-matched verification for this firmware.
    sqlx::query("INSERT INTO installer_media_boot_verifications (id, installer_media_id, checksum, firmware_mode, test_client_id, assigned_image_revision, observed_luns, boot_result, verified_at) VALUES ('verify-1', 'media-a', ?, 'uefi', 'client-a', 'diskless/master', '[]', 'passed', ?)")
        .bind(&checksum)
        .bind(now.to_rfc3339())
        .execute(&pool)
        .await
        .expect("boot verification");

    let images = ImageRepository::new(pool.clone());
    for (id, name) in [
        ("image-a", "diskless/master"),
        ("image-b", "diskless/windows-b"),
    ] {
        images
            .insert(&Image {
                id: id.into(),
                name: name.into(),
                kind: ImageKind::Master,
                os_type: OsType::Windows,
                size_gb: 20,
                path: std::path::PathBuf::from(format!("/dev/zvol/{name}")),
                format: ImageFormat::Raw,
                status: "ready".into(),
                description: None,
                parent_id: None,
                source_snapshot: None,
                checksum: None,
                is_default: false,
                created_at: now,
                updated_at: now,
            })
            .await
            .expect("image");
    }

    let clients = ClientRepository::new(pool.clone());
    let mut client = Client::create(CreateClient {
        installer_media_id: Some("media-a".into()),
        enabled: Some(true),
        name: "client-a".into(),
        mac: "02:00:00:00:00:01".into(),
        ip: CLIENT_IP.into(),
        master: "diskless/master".into(),
        snapshot: None,
        block_store: Some("block_client_a".into()),
        block_device: Some("/dev/zvol/diskless/master".into()),
        target_iqn: Some(TARGET_IQN.into()),
        pxe_mode: PxeMode::Uefi,
        keep_writeback: false,
        use_game_disk: false,
        game_disks: vec![],
        chap_enabled: false,
    })
    .expect("client");
    client.status = ClientStatus::Offline;
    clients.insert(&client).await.expect("insert client");
    let client_id = client.id.to_string();

    // A Windows client whose disk is a snapshot-backed clone, required by the
    // identity cache (capture needs an immutable source revision and owned disk).
    let mut identity_client = Client::create(CreateClient {
        installer_media_id: None,
        enabled: Some(true),
        name: "client-b".into(),
        mac: "02:00:00:00:00:02".into(),
        ip: "192.0.2.11".into(),
        master: "diskless/windows-b".into(),
        snapshot: Some("diskless/windows-b@rev-1".into()),
        block_store: Some("block_client_b".into()),
        block_device: Some("/dev/zvol/diskless/clients/client-b".into()),
        target_iqn: Some("iqn.2026-10.test:client-b".into()),
        pxe_mode: PxeMode::Uefi,
        keep_writeback: true,
        use_game_disk: false,
        game_disks: vec![],
        chap_enabled: false,
    })
    .expect("identity client");
    identity_client.status = ClientStatus::Offline;
    identity_client.writeback = Some("diskless/clients/client-b".into());
    clients
        .insert(&identity_client)
        .await
        .expect("insert identity client");
    let identity_client_id = identity_client.id.to_string();

    let mut settings = crate::core::config::Settings::default();
    settings.http.root_dir = root.path().to_string_lossy().into_owned();
    settings.http.server_ip = "192.0.2.1".into();
    settings.http.port = 8080;
    settings.dhcp.next_server_ip = "192.0.2.1".into();

    let state = AppState {
        config: crate::config::RuntimeConfig::default(),
        client_mutations: crate::state::ClientMutationLocks::default(),
        settings: Arc::new(tokio::sync::RwLock::new(settings)),
        db_pool: pool,
        config_path: std::path::PathBuf::new(),
        client_ips: Arc::new(tokio::sync::RwLock::new(Vec::new())),
        metrics_collector: Arc::new(crate::metrics::MetricsCollector::default()),
        ssh_executor: Arc::new(crate::ssh_executor::SshExecutor::new()),
        application,
    };
    let app = crate::api::routes::create_app(state.clone());
    let boot_app = crate::api::enroll::enroll_router(state.clone());
    Fixture {
        state,
        app,
        boot_app,
        admin,
        client_id,
        identity_client_id,
        backend,
        _root: root,
    }
}

#[tokio::test]
async fn installation_session_prepares_boots_reports_and_revokes_scoped_access() {
    let fixture = fixture().await;
    let client = fixture.client_id.clone();

    // Prepare: a durable, client-bound session using cataloged media and existing storage.
    let (status, session) = fixture
        .management(
            "POST",
            &format!("/api/clients/{client}/installation-sessions"),
            Some(json!({"recipe": "interactive", "confirm_erase": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "prepare failed: {session}");
    assert_eq!(session["state"], "ready");
    assert_eq!(session["progress"], 0);
    assert_eq!(session["expected_bytes"], DISK_BYTES as i64);
    assert_eq!(session["target_iqn"], TARGET_IQN);
    assert!(
        session.get("token").is_none(),
        "management responses must not leak the boot credential: {session}"
    );
    let id = session["id"].as_str().expect("session id").to_owned();
    let (_, _, token) = fixture.session(&id).await;
    assert_eq!(token.len(), 64, "boot credential must be a bounded token");

    // A second session cannot stack on the same disk before recovery is closed.
    let (status, _) = fixture
        .management(
            "POST",
            &format!("/api/clients/{client}/installation-sessions"),
            Some(json!({"recipe": "interactive", "confirm_erase": true})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Boot dispatch: the same call PXE readiness makes once storage is ready.
    let script = InstallationService::new(fixture.state.clone())
        .boot_for_client(&client)
        .await
        .expect("boot dispatch")
        .expect("session owns the boot payload");
    assert!(
        script.contains(&format!(
            "sanhook --drive 0x81 iscsi:192.0.2.1:::0:{TARGET_IQN}"
        )),
        "{script}"
    );
    assert!(script.contains("set keep-san 1"), "{script}");
    let (state, _, _) = fixture.session(&id).await;
    assert_eq!(state, "booting");

    // Scoped payload fetch is authorized by the session token and client address only.
    let (status, body) = fixture
        .scoped_get(&format!("/api/installer/{id}/{token}/boot"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(
        body.contains("sanhook") && body.contains(TARGET_IQN),
        "{body}"
    );
    assert_eq!(
        fixture
            .scoped_get(
                &format!("/api/installer/{id}/{}/boot", "b".repeat(64)),
                CLIENT_IP
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .scoped_get(&format!("/api/installer/{id}/{token}/boot"), "192.0.2.99")
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Progress reporting is monotonic and cannot claim completion from the client.
    let report = format!("/api/installer/{id}/{token}/report");
    assert_eq!(
        fixture
            .scoped(
                &report,
                CLIENT_IP,
                Some(json!({"state": "installing", "progress": 40, "message": "partitioning"}))
            )
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .scoped(
                &report,
                CLIENT_IP,
                Some(json!({"state": "installing", "progress": 10, "message": "reordered"}))
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .scoped(
                &report,
                CLIENT_IP,
                Some(json!({"state": "completed", "progress": 100, "message": "installed"}))
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .scoped(
                &report,
                "192.0.2.99",
                Some(json!({"state": "installing", "progress": 50, "message": "foreign"}))
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (state, progress, _) = fixture.session(&id).await;
    assert_eq!((state.as_str(), progress), ("installing", 40));

    // Management listing exposes progress and events, still without the credential.
    let (status, listed) = fixture
        .management(
            "GET",
            &format!("/api/clients/{client}/installation-sessions"),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed[0]["session"]["state"], "installing");
    assert_eq!(listed[0]["session"]["progress"], 40);
    assert!(listed[0]["session"].get("token").is_none());
    let events = listed[0]["events"].as_array().expect("events");
    assert!(
        events
            .iter()
            .any(|event| event["message"] == "partitioning"),
        "{events:?}"
    );

    // Cancelling revokes the boot credential; recovery still owns the disk.
    assert_eq!(
        fixture
            .management(
                "POST",
                &format!("/api/installation-sessions/{id}/cancel"),
                None
            )
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture
            .scoped_get(&format!("/api/installer/{id}/{token}/boot"), CLIENT_IP)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (state, _, _) = fixture.session(&id).await;
    assert_eq!(state, "cancelled");
}

/// Prepare a session through the management API and return (id, token).
async fn prepared_session(fixture: &Fixture) -> (String, String) {
    let (status, session) = fixture
        .management(
            "POST",
            &format!("/api/clients/{}/installation-sessions", fixture.client_id),
            Some(json!({"recipe": "interactive", "confirm_erase": true})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "prepare failed: {session}");
    let id = session["id"].as_str().expect("session id").to_owned();
    let (_, _, token) = fixture.session(&id).await;
    (id, token)
}

#[tokio::test]
async fn installation_sessions_are_denied_when_the_media_revision_changes() {
    let fixture = fixture().await;
    let (id, token) = prepared_session(&fixture).await;

    // The durable guard refuses to reassign the client while the session owns the disk.
    let reassignment =
        sqlx::query("UPDATE clients SET installer_media_id = 'media-linux' WHERE id = ?")
            .bind(&fixture.client_id)
            .execute(&fixture.state.db_pool)
            .await;
    let error = reassignment
        .expect_err("reassignment must be refused")
        .to_string();
    assert!(error.contains("reserved installation session"), "{error}");
    let media_id: Option<String> =
        sqlx::query_scalar("SELECT installer_media_id FROM clients WHERE id = ?")
            .bind(&fixture.client_id)
            .fetch_one(&fixture.state.db_pool)
            .await
            .expect("client assignment");
    assert_eq!(media_id.as_deref(), Some("media-a"));

    // Re-uploading the ISO gives the catalog entry a new revision.
    let root = fixture.state.settings.read().await.http.root_dir.clone();
    let media_file = std::path::Path::new(&root).join("installer-media/media-a.iso");
    let replacement = b"replaced installer media";
    std::fs::write(&media_file, replacement).expect("replace media");
    sqlx::query("UPDATE installer_media SET checksum = ?, byte_size = ? WHERE id = 'media-a'")
        .bind(format!("{:x}", Sha256::digest(replacement)))
        .bind(replacement.len() as i64)
        .execute(&fixture.state.db_pool)
        .await
        .expect("catalog revision");

    // Boot dispatch must refuse the changed media instead of booting different bytes.
    let error = InstallationService::new(fixture.state.clone())
        .boot_for_client(&fixture.client_id)
        .await
        .expect_err("changed media must deny boot");
    assert!(
        format!("{error:#}").contains("installation media revision changed"),
        "{error:#}"
    );
    assert_eq!(
        fixture.session(&id).await.0,
        "ready",
        "a denied boot must not advance the session"
    );
    assert_eq!(
        fixture
            .scoped_get(&format!("/api/installer/{id}/{token}/boot"), CLIENT_IP)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn installation_boot_is_denied_when_the_assigned_disk_is_resized() {
    let fixture = fixture().await;
    let (id, token) = prepared_session(&fixture).await;

    fixture
        .backend
        .virtual_size
        .store(DISK_BYTES + 1024 * 1024 * 1024, Ordering::SeqCst);

    let error = InstallationService::new(fixture.state.clone())
        .boot_for_client(&fixture.client_id)
        .await
        .expect_err("a resized disk must deny boot");
    assert!(
        format!("{error:#}").contains("installation disk size changed"),
        "{error:#}"
    );
    assert_eq!(fixture.session(&id).await.0, "ready");
    assert_eq!(
        fixture
            .scoped_get(&format!("/api/installer/{id}/{token}/boot"), CLIENT_IP)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn installation_boot_rejects_a_session_bound_to_a_different_assignment() {
    let fixture = fixture().await;
    sqlx::query("UPDATE clients SET installer_media_id = 'media-linux' WHERE id = ?")
        .bind(&fixture.client_id)
        .execute(&fixture.state.db_pool)
        .await
        .expect("assign linux media");
    // An imported or restored session can disagree with the client row without
    // passing through the durable reassignment guard.
    fixture
        .insert_session(
            "drifted-session",
            "media-linux",
            "interactive",
            &"f".repeat(64),
        )
        .await;
    sqlx::query("UPDATE installation_sessions SET target_iqn = 'iqn.2026-10.test:other-client' WHERE id = 'drifted-session'")
        .execute(&fixture.state.db_pool)
        .await
        .expect("drift session target");

    let error = InstallationService::new(fixture.state.clone())
        .boot_for_client(&fixture.client_id)
        .await
        .expect_err("a drifted session must deny boot");
    assert!(
        format!("{error:#}").contains("installation client assignment changed"),
        "{error:#}"
    );
    assert_eq!(fixture.session("drifted-session").await.0, "booting");
    assert_eq!(
        fixture
            .scoped_get(
                &format!("/api/installer/drifted-session/{}/boot", "f".repeat(64)),
                CLIENT_IP
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn identity_capture_apply_and_delete_are_client_bound_and_reversible() {
    let fixture = fixture().await;
    let client = fixture.identity_client_id.clone();
    let base = format!("/api/clients/{client}/identity");

    // Without a hardware observation the action explains the missing step.
    let (status, body) = fixture
        .management("POST", &format!("{base}/capture"), None)
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body.to_string().contains("hardware"),
        "unexpected error: {body}"
    );

    // A direct-master installation client owns no disk and cannot capture identity.
    let (status, body) = fixture
        .management(
            "POST",
            &format!("/api/clients/{}/identity/capture", fixture.client_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // The preboot inventory hook records the profile this client will be keyed by.
    assert_eq!(
        fixture.inventory("02:00:00:00:00:02", "192.0.2.11").await,
        StatusCode::OK
    );
    let (status, body) = fixture.management("GET", &base, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["profile"]["architecture"], "x86_64");
    assert_eq!(body["profile"]["firmware"], "uefi");
    assert!(body["profile"]["boot_nic_ids"]
        .as_str()
        .unwrap()
        .contains("VEN_8086"));
    assert!(body["cache"].is_null());

    // Capture snapshots the client's own disk and binds it to the observed profile.
    let (status, body) = fixture
        .management("POST", &format!("{base}/capture"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cache"]["state"], "ready");
    assert_eq!(body["cache"]["source_revision"], "diskless/windows-b@rev-1");
    assert_eq!(body["cache"]["dataset"], "diskless/clients/client-b");
    let snapshot = format!(
        "diskless/clients/client-b@{}",
        body["cache"]["snapshot"].as_str().expect("snapshot name")
    );
    assert!(fixture.backend.exists(&snapshot).unwrap());
    assert_eq!(
        fixture.client_disk(&client).await.0.as_deref(),
        Some("diskless/clients/client-b"),
        "capture must not move the client's disk"
    );

    // Apply attaches a replacement disk built from the baseline and keeps the original.
    let (status, body) = fixture
        .management("POST", &format!("{base}/apply"), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let replacement = body["cache"]["replacement"]
        .as_str()
        .expect("replacement dataset")
        .to_owned();
    assert_ne!(replacement, "diskless/clients/client-b");
    let (writeback, block_device) = fixture.client_disk(&client).await;
    assert_eq!(writeback.as_deref(), Some(replacement.as_str()));
    assert_eq!(block_device, Some(format!("/dev/zvol/{replacement}")));
    assert!(
        fixture.backend.exists("diskless/clients/client-b").unwrap(),
        "the original client disk is retained for recovery"
    );

    // Another client cannot adopt this baseline, and repeating apply is safe.
    let (status, body) = fixture
        .management(
            "POST",
            &format!("/api/clients/{}/identity/apply", fixture.client_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(
        fixture
            .management("POST", &format!("{base}/apply"), None)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        fixture.client_disk(&client).await.0.as_deref(),
        Some(replacement.as_str())
    );

    // Delete restores the original disk and removes the cache and baseline.
    let (status, body) = fixture.management("DELETE", &base, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["cache"].is_null());
    let (writeback, block_device) = fixture.client_disk(&client).await;
    assert_eq!(
        writeback.as_deref(),
        Some("diskless/clients/client-b"),
        "the original disk is restored"
    );
    assert_eq!(
        block_device,
        Some("/dev/zvol/diskless/clients/client-b".to_string())
    );
    assert!(!fixture.backend.exists(&replacement).unwrap());
    assert!(!fixture.backend.exists(&snapshot).unwrap());
}

#[tokio::test]
async fn unattended_linux_payloads_are_recipe_scoped_and_token_bound() {
    let fixture = fixture().await;
    let client = fixture.client_id.clone();
    sqlx::query("UPDATE clients SET installer_media_id = 'media-linux' WHERE id = ?")
        .bind(&client)
        .execute(&fixture.state.db_pool)
        .await
        .expect("assign linux media");

    let token = "c".repeat(64);
    fixture
        .insert_session("linux-session", "media-linux", "ubuntu", &token)
        .await;
    let base = format!("/api/installer/linux-session/{token}");

    // The boot script hands the client nothing but scoped, token-bearing URLs.
    let (status, boot) = fixture.scoped_get(&format!("{base}/boot"), CLIENT_IP).await;
    assert_eq!(status, StatusCode::OK, "{boot}");
    assert!(
        boot.contains("boot=casper")
            && boot.contains("autoinstall ds=nocloud-net;s=")
            && boot.contains(&format!("{base}/")),
        "{boot}"
    );
    assert!(boot.contains(&format!(
        "sanhook --drive 0x80 iscsi:192.0.2.1:::0:{TARGET_IQN}"
    )));

    // Cloud-init metadata is rendered for this session and client only.
    let (status, user_data) = fixture
        .scoped_get(&format!("{base}/user-data"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK, "{user_data}");
    assert!(user_data.contains("autoinstall:") && user_data.contains("username: operator"));
    assert!(user_data.contains(&format!("password: '$6$salt${}'", "a".repeat(86))));
    assert!(user_data
        .contains("/dev/disk/by-path/ip-192.0.2.1:3260-iscsi-iqn.2026-10.test:client-a-lun-0"));
    assert!(user_data.contains(&format!("{base}/disk-gate.sh")));
    assert!(user_data.contains(&format!("{base}/linux-finalize.sh")));
    assert!(user_data.contains(&format!("{base}/report")));

    let (status, meta_data) = fixture
        .scoped_get(&format!("{base}/meta-data"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(meta_data.contains("instance-id: linux-session"));

    // The disk gate carries the exact expected byte size and report URL.
    let (status, gate) = fixture
        .scoped_get(&format!("{base}/disk-gate.sh"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK, "{gate}");
    assert!(gate.contains(&DISK_BYTES.to_string()), "{gate}");
    assert!(gate.contains(TARGET_IQN) && gate.contains(&format!("{base}/report")));
    assert!(gate.contains("iqn.2026-01.client:client.client-a"));

    let (status, finalize) = fixture
        .scoped_get(&format!("{base}/linux-finalize.sh"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(finalize.contains("iqn.2026-01.client:client.client-a"));
    assert!(finalize.contains(TARGET_IQN));

    // The cataloged Linux ISO streams over the same scoped credential.
    let (status, media) = fixture
        .scoped_get(&format!("{base}/media.iso"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(media, "synthetic installer media");

    // Recipe scoping and credential binding are enforced on the same route.
    assert_eq!(
        fixture
            .scoped_get(&format!("{base}/ks.cfg"), CLIENT_IP)
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        fixture
            .scoped_get(
                &format!("/api/installer/linux-session/{}/boot", "d".repeat(64)),
                CLIENT_IP
            )
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // Progress reporting uses the same session and is clamped to forward movement.
    assert_eq!(
        fixture
            .scoped(
                &format!("{base}/report"),
                CLIENT_IP,
                Some(
                    json!({"state": "installing", "progress": 55, "message": "installing packages"})
                )
            )
            .await
            .0,
        StatusCode::OK
    );
    let (state, progress, _) = fixture.session("linux-session").await;
    assert_eq!((state.as_str(), progress), ("installing", 55));

    // The RHEL recipe reuses the same session plumbing with its own answer file.
    sqlx::query("UPDATE installation_sessions SET state = 'completed' WHERE id = 'linux-session'")
        .execute(&fixture.state.db_pool)
        .await
        .expect("close ubuntu session");
    let rhel_token = "e".repeat(64);
    fixture
        .insert_session("rhel-session", "media-linux", "rhel", &rhel_token)
        .await;
    let rhel = format!("/api/installer/rhel-session/{rhel_token}");
    let (status, kickstart) = fixture
        .scoped_get(&format!("{rhel}/ks.cfg"), CLIENT_IP)
        .await;
    assert_eq!(status, StatusCode::OK, "{kickstart}");
    assert!(kickstart.contains("ignoredisk --only-use=disk/by-path/ip-192.0.2.1:3260-iscsi-iqn.2026-10.test:client-a-lun-0"), "{kickstart}");
    assert!(kickstart.contains("--iscrypted"));
    assert!(kickstart.contains(&format!("{rhel}/report")));
    let (status, rhel_boot) = fixture.scoped_get(&format!("{rhel}/boot"), CLIENT_IP).await;
    assert_eq!(status, StatusCode::OK);
    assert!(rhel_boot.contains("rd.iscsi.firmware=1"));
    assert!(rhel_boot.contains("inst.ks="));
    assert!(rhel_boot.contains(&format!("{rhel}/ks.cfg")));
    assert!(rhel_boot.contains("inst.stage2="));
    assert!(rhel_boot.contains(&format!("{rhel}/initrd")));
}
