use crate::config;
use crate::state::AppState;
use crate::types::AuthError;
use log::info;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::State;

const LICENSE_SERVER_URL: &str = "https://license.example.com/api/verify"; // replace with real SaaS license server

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

pub fn verify_license_remote(key: &str) -> Result<LicenseVerifyResponse, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build http client: {}", e))?;

    let resp = client
        .post(LICENSE_SERVER_URL)
        .json(&serde_json::json!({ "license_key": key }))
        .send()
        .map_err(|e| format!("license server request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("license server returned status: {}", resp.status()));
    }

    let body = resp
        .json::<LicenseVerifyResponse>()
        .map_err(|e| format!("failed to parse license server response: {}", e))?;

    Ok(body)
}

#[expect(dead_code, reason = "Old Tauri command replaced by HTTP handler")]
pub async fn activate_license(state: State<'_, AppState>, key: &str) -> Result<String, AuthError> {
    if key.trim().is_empty() {
        return Err(AuthError {
            message: "License key cannot be empty".to_string(),
        });
    }
    if key.trim() == "trial" {
        // special case for trial license
        let mut cfg = config::read_config(state.clone())
            .await
            .map_err(|e| AuthError { message: e.to_string() })?;
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        // set trial license
        settings.insert(
            "license_key".to_string(),
            serde_json::to_value("trial").map_err(|e| AuthError {
                message: format!("failed to serialize license key: {}", e),
            })?,
        );
        settings.insert(
            "license_status".to_string(),
            serde_json::to_value("valid").map_err(|e| AuthError {
                message: format!("failed to serialize license status: {}", e),
            })?,
        );
        settings.insert(
            "license_expires".to_string(),
            serde_json::to_value("2027-10-12").map_err(|e| AuthError {
                message: format!("failed to serialize expires: {}", e),
            })?,
        );
        cfg.settings = serde_json::Value::Object(settings);
        config::write_config(&state.db_pool, &cfg)
            .await
            .map_err(|e| AuthError {
                message: format!("failed to save license: {}", e),
            })?;
        info!("Trial License activated: {}", key);
        Ok("Trial License activated".to_string())
    } else {
        // verify with remote
        let res: LicenseVerifyResponse =
            verify_license_remote(key).map_err(|e| AuthError { message: e.to_string() })?;
        if !res.valid {
            return Err(AuthError {
                message: res
                    .message
                    .unwrap_or_else(|| "License not valid".to_string()),
            });
        }

        // write license key and status to config
        let mut cfg = config::read_config(state.clone())
            .await
            .map_err(|e| AuthError { message: e.to_string() })?;
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        settings.insert(
            "license_key".to_string(),
            serde_json::to_value(key).map_err(|e| AuthError {
                message: format!("failed to serialize license key: {}", e),
            })?,
        );
        settings.insert(
            "license_status".to_string(),
            serde_json::to_value("valid").map_err(|e| AuthError {
                message: format!("failed to serialize license status: {}", e),
            })?,
        );
        if let Some(expires) = res.expires_at {
            settings.insert(
                "license_expires".to_string(),
                serde_json::to_value(expires).map_err(|e| AuthError {
                    message: format!("failed to serialize expires: {}", e),
                })?,
            );
        }
        cfg.settings = serde_json::Value::Object(settings);
        config::write_config(&state.db_pool, &cfg)
            .await
            .map_err(|e| AuthError {
                message: format!("failed to save license: {}", e),
            })?;

        info!("License activated: {}", key);
        Ok("License activated".to_string())
    }
}

#[expect(
    dead_code,
    reason = "License validation function kept for potential future use"
)]
pub fn ensure_license_valid() -> Result<(), AuthError> {
    let cfg = config::get_config();
    let settings = cfg.settings.as_object().ok_or_else(|| AuthError {
        message: "License not activated. Please activate to use the application.".to_string(),
    })?;

    let status = settings.get("license_status").and_then(|v| v.as_str());
    let expires = settings.get("license_expires").and_then(|v| v.as_str());

    match (status, expires) {
        (Some("valid"), Some(exp_str)) => {
            if let Ok(exp_date) = chrono::NaiveDate::parse_from_str(exp_str, "%Y-%m-%d") {
                let today = chrono::Local::now().naive_local().date();
                if exp_date >= today {
                    Ok(())
                } else {
                    Err(AuthError {
                        message: "License has expired.".to_string(),
                    })
                }
            } else {
                Err(AuthError {
                    message: "Invalid license expiration date format.".to_string(),
                })
            }
        }
        _ => Err(AuthError {
            message:
                "License not activated or status invalid. Please activate to use the application."
                    .to_string(),
        }),
    }
}

/// Return current license details (for dashboard display)
pub fn get_license_info() -> Result<LicenseInfo, String> {
    let cfg = config::get_config();
    let mut key: Option<String> = None;
    let mut status: Option<String> = None;
    let mut expires: Option<String> = None;

    if let Some(obj) = cfg.settings.as_object() {
        if let Some(v) = obj.get("license_key").and_then(|s| s.as_str()) {
            key = Some(v.to_string());
        }
        if let Some(v) = obj.get("license_status").and_then(|s| s.as_str()) {
            status = Some(v.to_string());
        }
        if let Some(v) = obj.get("license_expires").and_then(|s| s.as_str()) {
            expires = Some(v.to_string());
        }
    }

    Ok(LicenseInfo {
        license_key: key,
        license_status: status,
        license_expires: expires,
    })
}

/// HTTP handler version of activate_license (for API endpoints)
pub async fn activate_license_http(state: AppState, key: &str) -> Result<String, String> {
    if key.trim().is_empty() {
        return Err("License key cannot be empty".to_string());
    }
    if key.trim() == "trial" {
        // special case for trial license
        let mut cfg = config::read_config_db(&state.db_pool)
            .await
            .map_err(|e| format!("failed to read config: {}", e))?;
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        // set trial license
        settings.insert(
            "license_key".to_string(),
            serde_json::to_value("trial")
                .map_err(|e| format!("failed to serialize license key: {}", e))?,
        );
        settings.insert(
            "license_status".to_string(),
            serde_json::to_value("valid")
                .map_err(|e| format!("failed to serialize license status: {}", e))?,
        );
        settings.insert(
            "license_expires".to_string(),
            serde_json::to_value("2027-10-12")
                .map_err(|e| format!("failed to serialize expires: {}", e))?,
        );
        cfg.settings = serde_json::Value::Object(settings);
        config::write_config(&state.db_pool, &cfg)
            .await
            .map_err(|e| format!("failed to save license: {}", e))?;
        info!("Trial License activated: {}", key);
        Ok("Trial License activated".to_string())
    } else {
        // verify with remote
        let res: LicenseVerifyResponse = verify_license_remote(key)?;
        if !res.valid {
            return Err(res
                .message
                .unwrap_or_else(|| "License not valid".to_string()));
        }

        // write license key and status to config
        let mut cfg = config::read_config_db(&state.db_pool)
            .await
            .map_err(|e| format!("failed to read config: {}", e))?;
        let mut settings = cfg.settings.as_object().cloned().unwrap_or_default();
        settings.insert(
            "license_key".to_string(),
            serde_json::to_value(key)
                .map_err(|e| format!("failed to serialize license key: {}", e))?,
        );
        settings.insert(
            "license_status".to_string(),
            serde_json::to_value("valid")
                .map_err(|e| format!("failed to serialize license status: {}", e))?,
        );
        if let Some(expires) = res.expires_at {
            settings.insert(
                "license_expires".to_string(),
                serde_json::to_value(expires)
                    .map_err(|e| format!("failed to serialize expires: {}", e))?,
            );
        }
        cfg.settings = serde_json::Value::Object(settings);
        config::write_config(&state.db_pool, &cfg)
            .await
            .map_err(|e| format!("failed to save license: {}", e))?;

        info!("License activated: {}", key);
        Ok("License activated".to_string())
    }
}
