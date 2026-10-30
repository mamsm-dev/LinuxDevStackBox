// Release check (stable vs beta endpoint from app-settings.json).
//
// Linux packages are upgraded with the distribution package manager, so the
// app only checks for a newer published release and reports it; it never
// downloads or replaces itself.

use serde::Serialize;

use crate::utils::app_settings::read_app_settings;

const STABLE_ENDPOINT: &str =
    "https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest/download/latest.json";
const BETA_ENDPOINT: &str =
    "https://github.com/mamsm-dev/LinuxDevStackBox/releases/download/latest/latest.json";

pub fn updater_endpoint_string() -> String {
    if read_app_settings().update_channel == "beta" {
        BETA_ENDPOINT.to_string()
    } else {
        STABLE_ENDPOINT.to_string()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckMetadata {
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

#[tauri::command]
pub fn get_updater_endpoint() -> Result<String, String> {
    Ok(updater_endpoint_string())
}

fn version_tuple(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.strip_prefix('v').unwrap_or(version);
    let mut parts = core.split(['.', '-']);
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

#[tauri::command]
pub async fn check_app_update() -> Result<Option<UpdateCheckMetadata>, String> {
    let client = reqwest::Client::builder()
        .user_agent("LinuxDevStackBox")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let response = client
        .get(updater_endpoint_string())
        .send()
        .await
        .map_err(|e| format!("Update check failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("Update check returned HTTP {}", response.status()));
    }
    let raw: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid update manifest: {e}"))?;

    let version = raw
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("Update manifest is missing a version")?
        .to_string();
    let current = env!("CARGO_PKG_VERSION");
    let newer = match (version_tuple(&version), version_tuple(current)) {
        (Some(next), Some(cur)) => next > cur,
        _ => version != current,
    };
    if !newer {
        return Ok(None);
    }
    Ok(Some(UpdateCheckMetadata {
        current_version: current.to_string(),
        version,
        date: raw.get("pub_date").and_then(|v| v.as_str()).map(str::to_string),
        body: raw.get("notes").and_then(|v| v.as_str()).map(str::to_string),
        raw_json: raw,
    }))
}
