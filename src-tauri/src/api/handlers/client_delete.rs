use axum::{
    extract::{Path, State},
    http::StatusCode,
};

use crate::state::AppState;

pub async fn delete_client(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(), StatusCode> {
    let _client_guard = state.client_mutations.lock().await;

    let recovering: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM client_offline_resets WHERE client_id = ? AND operation IS NOT NULL",
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if recovering != 0 {
        return Err(StatusCode::CONFLICT);
    }

    tracing::info!(client_id = %id, "deleting client");

    let client = state
        .application
        .clients
        .get_by_string(&id)
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let settings = state.settings.read().await.clone();

    // Enrollment uses "pending" until an administrator assigns an image.
    // With no storage references, inventory deletion needs neither ZFS nor DHCP.
    let inventory_only = matches!(client.master.trim(), "" | "pending")
        && client.snapshot.is_none()
        && client.target_iqn.is_none()
        && client.writeback.is_none()
        && client.block_device.is_none()
        && client.block_store.is_none()
        && !client.use_game_disk
        && client.game_disks.is_empty();

    if !inventory_only {
        let storage = crate::application::client_storage_mapping::storage_from_client(&settings, &client)
            .map_err(|error| {
                tracing::error!(client_id = %id, %error, "could not reconstruct client storage during deletion");
                StatusCode::CONFLICT
            })?;
        state.application.storage.destroy_client_storage(&storage).map_err(|error| {
            tracing::error!(client_id = %id, %error, "client storage cleanup failed; retaining client for retry");
            StatusCode::CONFLICT
        })?;
        state.application.storage
            .destroy_client_game_clones(client.id.as_str(), client.target_iqn.as_deref())
            .map_err(|error| {
                tracing::error!(client_id = %id, %error, "game clone cleanup failed; retaining client for retry");
                StatusCode::CONFLICT
            })?;

        if let Err(error) =
            crate::infrastructure::dhcp::remove_client_ipxe_menu(client.mac.as_str()).await
        {
            tracing::warn!(client_id = %id, %error, "failed to remove boot menu for deleted client");
        }
    }

    state
        .application
        .clients
        .delete(&client.id)
        .await
        .map_err(|error| {
            tracing::error!(client_id = %id, %error, "database deletion failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    if let Err(error) = state.refresh_client_ips().await {
        tracing::warn!(%error, "failed to refresh client IP cache after deletion");
    }

    if !inventory_only {
        super::clients::refresh_dhcp(&state, &settings, "deleting client").await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::{
        image::{ImageBackend, ImageBackendInfo},
        iscsi::{
            ChapCredentials, IscsiLunState, IscsiProvisioner, IscsiTargetSpec, IscsiTargetState,
        },
    };
    use anyhow::Result;
    use std::{path::Path as FilePath, sync::Arc};
    struct UnusedImages;

    impl ImageBackend for UnusedImages {
        fn exists(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn create_volume(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn destroy(&self, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rename(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn clone_image(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn create_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn destroy_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn rollback_snapshot(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn resize(&self, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn import_raw(&self, _: &FilePath, _: &str, _: u64) -> Result<()> {
            unreachable!()
        }
        fn verify(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn info(&self, _: &str) -> Result<ImageBackendInfo> {
            unreachable!()
        }
        fn set_os_type(&self, _: &str, _: &str) -> Result<()> {
            unreachable!()
        }
        fn image_parent(&self) -> Result<String> {
            unreachable!()
        }
    }

    struct FailingIscsi {
        fail_remove: bool,
    }

    impl IscsiProvisioner for FailingIscsi {
        fn create_target(&self, _: &IscsiTargetSpec) -> Result<()> {
            unreachable!()
        }
        fn remove_target(&self, _: &str) -> Result<()> {
            unreachable!()
        }
        fn remove_target_with_backstores(&self, _: &str, _: &[String]) -> Result<()> {
            if self.fail_remove {
                anyhow::bail!("active session prevents removal")
            }
            Ok(())
        }
        fn target_exists(&self, _: &str) -> Result<bool> {
            unreachable!()
        }
        fn set_chap_auth(&self, _: &str, _: Option<&ChapCredentials>) -> Result<()> {
            unreachable!()
        }
        fn list_target_luns(&self, _: &str) -> Result<Vec<IscsiLunState>> {
            anyhow::bail!("active session")
        }
        fn inspect_target(&self, _: &IscsiTargetSpec) -> Result<IscsiTargetState> {
            unreachable!()
        }
        fn reconcile(&self, _: &IscsiTargetSpec) -> Result<()> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn failed_storage_cleanup_preserves_client_for_retry() {
        for fail_remove in [true, false] {
            let (mut state, _, _, _) = crate::api::security_tests::setup().await;
            Arc::get_mut(&mut state.application).unwrap().storage =
                Arc::new(crate::application::StorageService::new(
                    Arc::new(UnusedImages),
                    Arc::new(FailingIscsi { fail_remove }),
                ));
            sqlx::query("INSERT INTO clients (id,name,mac,ip,master,snapshot,target_iqn,enabled,created_at,updated_at) VALUES ('pc','PC001','00:11:22:33:44:55','192.168.1.101','diskless/image-disk/windows11','diskless/image-disk/windows11@ready','iqn.2024-01.com.diskless:client.pc001',1,'2026-10-08T00:00:00Z','2026-10-08T00:00:00Z')")
                .execute(&state.db_pool).await.unwrap();
            // Missing owned volume is safe to retry; game cleanup must still
            // succeed before the client can be forgotten.
            assert_eq!(
                delete_client(State(state.clone()), axum::extract::Path("pc".into())).await,
                Err(StatusCode::CONFLICT)
            );
            assert!(state
                .application
                .clients
                .get_by_string("pc")
                .await
                .unwrap()
                .is_some());
        }
    }
    #[tokio::test]
    async fn enrolled_inventory_client_can_be_deleted_before_storage_setup() {
        let (mut state, _, _, _) = crate::api::security_tests::setup().await;
        Arc::get_mut(&mut state.application).unwrap().storage =
            Arc::new(crate::application::StorageService::new(
                Arc::new(UnusedImages),
                Arc::new(FailingIscsi { fail_remove: true }),
            ));
        sqlx::query("INSERT INTO clients (id,name,mac,ip,master,enabled,created_at,updated_at) VALUES ('inventory','CLIENT-001','00:11:22:33:44:55','192.168.1.101','pending',1,'2026-10-08T00:00:00Z','2026-10-08T00:00:00Z')")
            .execute(&state.db_pool).await.unwrap();
        assert_eq!(
            delete_client(
                State(state.clone()),
                axum::extract::Path("inventory".into())
            )
            .await,
            Ok(())
        );
        assert!(state
            .application
            .clients
            .get_by_string("inventory")
            .await
            .unwrap()
            .is_none());
    }
}
