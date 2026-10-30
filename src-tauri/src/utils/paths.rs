// Path resolution helpers.
//
// Two distinct roots, following the XDG Base Directory Specification:
//   - Config root ($XDG_CONFIG_HOME/linuxdevstackbox): generated service
//     configuration the user is expected to edit (httpd.conf, my.cnf, php.ini,
//     vhost and HTTPS state).
//   - Data root   ($XDG_DATA_HOME/linuxdevstackbox): mutable runtime data
//     (logs, MySQL data, www projects, backups, the phpMyAdmin copy).
// Override both with the `LINUXDEVSTACKBOX_DATA_DIR` environment variable.
//
// Server binaries (httpd, mysqld/mariadbd, php, php-cgi and friends) are NOT
// bundled; they are resolved from the distribution packages described in
// README.md.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const APP_ID: &str = "linuxdevstackbox";

fn home_dir() -> PathBuf {
    // $HOME is set for every interactive Linux session; fall back for safety.
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

// Resolves the resource dir that ships static application assets (phpMyAdmin,
// templates). Debug builds use the repository root; release builds look next
// to the executable (local builds) and in /usr/lib/linuxdevstackbox (packages).
pub fn get_installation_path() -> PathBuf {
    #[cfg(debug_assertions)]
    {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("Cargo manifest has a parent")
            .to_path_buf()
    }
    #[cfg(not(debug_assertions))]
    {
        let exe_dir = env::current_exe()
            .expect("Cannot locate LinuxDevStackBox executable")
            .parent()
            .expect("Executable has a parent")
            .to_path_buf();
        let mut candidates = vec![
            exe_dir.clone(),
            PathBuf::from("/usr/lib").join(APP_ID),
            // The Tauri deb bundle derives its resource directory from the
            // product name, so accept the capitalized layout too.
            PathBuf::from("/usr/lib/LinuxDevStackBox"),
            exe_dir
                .parent()
                .map(|p| p.join("lib").join(APP_ID))
                .unwrap_or_else(|| exe_dir.clone()),
        ];
        candidates.dedup();
        candidates
            .into_iter()
            .find(|dir| dir.join("phpmyadmin").is_dir())
            .unwrap_or(exe_dir)
    }
}

// =====================================================================
// XDG user roots
// =====================================================================

pub fn get_user_data_root() -> PathBuf {
    if let Ok(custom) = env::var("LINUXDEVSTACKBOX_DATA_DIR") {
        let p = PathBuf::from(custom);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let base = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".local/share"));
    let p = base.join(APP_ID);
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn get_user_config_root() -> PathBuf {
    if let Ok(custom) = env::var("LINUXDEVSTACKBOX_DATA_DIR") {
        let p = PathBuf::from(custom).join("config");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(".config"));
    let p = base.join(APP_ID);
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn user_config_dir() -> PathBuf {
    get_user_config_root()
}

pub fn user_logs_dir() -> PathBuf {
    let p = get_user_data_root().join("logs");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Web root lives under the user data root so projects stay writable and
/// survive package upgrades.
pub fn user_www_dir() -> PathBuf {
    let p = get_user_data_root().join("www");
    let _ = std::fs::create_dir_all(&p);
    p
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

/// Converts a path to a forward-slash string safe for Apache config files.
/// Kept for template compatibility with the generated configuration files.
pub fn to_apache_path(p: &Path) -> String {
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
        "LinuxDevStackBox user data root: {}",
        get_user_data_root().display()
    );
}

// =====================================================================
// phpMyAdmin location
// =====================================================================

/// The read-only phpMyAdmin source shipped with the application.
pub fn phpmyadmin_source_dir() -> PathBuf {
    get_installation_path().join("phpmyadmin")
}

/// phpMyAdmin needs a writable config.inc.php and temp dirs, which cannot live
/// under a root-owned install prefix. The bundled copy is therefore cloned
/// into the user data root on first run; that copy is what Apache serves.
pub fn phpmyadmin_dir() -> PathBuf {
    let data_copy = get_user_data_root().join("phpmyadmin");
    if data_copy.join("index.php").is_file() {
        return data_copy;
    }
    let source = phpmyadmin_source_dir();
    if source.join("index.php").is_file() {
        return source;
    }
    data_copy
}

/// Clone the bundled phpMyAdmin into the user data root if it is not writable
/// where it was installed. Returns the directory that should be served.
pub fn ensure_phpmyadmin_copy() -> Result<PathBuf, String> {
    let source = phpmyadmin_source_dir();
    if !source.join("index.php").is_file() {
        return Err(format!(
            "phpMyAdmin is not installed at {}. The package may be broken; reinstall linuxdevstackbox.",
            source.display()
        ));
    }
    let config_target = source.join("config.inc.php");
    if is_writable_file(config_target) {
        return Ok(source);
    }
    let target = get_user_data_root().join("phpmyadmin");
    if !target.join("index.php").is_file() {
        copy_dir_recursive(&source, &target)?;
        println!("phpMyAdmin copied to {}", target.display());
    }
    Ok(target)
}

fn is_writable_file(path: PathBuf) -> bool {
    if !path.is_file() {
        return false;
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .is_ok()
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(target).map_err(|e| e.to_string())?;
    for entry in
        std::fs::read_dir(source).map_err(|e| format!("Cannot read {}: {e}", source.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_type = entry.file_type().map_err(|e| e.to_string())?;
        let dest = target.join(entry.file_name());
        if entry_type.is_dir() {
            // Runtime scratch dirs are regenerated by phpMyAdmin itself.
            if matches!(entry.file_name().to_str(), Some("tmp") | Some("upload") | Some("save")) {
                std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
                continue;
            }
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest).map_err(|e| {
                format!("Cannot copy {}: {e}", entry.path().display())
            })?;
        }
    }
    Ok(())
}

// =====================================================================
// Distribution binary resolution
// =====================================================================

const BIN_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

/// Look a command name up in PATH and the standard binary directories.
fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    if let Some(path_var) = env::var_os("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    BIN_DIRS
        .iter()
        .map(|dir| PathBuf::from(dir).join(name))
        .find(|candidate| candidate.is_file())
}

fn install_hint(apache: &str, mysql: &str, php: &str) -> String {
    format!(
        "Install the distribution packages first:\n\
         - Arch: sudo pacman -S {apache} {mysql} {php}\n\
         - Debian/Ubuntu: sudo apt install apache2 mariadb-server php php-cgi\n\
         - Fedora: sudo dnf install httpd mariadb-server php php-cgi"
    )
}

/// Resolve the Apache HTTP server binary (`httpd` on Arch/Fedora,
/// `apache2` on Debian). Error message lists per-distro install commands.
pub fn get_apache_binary() -> Result<PathBuf, String> {
    which("httpd")
        .or_else(|| which("apache2"))
        .ok_or_else(|| {
            format!(
                "The Apache HTTP server was not found. {}",
                install_hint("apache", "mariadb", "php php-cgi")
            )
        })
}

/// MariaDB and MySQL share a protocol but differ in tooling and data-dir
/// initialization. The flavor is decided once, from the server binary name or
/// its --version banner, and cached for the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MysqlFlavor {
    MariaDb,
    MySql,
}

impl MysqlFlavor {
    pub fn label(self) -> &'static str {
        match self {
            MysqlFlavor::MariaDb => "MariaDB",
            MysqlFlavor::MySql => "MySQL",
        }
    }
}

fn detect_mysql_flavor(server: &Path) -> MysqlFlavor {
    let name = server
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name.contains("mariadb") {
        return MysqlFlavor::MariaDb;
    }
    if let Ok(output) = std::process::Command::new(server)
        .arg("--version")
        .output()
    {
        if String::from_utf8_lossy(&output.stdout).contains("MariaDB") {
            return MysqlFlavor::MariaDb;
        }
    }
    MysqlFlavor::MySql
}

/// Resolve the MySQL/MariaDB server binary and its flavor.
pub fn get_mysqld_binary() -> Result<(PathBuf, MysqlFlavor), String> {
    for name in ["mysqld", "mariadbd", "mariadbd-safe"] {
        if let Some(path) = which(name) {
            let flavor = detect_mysql_flavor(&path);
            return Ok((path, flavor));
        }
    }
    Err(format!(
        "The MySQL/MariaDB server was not found. {}",
        install_hint("apache", "mariadb", "php php-cgi")
    ))
}

fn mysql_flavor() -> MysqlFlavor {
    static FLAVOR: OnceLock<MysqlFlavor> = OnceLock::new();
    *FLAVOR.get_or_init(|| match get_mysqld_binary() {
        Ok((_, flavor)) => flavor,
        Err(_) => MysqlFlavor::MySql,
    })
}

/// Resolve a MySQL client tool by generic and MariaDB-specific names.
pub fn get_mysql_tool(names: &[&str]) -> Result<PathBuf, String> {
    names
        .iter()
        .find_map(|name| which(name))
        .ok_or_else(|| {
            format!(
                "The {} client tool was not found. {}",
                names[0],
                install_hint("apache", "mariadb", "php php-cgi")
            )
        })
}

pub fn get_mysql_client_binary() -> Result<PathBuf, String> {
    get_mysql_tool(&["mysql", "mariadb"])
}

pub fn get_mysqldump_binary() -> Result<PathBuf, String> {
    get_mysql_tool(&["mysqldump", "mariadb-dump"])
}

pub fn get_mysqladmin_binary() -> Result<PathBuf, String> {
    get_mysql_tool(&["mysqladmin", "mariadb-admin"])
}

/// Data-dir initialization tool: MariaDB ships mariadb-install-db, MySQL uses
/// the server binary itself with --initialize-insecure.
pub fn get_mysql_install_binary() -> Result<(PathBuf, MysqlFlavor), String> {
    let flavor = mysql_flavor();
    if flavor == MysqlFlavor::MariaDb {
        if let Some(path) = which("mariadb-install-db").or_else(|| which("mysql-install-db")) {
            return Ok((path, flavor));
        }
    }
    get_mysqld_binary()
}

/// Resolve the PHP CLI binary.
pub fn get_php_binary() -> Result<PathBuf, String> {
    which("php").ok_or_else(|| {
        format!(
            "PHP was not found. {}",
            install_hint("apache", "mariadb", "php")
        )
    })
}

/// Resolve the PHP CGI binary used as the Apache handler.
pub fn get_php_cgi_binary() -> Result<PathBuf, String> {
    which("php-cgi").ok_or_else(|| {
        format!(
            "php-cgi was not found. {}",
            install_hint("apache", "mariadb", "php php-cgi")
        )
    })
}

pub fn get_openssl_binary() -> Result<PathBuf, String> {
    which("openssl").ok_or_else(|| "The openssl command-line tool was not found.".to_string())
}

// The active PHP installation directory. PHP comes from the distribution, so
// this is simply the directory that contains the php-cgi binary.
pub fn active_php_dir() -> PathBuf {
    get_php_cgi_binary()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("/usr/bin"))
}

/// Directory holding loadable PHP modules (`.so`), as configured in the
/// distribution PHP build.
pub fn php_module_dir() -> Result<PathBuf, String> {
    let php = get_php_binary()?;
    let output = std::process::Command::new(&php)
        .args(["-r", "echo ini_get('extension_dir');"])
        .output()
        .map_err(|e| format!("Failed to query PHP extension dir: {e}"))?;
    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !dir.is_empty() {
        return Ok(PathBuf::from(dir));
    }
    for candidate in [
        "/usr/lib/php/modules",
        "/usr/lib64/php/modules",
        "/usr/lib/php",
    ] {
        let p = PathBuf::from(candidate);
        if p.is_dir() {
            return Ok(p);
        }
    }
    Err("Could not determine the PHP extension directory.".into())
}

/// The distribution php.ini template used to seed the user configuration.
pub fn php_ini_template() -> Option<PathBuf> {
    [
        "/etc/php/php.ini",             // Arch
        "/etc/php.ini",                 // Fedora/RHEL
        "/etc/php/cgi/php.ini",         // Debian/Ubuntu (CGI SAPI)
        "/etc/php/8.4/cgi/php.ini",
        "/etc/php/8.3/cgi/php.ini",
        "/etc/php/8.2/cgi/php.ini",
        "/etc/php/cli/php.ini",
        "/etc/php/8.4/cli/php.ini",
        "/etc/php/8.3/cli/php.ini",
        "/etc/php/8.2/cli/php.ini",
    ]
    .into_iter()
    .find(|p| Path::new(p).is_file())
    .map(PathBuf::from)
}

pub fn active_php_ini() -> PathBuf {
    user_config_dir().join("php.ini")
}
