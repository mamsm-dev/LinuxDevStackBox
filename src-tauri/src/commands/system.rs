// System-wide diagnostic and setup commands.

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use crate::commands::apache::{start_apache, stop_apache};
use crate::commands::mysql::{start_mysql, stop_mysql};
use crate::utils::app_settings::{read_app_settings, write_app_settings, AppSettings};
use crate::utils::paths::{
    ensure_user_data_dirs, get_apache_binary, get_php_binary, get_user_data_root,
    get_user_config_root, user_www_dir,
};
use crate::utils::process::create_hidden_command;

#[tauri::command]
pub async fn check_binaries() -> Result<HashMap<String, bool>, String> {
    let mut binaries = HashMap::new();

    binaries.insert("apache".to_string(), get_apache_binary().is_ok());
    binaries.insert("mysql".to_string(), crate::utils::paths::get_mysqld_binary().is_ok());
    binaries.insert("php".to_string(), get_php_binary().is_ok());
    binaries.insert("php-cgi".to_string(), crate::utils::paths::get_php_cgi_binary().is_ok());

    Ok(binaries)
}

#[tauri::command]
pub async fn debug_paths() -> Result<HashMap<String, String>, String> {
    let mut paths = HashMap::new();

    paths.insert(
        "current_dir".to_string(),
        env::current_dir().unwrap_or_default().display().to_string(),
    );
    paths.insert(
        "data_root".to_string(),
        get_user_data_root().display().to_string(),
    );
    paths.insert(
        "config_root".to_string(),
        get_user_config_root().display().to_string(),
    );

    for (key, result) in [
        ("apache_path", get_apache_binary()),
        ("php_path", get_php_binary()),
        ("php_cgi_path", crate::utils::paths::get_php_cgi_binary()),
        ("mysql_path", crate::utils::paths::get_mysqld_binary().map(|(p, _)| p)),
    ] {
        match result {
            Ok(path) => {
                paths.insert(format!("{key}_exists"), "true".to_string());
                paths.insert(key.to_string(), path.display().to_string());
            }
            Err(error) => {
                paths.insert(format!("{key}_exists"), "false".to_string());
                paths.insert(key.to_string(), error);
            }
        }
    }

    Ok(paths)
}

#[tauri::command]
pub async fn debug_installation() -> Result<HashMap<String, String>, String> {
    let mut debug_info = HashMap::new();

    if let Ok(exe_path) = env::current_exe() {
        debug_info.insert("exe_path".to_string(), exe_path.display().to_string());
        if let Some(parent) = exe_path.parent() {
            debug_info.insert("exe_parent".to_string(), parent.display().to_string());
        }
    }

    let config_dir = get_user_config_root();
    let httpd_conf = config_dir.join("httpd.conf");
    let phpmyadmin_conf = config_dir.join("phpmyadmin.conf");

    debug_info.insert(
        "user_data_root".to_string(),
        get_user_data_root().display().to_string(),
    );
    debug_info.insert("config_dir".to_string(), config_dir.display().to_string());
    debug_info.insert("httpd_conf_path".to_string(), httpd_conf.display().to_string());
    debug_info.insert("httpd_conf_exists".to_string(), httpd_conf.exists().to_string());
    debug_info.insert(
        "phpmyadmin_conf_path".to_string(),
        phpmyadmin_conf.display().to_string(),
    );
    debug_info.insert(
        "phpmyadmin_conf_exists".to_string(),
        phpmyadmin_conf.exists().to_string(),
    );

    for (key, result) in [
        ("apache_bin_path", get_apache_binary()),
        ("php_bin_path", get_php_binary()),
        ("mysql_bin_path", crate::utils::paths::get_mysqld_binary().map(|(p, _)| p)),
    ] {
        match result {
            Ok(path) => {
                debug_info.insert(format!("{key}_exists"), "true".to_string());
                debug_info.insert(key.to_string(), path.display().to_string());
            }
            Err(error) => {
                debug_info.insert(format!("{key}_exists"), "false".to_string());
                debug_info.insert(key.to_string(), error);
            }
        }
    }

    let phpmyadmin_index = crate::utils::paths::phpmyadmin_dir().join("index.php");
    debug_info.insert(
        "phpmyadmin_path".to_string(),
        phpmyadmin_index.display().to_string(),
    );
    debug_info.insert(
        "phpmyadmin_exists".to_string(),
        phpmyadmin_index.exists().to_string(),
    );

    let mysql_config = get_user_config_root().join("my.cnf");
    debug_info.insert(
        "mysql_config_path".to_string(),
        mysql_config.display().to_string(),
    );
    debug_info.insert(
        "mysql_config_exists".to_string(),
        mysql_config.exists().to_string(),
    );

    Ok(debug_info)
}

#[tauri::command]
pub async fn stop_all_services() -> Result<String, String> {
    let mut results = Vec::new();

    match stop_mysql().await {
        Ok(_) => results.push("MySQL stopped".to_string()),
        Err(e) => results.push(format!("MySQL stop failed: {}", e)),
    }

    match stop_apache().await {
        Ok(_) => results.push("Apache stopped".to_string()),
        Err(e) => results.push(format!("Apache stop failed: {}", e)),
    }

    let summary = results.join("; ");
    if results.iter().any(|result| result.contains(" failed:")) { Err(summary) } else { Ok(summary) }
}

#[tauri::command]
pub async fn start_all_services() -> Result<String, String> {
    let mut results = Vec::new();

    match start_apache().await {
        Ok(_) => results.push("Apache started".to_string()),
        Err(e) => results.push(format!("Apache start failed: {}", e)),
    }

    match start_mysql().await {
        Ok(_) => results.push("MySQL started".to_string()),
        Err(e) => results.push(format!("MySQL start failed: {}", e)),
    }

    let summary = results.join("; ");
    if results.iter().any(|result| result.contains(" failed:")) { Err(summary) } else { Ok(summary) }
}

#[tauri::command]
pub async fn restart_all_services() -> Result<String, String> {
    stop_all_services().await?;
    tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    start_all_services().await
}

#[tauri::command]
pub async fn test_apache_config() -> Result<String, String> {
    let binary = get_apache_binary()?;
    let config_path = get_user_config_root().join("httpd.conf");

    if !config_path.exists() {
        return Err(format!(
            "Apache config not found at: {}",
            config_path.display()
        ));
    }

    let mut test_cmd = create_hidden_command(binary.as_os_str());
    test_cmd
        .arg("-f")
        .arg(&config_path)
        .arg("-t")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    match test_cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                Ok(format!(
                    "Apache config test PASSED\nOutput: {}\nPath used: {}",
                    stdout,
                    binary.display()
                ))
            } else {
                Err(format!(
                    "Apache config test FAILED\nError: {}\nOutput: {}\nPath used: {}",
                    stderr,
                    stdout,
                    binary.display()
                ))
            }
        }
        Err(e) => Err(format!(
            "Failed to run Apache config test: {}\nPath: {}",
            e,
            binary.display()
        )),
    }
}

#[tauri::command]
pub async fn test_mysql_config() -> Result<String, String> {
    let (mysqld_path, _) = crate::utils::paths::get_mysqld_binary()?;
    let config_path = get_user_config_root().join("my.cnf");

    if !config_path.exists() {
        return Err(format!(
            "MySQL config not found at: {}",
            config_path.display()
        ));
    }

    // MariaDB has no --validate-config; start it against a throwaway port
    // guard instead. --help validates option syntax on both flavors.
    let mut cmd = create_hidden_command(mysqld_path.as_os_str());
    cmd.arg(format!("--defaults-file={}", config_path.display()))
        .arg("--help")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let text = if stderr.trim().is_empty() { stdout.trim() } else { stderr.trim() };
            let rejected = text.contains("unknown variable")
                || text.contains("unknown option")
                || text.contains("Syntax error");
            if output.status.success() && !rejected {
                Ok(format!("MySQL config validation PASSED\nOutput: {}", text))
            } else {
                Err(format!("MySQL config validation FAILED\nError: {}", text))
            }
        }
        Err(e) => Err(format!("Failed to run MySQL config validation: {}", e)),
    }
}

#[tauri::command]
pub async fn create_directory_structure() -> Result<String, String> {
    // Ensure all user-data directories exist.
    ensure_user_data_dirs();

    let www_path = user_www_dir();

    let index_php_content = include_str!("../../templates/default_index.php");
    std::fs::write(www_path.join("index.php"), index_php_content).map_err(|e| e.to_string())?;

    let phpinfo_content = include_str!("../../templates/default_phpinfo.php");
    std::fs::write(www_path.join("phpinfo.php"), phpinfo_content).map_err(|e| e.to_string())?;

    let index_html_content = include_str!("../../templates/default_index.html");
    std::fs::write(www_path.join("index.html"), index_html_content).map_err(|e| e.to_string())?;

    let test_html_content = include_str!("../../templates/default_test.html");
    std::fs::write(www_path.join("test.html"), test_html_content).map_err(|e| e.to_string())?;

    Ok("Directory structure and default web files created successfully".to_string())
}

/// XDG autostart desktop entry used for "launch LinuxDevStackBox at login".
/// `~/.config/autostart` is honored by KDE Plasma, GNOME and most other
/// desktop environments, and requires no special privileges.
fn autostart_desktop_path() -> PathBuf {
    let config_base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_default().to_string_lossy().into_owned())
                .join(".config")
        });
    config_base.join("autostart").join(format!("{}.desktop", crate::utils::paths::APP_ID))
}

#[tauri::command]
pub async fn get_autostart() -> Result<bool, String> {
    Ok(autostart_desktop_path().is_file())
}

#[tauri::command]
pub async fn set_autostart(enabled: bool) -> Result<bool, String> {
    let path = autostart_desktop_path();
    if !enabled {
        // Removing a non-existent entry counts as already-disabled.
        match std::fs::remove_file(&path) {
            Ok(()) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("Failed to remove autostart entry: {}", e)),
        }
    }
    let exe = env::current_exe()
        .map_err(|e| format!("Failed to resolve current executable: {}", e))?;
    let entry = format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName=LinuxDevStackBox\n\
         Comment=Local PHP development stack\nExec={}\nIcon={}\nTerminal=false\n\
         Categories=Development;WebDevelopment;\nX-GNOME-Autostart-enabled=true\n",
        exe.display(),
        crate::utils::paths::APP_ID
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, entry)
        .map_err(|e| format!("Failed to write autostart entry: {}", e))?;
    Ok(true)
}

#[tauri::command]
pub async fn get_app_settings() -> Result<AppSettings, String> {
    Ok(read_app_settings())
}

#[tauri::command]
pub async fn set_app_settings(settings: AppSettings) -> Result<(), String> {
    write_app_settings(&settings)
}

/// Run an executable with a single argument and return its trimmed stdout
/// (or stderr fallback) on success. None on any failure.
fn read_version(exe: &std::path::Path, arg: &str) -> Option<String> {
    if !exe.exists() {
        return None;
    }
    let output = create_hidden_command(exe.as_os_str())
        .arg(arg)
        .output()
        .ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).to_string();
    }
    text.lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .map(|s| s.to_string())
}

fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content.lines().find_map(|line| {
                line.strip_prefix("PRETTY_NAME=")
                    .map(|v| v.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| "Linux".to_string())
}

#[tauri::command]
pub async fn get_system_info() -> Result<crate::types::SystemInfo, String> {
    let httpd = get_apache_binary().ok();
    let mysqld = crate::utils::paths::get_mysqld_binary().ok().map(|(p, _)| p);

    let mut php_versions: Vec<String> = Vec::new();
    let php_status = crate::commands::php::get_php_status().await.ok();
    if let Some(version) = php_status.and_then(|s| s.version) {
        let branch: String = version.split('.').take(2).collect::<Vec<_>>().join(".");
        php_versions.push(branch);
    }

    Ok(crate::types::SystemInfo {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        os_version: os_pretty_name(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        tauri_version: "2.x".to_string(),
        apache_version: httpd.and_then(|p| read_version(&p, "-v")),
        mysql_version: mysqld.and_then(|p| read_version(&p, "--version")),
        php_versions,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WwwProjectEntry {
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub async fn get_phpmyadmin_url() -> Result<String, String> {
    use crate::commands::ssl::get_ssl_status;
    use crate::utils::config_ports::apache_listen_port;

    let status = get_ssl_status().await?;
    if status.enabled && !status.sites.first().is_some_and(|s| s.verified) {
        return Err("Local HTTPS needs attention. Open Services / Apache / HTTPS and repair or start Apache.".into());
    }
    let ssl_enabled = status.enabled;
    let port = if ssl_enabled { crate::commands::ssl::https_port() } else { apache_listen_port() };
    let scheme = if ssl_enabled { "https" } else { "http" };
    let url = if (port == 80 && !ssl_enabled) || (port == 443 && ssl_enabled) {
        format!("{scheme}://localhost/phpmyadmin/")
    } else {
        format!("{scheme}://localhost:{port}/phpmyadmin/")
    };
    Ok(url)
}

#[tauri::command]
pub async fn list_www_projects() -> Result<Vec<WwwProjectEntry>, String> {
    let www = user_www_dir();
    let mut entries = Vec::new();
    if www.exists() {
        for entry in std::fs::read_dir(&www).map_err(|e| format!("Failed to read www: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read www entry: {}", e))?;
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    entries.push(WwwProjectEntry {
                        name: name.to_string(),
                        path: entry.path().display().to_string(),
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[tauri::command]
pub async fn export_diagnostics_bundle() -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let root = get_user_data_root();
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = root.join(format!("diagnostics-{secs}.json"));

    let settings = read_app_settings();
    let mut payload = serde_json::Map::new();
    payload.insert(
        "appVersion".into(),
        serde_json::Value::String(env!("CARGO_PKG_VERSION").to_string()),
    );
    payload.insert(
        "dataRoot".into(),
        serde_json::Value::String(root.display().to_string()),
    );
    payload.insert(
        "settings".into(),
        serde_json::to_value(&settings).map_err(|e| e.to_string())?,
    );
    payload.insert(
        "apachePort".into(),
        serde_json::json!(crate::utils::config_ports::apache_listen_port()),
    );
    payload.insert(
        "mysqlPort".into(),
        serde_json::json!(crate::utils::config_ports::mysql_port()),
    );

    let json = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write diagnostics: {}", e))?;
    Ok(path.display().to_string())
}
