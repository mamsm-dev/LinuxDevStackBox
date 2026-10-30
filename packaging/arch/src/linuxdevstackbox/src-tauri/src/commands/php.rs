// PHP commands.
//
// PHP comes from the distribution packages (php + php-cgi). Version upgrades
// are performed with the package manager, so the Windows download feed and
// junction-based version switching of the original app have no equivalent
// here. What is preserved: status reporting, the generated per-user php.ini,
// extension management, project PHP settings and PHP terminals.

use std::fs;
use std::path::Path;

use crate::types::{PHPVersionInfo, ServiceInfo};
use crate::utils::paths::{
    get_php_binary, get_php_cgi_binary, php_ini_template, php_module_dir, to_apache_path,
    user_config_dir, user_logs_dir, user_sessions_dir,
};
use crate::utils::process::create_hidden_command;

pub(crate) static PHP_RUNTIME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Minimum supported PHP branch. Older branches no longer receive upstream
// security fixes and are rejected before Apache can serve PHP.
const MIN_PHP: (u32, u32, u32) = (8, 2, 0);

fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<_> = version.split('.').collect();
    if parts.len() < 3 { return None; }
    let number = |value: &str| -> Option<u32> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) { return None; }
        value.parse().ok()
    };
    Some((number(parts[0])?, number(parts[1])?, number(parts[2])?))
}

fn is_at_least(version: &str, minimum: (u32, u32, u32)) -> bool {
    version_tuple(version).is_some_and(|actual| actual >= minimum)
}

pub(crate) fn read_runtime_version(exe: &Path) -> Option<String> {
    let output = create_hidden_command(exe.as_os_str())
        .args(["-n", "-r", "echo PHP_VERSION;"]).output().ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && version_tuple(&version).is_some()).then_some(version)
}

fn read_system_php_version() -> Option<String> {
    read_runtime_version(&get_php_binary().ok()?)
}

fn system_branch() -> Option<String> {
    let version = read_system_php_version()?;
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() < 2 { return None; }
    Some(format!("{}.{}", parts[0], parts[1]))
}

pub(crate) fn project_runtime(version: &str) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let php = get_php_binary()?;
    let php_cgi = get_php_cgi_binary()?;
    if !php.is_file() || !php_cgi.is_file() {
        return Err("The PHP runtime is incomplete. Install the php and php-cgi packages.".into());
    }
    let runtime_version = read_runtime_version(&php)
        .ok_or("Cannot read the installed PHP version.")?;
    if !runtime_version.starts_with(&format!("{version}.")) {
        return Err(format!(
            "PHP {version} is not the installed system runtime (found PHP {runtime_version}). \
             Install that branch with the package manager, or reset the project to the system PHP."
        ));
    }
    Ok((php_cgi.parent().map(Path::to_path_buf).unwrap_or_else(|| php_cgi.clone()),
        user_config_dir().join("php-runtimes").join(version).join("php.ini")))
}

pub(crate) fn prepare_project_runtime(version: &str) -> Result<(), String> {
    let (directory, ini) = project_runtime(version)?;
    let output = create_hidden_command(directory.join("php").as_os_str())
        .args(["-n", "-r", "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;"])
        .output().map_err(|e| e.to_string())?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != version {
        return Err(format!("The installed PHP {version} runtime failed validation."));
    }
    let module_dir = php_module_dir()?;
    if ini.is_file() {
        let content = fs::read_to_string(&ini).map_err(|e| e.to_string())?;
        let updated = temporary_defaults(&content, &to_apache_path(&crate::utils::paths::user_tmp_dir()));
        if content != updated { fs::write(&ini, updated).map_err(|e| e.to_string())?; }
        return Ok(());
    }
    let template = php_ini_template()
        .ok_or("No distribution php.ini template was found")?;
    let content = fs::read_to_string(&template).map_err(|e| e.to_string())?;
    let settings = content.lines().filter(|line| {
        let key = line.split('=').next().unwrap_or("").trim();
        !["extension_dir", "session.save_path", "error_log"].iter().any(|name| key.eq_ignore_ascii_case(name))
    }).collect::<Vec<_>>().join("\n");
    let sessions = user_sessions_dir();
    fs::create_dir_all(&sessions).map_err(|e| e.to_string())?;
    let header = format!("; LinuxDevStackBox pinned PHP {version}\nextension_dir=\"{}\"\nsession.save_path=\"{}\"\nerror_log=\"{}\"\n",
        to_apache_path(&module_dir), to_apache_path(&sessions),
        to_apache_path(&user_logs_dir().join(format!("php-{version}-error.log"))));
    fs::create_dir_all(ini.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(&ini, temporary_defaults(&format!("{header}{}", enable_essential_extensions(settings, &module_dir)), &to_apache_path(&crate::utils::paths::user_tmp_dir())))
        .map_err(|e| e.to_string())
}

pub(crate) fn validate_web_runtimes() -> Result<(), String> {
    let php = get_php_binary()?;
    let version = read_runtime_version(&php)
        .ok_or("Cannot read the installed PHP version. Install the php package.")?;
    if !is_at_least(&version, MIN_PHP) {
        return Err(format!(
            "PHP {version} is too old. LinuxDevStackBox requires PHP {}.{}.{} or newer; \
             upgrade the php package with your distribution's package manager.",
            MIN_PHP.0, MIN_PHP.1, MIN_PHP.2
        ));
    }
    for site in super::vhosts::load_vhosts()?.into_iter().filter(|site| site.enabled) {
        if let Some(branch) = site.php_version.as_deref() {
            project_runtime(branch).map_err(|e| format!("Project {}: {e}", site.domain))?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn get_php_status() -> Result<ServiceInfo, String> {
    let version = read_system_php_version();

    Ok(ServiceInfo {
        running: version.is_some(),
        pid: None,
        port: None,
        version,
    })
}

async fn check_active_php_version(version: &str) -> bool {
    system_branch().is_some_and(|branch| branch == version)
}

use crate::utils::php_defaults::{enable_extensions as enable_essential_extensions, temporary_defaults};

// Ensures the active PHP's php.ini has the minimum settings required for
// LinuxDevStackBox to work (session path, extension_dir, essential extensions).
// Called by start_apache on every start.
//
// The user-specific php.ini lives in the XDG config dir so it is never
// committed to the repository and never touches /etc.
pub fn patch_php_ini() {
    let php_ini = user_config_dir().join("php.ini");

    // Seed from the distribution template on first run.
    let first_run = !php_ini.exists();
    if first_run {
        match php_ini_template() {
            Some(template) => {
                if let Err(e) = std::fs::copy(&template, &php_ini) {
                    println!("Failed to copy php.ini template: {}", e);
                    return;
                }
            }
            None => {
                // Minimal base so PHP CGI still runs without a template.
                let _ = std::fs::write(&php_ini, "; Generated by LinuxDevStackBox\n");
            }
        }
    }

    let content = match std::fs::read_to_string(&php_ini) {
        Ok(c) => c,
        Err(_) => return,
    };

    let Ok(ext_dir) = php_module_dir() else {
        println!("patch_php_ini: cannot resolve the PHP extension directory");
        return;
    };
    let ext_dir_str = to_apache_path(&ext_dir);

    // Sessions go under the user data root so PHP CGI can always write there.
    let sessions_str = to_apache_path(&user_sessions_dir());
    let error_log = to_apache_path(&user_logs_dir().join("php_error.log"));

    let mut new_content = String::with_capacity(content.len() + 256);
    let mut session_save_path_set = false;
    let mut extension_dir_set = false;
    let mut display_errors_set = false;
    let mut log_errors_set = false;
    let mut error_log_set = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with(';') && trimmed.contains("session.save_path") {
            new_content.push_str(line);
            new_content.push('\n');
            if !session_save_path_set {
                new_content.push_str(&format!(
                    "session.save_path = \"{}\"\n",
                    sessions_str
                ));
                session_save_path_set = true;
            }
            continue;
        }
        if trimmed.starts_with("session.save_path") {
            new_content.push_str(&format!(
                "session.save_path = \"{}\"\n",
                sessions_str
            ));
            session_save_path_set = true;
            continue;
        }

        if trimmed.starts_with("display_errors") && !trimmed.starts_with(';') {
            new_content.push_str("display_errors = Off\n");
            display_errors_set = true;
            continue;
        }
        if trimmed.starts_with("log_errors") && !trimmed.starts_with(';') {
            new_content.push_str("log_errors = On\n");
            log_errors_set = true;
            continue;
        }
        if trimmed.starts_with("error_log") && !trimmed.starts_with(';') {
            new_content.push_str(&format!("error_log = \"{}\"\n", error_log));
            error_log_set = true;
            continue;
        }

        if trimmed.starts_with(';') && trimmed.contains("extension_dir") && trimmed.contains("ext") {
            new_content.push_str(line);
            new_content.push('\n');
            if !extension_dir_set {
                new_content.push_str(&format!("extension_dir = \"{}\"\n", ext_dir_str));
                extension_dir_set = true;
            }
            continue;
        }
        if trimmed.starts_with("extension_dir") {
            new_content.push_str(&format!("extension_dir = \"{}\"\n", ext_dir_str));
            extension_dir_set = true;
            continue;
        }

        // PHP 8.4 deprecation: these directives emit a startup deprecation
        // notice, which (because html_errors defaults to On) produces a
        // "<br />" before the CGI headers - corrupting every CGI response
        // with "Bad header: <br />" and a 500 to the client.  Comment them
        // out so PHP uses the modern defaults.
        if (trimmed.starts_with("session.sid_length")
            || trimmed.starts_with("session.sid_bits_per_character"))
            && !trimmed.starts_with(';')
        {
            new_content.push_str("; ");
            new_content.push_str(line);
            new_content.push_str("  ; commented out by LinuxDevStackBox (deprecated in PHP 8.4)\n");
            continue;
        }

        // Exclude E_DEPRECATED / E_STRICT from error_reporting. phpMyAdmin
        // 5.x triggers thousands of deprecation notices on PHP 8.4, which
        // under CGI both corrupt headers and blow past max_execution_time.
        if trimmed.starts_with("error_reporting") && !trimmed.starts_with(';') {
            new_content.push_str("error_reporting = E_ALL & ~E_DEPRECATED & ~E_STRICT\n");
            continue;
        }

        // html_errors = On wraps every error in HTML <br />, which corrupts
        // CGI responses (Apache reports "Bad header: <br />" and returns 500).
        if trimmed.starts_with("html_errors") && !trimmed.starts_with(';') {
            new_content.push_str("html_errors = Off\n");
            continue;
        }

        new_content.push_str(line);
        new_content.push('\n');
    }

    if !session_save_path_set {
        new_content.push_str(&format!(
            "\n; Added by LinuxDevStackBox\nsession.save_path = \"{}\"\n",
            sessions_str
        ));
    }
    if !extension_dir_set {
        new_content.push_str(&format!(
            "\n; Added by LinuxDevStackBox\nextension_dir = \"{}\"\n",
            ext_dir_str
        ));
    }
    if !display_errors_set { new_content.push_str("\ndisplay_errors = Off\n"); }
    if !log_errors_set { new_content.push_str("log_errors = On\n"); }
    if !error_log_set { new_content.push_str(&format!("error_log = \"{}\"\n", error_log)); }

    // Enable essential extensions if not already active.
    if first_run { new_content = enable_essential_extensions(new_content, &ext_dir); }

    let new_content = temporary_defaults(&new_content, &to_apache_path(&crate::utils::paths::user_tmp_dir()));
    if content != new_content { let _ = std::fs::write(&php_ini, new_content); }
    if let Ok(content) = std::fs::read_to_string(&php_ini) {
        let tmp = crate::utils::paths::to_apache_path(&crate::utils::paths::user_tmp_dir());
        if let Some(migrated) =
            crate::utils::php_ini_import::maybe_migrate_php_import_limits(&content, &tmp)
        {
            let _ = std::fs::write(&php_ini, migrated);
        }
    }
    println!("php.ini patched at user config dir: session.save_path={} extension_dir={}", sessions_str, ext_dir_str);
}


#[tauri::command]
pub async fn get_php_versions() -> Result<Vec<PHPVersionInfo>, String> {
    let mut versions = Vec::new();

    if let Some(branch) = system_branch() {
        let php = get_php_binary()?;
        let installed_version = read_system_php_version();
        // Upstream end-of-life branches are flagged so users know the
        // distribution will eventually stop patching them.
        let end_of_life = match version_tuple(&installed_version.clone().unwrap_or_default()) {
            Some((8, 2, _)) => false, // security-supported until end of 2026
            Some((major, minor, _)) => (major, minor) < (8, 2),
            None => false,
        };
        versions.push(PHPVersionInfo {
            version: branch.clone(),
            security_update_required: false,
            installed_version,
            latest_version: None,
            update_available: false,
            end_of_life,
            release_error: None,
            status: "installed".to_string(),
            path: php.display().to_string(),
            is_active: true,
            installed: true,
            download_url: String::new(),
        });
    } else {
        versions.push(PHPVersionInfo {
            version: "system".to_string(),
            security_update_required: false,
            installed_version: None,
            latest_version: None,
            update_available: false,
            end_of_life: false,
            release_error: Some(
                "PHP is not installed. Install the php and php-cgi packages with your distribution's package manager.".to_string(),
            ),
            status: "missing".to_string(),
            path: String::new(),
            is_active: false,
            installed: false,
            download_url: String::new(),
        });
    }

    Ok(versions)
}

#[tauri::command]
pub async fn toggle_php() -> Result<bool, String> {
    // PHP doesn't start/stop like a service, just return a status.
    Ok(true)
}

#[tauri::command]
pub async fn open_php_terminal(version: String) -> Result<String, String> {
    let _php = get_php_binary()?;
    if !check_active_php_version(&version).await && version != "system" {
        return Err(format!("PHP {version} is not the installed system runtime"));
    }

    let ini = crate::utils::paths::active_php_ini();
    let workdir = crate::utils::paths::user_www_dir();
    // The inner command is kept free of user input; only fixed paths are used.
    let inner = format!(
        "export PHPRC='{}'; cd '{}'; echo 'PHP {} terminal ready. Type php -v to verify.'; php -a; exec sh",
        ini.display(),
        workdir.display(),
        version
    );

    // Terminal emulator candidates cover KDE Plasma (konsole), GNOME
    // (gnome-terminal), and common X11/Wayland terminals.
    let candidates: [(&str, Vec<String>); 5] = [
        ("konsole", vec!["-e".into()]),
        ("gnome-terminal", vec!["--".into()]),
        ("xfce4-terminal", vec!["-x".into()]),
        ("alacritty", vec!["-e".into()]),
        ("xterm", vec!["-e".into()]),
    ];
    for (terminal, prefix) in candidates {
        let Ok(path) = which(terminal) else { continue };
        let mut command = std::process::Command::new(path);
        command.args(prefix).args(["sh", "-c", &inner]);
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        if command.spawn().is_ok() {
            return Ok(format!("Opened PHP {version} terminal"));
        }
    }
    Err("No terminal emulator was found. Install konsole, gnome-terminal or xterm, or use the built-in Terminal page.".into())
}

fn which(name: &str) -> Result<std::path::PathBuf, String> {
    let path_env = std::env::var("PATH").unwrap_or_default();
    for dir in path_env.split(':') {
        if dir.is_empty() { continue; }
        let candidate = std::path::PathBuf::from(dir).join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("{name} not found in PATH"))
}

// -- PHP Extensions ----------------------------------------------------------

/// Resolve a potentially full version string ("8.3.31") to the installed
/// branch name ("8.3").
fn resolve_branch(version: &str) -> String {
    let parts: Vec<&str> = version.splitn(3, '.').collect();
    if parts.len() >= 2 {
        return format!("{}.{}", parts[0], parts[1]);
    }
    version.to_string()
}

/// Strip a single trailing semicolon-comment and surrounding whitespace from
/// the value side of an `extension=<name>` directive.
fn extract_ext_value(raw: &str) -> String {
    let no_comment = raw.split(';').next().unwrap_or("").trim();
    let value = no_comment.trim_matches('"').trim();
    if value.contains(['/', '\\']) { return String::new(); }
    let value = value.strip_suffix(".dll").or_else(|| value.strip_suffix(".so")).unwrap_or(value);
    let value = value.strip_prefix("php_").unwrap_or(value);
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return String::new();
    }
    value.to_string()
}

/// Returns Some(("extension"|"zend_extension", value, was_commented)) if the
/// trimmed line is an extension directive, otherwise None.
fn parse_ext_line(line: &str) -> Option<(&'static str, String, bool)> {
    let trimmed = line.trim_start();
    let (was_commented, rest) = if let Some(stripped) = trimmed.strip_prefix(';') {
        (true, stripped.trim_start())
    } else {
        (false, trimmed)
    };
    let (kind, after_kw) = if let Some(after) = rest.strip_prefix("zend_extension") {
        ("zend_extension", after)
    } else {
        let after = rest.strip_prefix("extension")?;
        ("extension", after)
    };
    let after_eq = after_kw.trim_start().strip_prefix('=')?;
    let value = extract_ext_value(after_eq);
    if value.is_empty() {
        return None;
    }
    Some((kind, value, was_commented))
}

#[tauri::command]
pub async fn list_php_extensions(version: String) -> Result<Vec<crate::types::PhpExtension>, String>
{
    use std::collections::BTreeMap;

    let _branch = resolve_branch(&version);
    let module_dir = php_module_dir()?;

    // 1. Discover modules shipped by the distribution.
    let mut map: BTreeMap<String, crate::types::PhpExtension> = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(&module_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if path.extension().and_then(|s| s.to_str()) != Some("so") {
                continue;
            }
            let name = stem.strip_prefix("php_").unwrap_or(stem).to_string();
            map.insert(
                name.clone(),
                crate::types::PhpExtension {
                    name,
                    enabled: false,
                    dll_present: true,
                },
            );
        }
    }

    let ini_path = crate::utils::paths::active_php_ini();
    if ini_path.exists() {
        let contents = fs::read_to_string(&ini_path).map_err(|e| e.to_string())?;
        for line in contents.lines() {
            if let Some((_kind, value, was_commented)) = parse_ext_line(line) {
                let entry = map
                    .entry(value.clone())
                    .or_insert_with(|| crate::types::PhpExtension {
                        name: value,
                        enabled: false,
                        dll_present: false,
                    });
                if !was_commented {
                    entry.enabled = true;
                }
            }
        }
    }

    Ok(map.into_values().collect())
}

#[tauri::command]
pub async fn toggle_php_extension(
    version: String,
    name: String,
    enable: bool,
) -> Result<bool, String> {
    let branch = resolve_branch(&version);
    if !check_active_php_version(&branch).await && version != "system" {
        return Err("Select this PHP version before changing its extensions.".into());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || name.is_empty() {
        return Err("Invalid extension name".into());
    }
    let ini_path = crate::utils::paths::active_php_ini();
    if !ini_path.exists() {
        return Err(format!("php.ini not found for PHP {}", version));
    }

    let contents = fs::read_to_string(&ini_path).map_err(|e| e.to_string())?;
    let mut out_lines: Vec<String> = Vec::with_capacity(contents.lines().count() + 1);
    let mut found = false;

    for line in contents.lines() {
        if let Some((kind, value, was_commented)) = parse_ext_line(line) {
            if value.eq_ignore_ascii_case(&name) {
                found = true;
                let new_line = if enable {
                    if was_commented {
                        format!("{}={}", kind, value)
                    } else {
                        line.to_string()
                    }
                } else if was_commented {
                    line.to_string()
                } else {
                    format!(";{}={}", kind, value)
                };
                out_lines.push(new_line);
                continue;
            }
        }
        out_lines.push(line.to_string());
    }

    if !found && enable {
        // No existing directive to toggle; append a fresh one. Since PHP 7.4
        // a bare name resolves against extension_dir, so no path is needed.
        if !out_lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            out_lines.push(String::new());
        }
        out_lines.push(format!("extension={}", name));
    }

    let mut joined = out_lines.join("\n");
    if contents.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    fs::write(&ini_path, joined).map_err(|e| e.to_string())?;
    Ok(true)
}


#[cfg(test)]
mod php_tests {
    use super::{is_at_least, parse_ext_line, version_tuple};
    #[test]
    fn version_ordering_and_floor() {
        assert!(is_at_least("8.2.35", (8, 2, 0)));
        assert!(is_at_least("8.4.12", (8, 2, 0)));
        assert!(!is_at_least("8.1.34", (8, 2, 0)));
        assert!(!is_at_least("7.4.33", (8, 2, 0)));
        assert!(version_tuple("8.4.12") > version_tuple("8.4.9"));
        assert_eq!(version_tuple("8.4"), None);
    }

    #[test]
    fn parses_only_supported_extension_names() {
        assert_eq!(
            parse_ext_line("extension=mysqli"),
            Some(("extension", "mysqli".to_string(), false))
        );
        assert_eq!(
            parse_ext_line("extension=mysqli.so"),
            Some(("extension", "mysqli".to_string(), false))
        );
        assert_eq!(
            parse_ext_line("; extension=redis.so"),
            Some(("extension", "redis".to_string(), true))
        );
        assert_eq!(
            parse_ext_line("; extension=/usr/lib/php/modules/mysqli.so"),
            None
        );
    }
}
