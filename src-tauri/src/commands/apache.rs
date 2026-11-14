// Apache service commands.

use std::time::Duration;
use tokio::time::sleep;

use crate::commands::php::patch_php_ini;
use crate::types::ServiceInfo;
use crate::utils::config_ports::{apache_listen_port, default_http_port};
use crate::utils::config_version::needs_config_migration;
use crate::utils::paths::{
    get_apache_exe, get_apache_modules_dir, get_installation_path, get_mime_types_file,
    prepare_apache_server_root, user_config_dir, user_logs_dir, user_www_dir,
};
use crate::utils::process::{
    create_hidden_command, ensure_port_available, find_our_processes, is_our_process_running, kill_pid,
};

pub const APACHE_CONFIG_VERSION: u32 = 11;

/// Path (relative to the `/php/` ScriptAlias) of the php-cgi binary inside a
/// PHP runtime dir, e.g. `/php/php-cgi` or `/php/bin/php-cgi`.
pub fn php_cgi_action(runtime: &std::path::Path) -> String {
    let flat = runtime.join(crate::utils::paths::PHP_CGI_NAME);
    if flat.exists() {
        format!("/php/{}", crate::utils::paths::PHP_CGI_NAME)
    } else {
        format!("/php/bin/{}", crate::utils::paths::PHP_CGI_NAME)
    }
}

/// `LoadModule` lines for modules that actually exist as shared objects in the
/// modules dir (system Apache builds ship most modules as .so files and
/// compile some in statically; only emit what is present).
fn module_lines(modules_dir: Option<&std::path::Path>, ssl: bool) -> String {
    let Some(dir) = modules_dir else { return String::new() };
    let has = |m: &str| dir.join(format!("mod_{m}.so")).exists();
    let mut out = String::new();
    // Exactly one MPM must be loaded when MPMs are shared objects.
    let mpm = ["event", "worker", "prefork"].into_iter().find(|m| has(&format!("mpm_{m}")));
    if let Some(m) = mpm {
        out.push_str(&format!("LoadModule mpm_{m}_module modules/mod_mpm_{m}.so\n"));
    }
    let mut push = |name: &str, file: &str| {
        if has(file) {
            out.push_str(&format!("LoadModule {name}_module modules/mod_{file}.so\n"));
        }
    };
    push("unixd", "unixd");
    push("authz_core", "authz_core");
    push("authz_host", "authz_host");
    push("access_compat", "access_compat");
    push("dir", "dir");
    push("mime", "mime");
    push("log_config", "log_config");
    push("alias", "alias");
    push("actions", "actions");
    push("headers", "headers");
    push("env", "env");
    push("setenvif", "setenvif");
    push("rewrite", "rewrite");
    // mod_cgi is for prefork; threaded MPMs need mod_cgid.
    if mpm == Some("prefork") {
        push("cgi", "cgi");
        push("cgid", "cgid");
    } else {
        push("cgid", "cgid");
        push("cgi", "cgi");
    }
    if ssl {
        push("socache_shmcb", "socache_shmcb");
    }
    out
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
        let backup = config_path.with_extension(format!("pre-v{APACHE_CONFIG_VERSION}.conf"));
        if !backup.exists() { std::fs::copy(&config_path, backup).map_err(|e| e.to_string())?; }
        // apache_listen_port() reads the existing config, so the user's port survives.
        create_default_apache_config().await?;
    }
    harden_directory_listing(&config_path)?;
    crate::utils::apache_overrides::migrate(&config_path, &get_apache_exe(&get_installation_path()))
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
            "linuxdevstackbox-httpd-test-{}.conf",
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

fn apache_exe_path() -> std::path::PathBuf {
    let base = get_installation_path();
    get_apache_exe(&base)
}

#[tauri::command]
pub async fn get_apache_status() -> Result<ServiceInfo, String> {
    let exe = apache_exe_path();
    let our_pids = find_our_processes("httpd", &exe);
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

#[tauri::command]
pub async fn start_apache() -> Result<bool, String> {
    let _runtime_guard = super::php::PHP_RUNTIME_LOCK.try_lock()
        .map_err(|_| "Wait for the PHP update or version switch to finish before starting Apache.")?;
    let base_path = get_installation_path();

    let apache_path = get_apache_exe(&base_path);
    if !apache_path.exists() {
        return Err(format!(
            "Apache binary not found at {}. Please ensure Apache is installed.",
            apache_path.display()
        ));
    }

    if is_our_process_running("httpd", &apache_path) {
        return Ok(true);
    }
    super::php::validate_web_runtimes()?;

    ensure_apache_config().await?;

    let config_path = user_config_dir().join("httpd.conf");

    let ssl_conf_path = user_config_dir().join("ssl.conf");
    // If ssl.conf is absent but certs already exist (e.g. it was deleted by
    // the migration above, or the file was lost), regenerate it automatically
    // so HTTPS comes back without the user having to click "Enable SSL" again.
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
    // Phase 5.2 - fail fast if the configured listen port is taken.
    ensure_port_available(listen_port, "Apache")?;

    let output = create_hidden_command(&apache_path.to_string_lossy()).arg("-f").arg(&config_path).arg("-t").current_dir(&base_path).output().map_err(|e| e.to_string())?;
    if !output.status.success() { return Err(format!("Apache configuration validation failed ({}): {}", output.status, String::from_utf8_lossy(&output.stderr))); }

    let mut child = match create_hidden_command(&apache_path.to_string_lossy())
        .arg("-f")
        .arg(&config_path)
        .current_dir(&base_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Err(format!("Failed to start Apache: {}", e)),
    };

    println!("Apache started with PID: {}", child.id());

    sleep(Duration::from_secs(2)).await;

    // Apache daemonizes: the launched parent exits after forking the real
    // server. Treat a parent exit as non-fatal (this also reaps it) and let the
    // port check below decide whether the server is actually up.
    match child.try_wait() {
        Ok(Some(status)) => {
            println!(
                "Apache parent process exited (code: {}) - checking port {} for worker process",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                listen_port
            );
        }
        Ok(None) => {} // parent still running - good
        Err(e) => {
            eprintln!("Could not query Apache child status: {}", e);
        }
    }

    if is_our_process_running("httpd", &apache_path)
        && std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], listen_port)),
            Duration::from_secs(2),
        )
        .is_ok()
    {
        Ok(true)
    } else {
        Err(format!("LinuxDevStackBox Apache did not become ready on port {listen_port}. Check the Apache error log."))
    }
}

#[tauri::command]
pub async fn stop_apache() -> Result<bool, String> {
    let _maintenance = super::log_management::MaintenanceAfterStop;
    let apache_path = apache_exe_path();

    if !is_our_process_running("httpd", &apache_path) {
        return Ok(true);
    }

    if apache_path.exists() {
        let config_path = user_config_dir().join("httpd.conf");

        let _ = create_hidden_command(&apache_path.to_string_lossy())
            .args(["-f", config_path.to_str().unwrap_or(""), "-k", "stop"])
            .output();

        sleep(Duration::from_millis(2000)).await;
    }

    let mut errors: Vec<String> = Vec::new();
    for attempt in 0..4 {
        let pids = find_our_processes("httpd", &apache_path);
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

    if find_our_processes("httpd", &apache_path).is_empty() {
        Ok(true)
    } else {
        Err(format!(
            "LinuxDevStackBox Apache processes are still running after multiple stop attempts.\n\n\
            Try:\n\
            1. Close any browser tabs accessing localhost\n\
            2. Check `ps aux | grep -E 'apache2|httpd'` for leftover processes\n\
            3. Stop them manually with `kill <pid>`\n\n\
            Errors: {}",
            errors.join("; ")
        ))
    }
}

pub async fn create_default_apache_config() -> Result<(), String> {
    let install_path = get_installation_path();
    let port = {
        let existing = apache_listen_port();
        if existing == 0 { default_http_port() } else { existing }
    };

    let apache_root = prepare_apache_server_root(&install_path)?;
    let www_root = user_www_dir();
    let logs_root = user_logs_dir();
    let run_dir = logs_root.join("run");
    std::fs::create_dir_all(&run_dir).map_err(|e| e.to_string())?;
    let phpmyadmin_root = crate::utils::phpmyadmin_config::phpmyadmin_root();
    let config_dir = user_config_dir();

    let ap = crate::utils::paths::to_apache_path;
    let modules = module_lines(get_apache_modules_dir(&install_path).as_deref(), false);
    let mime_types = get_mime_types_file(&install_path)
        .map(|p| format!("TypesConfig \"{}\"\n", ap(&p)))
        .unwrap_or_default();

    // PHP-CGI: the active runtime dir holds php-cgi (or bin/php-cgi).
    // Always go through the stable `current` symlink (maintained by the PHP
    // module, flat `php-cgi` inside), so the config stays valid when the active
    // PHP version changes or runtime dirs are created after this file.
    let php_cgi_dir = crate::utils::paths::user_php_runtime_root().join("current");
    let php_action = format!("/php/{}", crate::utils::paths::PHP_CGI_NAME);

    let config_content = format!(
        r#"{override_baseline}# configVersion: {version}
# Apache Configuration for LinuxDevStackBox
# Managed by LinuxDevStackBox. Edits to this file are preserved across upgrades
# unless the configVersion is bumped, which triggers a migration.
ServerRoot "{apache_root_str}"
PidFile "{logs_root_str}/httpd.pid"
DefaultRuntimeDir "{run_dir_str}"
Listen 127.0.0.1:{port}
Listen [::1]:{port}

# Modules (only those present on this system are loaded)
{modules}
ServerName localhost:{port}
DocumentRoot "{www_root_str}"

<Directory "{www_root_str}">
    Options -Indexes +FollowSymLinks
    AllowOverride All
    Require all granted
    DirectoryIndex index.php index.html index.htm
</Directory>

# MIME Types
{mime_types}AddType text/html .html .htm

# PHP CGI configuration
ScriptAlias /php/ "{php_cgi_dir_str}/"
Action php-script {php_action}
AddHandler php-script .php
AddType application/x-httpd-php .php

# Tell PHP-CGI where to find its php.ini.  The user-config php.ini is written
# to the user data dir so it never pollutes the installation directory.
SetEnv PHPRC "{user_config}/"

<Directory "{php_cgi_dir_str}">
    AllowOverride None
    Options +ExecCGI +FollowSymLinks
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
<VirtualHost *:{port}>
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
    Action php-script {php_action}
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
        version = APACHE_CONFIG_VERSION,
        apache_root_str = ap(&apache_root),
        logs_root_str = ap(&logs_root),
        run_dir_str = ap(&run_dir),
        port = port,
        modules = modules,
        www_root_str = ap(&www_root),
        mime_types = mime_types,
        php_cgi_dir_str = ap(&php_cgi_dir),
        php_action = php_action,
        user_config = ap(&config_dir),
        pma_root = ap(&phpmyadmin_root),
    );

    let phpmyadmin_config = format!(
        r#"# phpMyAdmin Virtual Host Configuration

# Create an alias for phpMyAdmin at /phpmyadmin
Alias /phpmyadmin "{pma}"

<Directory "{pma}">
    Options -Indexes +FollowSymLinks
    AllowOverride None
    DirectoryIndex index.php index.html
    Require ip 127.0.0.1
    Require ip ::1

    # Handle PHP files through CGI
    AddHandler php-script .php
    Action php-script {php_action}

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
Alias /pma "{pma}"
"#,
        pma = ap(&phpmyadmin_root),
        php_action = php_action,
    );

    std::fs::write(config_dir.join("httpd.conf"), config_content).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("phpmyadmin.conf"), phpmyadmin_config)
        .map_err(|e| e.to_string())?;

    // Seed the user www dir with default files if it is empty.
    seed_www_dir();
    Ok(())
}

// Ensures the web root directory exists and contains a minimal index.html.
// user_www_dir() is ~/LinuxDevStackBox/www (seeded from the bundled www/).
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
    let base_path = get_installation_path();
    let apache_path = get_apache_exe(&base_path);

    if !apache_path.exists() {
        return None;
    }

    match create_hidden_command(apache_path.to_str()?)
        .arg("-v")
        .output()
    {
        Ok(output) => {
            let version_str = String::from_utf8_lossy(&output.stdout);
            if let Some(start) = version_str.find("Apache/") {
                let version: String = version_str[start + 7..]
                    .chars()
                    .take_while(|c| !c.is_whitespace())
                    .collect();
                if !version.is_empty() {
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
