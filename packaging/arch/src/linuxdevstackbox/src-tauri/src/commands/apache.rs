// Apache service commands.

use std::sync::OnceLock;
use std::time::Duration;
use tokio::time::sleep;

use crate::commands::php::patch_php_ini;
use crate::types::ServiceInfo;
use crate::utils::config_ports::apache_listen_port;
use crate::utils::config_version::needs_config_migration;
use crate::utils::paths::{
    get_apache_binary, get_php_cgi_binary, get_user_data_root, phpmyadmin_dir, to_apache_path,
    user_config_dir, user_logs_dir, user_www_dir,
};
use crate::utils::process::{
    create_hidden_command, create_service_command, ensure_port_available, find_our_processes,
    is_our_process_running, kill_pid,
};

pub const APACHE_CONFIG_VERSION: u32 = 10;

/// Parse ` -D HTTPD_ROOT="/etc/httpd"` from `httpd -V` output.
fn parse_httpd_root(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(rest) = line.trim().strip_prefix("-D HTTPD_ROOT=") {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

/// Resolved Apache runtime paths (binary, ServerRoot, modules dir, mime.types).
pub struct ApacheRuntime {
    pub binary: std::path::PathBuf,
    pub server_root: std::path::PathBuf,
    pub modules_dir: std::path::PathBuf,
    pub mime_types: std::path::PathBuf,
}

fn locate_modules_dir(server_root: &std::path::Path) -> Option<std::path::PathBuf> {
    [
        server_root.join("modules"),
        std::path::PathBuf::from("/usr/lib/httpd/modules"),
        std::path::PathBuf::from("/usr/lib64/httpd/modules"),
        std::path::PathBuf::from("/usr/lib/apache2/modules"),
    ]
    .into_iter()
    .find(|dir| dir.join("mod_dir.so").is_file())
}

fn locate_mime_types(server_root: &std::path::Path) -> Option<std::path::PathBuf> {
    [
        server_root.join("conf").join("mime.types"),
        server_root.join("mime.types"),
        std::path::PathBuf::from("/etc/httpd/conf/mime.types"),
        std::path::PathBuf::from("/etc/apache2/mime.types"),
        std::path::PathBuf::from("/etc/mime.types"),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

/// Resolve the Apache binary plus ServerRoot/modules/mime.types locations.
/// Cached: `httpd -V` is an external call and the layout never changes mid-run.
pub fn apache_runtime() -> Result<&'static ApacheRuntime, String> {
    static RUNTIME: OnceLock<Option<ApacheRuntime>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            let binary = get_apache_binary().ok()?;
            let output = create_hidden_command(&binary).arg("-V").output().ok()?;
            let text = String::from_utf8_lossy(&output.stdout).into_owned();
            let server_root = parse_httpd_root(&text)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| std::path::PathBuf::from("/etc/httpd"));
            let modules_dir = locate_modules_dir(&server_root)?;
            let mime_types = locate_mime_types(&server_root)?;
            Some(ApacheRuntime {
                binary,
                server_root,
                modules_dir,
                mime_types,
            })
        })
        .as_ref()
        .ok_or_else(|| {
            "The Apache HTTP server was not found. Install the distribution \
             package: `sudo pacman -S apache` (Arch), `sudo apt install apache2` \
             (Debian/Ubuntu) or `sudo dnf install httpd` (Fedora)."
                .to_string()
        })
}

/// Ensure httpd.conf exists and is migrated to the current template version.
pub async fn ensure_apache_config() -> Result<(), String> {
    let config_path = user_config_dir().join("httpd.conf");
    if !config_path.exists() {
        return create_default_apache_config().await;
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read Apache config: {}", e))?;
    if needs_config_migration(&content, APACHE_CONFIG_VERSION) {
        println!(
            "Detected outdated Apache config. Migrating to configVersion {}...",
            APACHE_CONFIG_VERSION
        );
        let backup = config_path.with_extension("pre-v10.conf");
        if !backup.exists() { std::fs::copy(&config_path, backup).map_err(|e| e.to_string())?; }
        let port = apache_listen_port();
        create_default_apache_config().await?;
        if port != 8080 {
            let generated = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            std::fs::write(&config_path, generated.replace(":8080", &format!(":{port}"))).map_err(|e| e.to_string())?;
        }
    }
    harden_directory_listing(&config_path)?;
    crate::utils::apache_overrides::migrate(&config_path, &apache_runtime()?.binary)
}

/// Disable automatic directory indexes in managed Apache directories while
/// preserving the rest of each active Options directive.
fn harden_directory_listing(config_path: &std::path::Path) -> Result<(), String> {
    let content = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read Apache config: {e}"))?;
    let mut changed = false;
    let lines: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#')
                || !trimmed
                    .get(..7)
                    .is_some_and(|s| s.eq_ignore_ascii_case("Options"))
            {
                return line.to_owned();
            }
            let mut parts = trimmed.split_whitespace();
            let Some(directive) = parts.next() else {
                return line.to_owned();
            };
            let options: Vec<&str> = parts.collect();
            if !options.iter().any(|option| {
                option
                    .trim_start_matches(['+', '-'])
                    .eq_ignore_ascii_case("Indexes")
            }) {
                return line.to_owned();
            }
            changed = true;
            // Apache requires every option in a directive to use an explicit
            // `+` or `-` once any option does. Keep the existing effective
            // behavior while disabling Indexes.
            let replacement = options
                .iter()
                .map(|option| {
                    if option
                        .trim_start_matches(['+', '-'])
                        .eq_ignore_ascii_case("Indexes")
                    {
                        "-Indexes".to_owned()
                    } else if option.starts_with('+') || option.starts_with('-') {
                        (*option).to_owned()
                    } else {
                        format!("+{option}")
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            let indent = &line[..line.len() - trimmed.len()];
            format!("{indent}{directive} {replacement}")
        })
        .collect();
    if changed {
        let mut updated = lines.join("\n");
        if content.ends_with('\n') {
            updated.push('\n');
        }
        std::fs::write(config_path, updated)
            .map_err(|e| format!("Failed to disable Apache directory indexes: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod apache_config_tests {
    use super::harden_directory_listing;

    #[test]
    fn disables_indexes_without_changing_other_options() {
        let path = std::env::temp_dir().join(format!(
            "lds-httpd-test-{}.conf",
            std::process::id()
        ));
        std::fs::write(
            &path,
            "# Options Indexes\n<Directory x>\n    Options Indexes FollowSymLinks\n</Directory>\n",
        )
        .unwrap();
        harden_directory_listing(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(content.contains("# Options Indexes"));
        assert!(content.contains("Options -Indexes +FollowSymLinks"));
    }
}

fn httpd_conf_path() -> std::path::PathBuf {
    user_config_dir().join("httpd.conf")
}

#[tauri::command]
pub async fn get_apache_status() -> Result<ServiceInfo, String> {
    let marker = httpd_conf_path();
    let our_pids = find_our_processes("httpd", &marker);
    let running = !our_pids.is_empty();
    let pid = our_pids.first().copied();

    let version = get_apache_version().await;

    Ok(ServiceInfo {
        running,
        pid,
        port: Some(apache_listen_port()),
        version,
    })
}

/// Directory holding a php-cgi symlink so the CGI handler is granted
/// execution only for that one file instead of an entire system binary dir.
fn php_cgi_staging_dir() -> std::path::PathBuf {
    get_user_data_root().join("cgi-bin")
}

pub(crate) fn stage_php_cgi() -> Result<std::path::PathBuf, String> {
    let php_cgi = get_php_cgi_binary()?;
    let dir = php_cgi_staging_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let link = dir.join("php-cgi");
    let target_changed = std::fs::read_link(&link)
        .map(|current| current != php_cgi)
        .unwrap_or(true);
    if target_changed || !link.exists() {
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&php_cgi, &link)
            .map_err(|e| format!("Cannot stage php-cgi handler: {e}"))?;
    }
    Ok(dir)
}

#[tauri::command]
pub async fn start_apache() -> Result<bool, String> {
    let _runtime_guard = super::php::PHP_RUNTIME_LOCK.try_lock()
        .map_err(|_| "Wait for the PHP update or version switch to finish before starting Apache.")?;

    let runtime = apache_runtime()?;
    let config_path = httpd_conf_path();

    if is_our_process_running("httpd", &config_path) {
        return Ok(true);
    }
    super::php::validate_web_runtimes()?;

    ensure_apache_config().await?;

    let config_path = httpd_conf_path();

    // If ssl.conf exists but references stale paths, delete it and immediately
    // regenerate it with correct paths so the user doesn't have to do anything.
    let ssl_conf_path = user_config_dir().join("ssl.conf");
    if ssl_conf_path.exists() {
        let has_stale_paths = std::fs::read_to_string(&ssl_conf_path)
            .map(|c| c.contains("//?/") || c.contains(r"\\?\"))
            .unwrap_or(false);
        if has_stale_paths {
            println!("ssl.conf contains stale paths - removing and regenerating ssl.conf");
            let _ = std::fs::remove_file(&ssl_conf_path);
        }
    }
    // If ssl.conf is absent but HTTPS is enabled (e.g. the file was lost),
    // regenerate it automatically so HTTPS comes back without user action.
    if !ssl_conf_path.exists() && crate::commands::ssl::https_enabled() {
        let entries = crate::commands::vhosts::load_vhosts()?;
        let config = crate::commands::ssl::render_ssl(&entries)?;
        std::fs::write(&ssl_conf_path, config).map_err(|e| e.to_string())?;
    }

    // Ensure php.ini has session.save_path and extension_dir set correctly
    // so phpMyAdmin and PHP apps work without manual php.ini editing.
    patch_php_ini();
    let _ = crate::utils::phpmyadmin_config::patch_phpmyadmin_config();

    // Seed the www dir with default files if it is still empty.
    seed_www_dir();

    let listen_port = apache_listen_port();
    // Fail fast if the configured listen port is taken.
    ensure_port_available(listen_port, "Apache")?;

    let php_cgi_dir = stage_php_cgi()?;

    let output = create_hidden_command(&runtime.binary)
        .arg("-f")
        .arg(&config_path)
        .arg("-t")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() { return Err(format!("Apache configuration validation failed ({}): {}", output.status, String::from_utf8_lossy(&output.stderr))); }

    // -DFOREGROUND keeps the parent as our child so start failure detection and
    // process-group shutdown work; the process group isolates the server tree.
    let mut child = match create_service_command(runtime.binary.as_os_str())
        .arg("-DFOREGROUND")
        .arg("-f")
        .arg(&config_path)
        .env("PHPRC", user_config_dir())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Err(format!("Failed to start Apache: {}", e)),
    };
    let _ = php_cgi_dir;

    println!("Apache started with PID: {}", child.id());

    sleep(Duration::from_secs(2)).await;

    match child.try_wait() {
        Ok(Some(status)) => {
            let _ = child;
            return Err(format!(
                "Apache exited immediately (code: {}). Check the Apache error log at {}.",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                user_logs_dir().join("error.log").display()
            ));
        }
        Ok(None) => {} // still running - good
        Err(e) => {
            eprintln!("Could not query Apache child status: {}", e);
        }
    }

    if is_our_process_running("httpd", &config_path)
        && std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], listen_port)),
            Duration::from_secs(2),
        )
        .is_ok()
    {
        Ok(true)
    } else {
        Err(format!("LinuxDevStackBox Apache did not become ready on port {listen_port}. Check the Apache error log at {}.", user_logs_dir().join("error.log").display()))
    }
}

#[tauri::command]
pub async fn stop_apache() -> Result<bool, String> {
    let _maintenance = super::log_management::MaintenanceAfterStop;
    let config_path = httpd_conf_path();

    if !is_our_process_running("httpd", &config_path) {
        return Ok(true);
    }

    if let Ok(runtime) = apache_runtime() {
        let _ = create_hidden_command(&runtime.binary)
            .args(["-f", config_path.to_str().unwrap_or(""), "-k", "stop"])
            .output();

        sleep(Duration::from_millis(2000)).await;
    }

    let mut errors: Vec<String> = Vec::new();
    for attempt in 0..4 {
        let pids = find_our_processes("httpd", &config_path);
        if pids.is_empty() {
            return Ok(true);
        }
        for pid in pids {
            if let Err(e) = kill_pid(pid) {
                errors.push(e);
            }
        }
        sleep(Duration::from_millis(if attempt == 0 { 1000 } else { 500 })).await;
    }

    if find_our_processes("httpd", &config_path).is_empty() {
        Ok(true)
    } else {
        Err(format!(
            "LinuxDevStackBox Apache processes are still running after multiple kill attempts.\n\n\
            Try:\n\
            1. Close any browser tabs accessing localhost\n\
            2. Run `pkill -f {}` in a terminal\n\n\
            Errors: {}",
            config_path.display(),
            errors.join("; ")
        ))
    }
}

pub async fn create_default_apache_config() -> Result<(), String> {
    let runtime = apache_runtime()?;
    let data_root = get_user_data_root();

    let server_root = to_apache_path(&runtime.server_root);
    let modules = to_apache_path(&runtime.modules_dir);
    let www_root = user_www_dir();
    let logs_root = user_logs_dir();
    let phpmyadmin_root = phpmyadmin_dir();

    let user_config = to_apache_path(&user_config_dir());
    let www_root_str = to_apache_path(&www_root);
    let logs_root_str = to_apache_path(&logs_root);
    let mime_types = to_apache_path(&runtime.mime_types);

    // The CGI handler lives behind a symlink in the user data dir so Apache
    // only grants execution for that single file.
    let php_cgi_dir = stage_php_cgi()?;
    let php_cgi_dir_str = to_apache_path(&php_cgi_dir);

    let config_content = format!(
        r#"{override_baseline}# configVersion: 10
# Apache Configuration for LinuxDevStackBox
# Managed by LinuxDevStackBox. Edits to this file are preserved across upgrades
# unless the configVersion is bumped, which triggers a migration.
ServerRoot "{server_root}"
PidFile "{logs_root_str}/httpd.pid"
Listen 127.0.0.1:8080

# Essential modules (distribution Apache ships all of these)
LoadModule mpm_prefork_module {modules}/mod_mpm_prefork.so
LoadModule unixd_module {modules}/mod_unixd.so
LoadModule dir_module {modules}/mod_dir.so
LoadModule mime_module {modules}/mod_mime.so
LoadModule rewrite_module {modules}/mod_rewrite.so
LoadModule authz_core_module {modules}/mod_authz_core.so
LoadModule authz_host_module {modules}/mod_authz_host.so
LoadModule access_compat_module {modules}/mod_access_compat.so
LoadModule log_config_module {modules}/mod_log_config.so
LoadModule cgi_module {modules}/mod_cgi.so
LoadModule alias_module {modules}/mod_alias.so
LoadModule actions_module {modules}/mod_actions.so
LoadModule headers_module {modules}/mod_headers.so
LoadModule env_module {modules}/mod_env.so
LoadModule setenvif_module {modules}/mod_setenvif.so

ServerName localhost
DocumentRoot "{www_root_str}"

<Directory "{www_root_str}">
    Options -Indexes +FollowSymLinks
    AllowOverride All
    Require all granted
    DirectoryIndex index.php index.html index.htm
</Directory>

# MIME Types
TypesConfig "{mime_types}"
AddType text/html .html .htm

# PHP CGI configuration
ScriptAlias /php/ "{php_cgi_dir_str}/"
Action php-script /php/php-cgi
AddHandler php-script .php
AddType application/x-httpd-php .php

# Tell PHP-CGI where to find its php.ini.  The user-config php.ini is written
# to the XDG config dir so it never pollutes system configuration.
SetEnv PHPRC "{user_config}/"

<Directory "{php_cgi_dir_str}">
    AllowOverride None
    Options ExecCGI
    Require all granted
</Directory>

# Error and Access logs
ErrorLog "{logs_root_str}/error.log"
CustomLog "{logs_root_str}/access.log" common

# Security
ServerTokens Prod
ServerSignature Off

# phpMyAdmin Configuration (optional - file created on first run)
IncludeOptional "{user_config}/phpmyadmin.conf"
# SSL Configuration (optional - enabled via HTTPS/SSL page)
IncludeOptional "{user_config}/ssl.conf"
# Default virtual host for localhost.
# This is always the FIRST VirtualHost. Once user vhosts are added, Apache
# uses name-based selection; localhost must be first so http://localhost/
# and http://localhost/phpmyadmin always work regardless of user vhosts.
<VirtualHost *:8080>
    ServerName localhost
    ServerAlias 127.0.0.1
    DocumentRoot "{www_root_str}"
    <Directory "{www_root_str}">
        Options -Indexes +FollowSymLinks
        AllowOverride All
        Require all granted
        DirectoryIndex index.php index.html index.htm
    </Directory>

    # PHP handler - explicit inside VirtualHost so it is not subject to
    # global-scope merge behavior in name-based virtual host mode.
    AddHandler php-script .php
    Action php-script /php/php-cgi
    AddType application/x-httpd-php .php

    # phpMyAdmin - Alias inside VirtualHost because global-scope Alias
    # directives are not reliably inherited in name-based vhost mode.
    Alias /phpmyadmin "{pma_root}"
    Alias /pma "{pma_root}"
    <Directory "{pma_root}">
        Options -Indexes +FollowSymLinks
        AllowOverride None
        DirectoryIndex index.php
        Require all granted
        <FilesMatch "\.php$">
            SetHandler php-script
        </FilesMatch>
        <Files "config.inc.php">
            Require all denied
        </Files>
    </Directory>
</VirtualHost>
# Virtual Hosts (optional - managed via Virtual Hosts page)
IncludeOptional "{user_config}/vhosts.conf"
"#,
        override_baseline = crate::utils::apache_overrides::BASELINE,
        server_root = server_root,
        logs_root_str = logs_root_str,
        www_root_str = www_root_str,
        modules = modules,
        mime_types = mime_types,
        php_cgi_dir_str = php_cgi_dir_str,
        user_config = user_config,
        pma_root = phpmyadmin_root.display().to_string()
    );

    let phpmyadmin_config = format!(
        r#"# phpMyAdmin Virtual Host Configuration

# Create an alias for phpMyAdmin at /phpmyadmin
Alias /phpmyadmin "{}"

<Directory "{}">
    Options -Indexes +FollowSymLinks
    AllowOverride None
    DirectoryIndex index.php index.html
    Require ip 127.0.0.1
    Require ip ::1

    # Handle PHP files through CGI
    AddHandler php-script .php
    Action php-script /php/php-cgi

    # Security rules
    <Files "*.php">
        SetHandler php-script
    </Files>

    <Files "config.inc.php">
        Require all denied
    </Files>

    <Files "setup.php">
        Require all denied
    </Files>
</Directory>

# Alias for easy access
Alias /pma "{}"
"#,
        phpmyadmin_root.display(),
        phpmyadmin_root.display(),
        phpmyadmin_root.display()
    );

    let config_dir = user_config_dir();
    std::fs::write(config_dir.join("httpd.conf"), config_content).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("phpmyadmin.conf"), phpmyadmin_config)
        .map_err(|e| e.to_string())?;

    // Seed the user www dir with default files if it is empty.
    seed_www_dir();

    let _ = data_root;
    Ok(())
}

// Ensures the web root directory exists and contains a minimal index.html.
// Files are only written if absent, so user edits are never overwritten.
fn seed_www_dir() {
    let www_dir = user_www_dir();
    let index = www_dir.join("index.html");
    if !index.exists() {
        let _ = std::fs::write(
            &index,
            "<!DOCTYPE html><html><head><title>LinuxDevStackBox</title></head>\
             <body><h1>LinuxDevStackBox is running</h1>\
             <p>Place your PHP projects in this folder.</p>\
             <p><a href=\"/phpmyadmin/\">phpMyAdmin</a></p></body></html>\n",
        );
    }
}

async fn get_apache_version() -> Option<String> {
    let binary = get_apache_binary().ok()?;

    match create_hidden_command(&binary).arg("-v").output() {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if let Some(start) = version_str.find("Apache/") {
                if let Some(end) = version_str[start + 7..].find(" ") {
                    let version = version_str[start + 7..start + 7 + end].to_string();
                    return Some(version);
                }
            }
            None
        }
        Err(e) => {
            println!("Failed to get Apache version: {}", e);
            None
        }
    }
}

#[tauri::command]
pub async fn toggle_apache() -> Result<bool, String> {
    let status = get_apache_status().await?;
    if status.running {
        stop_apache().await?;
        Ok(false)
    } else {
        start_apache().await?;
        Ok(true)
    }
}
