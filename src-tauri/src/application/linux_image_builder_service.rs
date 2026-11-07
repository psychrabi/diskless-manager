use crate::{
    application::image_service::ImageService,
    core::image::{CreateImageRequest, Image, ImageFormat},
};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    process::{Output, Stdio},
};
use tokio::process::Command;

const EXECUTABLE: &str = "/usr/bin/diskless-manager";

#[derive(Clone)]
pub struct LinuxImageBuilderService {
    images: ImageService,
}

#[derive(Serialize)]
pub struct LinuxImageAccount {
    pub username: String,
    pub password: String,
    pub sudo_user: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinuxImagePreflight {
    pub distro: String,
    pub minimum_size_gb: u64,
}

impl LinuxImageBuilderService {
    pub fn new(images: ImageService) -> Self {
        Self { images }
    }

    /// Inspect the supported live-ISO layout without creating or modifying a ZVOL.
    pub async fn preflight(&self, iso: &Path, size_gb: u64) -> Result<LinuxImagePreflight> {
        if !iso.is_absolute() {
            bail!("installer ISO path must be absolute");
        }
        if size_gb < 20 {
            bail!("Linux boot images must be at least 20 GiB");
        }
        let size = size_gb.to_string();
        let output = run_helper(
            [
                "--check".to_owned(),
                "--iso".to_owned(),
                iso.display().to_string(),
                "--size-gb".to_owned(),
                size,
            ],
            None,
        )
        .await?;
        serde_json::from_slice(&output.stdout)
            .context("Linux image helper returned invalid preflight data")
    }

    /// Create a private catalog draft, build it, then publish only after the
    /// helper has completed every filesystem, initramfs, and UEFI step.
    pub async fn build(
        &self,
        iso: &Path,
        name: String,
        size_gb: u64,
        account: LinuxImageAccount,
    ) -> Result<Image> {
        if !iso.is_absolute() {
            bail!("installer ISO path must be absolute");
        }
        if size_gb < 20 {
            bail!("Linux boot images must be at least 20 GiB");
        }
        validate_linux_image_account(&account.username, &account.password)?;
        let draft = self
            .images
            .create_draft(CreateImageRequest {
                name,
                os_type: "linux".into(),
                size_gb,
                format: Some(ImageFormat::Raw.to_string()),
                description: Some("Linux iSCSI boot image build in progress".into()),
            })
            .await?;
        let account_settings = serde_json::to_vec(&account)?;
        let result = run_helper(
            [
                "--build".to_owned(),
                "--iso".to_owned(),
                iso.display().to_string(),
                "--volume".to_owned(),
                draft.name.clone(),
            ],
            Some(account_settings),
        )
        .await;
        if let Err(error) = result {
            return match self.images.fail_draft(&draft.id).await {
                Ok(_) => Err(error).context(
                    "Linux image build failed; the partial image remains visible for deletion",
                ),
                Err(persist_error) => Err(error).context(format!(
                    "failed to mark incomplete image '{}' as failed: {persist_error:#}",
                    draft.name
                )),
            };
        }
        match self
            .images
            .publish_draft(
                &draft.id,
                "Built as a generic iBFT-aware Linux iSCSI image".into(),
            )
            .await
        {
            Ok(image) => Ok(image),
            Err(error) => match self.images.fail_draft(&draft.id).await {
                Ok(_) => Err(error).context(
                    "Linux image was built, but catalog publishing failed; the image is marked failed",
                ),
                Err(mark_error) => Err(error).context(format!(
                    "Linux image was built, catalog publishing failed, and the image could not be marked failed: {mark_error:#}"
                )),
            },
        }
    }
}

pub fn validate_linux_image_account(username: &str, password: &str) -> Result<()> {
    anyhow::ensure!(
        (1..=32).contains(&username.len())
            && username
                .as_bytes()
                .first()
                .is_some_and(|character| character.is_ascii_lowercase() || *character == b'_')
            && username.bytes().all(|character| {
                character.is_ascii_lowercase()
                    || character.is_ascii_digit()
                    || matches!(character, b'_' | b'-')
            }),
        "username must start with a lowercase letter or underscore and contain only lowercase letters, digits, underscores, or hyphens"
    );
    anyhow::ensure!(
        username != "root",
        "root cannot be selected as the Linux image user"
    );
    anyhow::ensure!(
        !password.is_empty() && password.len() <= 256 && !password.chars().any(char::is_control),
        "password must contain 1 to 256 non-control characters"
    );
    Ok(())
}

async fn run_helper<I, S>(arguments: I, stdin: Option<Vec<u8>>) -> Result<Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("sudo");
    command
        .args(["-n", EXECUTABLE, "internal-linux-image"])
        .args(arguments);
    let output = if let Some(mut input) = stdin {
        use tokio::io::AsyncWriteExt;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("start privileged Linux image builder")?;
        let write_result = child
            .stdin
            .take()
            .context("Linux image helper stdin unavailable")?
            .write_all(&input)
            .await;
        input.fill(0);
        if let Err(error) = write_result {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(error).context("send Linux image account settings");
        }
        child
            .wait_with_output()
            .await
            .context("wait for privileged Linux image builder")?
    } else {
        command
            .output()
            .await
            .context("start privileged Linux image builder")?
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let message = if stderr.is_empty() {
            format!("privileged Linux image builder failed ({})", output.status)
        } else if stderr.to_ascii_lowercase().contains("password is required") {
            "privileged access is not configured; run Privileged Access setup in the app using the server's pkexec prompt".to_owned()
        } else {
            format!("privileged Linux image builder failed: {stderr}")
        };
        bail!("{message}");
    }
    Ok(output)
}
