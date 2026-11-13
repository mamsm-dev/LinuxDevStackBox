// Application update check.
//
// LinuxDevStackBox is distributed as an AppImage (or a package from your
// distribution). In-app self-updating through tauri-plugin-updater needs signed
// update artifacts and a release feed, which this build does not ship, so the
// check always reports "no update available". The command names and result
// shape are kept so the frontend updater UI keeps working unchanged
// (`null` = up to date; it never reaches the download/install phase).

use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UpdateCheckMetadata {
    rid: u32,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

/// Empty string: there is no update feed for this build.
pub fn updater_endpoint_string() -> String {
    String::new()
}

#[tauri::command]
pub fn get_updater_endpoint() -> Result<String, String> {
    Ok(updater_endpoint_string())
}

#[tauri::command]
pub async fn check_app_update() -> Result<Option<UpdateCheckMetadata>, String> {
    Ok(None)
}
