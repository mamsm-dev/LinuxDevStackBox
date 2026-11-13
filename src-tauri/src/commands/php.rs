// PHP commands (Linux).
//
// PHP versions come from two places:
//   1. bundled builds:  <install>/php/<ver>/{php-cgi,php} (or bin/ below it)
//   2. system packages: Debian/Ubuntu (Ondrej) /usr/bin/php-cgi8.3,
//      /usr/lib/cgi-bin/php8.3, Fedora/Remi /opt/remi/php83/root/usr/bin,
//      Arch/Alpine/generic /usr/bin/php-cgi.
// Every detected version gets a runtime directory
// `<data>/php/<ver>/` containing `php-cgi`, `php` and `ext` symlinks, and
// `<data>/php/current` points at the active one. Apache uses
// `<data>/php/current/php-cgi`, so switching versions is an atomic symlink swap.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::types::{PHPVersionInfo, ServiceInfo};
use crate::utils::paths::{
    find_in_path, get_installation_path, php_cgi_in, php_cli_in, to_apache_path, user_config_dir,
    user_php_runtime_root, user_sessions_dir,
};
use crate::utils::process::create_hidden_command;

// Branches we surface in the UI.
const PHP_BRANCHES: &[&str] = &["8.2", "8.3", "8.4", "8.5"];

// Preferred default when nothing is selected yet; also the version a flat
// bundled layout (php/php-cgi) is treated as.
const BUNDLED_PHP_VERSION: &str = "8.3";
pub(crate) static PHP_RUNTIME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// Minimum upstream fixes. Only enforced for bundled builds: distribution
// packages backport security fixes without changing the upstream version.
fn security_minimum(branch: &str) -> Option<&'static str> {
    match branch {
        "8.2" => Some("8.2.35"),
        "8.3" => Some("8.3.36"),
        "8.4" => Some("8.4.27"),
        "8.5" => Some("8.5.12"),
        _ => None,
    }
}

fn release_tuple(version: &str) -> Option<(u32, u32, u32, u32)> {
    let (base, revision) = match version.split_once('-') {
        Some((base, revision)) => (base, revision),
        None => (version, "0"),
    };
    let number = |value: &str| -> Option<u32> {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        value.parse().ok()
    };
    let parts: Vec<_> = base.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((number(parts[0])?, number(parts[1])?, number(parts[2])?, number(revision)?))
}

fn version_tuple(version: &str) -> Option<(u32, u32, u32)> {
    release_tuple(version).map(|(major, minor, patch, _)| (major, minor, patch))
}

fn meets_security_minimum(branch: &str, version: &str) -> bool {
    match (security_minimum(branch).and_then(version_tuple), version_tuple(version)) {
        (Some(min), Some(actual)) => actual.0 == min.0 && actual.1 == min.1 && actual >= min,
        _ => false,
    }
}

/// Parses `PHP 8.3.6-0ubuntu0.24.04.5 (cli) ...` into `8.3.6`.
fn parse_php_version(output: &str) -> Option<String> {
    let token = output.lines().next()?.split_whitespace().nth(1)?;
    let version: String = token.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    version_tuple(&version).map(|_| version)
}

/// Works for both `php` and `php-cgi` (`-v` is supported by both).
fn read_runtime_version(exe: &Path) -> Option<String> {
    let output = create_hidden_command(&exe.to_string_lossy()).args(["-n", "-v"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_php_version(&String::from_utf8_lossy(&output.stdout))
}

fn branch_of(version: &str) -> Option<String> {
    let (major, minor, _) = version_tuple(version)?;
    Some(format!("{major}.{minor}"))
}

fn is_exec(p: &Path) -> bool {
    fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

fn first_exec(candidates: Vec<String>) -> Option<PathBuf> {
    candidates.into_iter().map(PathBuf::from).find(|p| is_exec(p))
}

fn php_root() -> PathBuf {
    get_installation_path().join("php")
}

fn runtime_dir(version: &str) -> PathBuf {
    user_php_runtime_root().join(version)
}

fn is_bundled_path(p: &Path) -> bool {
    let base = fs::canonicalize(get_installation_path()).unwrap_or_else(|_| get_installation_path());
    fs::canonicalize(p).map(|c| c.starts_with(&base)).unwrap_or(false)
}

// -- distro helpers ---------------------------------------------------------

fn install_hint(branch: &str) -> String {
    let n = branch.replace('.', "");
    let release = fs::read_to_string("/etc/os-release").unwrap_or_default().to_lowercase();
    let has = |id: &str| release.lines().any(|l| (l.starts_with("id=") || l.starts_with("id_like=")) && l.contains(id));
    if has("debian") || has("ubuntu") {
        format!("Install it with: sudo apt install php{branch}-cgi php{branch}-cli (older releases may need the ondrej/php PPA, https://deb.sury.org/).")
    } else if has("fedora") || has("rhel") || has("centos") {
        format!("Install it with: sudo dnf install php{n}-php-cgi php{n}-php-cli (Remi repository, https://rpms.remirepo.net/) or sudo dnf install php-cgi php-cli for the distribution default.")
    } else if has("arch") {
        "Install it with: sudo pacman -S php php-cgi (other branches: AUR php82/php83 packages).".to_string()
    } else if has("suse") {
        format!("Install it with: sudo zypper install php{} php{}-cgi", &n[..1], &n[..1])
    } else if has("alpine") {
        format!("Install it with: apk add php{n} php{n}-cgi")
    } else {
        format!("Install PHP {branch} with CGI support (php-cgi and php CLI) using your distribution's package manager.")
    }
}

fn install_url() -> &'static str {
    let release = fs::read_to_string("/etc/os-release").unwrap_or_default().to_lowercase();
    if release.contains("debian") || release.contains("ubuntu") {
        "https://deb.sury.org/"
    } else if release.contains("fedora") || release.contains("rhel") || release.contains("centos") {
        "https://rpms.remirepo.net/"
    } else {
        "https://www.php.net/downloads"
    }
}

// -- discovery --------------------------------------------------------------

#[derive(Clone, Debug)]
struct PhpInstall {
    cgi: PathBuf,
    cli: Option<PathBuf>,
}

fn bundled_install(branch: &str) -> Option<PhpInstall> {
    let root = php_root();
    let mut dirs = vec![root.join(branch)];
    if branch == BUNDLED_PHP_VERSION {
        dirs.push(root.clone());
    }
    for dir in dirs {
        let cgi = php_cgi_in(&dir);
        if is_exec(&cgi) {
            let cli = php_cli_in(&dir);
            return Some(PhpInstall { cgi, cli: is_exec(&cli).then_some(cli) });
        }
    }
    None
}

fn system_install(branch: &str) -> Option<PhpInstall> {
    let n = branch.replace('.', "");
    let remi = format!("/opt/remi/php{n}/root/usr/bin");
    let cgi = first_exec(vec![
        format!("/usr/bin/php-cgi{branch}"),
        format!("/usr/lib/cgi-bin/php{branch}"),
        format!("/usr/bin/php-cgi{n}"),
        format!("/usr/bin/php{n}-cgi"),
        format!("/usr/local/bin/php-cgi{branch}"),
        format!("{remi}/php-cgi"),
    ])?;
    let cli = first_exec(vec![
        format!("/usr/bin/php{branch}"),
        format!("/usr/bin/php{n}"),
        format!("/usr/local/bin/php{branch}"),
        format!("{remi}/php"),
    ]);
    Some(PhpInstall { cgi, cli })
}

/// Unversioned `php-cgi` / `php` from PATH (Arch, Fedora default, Alpine, ...).
fn generic_install() -> Option<(String, PhpInstall)> {
    let cgi = find_in_path("php-cgi")?;
    let branch = branch_of(&read_runtime_version(&cgi)?)?;
    if !PHP_BRANCHES.contains(&branch.as_str()) {
        return None;
    }
    let cli = find_in_path("php").filter(|p| {
        read_runtime_version(p).and_then(|v| branch_of(&v)).as_deref() == Some(branch.as_str())
    });
    Some((branch, PhpInstall { cgi, cli }))
}

fn discover_installs() -> BTreeMap<String, PhpInstall> {
    let mut found = BTreeMap::new();
    for branch in PHP_BRANCHES {
        if let Some(install) = bundled_install(branch).or_else(|| system_install(branch)) {
            found.insert((*branch).to_string(), install);
        }
    }
    if let Some((branch, install)) = generic_install() {
        found.entry(branch).or_insert(install);
    }
    found
}

fn set_symlink(target: &Path, link: &Path) -> Result<(), String> {
    if fs::read_link(link).map(|t| t == target).unwrap_or(false) {
        return Ok(());
    }
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.is_dir() => return Err(format!("{} is a directory", link.display())),
        Ok(_) => fs::remove_file(link).map_err(|e| e.to_string())?,
        Err(_) => {}
    }
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| format!("Cannot link {} -> {}: {e}", link.display(), target.display()))
}

fn config_extension_dir(branch: &str, install: &PhpInstall) -> Option<PathBuf> {
    // 1. Ask the CLI (ignoring every ini, so this is the compiled-in default).
    if let Some(cli) = &install.cli {
        if let Ok(out) = Command::new(cli).args(["-n", "-r", "echo ini_get('extension_dir');"]).output() {
            let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if out.status.success() && dir.is_dir() {
                return Some(dir);
            }
        }
    }
    // 2. php-config of the same branch.
    let n = branch.replace('.', "");
    for name in [format!("php-config{branch}"), format!("php-config{n}"), "php-config".to_string()] {
        let Some(cfg) = find_in_path(&name) else { continue };
        let version = Command::new(&cfg).arg("--version").output().ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        if !version.starts_with(&format!("{branch}.")) {
            continue;
        }
        if let Ok(out) = Command::new(&cfg).arg("--extension-dir").output() {
            let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if dir.is_dir() {
                return Some(dir);
            }
        }
    }
    // 3. Newest Debian-style /usr/lib/php/<api> directory containing modules.
    let mut apis: Vec<PathBuf> = fs::read_dir("/usr/lib/php").ok()?.flatten().map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit())))
        .filter(|p| p.join("mysqli.so").exists() || p.join("opcache.so").exists())
        .collect();
    apis.sort();
    apis.pop()
}

fn ensure_runtime_dir(branch: &str, install: &PhpInstall) -> Result<PathBuf, String> {
    let dir = runtime_dir(branch);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    set_symlink(&install.cgi, &dir.join("php-cgi"))?;
    match &install.cli {
        Some(cli) => set_symlink(cli, &dir.join("php"))?,
        None => {
            let _ = fs::remove_file(dir.join("php"));
        }
    }
    let ext_link = dir.join("ext");
    if !ext_link.exists() {
        if let Some(ext) = config_extension_dir(branch, install) {
            set_symlink(&ext, &ext_link)?;
        }
    }
    Ok(dir)
}

fn current_link() -> PathBuf {
    user_php_runtime_root().join("current")
}

fn current_branch() -> Option<String> {
    let target = fs::read_link(current_link()).ok()?;
    target.file_name().map(|n| n.to_string_lossy().into_owned())
}

fn point_current_to(branch: &str) -> Result<(), String> {
    let root = user_php_runtime_root();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let tmp = root.join(format!(".current-{stamp}"));
    std::os::unix::fs::symlink(runtime_dir(branch), &tmp).map_err(|e| e.to_string())?;
    // rename(2) over an existing symlink is atomic.
    fs::rename(&tmp, current_link()).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Could not switch PHP: {e}")
    })
}

/// Refreshes runtime dirs for all detected installs and makes sure `current`
/// points at a working version (keeping the user's choice when still valid).
fn sync_runtime_dirs() -> BTreeMap<String, PhpInstall> {
    let installs = discover_installs();
    for (branch, install) in &installs {
        if let Err(e) = ensure_runtime_dir(branch, install) {
            println!("PHP runtime {branch}: {e}");
        }
    }
    let valid = current_branch().is_some_and(|b| installs.contains_key(&b)) && is_exec(&current_link().join("php-cgi"));
    if !valid {
        let pick = if installs.contains_key(BUNDLED_PHP_VERSION) {
            Some(BUNDLED_PHP_VERSION.to_string())
        } else {
            installs.keys().next_back().cloned()
        };
        if let Some(branch) = pick {
            if let Err(e) = point_current_to(&branch) {
                println!("{e}");
            }
        }
    }
    installs
}

/// Runtime dir for `version`, syncing first when it does not exist yet.
fn php_branch_dir(version: &str) -> PathBuf {
    let dir = runtime_dir(version);
    if !is_exec(&dir.join("php-cgi")) {
        let _ = sync_runtime_dirs();
    }
    dir
}

/// Primary executable of a runtime dir: the CLI if present, else php-cgi.
fn php_branch_exe(version: &str) -> PathBuf {
    let dir = php_branch_dir(version);
    let cli = dir.join("php");
    if is_exec(&cli) { cli } else { dir.join("php-cgi") }
}

pub(crate) fn project_runtime(version: &str) -> Result<(PathBuf, PathBuf), String> {
    if !PHP_BRANCHES.contains(&version) {
        return Err("Choose a supported installed PHP version.".into());
    }
    let directory = php_branch_dir(version);
    let cgi = directory.join("php-cgi");
    if !is_exec(&cgi) {
        return Err(format!("PHP {version} is not installed completely. {}", install_hint(version)));
    }
    let runtime = read_runtime_version(&cgi).ok_or_else(|| format!("PHP {version} cannot be executed. Reinstall it."))?;
    if is_bundled_path(&cgi) && !meets_security_minimum(version, &runtime) {
        return Err(format!("PHP {version} needs a supported security update. Update it in PHP Versions before assigning it to a project."));
    }
    Ok((directory, user_config_dir().join("php-runtimes").join(version).join("php.ini")))
}

/// Candidate php.ini templates, most specific first.
fn php_ini_templates(branch: &str, dir: &Path) -> Vec<PathBuf> {
    let n = branch.replace('.', "");
    let mut list = vec![
        php_root().join(branch).join("php.ini"),
        php_root().join(branch).join("php.ini-production"),
        php_root().join("php.ini"),
        dir.join("php.ini"),
        dir.join("php.ini-production"),
    ];
    for p in [
        format!("/usr/lib/php/{branch}/php.ini-production"),
        format!("/etc/php/{branch}/cgi/php.ini"),
        format!("/etc/php/{branch}/cli/php.ini"),
        format!("/etc/php{n}/php.ini"),
        format!("/etc/opt/remi/php{n}/php.ini"),
        "/usr/share/php/php.ini-production".to_string(),
        "/etc/php/php.ini".to_string(),
        "/etc/php.ini".to_string(),
    ] {
        list.push(PathBuf::from(p));
    }
    list
}

const MINIMAL_PHP_INI: &str = "; configVersion=1\nmemory_limit=256M\nupload_max_filesize=64M\npost_max_size=64M\nmax_execution_time=30\nmax_input_time=60\n";

fn template_content(branch: &str, dir: &Path) -> String {
    php_ini_templates(branch, dir)
        .into_iter()
        .find(|p| p.is_file())
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_else(|| MINIMAL_PHP_INI.to_string())
}

pub(crate) fn prepare_project_runtime(version: &str) -> Result<(), String> {
    let (directory, ini) = project_runtime(version)?;
    let runtime = read_runtime_version(&directory.join("php-cgi")).and_then(|v| branch_of(&v));
    if runtime.as_deref() != Some(version) {
        return Err(format!("The installed PHP {version} runtime failed validation. Reinstall it."));
    }
    let tmp = to_apache_path(&crate::utils::paths::user_tmp_dir());
    if ini.is_file() {
        let content = fs::read_to_string(&ini).map_err(|e| e.to_string())?;
        let updated = temporary_defaults(&content, &tmp);
        if content != updated {
            fs::write(&ini, updated).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let settings = template_content(version, &directory)
        .lines()
        .filter(|line| {
            let key = line.split('=').next().unwrap_or("").trim();
            !["extension_dir", "session.save_path", "error_log"].iter().any(|name| key.eq_ignore_ascii_case(name))
        })
        .collect::<Vec<_>>()
        .join("\n");
    let sessions = user_sessions_dir();
    fs::create_dir_all(&sessions).map_err(|e| e.to_string())?;
    let ext = directory.join("ext");
    let mut header = format!("; LinuxDevStackBox pinned PHP {version}\n");
    if ext.is_dir() {
        header.push_str(&format!("extension_dir=\"{}\"\n", to_apache_path(&ext)));
    }
    header.push_str(&format!(
        "session.save_path=\"{}\"\nerror_log=\"{}\"\n",
        to_apache_path(&sessions),
        to_apache_path(&crate::utils::paths::user_logs_dir().join(format!("php-{version}-error.log")))
    ));
    fs::create_dir_all(ini.parent().unwrap()).map_err(|e| e.to_string())?;
    let body = enable_essential_extensions(settings, &ext);
    fs::write(ini, temporary_defaults(&format!("{header}{body}"), &tmp)).map_err(|e| e.to_string())
}

pub(crate) fn validate_web_runtimes() -> Result<(), String> {
    let installs = sync_runtime_dirs();
    if installs.is_empty() {
        return Err(format!("No PHP installation with CGI support was found. {}", install_hint(BUNDLED_PHP_VERSION)));
    }
    let cgi = current_link().join("php-cgi");
    let version = read_runtime_version(&cgi).ok_or("Cannot read the active PHP version. Repair or switch PHP in PHP Versions.")?;
    let branch = branch_of(&version).ok_or("Invalid active PHP version")?;
    if is_bundled_path(&cgi) && !meets_security_minimum(&branch, &version) {
        return Err(format!("PHP {version} requires a security update before Apache can start. Open Services > PHP > Versions and update or activate a patched supported branch."));
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
    let version = get_current_php_version().await;
    Ok(ServiceInfo { running: version.is_some(), pid: None, port: None, version })
}

async fn get_current_php_version() -> Option<String> {
    let mut cgi = current_link().join("php-cgi");
    if !is_exec(&cgi) {
        let _ = sync_runtime_dirs();
        cgi = current_link().join("php-cgi");
    }
    if !is_exec(&cgi) {
        return None;
    }
    read_runtime_version(&cgi)
}

async fn check_active_php_version(version: &str) -> bool {
    if current_branch().is_none() {
        let _ = sync_runtime_dirs();
    }
    current_branch().is_some_and(|b| b == version)
}

async fn update_php_config(_version: &str) -> Result<(), String> {
    patch_php_ini();
    // httpd.conf points at <data>/php/current/php-cgi (a symlink); Apache picks
    // up a version switch on its next restart - no conf rewrite needed.
    Ok(())
}

use crate::utils::php_defaults::{enable_extensions as enable_essential_extensions, temporary_defaults};

// Ensures the active PHP's php.ini has the minimum settings required for
// LinuxDevStackBox to work (session path, extension_dir, essential extensions).
// Called by start_apache on every start.
//
// This writes to the USER CONFIG directory (~/.local/share/linuxdevstackbox/config/php.ini),
// never to the installation directory (read-only inside an AppImage). Callers
// that launch PHP must set PHPRC=<config dir> and PHP_INI_SCAN_DIR="" so the
// distribution's conf.d snippets do not interfere.
pub fn patch_php_ini() {
    let _ = sync_runtime_dirs();
    let branch = current_branch().unwrap_or_else(|| BUNDLED_PHP_VERSION.to_string());
    let php_dir = runtime_dir(&branch);

    // The user-specific php.ini lives in the user config dir.
    let php_ini = user_config_dir().join("php.ini");

    // Seed from the best template (bundled, distro production, or minimal).
    let first_run = !php_ini.exists();
    if first_run {
        if let Err(e) = std::fs::write(&php_ini, template_content(&branch, &php_dir)) {
            println!("Failed to create php.ini: {}", e);
            return;
        }
    }

    let content = match std::fs::read_to_string(&php_ini) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Absolute extension dir of the active runtime (symlink to the system dir).
    let ext_dir = php_dir.join("ext");
    let ext_dir_str = to_apache_path(&ext_dir);
    let ext_dir_known = ext_dir.is_dir();

    // Sessions go under the user data root so PHP CGI can always write there.
    let sessions_str = to_apache_path(&user_sessions_dir());
    let error_log = to_apache_path(&crate::utils::paths::user_logs_dir().join("php_error.log"));

    let mut new_content = String::with_capacity(content.len() + 256);
    let mut session_save_path_set = false;
    let mut extension_dir_set = !ext_dir_known;
    let mut display_errors_set = false;
    let mut log_errors_set = false;
    let mut error_log_set = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Replace commented-out or wrong session.save_path.
        if trimmed.starts_with(';') && trimmed.contains("session.save_path") {
            // Skip the comment line - we'll append the real value below.
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
            // Overwrite any existing value with the correct path.
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

        // Replace commented-out or wrong extension_dir with an absolute path
        // to the installation's ext/ folder.  Absolute is correct here because
        // this php.ini is in user_config_dir, not next to the PHP binaries.
        if ext_dir_known && trimmed.starts_with(';') && trimmed.contains("extension_dir") && trimmed.contains("ext") {
            new_content.push_str(line);
            new_content.push('\n');
            if !extension_dir_set {
                new_content.push_str(&format!("extension_dir = \"{}\"\n", ext_dir_str));
                extension_dir_set = true;
            }
            continue;
        }
        if ext_dir_known && trimmed.starts_with("extension_dir") {
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
        // 5.x triggers thousands of deprecation notices on PHP 8.4 (vendor
        // thecodingmachine/safe), which under CGI both corrupt headers and
        // blow past max_execution_time during autoload.  Same default that
        // XAMPP and WAMP use for production php.ini.
        if trimmed.starts_with("error_reporting") && !trimmed.starts_with(';') {
            new_content.push_str("error_reporting = E_ALL & ~E_DEPRECATED & ~E_STRICT\n");
            continue;
        }

        // html_errors = On wraps every error in HTML <br />, which corrupts
        // CGI responses (Apache reports "Bad header: <br />" and returns 500).
        // Plain-text errors still go to the log; the response stays clean.
        if trimmed.starts_with("html_errors") && !trimmed.starts_with(';') {
            new_content.push_str("html_errors = Off\n");
            continue;
        }

        new_content.push_str(line);
        new_content.push('\n');
    }

    // Append if the directives were never encountered.
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
    let installs = sync_runtime_dirs();
    let mut versions = Vec::new();

    for version in PHP_BRANCHES {
        let installed = installs.contains_key(*version);
        let dir = runtime_dir(version);
        let cgi = dir.join("php-cgi");
        let exe = if is_exec(&dir.join("php")) { dir.join("php") } else { cgi.clone() };
        let installed_version = if installed { read_runtime_version(&cgi) } else { None };
        // Distribution builds backport fixes, so only bundled builds are judged by version.
        let security_update_required = installed
            && is_bundled_path(&cgi)
            && !installed_version.as_deref().is_some_and(|v| meets_security_minimum(version, v));
        let active = installed && check_active_php_version(version).await;

        versions.push(PHPVersionInfo {
            version: (*version).to_string(),
            installed_version,
            latest_version: None,
            update_available: false,
            security_update_required,
            end_of_life: security_minimum(version).is_none(),
            release_error: None,
            status: if installed { "installed".to_string() } else { "available".to_string() },
            path: exe.display().to_string(),
            is_active: active,
            installed,
            download_url: install_url().to_string(),
        });
    }

    Ok(versions)
}

#[tauri::command]
pub async fn switch_php_version(version: String) -> Result<bool, String> {
    let _guard = super::ssl::SITE_LOCK.lock().await;
    let _process_lock = super::ssl::lock_configuration()?;
    let _runtime_guard = PHP_RUNTIME_LOCK.try_lock().map_err(|_| "A PHP runtime operation is in progress.")?;
    if !PHP_BRANCHES.contains(&version.as_str()) {
        return Err("Unsupported PHP version".into());
    }
    let installs = sync_runtime_dirs();
    if !installs.contains_key(&version) {
        return Err(format!("PHP {version} is not installed. {}", install_hint(&version)));
    }
    let cgi = runtime_dir(&version).join("php-cgi");
    let runtime = read_runtime_version(&cgi).ok_or_else(|| format!("PHP {version} cannot be executed."))?;
    if is_bundled_path(&cgi) && !meets_security_minimum(&version, &runtime) {
        return Err("Update this PHP runtime to a supported security release before activating it.".into());
    }
    point_current_to(&version)?;
    update_php_config(&version).await?;
    Ok(true)
}

// ----------------------------------------------------------------------
// PHP installation.
// PHP is not downloaded on Linux: builds come from the distribution's package
// manager (or a bundled copy). This command re-detects installations and
// reports the exact package command when a branch is missing. Progress is
// still emitted on `php-download-progress` so the existing UI shows the result.
// ----------------------------------------------------------------------

#[derive(Serialize, Clone)]
struct PhpDownloadProgress {
    version: String,
    stage: String, // "resolving" | "complete" | "error"
    percent: u32,
    downloaded: u64,
    total: u64,
    message: Option<String>,
}

fn emit_progress(app: &AppHandle, payload: PhpDownloadProgress) {
    let _ = app.emit("php-download-progress", payload);
}

#[tauri::command]
pub async fn download_php_version(app: AppHandle, version: String) -> Result<bool, String> {
    let _runtime_guard = PHP_RUNTIME_LOCK.try_lock().map_err(|_| "A PHP runtime operation is in progress.")?;
    if !PHP_BRANCHES.contains(&version.as_str()) {
        return Err("Unsupported PHP version".into());
    }
    let progress = |stage: &str, percent: u32, message: String| PhpDownloadProgress {
        version: version.clone(),
        stage: stage.to_string(),
        percent,
        downloaded: 0,
        total: 0,
        message: Some(message),
    };
    emit_progress(&app, progress("resolving", 0, "Looking for installed PHP builds...".into()));

    let installs = sync_runtime_dirs();
    if installs.contains_key(&version) {
        emit_progress(&app, progress("complete", 100, format!("PHP {version} is installed. Update it with your package manager.")));
        return Ok(true);
    }
    let message = format!(
        "PHP {version} (with CGI support) is not installed on this system. {} Then press Refresh.",
        install_hint(&version)
    );
    emit_progress(&app, progress("error", 0, message.clone()));
    Err(message)
}

#[tauri::command]
pub async fn toggle_php() -> Result<bool, String> {
    // PHP doesn't start/stop like a service, just return a status.
    Ok(true)
}

#[tauri::command]
pub async fn open_php_terminal(version: String) -> Result<String, String> {
    if !PHP_BRANCHES.contains(&version.as_str()) {
        return Err("Unsupported PHP version".into());
    }
    let dir = php_branch_dir(&version);
    if !is_exec(&dir.join("php-cgi")) && !is_exec(&dir.join("php")) {
        return Err(format!("PHP {version} is not installed. {}", install_hint(&version)));
    }

    let shell = std::env::var("SHELL").ok().filter(|s| Path::new(s).is_file()).unwrap_or_else(|| "/bin/bash".to_string());
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap_or_default());
    // (terminal binary, arguments that introduce the command to run)
    let terminals: &[(&str, &[&str])] = &[
        ("x-terminal-emulator", &["-e"]),
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-x"]),
        ("mate-terminal", &["-x"]),
        ("lxterminal", &["-e"]),
        ("kitty", &[]),
        ("alacritty", &["-e"]),
        ("foot", &[]),
        ("xterm", &["-e"]),
    ];
    for (name, prefix) in terminals {
        let Some(binary) = find_in_path(name) else { continue };
        let spawned = Command::new(binary)
            .args(*prefix)
            .args(["env".to_string(), format!("PATH={path}"), format!("PHPRC={}", user_config_dir().display()), "PHP_INI_SCAN_DIR=".to_string(), shell.clone()])
            .current_dir(crate::utils::paths::user_www_dir())
            .spawn();
        if spawned.is_ok() {
            return Ok(format!("Opened PHP {} terminal", version));
        }
    }
    Err("No terminal emulator found. Use the built-in Terminal page instead.".to_string())
}

// -- PHP Extensions ----------------------------------------------------------

/// Resolve a potentially full version string ("8.3.31") to the runtime
/// branch directory name ("8.3").
fn resolve_branch(version: &str) -> String {
    if php_branch_dir(version).join("php-cgi").exists() {
        return version.to_string();
    }
    let parts: Vec<&str> = version.splitn(3, '.').collect();
    if parts.len() >= 2 {
        let mm = format!("{}.{}", parts[0], parts[1]);
        if php_branch_dir(&mm).join("php-cgi").exists() {
            return mm;
        }
    }
    if let Ok(entries) = std::fs::read_dir(user_php_runtime_root()) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name != "current" && !name.starts_with('.') && (version.starts_with(name) || name.starts_with(version)) {
                    return name.to_string();
                }
            }
        }
    }
    version.to_string()
}

/// Strip a single trailing semicolon-comment and surrounding whitespace from
/// the value side of an `extension=<name>` directive.
/// e.g. `mbstring      ; depends on iconv` -> `mbstring`.
fn extract_ext_value(raw: &str) -> String {
    let no_comment = raw.split(';').next().unwrap_or("").trim();
    // Some inis use `extension="curl"`; strip quotes.
    let value = no_comment.trim_matches('"').trim();
    // Accept PHP's extension names and DLL spellings, but reject paths and
    // sample placeholders such as `/path/to/extension/mysqli.so`.
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

/// Extensions compiled into the binary (`php -n -m`), lowercase.
fn builtin_modules(dir: &Path) -> Vec<String> {
    let exe = if is_exec(&dir.join("php")) { dir.join("php") } else { return Vec::new() };
    Command::new(exe).args(["-n", "-m"]).output().ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty() && !l.starts_with('[')).collect())
        .unwrap_or_default()
}

// Zend extensions are loaded with `zend_extension=`, not `extension=`.
fn extension_kind(name: &str) -> &'static str {
    if matches!(name, "opcache" | "xdebug") { "zend_extension" } else { "extension" }
}

#[tauri::command]
pub async fn list_php_extensions(version: String) -> Result<Vec<crate::types::PhpExtension>, String> {

    let branch = resolve_branch(&version);
    let branch_dir = php_branch_dir(&branch);
    if !branch_dir.join("php-cgi").exists() {
        return Err(format!("PHP {} is not installed", version));
    }

    // 1. Discover shared objects (<name>.so) shipped in the extension dir.
    let mut map: BTreeMap<String, crate::types::PhpExtension> = BTreeMap::new();
    if let Ok(entries) = fs::read_dir(branch_dir.join("ext")) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
            if path.extension().and_then(|s| s.to_str()) != Some("so") {
                continue;
            }
            let name = stem.strip_prefix("php_").unwrap_or(stem).to_string();
            map.insert(name.clone(), crate::types::PhpExtension { name, enabled: false, dll_present: true });
        }
    }

    // 2. Overlay php.ini directives. Names without a .so may be built in.
    let ini_path = crate::utils::paths::active_php_ini();
    if ini_path.exists() {
        let builtin = builtin_modules(&branch_dir);
        let contents = fs::read_to_string(&ini_path).map_err(|e| e.to_string())?;
        for line in contents.lines() {
            if let Some((_kind, value, was_commented)) = parse_ext_line(line) {
                let is_builtin = builtin.contains(&value.to_lowercase());
                let entry = map.entry(value.clone()).or_insert_with(|| crate::types::PhpExtension {
                    name: value,
                    enabled: false,
                    dll_present: is_builtin,
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
pub async fn toggle_php_extension(version: String, name: String, enable: bool) -> Result<bool, String> {
    let branch = resolve_branch(&version);
    if !check_active_php_version(&branch).await {
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
                    if was_commented { format!("{}={}", kind, value) } else { line.to_string() }
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
        // No existing directive to toggle; append one for extensions that ship
        // as .so files but were never listed in php.ini.
        if !out_lines.last().map(|l| l.is_empty()).unwrap_or(false) {
            out_lines.push(String::new());
        }
        out_lines.push(format!("{}={}", extension_kind(&name), name));
    }

    let mut joined = out_lines.join("\n");
    if contents.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    fs::write(&ini_path, joined).map_err(|e| e.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_distribution_version_banners() {
        assert_eq!(parse_php_version("PHP 8.3.6-0ubuntu0.24.04.5 (cli) (built: x)\nCopyright"), Some("8.3.6".into()));
        assert_eq!(parse_php_version("PHP 8.4.1 (cgi-fcgi) (built: x)"), Some("8.4.1".into()));
        assert_eq!(parse_php_version("not php"), None);
        assert_eq!(branch_of("8.3.6").as_deref(), Some("8.3"));
    }

    #[test]
    fn security_baselines_apply_to_exact_branches() {
        for (branch, fixed, old) in [("8.2", "8.2.35", "8.2.34"), ("8.3", "8.3.36", "8.3.35"), ("8.4", "8.4.27", "8.4.26"), ("8.5", "8.5.12", "8.5.11")] {
            assert!(meets_security_minimum(branch, fixed));
            assert!(!meets_security_minimum(branch, old));
            assert!(!meets_security_minimum(branch, "8.6.0"));
        }
        assert!(!meets_security_minimum("8.1", "8.1.34"));
        assert!(version_tuple("8.5.12") > version_tuple("8.5.9"));
        assert_eq!(version_tuple("8.5"), None);
        assert_eq!(PHP_BRANCHES, &["8.2", "8.3", "8.4", "8.5"]);
    }

    #[test]
    fn symlinks_are_created_and_retargeted() {
        let root = std::env::temp_dir().join(format!("lds-php-link-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let (a, b, link) = (root.join("a"), root.join("b"), root.join("link"));
        fs::write(&a, "").unwrap();
        fs::write(&b, "").unwrap();
        set_symlink(&a, &link).unwrap();
        set_symlink(&a, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), a);
        set_symlink(&b, &link).unwrap();
        assert_eq!(fs::read_link(&link).unwrap(), b);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn zend_extensions_use_their_own_directive() {
        assert_eq!(extension_kind("opcache"), "zend_extension");
        assert_eq!(extension_kind("mysqli"), "extension");
    }

    #[test]
    fn parses_only_supported_extension_names() {
        assert_eq!(parse_ext_line("extension=php_mysqli.dll"), Some(("extension", "mysqli".to_string(), false)));
        assert_eq!(parse_ext_line("extension=mysqli.so"), Some(("extension", "mysqli".to_string(), false)));
        assert_eq!(parse_ext_line("; extension=redis.so"), Some(("extension", "redis".to_string(), true)));
        assert_eq!(parse_ext_line("; extension=/path/to/extension/mysqli.so"), None);
    }
}
