// Application update check and installation (GitHub Releases).
//
// On startup the frontend asks `check_app_update`, which reads the latest
// release of the GitHub repository and compares its tag with this build's
// version. When a newer release exists and this install can be updated
// (AppImage, or a deb/rpm/pacman package), the user is asked whether to update;
// `download_and_install_update` then downloads the matching asset, verifies it
// against the release's SHA256SUMS.txt and installs it:
//   - AppImage: replaces the AppImage file in place
//   - pacman / deb / rpm: installs the package through `pkexec`
// Other installs (from source, unknown packaging) are only notified.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;

use crate::utils::process::host_command;

const REPO: &str = "mamsm-dev/LinuxDevStackBox";
const API_LATEST: &str = "https://api.github.com/repos/mamsm-dev/LinuxDevStackBox/releases/latest";

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum InstallKind {
    AppImage,
    Pacman,
    Deb,
    Rpm,
    Unknown,
}

#[derive(Clone)]
struct Pending {
    kind: InstallKind,
    version: String,
    release_url: String,
    asset_name: String,
    asset_url: String,
    asset_size: u64,
    sums_url: Option<String>,
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckMetadata {
    rid: u32,
    current_version: String,
    version: String,
    date: Option<String>,
    body: Option<String>,
    raw_json: serde_json::Value,
}

pub fn updater_endpoint_string() -> String {
    API_LATEST.to_string()
}

#[tauri::command]
pub fn get_updater_endpoint() -> Result<String, String> {
    Ok(updater_endpoint_string())
}

// -- helpers ------------------------------------------------------------------

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches(['v', 'V']);
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

fn is_newer(remote: &str, local: &str) -> bool {
    match (parse_version(remote), parse_version(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
}

fn detect_install_kind() -> InstallKind {
    if std::env::var_os("APPIMAGE").is_some() {
        return InstallKind::AppImage;
    }
    let Ok(exe) = std::env::current_exe() else { return InstallKind::Unknown };
    let exe = exe.to_string_lossy().into_owned();
    if !exe.starts_with("/usr/") {
        return InstallKind::Unknown;
    }
    let owned = |cmd: &str, args: &[&str]| {
        host_command(cmd)
            .args(args)
            .arg(&exe)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    if owned("pacman", &["-Qo"]) {
        InstallKind::Pacman
    } else if owned("dpkg", &["-S"]) {
        InstallKind::Deb
    } else if owned("rpm", &["-qf"]) {
        InstallKind::Rpm
    } else {
        InstallKind::Unknown
    }
}

fn pick_asset(kind: InstallKind, assets: &[GhAsset]) -> Option<&GhAsset> {
    assets.iter().find(|a| match kind {
        InstallKind::AppImage => a.name.ends_with(".AppImage"),
        InstallKind::Pacman => a.name.ends_with(".pkg.tar.zst"),
        InstallKind::Deb => a.name.ends_with(".deb"),
        InstallKind::Rpm => a.name.ends_with(".rpm"),
        InstallKind::Unknown => false,
    })
}

/// Expected hash of `name` from the text of a `sha256sum` file.
fn expected_hash(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let file = it.next()?.trim_start_matches('*');
        (file == name).then(|| hash.to_ascii_lowercase())
    })
}

fn client(timeout: Option<Duration>) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(format!("LinuxDevStackBox/{}", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15));
    if let Some(t) = timeout {
        builder = builder.timeout(t);
    }
    builder.build().map_err(|e| e.to_string())
}

// -- check ----------------------------------------------------------------------

#[tauri::command]
pub async fn check_app_update() -> Result<Option<UpdateCheckMetadata>, String> {
    // Release assets are built for x86_64 only.
    if std::env::consts::ARCH != "x86_64" {
        return Ok(None);
    }
    let release: GhRelease = client(Some(Duration::from_secs(20)))?
        .get(API_LATEST)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .error_for_status()
        .map_err(|e| format!("GitHub returned an error: {e}"))?
        .json()
        .await
        .map_err(|e| format!("Unexpected GitHub response: {e}"))?;

    let current = env!("CARGO_PKG_VERSION");
    if !is_newer(&release.tag_name, current) {
        *PENDING.lock().unwrap() = None;
        return Ok(None);
    }

    let version = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    let kind = tokio::task::spawn_blocking(detect_install_kind)
        .await
        .unwrap_or(InstallKind::Unknown);
    let asset = pick_asset(kind, &release.assets);
    let pending = asset.map(|a| Pending {
        kind,
        version: version.clone(),
        release_url: release.html_url.clone(),
        asset_name: a.name.clone(),
        asset_url: a.browser_download_url.clone(),
        asset_size: a.size,
        sums_url: release
            .assets
            .iter()
            .find(|x| x.name == "SHA256SUMS.txt")
            .map(|x| x.browser_download_url.clone()),
    });
    let installable = pending.is_some();
    *PENDING.lock().unwrap() = pending;

    Ok(Some(UpdateCheckMetadata {
        rid: 1,
        current_version: current.to_string(),
        version,
        date: release.published_at,
        body: release.body,
        raw_json: serde_json::json!({
            "repository": REPO,
            "releaseUrl": release.html_url,
            "installKind": kind,
            "installable": installable,
            "assetName": asset.map(|a| a.name.clone()),
        }),
    }))
}

// -- download + install -----------------------------------------------------------

fn run_privileged(args: &[&str]) -> Result<(), String> {
    let status = host_command("pkexec")
        .args(args)
        .status()
        .map_err(|e| format!("Could not run pkexec (is polkit installed?): {e}"))?;
    match status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err("Authentication was cancelled or denied.".into()),
        _ => Err(format!("The package manager failed ({status}). Run the update manually.")),
    }
}

fn have(cmd: &str) -> bool {
    crate::utils::paths::find_in_path(cmd).is_some()
}

fn install(kind: InstallKind, file: &Path) -> Result<(), String> {
    let path = file.to_string_lossy().into_owned();
    match kind {
        InstallKind::AppImage => {
            use std::os::unix::fs::PermissionsExt;
            let target = PathBuf::from(
                std::env::var_os("APPIMAGE").ok_or("APPIMAGE is not set; cannot replace the AppImage.")?,
            );
            let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let staged = target.with_file_name(format!("{name}.new"));
            std::fs::copy(file, &staged).map_err(|e| {
                format!("Cannot write next to {} ({e}). Move the AppImage to a writable folder or update it manually.", target.display())
            })?;
            std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
            std::fs::rename(&staged, &target).map_err(|e| e.to_string())
        }
        InstallKind::Pacman => run_privileged(&["pacman", "-U", "--noconfirm", &path]),
        InstallKind::Deb => {
            if have("apt-get") {
                run_privileged(&["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", &path])
            } else {
                run_privileged(&["dpkg", "-i", &path])
            }
        }
        InstallKind::Rpm => {
            if have("dnf") {
                run_privileged(&["dnf", "install", "-y", &path])
            } else if have("zypper") {
                run_privileged(&["zypper", "--non-interactive", "--no-gpg-checks", "install", &path])
            } else {
                run_privileged(&["rpm", "-U", &path])
            }
        }
        InstallKind::Unknown => Err("This installation cannot be updated automatically.".into()),
    }
}

/// Downloads, verifies and installs the update found by `check_app_update`.
/// Progress is reported on `on_event` with the same event names the frontend
/// already handles (`Started`, `Progress`, `Finished`).
#[tauri::command]
pub async fn download_and_install_update(on_event: Channel<serde_json::Value>) -> Result<(), String> {
    let pending = PENDING.lock().unwrap().clone().ok_or_else(|| {
        "This installation cannot be updated automatically. Download the new version from the releases page: \
         https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest"
            .to_string()
    })?;

    let dir = crate::utils::paths::user_tmp_dir().join("update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(&pending.asset_name);

    let http = client(None)?;
    let sums_url = pending
        .sums_url
        .clone()
        .ok_or("The release has no SHA256SUMS.txt; refusing to install an unverified update.")?;
    let sums = http
        .get(&sums_url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Could not download checksums: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let expected = expected_hash(&sums, &pending.asset_name)
        .ok_or("SHA256SUMS.txt does not list the update file.")?;

    let response = http
        .get(&pending.asset_url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("Download failed: {e}"))?;
    let total = response.content_length().unwrap_or(pending.asset_size);
    let _ = on_event.send(serde_json::json!({ "event": "Started", "data": { "contentLength": total } }));

    let mut out = tokio::fs::File::create(&file).await.map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {e}"))?;
        hasher.update(&chunk);
        out.write_all(&chunk).await.map_err(|e| e.to_string())?;
        let _ = on_event.send(serde_json::json!({ "event": "Progress", "data": { "chunkLength": chunk.len() } }));
    }
    out.flush().await.map_err(|e| e.to_string())?;
    drop(out);

    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        let _ = std::fs::remove_file(&file);
        return Err("The downloaded update failed its checksum check and was discarded.".into());
    }

    let kind = pending.kind;
    let install_path = file.clone();
    let result = tokio::task::spawn_blocking(move || install(kind, &install_path))
        .await
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_dir_all(&dir);
    result?;

    let _ = on_event.send(serde_json::json!({ "event": "Finished" }));
    println!("Updated to {} ({})", pending.version, pending.release_url);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> GhAsset {
        GhAsset { name: name.into(), browser_download_url: format!("https://x/{name}"), size: 1 }
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("v0.2.0", "0.1.22"));
        assert!(is_newer("0.10.0", "0.9.9"));
        assert!(!is_newer("v0.1.22", "0.1.22"));
        assert!(!is_newer("v0.1.9", "0.1.22"));
        assert!(!is_newer("garbage", "0.1.22"));
        assert_eq!(parse_version("v1.2.3-beta.1"), Some((1, 2, 3)));
    }

    #[test]
    fn picks_asset_matching_install_kind() {
        let assets = vec![
            asset("LinuxDevStackBox_0.2.0_amd64.AppImage"),
            asset("LinuxDevStackBox_0.2.0_amd64.deb"),
            asset("LinuxDevStackBox-0.2.0-1.x86_64.rpm"),
            asset("linuxdevstackbox-0.2.0-1-x86_64.pkg.tar.zst"),
            asset("SHA256SUMS.txt"),
        ];
        assert!(pick_asset(InstallKind::AppImage, &assets).unwrap().name.ends_with(".AppImage"));
        assert!(pick_asset(InstallKind::Deb, &assets).unwrap().name.ends_with(".deb"));
        assert!(pick_asset(InstallKind::Rpm, &assets).unwrap().name.ends_with(".rpm"));
        assert!(pick_asset(InstallKind::Pacman, &assets).unwrap().name.ends_with(".pkg.tar.zst"));
        assert!(pick_asset(InstallKind::Unknown, &assets).is_none());
    }

    #[test]
    fn reads_hash_from_sha256sum_output() {
        let sums = "AABB  other.deb\nccdd1122 *app.AppImage\n";
        assert_eq!(expected_hash(sums, "app.AppImage").as_deref(), Some("ccdd1122"));
        assert_eq!(expected_hash(sums, "other.deb").as_deref(), Some("aabb"));
        assert_eq!(expected_hash(sums, "missing"), None);
    }
}
