use crate::types::AuthError;
use crate::config;
use crate::utils::append_log;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

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

#[tauri::command]
pub fn activate_license(key: &str) -> Result<String, AuthError> {
    if key.trim().is_empty() {
        return Err(AuthError {
            message: "License key cannot be empty".to_string(),
        });
    }
    if key.trim() == "trial" {
        // special case for trial license
        let mut cfg = config::read_config();
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
        config::write_config(&cfg).map_err(|e| AuthError {
            message: format!("failed to save license: {}", e),
        })?;
        append_log("INFO", &format!("Trial License activated: {}", key));
        Ok("Trial License activated".to_string())
    } else {
        // verify with remote
        let res: LicenseVerifyResponse =
            verify_license_remote(key).map_err(|e| AuthError { message: e })?;
        if !res.valid {
            return Err(AuthError {
                message: res
                    .message
                    .unwrap_or_else(|| "License not valid".to_string()),
            });
        }

        // write license key and status to config
        let mut cfg = config::read_config();
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
        config::write_config(&cfg).map_err(|e| AuthError {
            message: format!("failed to save license: {}", e),
        })?;

        append_log("INFO", &format!("License activated: {}", key));
        Ok("License activated".to_string())
    }
}

pub fn ensure_license_valid() -> Result<(), AuthError> {
    // read local config
    let cfg = config::read_config();
    if let Some(obj) = cfg.settings.as_object() {
        // Check if the license status is valid and the license_expires is in the future
        if let (Some(status), Some(expires)) = (
            obj.get("license_status").and_then(|v| v.as_str()),
            obj.get("license_expires").and_then(|v| v.as_str()),
        ) {
            if status == "valid" {
                if let Ok(expiry_date) = chrono::NaiveDate::parse_from_str(expires, "%Y-%m-%d") {
                    if expiry_date >= chrono::Local::now().naive_local().date() {
                        return Ok(());
                    } else {
                        return Err(AuthError {
                            message: "License has expired.".to_string(),
                        });
                    }
                }
            }
        }
        use chrono::{NaiveDate, Utc};

        if let Some(val) = obj.get("license_status") {
            if val.as_str() == Some("valid") {
                if let Some(exp_val) = obj.get("license_expires").and_then(|v| v.as_str()) {
                    if let Ok(exp_date) = NaiveDate::parse_from_str(exp_val, "%Y-%m-%d") {
                        let today = Utc::now().date_naive();
                        if exp_date >= today {
                            return Ok(());
                        } else {
                            return Err(AuthError {
                                message: "License has expired.".to_string(),
                            });
                        }
                    } else {
                        return Err(AuthError {
                            message: "Invalid license expiration date format.".to_string(),
                        });
                    }
                } else {
                    return Err(AuthError {
                        message: "License expiration date missing.".to_string(),
                    });
                }
            }
        }
        // removed stray early return so remote re-check can run if license_key exists
        // if let Some(val) = obj.get("license_key") {
        //     if let Some(_key) = val.as_str() {
        //         // try to verify remote (best-effort)
        //         match verify_license_remote(key) {
        //             Ok(r) => {
        //                 if r.valid {
        //                     // persist valid status
        //                     let mut cfg2 = config::read_config();
        //                     let mut settings = cfg2.settings.as_object().cloned().unwrap_or_default();
        //                     settings.insert("license_status".to_string(), serde_json::to_value("valid").unwrap_or_else(|_| serde_json::Value::String("valid".into())));
        //                     if let Some(ex) = r.expires_at {
        //                         settings.insert("license_expires".to_string(), serde_json::to_value(ex).unwrap_or(serde_json::Value::Null));
        //                     }
        //                     cfg2.settings = serde_json::Value::Object(settings);
        //                     let _ = config::write_config(&cfg2);
        //                     return Ok(());
        //                 } else {
        //                     return Err(AuthError { message: r.message.unwrap_or_else(|| "License invalid".to_string()) });
        //                 }
        //             }
        //             Err(e) => {
        //                 // If remote check fails, allow local cached valid status only.
        //                 if let Some(val) = obj.get("license_status") {
        //                     if val.as_str() == Some("valid") {
        //                         append_log("WARN", "License server unreachable, using cached license status");
        //                         return Ok(());
        //                     }
        //                 }
        //                 return Err(AuthError { message: format!("License verification failed: {}", e) });
        //             }
        //         }
        //     }
        // }
    }
    Err(AuthError {
        message: "License not activated. Please activate to use the application.".to_string(),
    })
}

/// Return current license details (for dashboard display)
#[tauri::command]
pub fn get_license_info() -> Result<LicenseInfo, String> {
    let cfg = config::read_config();
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
