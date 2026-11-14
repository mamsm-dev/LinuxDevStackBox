// Path resolution helpers (Linux).
//
// Three distinct roots:
//   - Installation path: where *bundled* resources live (apache/, mysql/, php/,
//     phpmyadmin/, config templates). Read-only when running from an AppImage
//     (squashfs), so user data must never live here.
//   - User data root:    where runtime data lives (configs, logs, databases,
//     generated Apache server root, PHP runtime dirs).
//     Default: $XDG_DATA_HOME/linuxdevstackbox (~/.local/share/linuxdevstackbox)
//     Override: env var `LINUXDEVSTACKBOX_DATA_DIR`.
//   - Web root:          ~/LinuxDevStackBox/www (override: `LINUXDEVSTACKBOX_WWW_DIR`).
//
// Server binaries (Apache, MySQL/MariaDB, PHP) are resolved in this order:
//   1. a bundled copy inside the installation path (apache/bin/httpd, ...),
//   2. the system installation found in $PATH or the usual sbin directories.

use std::env;
use std::path::{Path, PathBuf};

pub const APP_NAME: &str = "LinuxDevStackBox";
pub const APP_SLUG: &str = "linuxdevstackbox";

// Helper function to get the project root directory.
// Used by a few legacy helpers; prefer get_installation_path or get_user_data_root.
pub fn get_project_root() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    if current_dir.file_name().and_then(|name| name.to_str()) == Some("src-tauri") {
        Ok(current_dir.parent().unwrap_or(&current_dir).to_path_buf())
    } else {
        Ok(current_dir)
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

// Resolves the install dir that contains apache/, php/, mysql/, phpmyadmin/.
pub fn get_installation_path() -> PathBuf {
    if let Ok(custom) = env::var("LINUXDEVSTACKBOX_HOME") {
        let p = PathBuf::from(custom);
        if p.is_dir() {
            return p;
        }
    }
    // Debug builds use the repository checkout.
    #[cfg(debug_assertions)]
    {
        return PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Cargo manifest has a parent")
            .to_path_buf();
    }
    // Release builds resolve only beside their own executable. Tauri's
    // AppImage/deb bundles place resources in `<prefix>/lib/<productName>/`
    // while the executable lives in `<prefix>/bin/`.
    #[cfg(not(debug_assertions))]
    {
        let exe = env::current_exe().expect("Cannot locate LinuxDevStackBox executable");
        let exe_dir = exe.parent().expect("Executable has a parent").to_path_buf();
        if let Some(prefix) = exe_dir.parent() {
            for name in [APP_NAME, APP_SLUG] {
                let candidate = prefix.join("lib").join(name);
                if candidate.is_dir() {
                    return candidate;
                }
            }
        }
        exe_dir
    }
}

// =====================================================================
// User Data Root
// =====================================================================

pub fn get_user_data_root() -> PathBuf {
    let p = if let Ok(custom) = env::var("LINUXDEVSTACKBOX_DATA_DIR") {
        PathBuf::from(custom)
    } else if let Some(xdg) = env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(xdg).join(APP_SLUG)
    } else {
        home_dir().join(".local").join("share").join(APP_SLUG)
    };
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_config_dir() -> PathBuf {
    let p = get_user_data_root().join("config");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_logs_dir() -> PathBuf {
    let p = get_user_data_root().join("logs");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Web root: `~/LinuxDevStackBox/www` (visible and writable for the user, which
/// is impossible inside a read-only AppImage). On first creation it is seeded
/// from the bundled `www` directory if there is one.
pub fn user_www_dir() -> PathBuf {
    let p = match env::var("LINUXDEVSTACKBOX_WWW_DIR") {
        Ok(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => home_dir().join(APP_NAME).join("www"),
    };
    if !p.exists() {
        let _ = std::fs::create_dir_all(&p);
        let bundled = get_installation_path().join("www");
        if bundled.is_dir() {
            let _ = copy_dir_recursive(&bundled, &p);
        }
    }
    p
}

fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub fn user_mysql_data_dir() -> PathBuf {
    let p = get_user_data_root().join("mysql-data");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_tmp_dir() -> PathBuf {
    let p = get_user_data_root().join("tmp");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_sessions_dir() -> PathBuf {
    let p = user_tmp_dir().join("sessions");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Directory with per-PHP-version runtime dirs created by the app
/// (`<data>/php/<version>/` holding `php-cgi`, `php` symlinks, `ext` etc.).
pub fn user_php_runtime_root() -> PathBuf {
    let p = get_user_data_root().join("php");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Writable Apache "ServerRoot" used when Apache comes from the system
/// package (its own ServerRoot, e.g. /etc/apache2, is not writable/usable).
pub fn user_apache_root() -> PathBuf {
    let p = get_user_data_root().join("apache-root");
    let _ = std::fs::create_dir_all(&p);
    p
}

// Converts a path to a string safe for Apache config files.
pub fn to_apache_path(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

pub fn user_backups_dir() -> PathBuf {
    let p = get_user_data_root().join("backups");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_config_backups_dir() -> PathBuf {
    let p = get_user_data_root().join("config-backups");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn ensure_user_data_dirs() {
    let _ = user_config_dir();
    let _ = user_logs_dir();
    let _ = user_www_dir();
    let _ = user_mysql_data_dir();
    let _ = user_backups_dir();
    let _ = user_config_backups_dir();
    println!(
        "{APP_NAME} user data root: {}",
        get_user_data_root().display()
    );
}

// =====================================================================
// Executable lookup
// =====================================================================

const SYSTEM_BIN_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
    "/opt/lampp/bin",
];

fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Looks `name` up in $PATH plus the usual sbin directories (sbin is often
/// missing from a normal user's PATH, but that is where apache2/mysqld live).
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|v| env::split_paths(&v).collect())
        .unwrap_or_default();
    for d in SYSTEM_BIN_DIRS {
        let d = PathBuf::from(d);
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs.into_iter()
        .map(|d| d.join(name))
        .find(|c| is_executable_file(c))
}

/// First existing executable among bundled candidates, then system names.
fn resolve_binary(bundled: &[PathBuf], system_names: &[&str]) -> Option<PathBuf> {
    if let Some(found) = bundled.iter().find(|c| is_executable_file(c)) {
        return Some(found.clone());
    }
    system_names.iter().find_map(|n| find_in_path(n))
}

fn bundled_candidates(base: &Path, dir: &str, names: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for n in names {
        out.push(base.join(dir).join("bin").join(n));
        out.push(base.join(dir).join(n));
    }
    out
}

/// True when `exe` lives inside the bundled installation (not a system binary).
pub fn is_bundled_binary(base: &Path, exe: &Path) -> bool {
    exe.starts_with(base)
}

/// Apache binary (`httpd` / `apache2`). Falls back to a non-existent bundled
/// path so callers can still print a meaningful "not found" message.
pub fn get_apache_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &bundled_candidates(base, "apache", &["httpd", "apache2"]),
        &["apache2", "httpd"],
    )
    .unwrap_or_else(|| base.join("apache").join("bin").join("httpd"))
}

/// `mysqld` or `mariadbd`.
pub fn get_mysqld_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &bundled_candidates(base, "mysql", &["mysqld", "mariadbd"]),
        &["mysqld", "mariadbd"],
    )
    .unwrap_or_else(|| base.join("mysql").join("bin").join("mysqld"))
}

/// `mysql` / `mariadb` command line client.
pub fn get_mysql_client_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &bundled_candidates(base, "mysql", &["mysql", "mariadb"]),
        &["mysql", "mariadb"],
    )
    .unwrap_or_else(|| base.join("mysql").join("bin").join("mysql"))
}

/// `mysqldump` / `mariadb-dump`.
pub fn get_mysqldump_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &bundled_candidates(base, "mysql", &["mysqldump", "mariadb-dump"]),
        &["mysqldump", "mariadb-dump"],
    )
    .unwrap_or_else(|| base.join("mysql").join("bin").join("mysqldump"))
}

/// `mysqladmin` / `mariadb-admin`.
pub fn get_mysqladmin_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &bundled_candidates(base, "mysql", &["mysqladmin", "mariadb-admin"]),
        &["mysqladmin", "mariadb-admin"],
    )
    .unwrap_or_else(|| base.join("mysql").join("bin").join("mysqladmin"))
}

/// `mysql_install_db` (MariaDB data directory initialisation), if available.
pub fn get_mysql_install_db_exe(base: &Path) -> Option<PathBuf> {
    resolve_binary(
        &bundled_candidates(base, "mysql", &["mysql_install_db", "mariadb-install-db"]),
        &["mysql_install_db", "mariadb-install-db"],
    )
}

/// Directory containing the MySQL/MariaDB binaries.
pub fn get_mysql_bin_dir(base: &Path) -> PathBuf {
    get_mysqld_exe(base)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| base.join("mysql").join("bin"))
}

/// True when the server binary is MariaDB (different initialisation flow and
/// slightly different SQL for users/passwords than Oracle MySQL).
pub fn is_mariadb(mysqld: &Path) -> bool {
    if mysqld
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.contains("mariadb"))
        .unwrap_or(false)
    {
        return true;
    }
    crate::utils::process::host_command(mysqld)
        .arg("--version")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("mariadb"))
        .unwrap_or(false)
}

/// System/bundled `openssl` CLI.
pub fn get_openssl_exe(base: &Path) -> PathBuf {
    resolve_binary(
        &[base.join("apache").join("bin").join("openssl")],
        &["openssl"],
    )
    .unwrap_or_else(|| PathBuf::from("openssl"))
}

/// System `mkcert` or the bundled one in `tools/mkcert/`.
pub fn get_mkcert_exe(base: &Path) -> Option<PathBuf> {
    resolve_binary(&[base.join("tools").join("mkcert").join("mkcert")], &["mkcert"])
}

/// Directory with Apache's loadable modules (`mod_*.so`).
pub fn get_apache_modules_dir(base: &Path) -> Option<PathBuf> {
    let bundled = base.join("apache").join("modules");
    if bundled.join("mod_dir.so").exists() {
        return Some(bundled);
    }
    [
        "/usr/lib/apache2/modules",
        "/usr/lib64/apache2/modules",
        "/usr/lib/httpd/modules",
        "/usr/lib64/httpd/modules",
        "/usr/lib/x86_64-linux-gnu/apache2/modules",
        "/usr/local/apache2/modules",
        "/opt/lampp/modules",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.join("mod_dir.so").exists() || p.join("mod_mime.so").exists())
}

/// A `mime.types` file for Apache's `TypesConfig`.
pub fn get_mime_types_file(base: &Path) -> Option<PathBuf> {
    [
        base.join("apache").join("conf").join("mime.types"),
        PathBuf::from("/etc/apache2/mime.types"),
        PathBuf::from("/etc/httpd/conf/mime.types"),
        PathBuf::from("/etc/mime.types"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Apache `ServerRoot` to use. Bundled Apache uses its own directory; a system
/// Apache gets a generated, writable root with a `modules` symlink.
pub fn prepare_apache_server_root(base: &Path) -> Result<PathBuf, String> {
    let exe = get_apache_exe(base);
    if is_bundled_binary(base, &exe) {
        return Ok(base.join("apache"));
    }
    let root = user_apache_root();
    for sub in ["conf", "logs", "run"] {
        std::fs::create_dir_all(root.join(sub)).map_err(|e| e.to_string())?;
    }
    if let Some(modules) = get_apache_modules_dir(base) {
        let link = root.join("modules");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&modules, &link).map_err(|e| e.to_string())?;
    }
    if let Some(mime) = get_mime_types_file(base) {
        let link = root.join("conf").join("mime.types");
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&mime, &link).map_err(|e| e.to_string())?;
    }
    Ok(root)
}

// =====================================================================
// PHP runtime
// =====================================================================

/// File name of the PHP CGI binary inside a PHP runtime dir.
pub const PHP_CGI_NAME: &str = "php-cgi";
/// File name of the PHP CLI binary inside a PHP runtime dir.
pub const PHP_CLI_NAME: &str = "php";

fn php_dir_has_binary(dir: &Path) -> bool {
    is_executable_file(&dir.join(PHP_CGI_NAME))
        || is_executable_file(&dir.join(PHP_CLI_NAME))
        || is_executable_file(&dir.join("bin").join(PHP_CGI_NAME))
        || is_executable_file(&dir.join("bin").join(PHP_CLI_NAME))
}

/// Shared runtime resolution. Never resolves a PHP binary from PATH directly:
/// system PHP versions are exposed through runtime dirs created by the app
/// (see `commands/php.rs`).
pub fn active_php_dir() -> PathBuf {
    let bundled = get_installation_path().join("php");
    let runtime = user_php_runtime_root();
    for root in [&runtime, &bundled] {
        for candidate in [root.join("current"), root.join("8.3"), root.clone()] {
            if php_dir_has_binary(&candidate) {
                return candidate;
            }
        }
    }
    runtime.join("8.3")
}

/// `php-cgi` inside a PHP runtime dir (supports `<dir>/php-cgi` and `<dir>/bin/php-cgi`).
pub fn php_cgi_in(dir: &Path) -> PathBuf {
    let flat = dir.join(PHP_CGI_NAME);
    if flat.exists() { flat } else { dir.join("bin").join(PHP_CGI_NAME) }
}

/// `php` CLI inside a PHP runtime dir.
pub fn php_cli_in(dir: &Path) -> PathBuf {
    let flat = dir.join(PHP_CLI_NAME);
    if flat.exists() { flat } else { dir.join("bin").join(PHP_CLI_NAME) }
}

pub fn active_php_ini() -> PathBuf { user_config_dir().join("php.ini") }

/// True when the kernel lets unprivileged users bind ports < 1024
/// (net.ipv4.ip_unprivileged_port_start <= 80) or we are root.
pub fn can_bind_privileged_ports() -> bool {
    if env::var("USER").map(|u| u == "root").unwrap_or(false) {
        return true;
    }
    std::fs::read_to_string("/proc/sys/net/ipv4/ip_unprivileged_port_start")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .map(|p| p <= 80)
        .unwrap_or(false)
}
