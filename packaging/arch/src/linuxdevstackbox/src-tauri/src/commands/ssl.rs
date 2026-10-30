//! Trusted local HTTPS.
//!
//! A per-user local CA is generated with the system `openssl` tool; leaf
//! certificates are signed by it. Trust installation copies the CA into the
//! distribution CA store via `pkexec` (Arch: /etc/ca-certificates/trust-source,
//! Debian: /usr/local/share/ca-certificates, Fedora: /etc/pki/ca-trust) and
//! refreshes the bundle. CA keys are per-user, never bundled or exported.
use super::vhosts::{load_vhosts, VhostEntry};
use crate::utils::{paths::*, process::create_hidden_command};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, time::Duration};

pub static SITE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn tls_dir() -> PathBuf {
    user_config_dir().join("https")
}
pub fn ca_dir() -> PathBuf {
    tls_dir().join("ca")
}
pub fn site_id(host: &str) -> String {
    format!("{:x}", Sha256::digest(host.to_ascii_lowercase().as_bytes()))
}
pub fn leaf(host: &str) -> PathBuf {
    tls_dir().join(format!("{}.pem", site_id(host)))
}
pub fn leaf_key(host: &str) -> PathBuf {
    tls_dir().join(format!("{}-key.pem", site_id(host)))
}

fn openssl(args: &[&str]) -> Result<std::process::Output, String> {
    let exe = get_openssl_binary()?;
    create_hidden_command(exe.as_os_str())
        .args(args)
        .output()
        .map_err(|e| e.to_string())
}

fn openssl_ok(args: &[&str]) -> Result<Vec<u8>, String> {
    let out = openssl(args)?;
    if !out.status.success() {
        return Err(format!(
            "openssl failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(out.stdout)
}

fn restrict_permissions(path: &std::path::Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| format!("Cannot secure {}: {e}", path.display()))
}

/// System CA bundle candidates, per distribution family.
fn system_ca_bundles() -> &'static [&'static str] {
    &[
        "/etc/ssl/certs/ca-certificates.crt",                  // Arch, Debian
        "/etc/pki/tls/certs/ca-bundle.crt",                    // Fedora
        "/etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem",   // Fedora (p11-kit)
    ]
}

pub fn ca_trusted() -> bool {
    let cert = ca_dir().join("rootCA.pem");
    if !cert.exists() {
        return false;
    }
    // Authoritative check: does the system trust store (the one reqwest and
    // browsers use) already verify our CA?
    for bundle in system_ca_bundles() {
        if !std::path::Path::new(bundle).is_file() {
            continue;
        }
        if openssl(&["verify", "-CAfile", bundle, cert.to_str().unwrap_or("")])
            .is_ok_and(|o| o.status.success())
        {
            return true;
        }
    }
    // The anchor may be installed but the bundle not refreshed yet.
    ca_anchor_installed()
}

fn ca_anchor_installed() -> bool {
    [
        "/etc/ca-certificates/trust-source/anchors/linuxdevstackbox-rootCA.pem",
        "/usr/local/share/ca-certificates/linuxdevstackbox-rootCA.crt",
        "/etc/pki/ca-trust/source/anchors/linuxdevstackbox-rootCA.pem",
    ]
    .iter()
    .any(|p| std::path::Path::new(p).is_file())
}

/// Runs as root under `pkexec` (invoked with the binary's own --ca-trust-operation flag).
pub fn elevated_ca_trust(action: &str) -> Result<(), String> {
    let source = ca_dir().join("rootCA.pem");
    if !source.is_file() {
        return Err("The local CA does not exist yet. Set up trusted HTTPS first.".into());
    }
    let (dest, refresh): (&str, &str) = if std::path::Path::new("/etc/debian_version").exists() {
        (
            "/usr/local/share/ca-certificates/linuxdevstackbox-rootCA.crt",
            "update-ca-certificates",
        )
    } else if std::path::Path::new("/etc/pki").exists() {
        (
            "/etc/pki/ca-trust/source/anchors/linuxdevstackbox-rootCA.pem",
            "update-ca-trust",
        )
    } else {
        (
            "/etc/ca-certificates/trust-source/anchors/linuxdevstackbox-rootCA.pem",
            "update-ca-trust",
        )
    };

    let run_refresh = |action: &str| -> Result<(), String> {
        if which_exists(action) {
            let out = create_hidden_command(action).output().map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(format!(
                    "{} failed: {}",
                    action,
                    String::from_utf8_lossy(&out.stderr).trim()
                ));
            }
        }
        Ok(())
    };

    match action {
        "install" => {
            if let Some(parent) = std::path::Path::new(dest).parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&source, dest)
                .map_err(|e| format!("Cannot install the CA anchor: {e}"))?;
            restrict_permissions(std::path::Path::new(dest), 0o644)?;
            run_refresh(refresh)
        }
        "remove" => {
            match fs::remove_file(dest) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(format!("Cannot remove the CA anchor: {e}")),
            }
            run_refresh(refresh)
        }
        _ => Err("Invalid CA trust operation".into()),
    }
}

fn which_exists(name: &str) -> bool {
    let path_env = std::env::var("PATH").unwrap_or_default();
    path_env.split(':').any(|dir| std::path::Path::new(dir).join(name).is_file())
}

fn pkexec(args: &[&str]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let out = create_hidden_command("pkexec")
        .arg(&exe)
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run pkexec: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "Elevated CA trust operation failed ({}). {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

fn install_ca_trust() -> Result<(), String> {
    if ca_anchor_installed() || ca_trusted() {
        return Ok(());
    }
    pkexec(&["--ca-trust-operation", "install"])
}

pub fn ensure_ca(trust: bool) -> Result<(), String> {
    fs::create_dir_all(ca_dir()).map_err(|e| e.to_string())?;
    // Protect keys regardless of how permissive the inherited umask is.
    restrict_permissions(&tls_dir(), 0o700)?;
    restrict_permissions(&ca_dir(), 0o700)?;
    let root_ca = ca_dir().join("rootCA.pem");
    let root_key = ca_dir().join("rootCA-key.pem");
    if !root_ca.exists() {
        if root_key.exists() {
            return Err("The CA certificate is missing but its private key exists. Delete the HTTPS directory and regenerate, or restore the certificate.".into());
        }
        let subject = format!(
            "/O=LinuxDevStackBox/OU=Local Development CA/CN=LinuxDevStackBox Local CA ({})",
            whoami_username()
        );
        let out = openssl(&[
            "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-days", "3650", "-nodes",
            "-keyout", root_key.to_str().unwrap_or(""),
            "-out", root_ca.to_str().unwrap_or(""),
            "-subj", &subject,
            "-addext", "basicConstraints=critical,CA:TRUE",
            "-addext", "keyUsage=critical,keyCertSign,cRLSign",
        ])?;
        if !out.status.success() {
            return Err(format!(
                "CA generation failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    restrict_permissions(&root_key, 0o600)?;
    restrict_permissions(&root_ca, 0o644)?;
    if trust {
        install_ca_trust()?;
    }
    if !ca_dir().join("rootCA.pem").exists() {
        return Err("Set up trusted HTTPS first.".into());
    }
    Ok(())
}

fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

fn san_value(name: &str) -> String {
    if name.parse::<std::net::IpAddr>().is_ok() {
        format!("IP:{name}")
    } else {
        format!("DNS:{name}")
    }
}

pub fn ensure_leaf(host: &str, aliases: &[String]) -> Result<(), String> {
    let cert = leaf(host);
    let names: Vec<String> = if host == "localhost" {
        vec!["localhost".into(), "127.0.0.1".into(), "::1".into()]
    } else {
        std::iter::once(host.to_string())
            .chain(aliases.iter().cloned())
            .collect()
    };
    let manifest = tls_dir().join(format!("{}.names.json", site_id(host)));
    let wanted = serde_json::to_string(&names).map_err(|e| e.to_string())?;
    let fresh = cert.exists()
        && leaf_key(host).exists()
        && key_matches_certificate(host)
        && fs::read_to_string(&manifest).ok().as_deref() == Some(&wanted)
        && openssl(&["x509", "-checkend", "2592000", "-noout", "-in", cert.to_str().unwrap_or("")])
            .is_ok_and(|o| o.status.success())
        && openssl(&[
            "verify", "-CAfile", ca_dir().join("rootCA.pem").to_str().unwrap_or(""),
            cert.to_str().unwrap_or(""),
        ])
        .is_ok_and(|o| o.status.success());
    if fresh {
        return Ok(());
    }
    let pending = cert.with_extension("pending.pem");
    let pending_key = leaf_key(host).with_extension("pending.pem");
    let csr = tls_dir().join(format!("{}.pending.csr", site_id(host)));
    let ext_file = tls_dir().join(format!("{}.pending.ext", site_id(host)));

    let out = openssl(&[
        "req", "-new", "-newkey", "rsa:2048", "-sha256", "-nodes",
        "-keyout", pending_key.to_str().unwrap_or(""),
        "-out", csr.to_str().unwrap_or(""),
        "-subj", &format!("/CN={host}"),
    ])?;
    if !out.status.success() {
        return Err(format!(
            "Key generation failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    restrict_permissions(&pending_key, 0o600)?;

    let sans = names.iter().map(|n| san_value(n)).collect::<Vec<_>>().join(",");
    fs::write(
        &ext_file,
        format!(
            "basicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName={sans}\n"
        ),
    )
    .map_err(|e| e.to_string())?;

    let out = openssl(&[
        "x509", "-req", "-sha256", "-days", "825",
        "-in", csr.to_str().unwrap_or(""),
        "-CA", ca_dir().join("rootCA.pem").to_str().unwrap_or(""),
        "-CAkey", ca_dir().join("rootCA-key.pem").to_str().unwrap_or(""),
        "-CAcreateserial",
        "-extfile", ext_file.to_str().unwrap_or(""),
        "-out", pending.to_str().unwrap_or(""),
    ])?;
    let _ = fs::remove_file(&csr);
    let _ = fs::remove_file(&ext_file);
    if !out.status.success() {
        return Err(format!(
            "Certificate signing failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    fs::rename(&pending, &cert).map_err(|e| e.to_string())?;
    fs::rename(&pending_key, leaf_key(host)).map_err(|e| e.to_string())?;
    fs::write(manifest, wanted).map_err(|e| e.to_string())?;
    Ok(())
}

fn key_matches_certificate(host: &str) -> bool {
    let cert = openssl(&["x509", "-pubkey", "-noout", "-in", leaf(host).to_str().unwrap_or("")]);
    let key = openssl(&["pkey", "-pubout", "-in", leaf_key(host).to_str().unwrap_or("")]);
    matches!((cert, key), (Ok(c), Ok(k)) if c.status.success() && k.status.success() && c.stdout == k.stdout)
}

pub fn https_port() -> u16 {
    fs::read_to_string(user_config_dir().join("https-port"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|p| *p > 0)
        .unwrap_or(443)
}

pub fn site_url(host: &str) -> String {
    let port = https_port();
    if port == 443 {
        format!("https://{host}/")
    } else {
        format!("https://{host}:{port}/")
    }
}

pub fn https_enabled() -> bool {
    user_config_dir().join("https-enabled").is_file()
}

pub fn render_ssl(entries: &[VhostEntry]) -> Result<String, String> {
    if !https_enabled() {
        return Ok(String::new());
    }
    // The SSL module must be loaded with an absolute path so the generated
    // configuration works no matter where the distribution puts its modules.
    let ssl_module = crate::commands::apache::apache_runtime()
        .ok()
        .map(|runtime| runtime.modules_dir.join("mod_ssl.so"))
        .unwrap_or_else(|| PathBuf::from("modules/mod_ssl.so"));
    let mut conf = format!(
        "# Managed LinuxDevStackBox HTTPS\nLoadModule ssl_module {}\nListen 127.0.0.1:443\nListen [::1]:443\nSSLSessionCache none\nSSLProtocol -all +TLSv1.2 +TLSv1.3\n",
        ssl_module.display()
    );
    conf = conf.replace(":443", &format!(":{}", https_port()));
    let localhost = VhostEntry::localhost();
    for site in std::iter::once(&localhost).chain(entries.iter().filter(|s| s.enabled && s.https)) {
        if !leaf(&site.domain).exists() || !leaf_key(&site.domain).exists() {
            return Err(format!(
                "Certificate missing for {}. Repair HTTPS.",
                site.domain
            ));
        }
        conf.push_str(&super::vhosts::render_site(site, true)?);
    }
    Ok(conf)
}

pub fn health_alias(host: &str) -> String {
    let file = tls_dir().join(format!("{}.txt", site_id(host)));
    format!(
        "Alias /.linuxdevstackbox-health \"{}\"\n<Directory \"{}\">\nRequire local\n</Directory>\n",
        to_apache_path(&file),
        to_apache_path(&tls_dir())
    )
}

pub fn write_health(host: &str) -> Result<(), String> {
    fs::create_dir_all(tls_dir()).map_err(|e| e.to_string())?;
    fs::write(
        tls_dir().join(format!("{}.txt", site_id(host))),
        site_id(host),
    )
    .map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SiteHealth {
    pub domain: String,
    pub enabled: bool,
    pub https: bool,
    pub trusted: bool,
    pub verified: bool,
    pub message: String,
    pub expiry: Option<String>,
    pub url: String,
}

#[derive(Serialize)]
pub struct SslStatus {
    pub ca_exists: bool,
    pub ca_trusted: bool,
    pub enabled: bool,
    pub sites: Vec<SiteHealth>,
    pub setup_error: Option<String>,
    pub port: u16,
}

pub async fn probe(site: &VhostEntry) -> SiteHealth {
    let mut health = SiteHealth {
        domain: site.domain.clone(),
        enabled: site.enabled,
        https: site.https,
        trusted: ca_trusted(),
        verified: false,
        message: "HTTPS setup required".into(),
        expiry: None,
        url: site_url(&site.domain),
    };
    if let Ok(o) = openssl(&["x509", "-noout", "-enddate", "-in", leaf(&site.domain).to_str().unwrap_or("")]) {
        if o.status.success() {
            health.expiry = Some(
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .trim_start_matches("notAfter=")
                    .into(),
            );
        }
    }
    if !site.enabled {
        health.message = "Site disabled".into();
        return health;
    }
    if !site.https {
        health.message = "HTTP only".into();
        health.url = format!(
            "http://{}:{}/",
            site.domain,
            crate::utils::config_ports::apache_listen_port()
        );
        return health;
    }
    if !https_enabled() {
        return health;
    }
    if !health.trusted {
        health.message = "System trust missing - repair HTTPS".into();
        return health;
    }
    if !crate::utils::process::is_our_process_running(
        "httpd",
        &user_config_dir().join("httpd.conf"),
    ) {
        health.message = "Configured; Apache stopped".into();
        return health;
    }
    let result: Result<(), String> = async {
        // System DNS and the system trust store are both exercised. No insecure fallback.
        let client = reqwest::Client::builder()
            .use_native_tls()
            .no_proxy()
            .tls_info(true)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;
        for name in std::iter::once(&site.domain).chain(site.aliases.iter()) {
            let response = client
                .get(format!("{}.linuxdevstackbox-health", site_url(name)))
                .send()
                .await
                .map_err(|e| format!("HTTPS verification failed for {name}: {e}"))?;
            let peer = response
                .extensions()
                .get::<reqwest::tls::TlsInfo>()
                .and_then(|i| i.peer_certificate())
                .ok_or("No peer certificate available")?;
            let expected = openssl_ok(&["x509", "-outform", "DER", "-in", leaf(&site.domain).to_str().unwrap_or("")])?;
            if Sha256::digest(peer) != Sha256::digest(&expected) {
                return Err("Apache is serving an unexpected certificate".into());
            }
            if !response.status().is_success()
                || response.text().await.map_err(|e| e.to_string())?.trim() != site_id(&site.domain)
            {
                return Err("Hostname is not serving the expected virtual host".into());
            }
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => {
            health.verified = true;
            health.message = "Trusted HTTPS".into();
        }
        Err(e) => health.message = e,
    }
    health
}

#[tauri::command]
pub async fn get_ssl_status() -> Result<SslStatus, String> {
    let mut sites = vec![probe(&VhostEntry::localhost()).await];
    for site in load_vhosts()? {
        sites.push(probe(&site).await);
    }
    Ok(SslStatus {
        ca_exists: ca_dir().join("rootCA.pem").exists(),
        ca_trusted: ca_trusted(),
        enabled: https_enabled(),
        port: https_port(),
        sites,
        setup_error: fs::read_to_string(user_config_dir().join("https-setup-error.txt")).ok(),
    })
}

pub fn lock_configuration() -> Result<std::fs::File, String> {
    fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(user_config_dir().join("https-operation.lock"))
        .map_err(|_| {
            "Another site or HTTPS operation is running. Wait for it to finish, then retry.".into()
        })
}

pub async fn provision() -> Result<String, String> {
    provision_inner(true, None).await
}

pub async fn renew_existing() -> Result<String, String> {
    provision_inner(false, None).await
}

pub(super) fn reject_unrecorded_sites(configuration: &str, entries: &[VhostEntry]) -> Result<(), String> {
    for line in configuration.lines() {
        let mut words = line.split_whitespace();
        if !words.next().is_some_and(|word| word.eq_ignore_ascii_case("ServerName")) {
            continue;
        }
        if let Some(name) = words.next() {
            let name = name.trim_matches('"');
            if !name.eq_ignore_ascii_case("localhost")
                && !entries.iter().any(|site| site.enabled && site.domain.eq_ignore_ascii_case(name))
            {
                return Err(format!("Apache contains {name}, but it has no active saved project record. The change was stopped to preserve the existing configuration. Review Projects and reconnect the existing folder before retrying."));
            }
        }
    }
    Ok(())
}

async fn provision_inner(install_trust: bool, port: Option<u16>) -> Result<String, String> {
    let _guard = SITE_LOCK.lock().await;
    let _process_lock = lock_configuration()?;
    let config = user_config_dir();
    let entries = load_vhosts()?;
    // Renewal must not erase sites that survived only in generated configuration.
    // Run before any certificate, marker, config write or service restart.
    for name in ["vhosts.conf", "ssl.conf"] {
        match fs::read_to_string(config.join(name)) {
            Ok(content) => reject_unrecorded_sites(&content, &entries)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(format!("Cannot inspect {name} before HTTPS setup: {error}")),
        }
    }
    let mut paths = vec![
        config.join("https-enabled"),
        config.join("https-port"),
        config.join("httpd.conf"),
        config.join("phpmyadmin.conf"),
    ];
    for site in std::iter::once(VhostEntry::localhost()).chain(entries.iter().cloned()) {
        paths.extend([
            leaf(&site.domain),
            leaf_key(&site.domain),
            tls_dir().join(format!("{}.names.json", site_id(&site.domain))),
        ]);
    }
    let saved = super::vhosts::snapshot(&paths)?;
    let was_running = crate::utils::process::is_our_process_running(
        "httpd",
        &config.join("httpd.conf"),
    );
    let result: Result<(), String> = async {
        if let Some(port) = port {
            if port == 0 || port == crate::utils::config_ports::apache_listen_port() {
                return Err("Choose a separate HTTPS port between 1 and 65535.".into());
            }
            fs::write(config.join("https-port"), port.to_string()).map_err(|e| e.to_string())?;
        }
        super::apache::ensure_apache_config().await?;
        ensure_ca(install_trust)?;
        ensure_leaf("localhost", &[])?;
        for s in entries.iter().filter(|s| s.enabled && s.https) {
            ensure_leaf(&s.domain, &s.aliases)?;
        }
        fs::write(config.join("https-enabled"), "1").map_err(|e| e.to_string())?;
        if !install_trust
            && saved.iter().all(|(p, bytes)| fs::read(p).ok() == *bytes)
            && fs::read_to_string(config.join("ssl.conf")).ok().as_deref()
                == Some(&render_ssl(&entries)?)
        {
            return Ok(());
        }
        super::vhosts::apply_configuration(&entries).await
    }
    .await;
    if let Err(error) = result {
        super::vhosts::restore(&saved).map_err(|e| format!("{error}; rollback: {e}"))?;
        if was_running {
            let _ = super::apache::stop_apache().await;
            super::apache::start_apache()
                .await
                .map_err(|e| format!("{error}; restored files but restart failed: {e}"))?;
        }
        return Err(error);
    }
    let _ = fs::remove_file(config.join("https-setup-error.txt"));
    Ok(probe(&VhostEntry::localhost()).await.message)
}

#[tauri::command]
pub async fn enable_ssl(port: Option<u16>) -> Result<String, String> {
    provision_inner(true, port).await
}

#[tauri::command]
pub async fn trust_ssl_ca() -> Result<String, String> {
    provision().await
}

#[tauri::command]
pub async fn generate_ssl_cert() -> Result<String, String> {
    provision().await
}

#[tauri::command]
pub async fn disable_ssl() -> Result<String, String> {
    let _guard = SITE_LOCK.lock().await;
    let _process_lock = lock_configuration()?;
    let marker = user_config_dir().join("https-enabled");
    if marker.exists() {
        fs::remove_file(&marker).map_err(|e| e.to_string())?;
    }
    if let Err(e) = super::vhosts::apply_configuration(&load_vhosts()?).await {
        let _ = fs::write(marker, "1");
        return Err(e);
    }
    Ok("HTTPS disabled; certificates and projects preserved".into())
}

pub fn repair_ssl_conf() { /* Configuration is reconciled explicitly, never silently re-enabled. */ }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renewal_refuses_to_erase_unrecorded_or_disabled_sites() {
        let config = "# ServerName ignored.test\nServerName localhost\n ServerName \"shop.test\"\n";
        assert!(reject_unrecorded_sites(config, &[]).is_err());
        let mut site = VhostEntry::localhost();
        site.domain = "SHOP.test".into();
        assert!(reject_unrecorded_sites(config, &[site.clone()]).is_ok());
        site.enabled = false;
        assert!(reject_unrecorded_sites(config, &[site]).is_err());
        assert!(reject_unrecorded_sites("ServerName localhost\n# ServerName old.test", &[]).is_ok());
    }

    #[test]
    fn filenames_are_case_normalized_and_do_not_embed_host_input() {
        assert_eq!(site_id("SHOP.test"), site_id("shop.test"));
        assert_eq!(site_id("../bad").len(), 64);
    }

    #[test]
    fn san_values_use_ip_syntax_for_addresses() {
        assert_eq!(san_value("localhost"), "DNS:localhost");
        assert_eq!(san_value("127.0.0.1"), "IP:127.0.0.1");
        assert_eq!(san_value("::1"), "IP:::1");
    }
}
