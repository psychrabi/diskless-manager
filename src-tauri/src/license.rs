use crate::config;
use crate::state::AppState;
use log::info;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const LICENSE_SERVER_URL: &str = "https://license.example.com/api/verify";

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseVerifyResponse {
    valid: bool,
    message: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub license_key: Option<String>,
    pub license_status: Option<String>,
    pub license_expires: Option<String>,
}

fn verify_license_remote(key: &str) -> Result<LicenseVerifyResponse, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| format!("failed to build http client: {error}"))?;

    let response = client
        .post(LICENSE_SERVER_URL)
        .json(&serde_json::json!({ "license_key": key }))
        .send()
        .map_err(|error| format!("license server request failed: {error}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "license server returned status: {}",
            response.status()
        ));
    }

    response
        .json::<LicenseVerifyResponse>()
        .map_err(|error| format!("failed to parse license server response: {error}"))
}

pub fn get_license_info() -> Result<LicenseInfo, String> {
    let config = config::get_config();
    let settings = config.settings.as_object();

    Ok(LicenseInfo {
        license_key: settings
            .and_then(|values| values.get("license_key"))
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        license_status: settings
            .and_then(|values| values.get("license_status"))
            .and_then(|value| value.as_str())
            .map(str::to_owned),
        license_expires: settings
            .and_then(|values| values.get("license_expires"))
            .and_then(|value| value.as_str())
            .map(str::to_owned),
    })
}

pub async fn activate_license_http(state: AppState, key: &str) -> Result<String, String> {
    if key.trim().is_empty() {
        return Err("License key cannot be empty".to_string());
    }

    let is_trial = key.trim() == "trial";
    let (status, expires, message) = if is_trial {
        (
            "valid".to_string(),
            Some("2027-10-12".to_string()),
            "Trial License activated".to_string(),
        )
    } else {
        let response = verify_license_remote(key)?;
        if !response.valid {
            return Err(response
                .message
                .unwrap_or_else(|| "License not valid".to_string()));
        }

        (
            "valid".to_string(),
            response.expires_at,
            "License activated".to_string(),
        )
    };

    let mut config = config::read_config_db(&state.db_pool)
        .await
        .map_err(|error| format!("failed to read config: {error}"))?;
    let mut settings = config.settings.as_object().cloned().unwrap_or_default();
    let stored_key = if is_trial { "trial" } else { key };
    settings.insert("license_key".to_string(), serde_json::json!(stored_key));
    settings.insert("license_status".to_string(), serde_json::json!(status));
    if let Some(expires) = expires {
        settings.insert("license_expires".to_string(), serde_json::json!(expires));
    }
    config.settings = serde_json::Value::Object(settings);
    config::write_config(&state.db_pool, &config)
        .await
        .map_err(|error| format!("failed to save license: {error}"))?;

    info!("{message}: {key}");
    Ok(message)
}
