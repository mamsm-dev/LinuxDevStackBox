//! Trusted local HTTPS. CA keys are per-user, never bundled or exported.
//! The CA and site certificates are generated with the `openssl` CLI; the CA is
//! trusted through the system store (pkexec) and, best effort, browser NSS DBs.
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
fn openssl_run(args: &[String]) -> Result<std::process::Output, String> {
    create_hidden_command(&get_openssl_exe(&get_installation_path()).to_string_lossy())
        .args(args)
        .output()
        .map_err(|e| format!("Cannot run openssl (install the openssl package): {e}"))
}
fn openssl(args: &[&str], path: &std::path::Path) -> Result<std::process::Output, String> {
    let mut all: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    all.push(path.to_string_lossy().into_owned());
    openssl_run(&all)
}
fn chmod(path: &std::path::Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
}
fn ensure_ok(out: std::process::Output, what: &str) -> Result<(), String> {
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{what} failed: {}", String::from_utf8_lossy(&out.stderr)))
    }
}
/// True when this process runs as root (uid 0).
pub fn running_as_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(1).map(|u| u == "0"))
        })
        .unwrap_or(false)
}

const CA_FILE_NAME: &str = "linuxdevstackbox-ca.crt";

/// Where the CA is installed for the system trust store, per distribution family.
fn system_anchor_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/usr/local/share/ca-certificates").join(CA_FILE_NAME),
        PathBuf::from("/etc/pki/ca-trust/source/anchors").join(CA_FILE_NAME),
        PathBuf::from("/etc/ca-certificates/trust-source/anchors").join(CA_FILE_NAME),
        PathBuf::from("/usr/share/pki/trust/anchors").join(CA_FILE_NAME),
    ]
}

/// The CA counts as trusted when our current root certificate is installed in
/// the system trust store (what OpenSSL/native-tls clients use).
pub fn ca_trusted() -> bool {
    let Ok(ours) = fs::read(ca_dir().join("rootCA.pem")) else {
        return false;
    };
    system_anchor_paths()
        .iter()
        .any(|p| fs::read(p).map(|installed| installed == ours).unwrap_or(false))
}

/// Installs the CA into the system trust store (via pkexec unless root).
fn install_system_trust(ca: &std::path::Path) -> Result<(), String> {
    let (dest, script) = if find_in_path("update-ca-certificates").is_some() {
        (
            "/usr/local/share/ca-certificates",
            r#"mkdir -p "$2" && install -m 644 "$1" "$2/linuxdevstackbox-ca.crt" && update-ca-certificates"#,
        )
    } else if find_in_path("update-ca-trust").is_some() {
        let dir = if PathBuf::from("/etc/pki/ca-trust/source/anchors").is_dir() {
            "/etc/pki/ca-trust/source/anchors"
        } else {
            "/etc/ca-certificates/trust-source/anchors"
        };
        (
            dir,
            r#"mkdir -p "$2" && install -m 644 "$1" "$2/linuxdevstackbox-ca.crt" && update-ca-trust extract"#,
        )
    } else {
        return Err("Cannot find update-ca-certificates or update-ca-trust to install the local CA. Install the ca-certificates package.".into());
    };
    let mut cmd = if running_as_root() {
        create_hidden_command("sh")
    } else {
        if find_in_path("pkexec").is_none() {
            return Err(format!(
                "pkexec (polkit) is required to trust the local CA. As root, copy {} to {dest}/{CA_FILE_NAME} and refresh the trust store.",
                ca.display()
            ));
        }
        let mut c = create_hidden_command("pkexec");
        c.arg("sh");
        c
    };
    let out = cmd
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(ca)
        .arg(dest)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "Could not install the local CA into the system trust store (permission declined?): {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

/// Best effort: browsers on Linux (Chrome/Chromium, Firefox) use NSS databases
/// instead of the system store. Needs `certutil` (libnss3-tools / nss-tools).
fn install_nss_trust(ca: &std::path::Path) {
    let Some(certutil) = find_in_path("certutil") else {
        return;
    };
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let mut dbs = vec![home.join(".pki").join("nssdb")];
    if let Ok(profiles) = fs::read_dir(home.join(".mozilla").join("firefox")) {
        for p in profiles.flatten() {
            if p.path().join("cert9.db").exists() {
                dbs.push(p.path());
            }
        }
    }
    for db in dbs {
        if !db.exists() {
            if fs::create_dir_all(&db).is_err() {
                continue;
            }
            let _ = create_hidden_command(&certutil.to_string_lossy())
                .args(["-N", "--empty-password", "-d"])
                .arg(format!("sql:{}", db.display()))
                .output();
        }
        let spec = format!("sql:{}", db.display());
        let nick = "LinuxDevStackBox Local CA";
        let _ = create_hidden_command(&certutil.to_string_lossy())
            .args(["-D", "-n", nick, "-d", &spec])
            .output();
        let _ = create_hidden_command(&certutil.to_string_lossy())
            .args(["-A", "-t", "C,,", "-n", nick, "-d", &spec, "-i"])
            .arg(ca)
            .output();
    }
}

fn create_ca() -> Result<(), String> {
    let dir = ca_dir();
    let key = dir.join("rootCA-key.pem");
    let cert = dir.join("rootCA.pem");
    let out = openssl_run(&[
        "req".into(),
        "-x509".into(),
        "-newkey".into(),
        "rsa:3072".into(),
        "-nodes".into(),
        "-sha256".into(),
        "-days".into(),
        "3650".into(),
        "-keyout".into(),
        key.to_string_lossy().into_owned(),
        "-out".into(),
        cert.to_string_lossy().into_owned(),
        "-subj".into(),
        "/O=LinuxDevStackBox/CN=LinuxDevStackBox Local CA".into(),
        "-addext".into(),
        "basicConstraints=critical,CA:TRUE".into(),
        "-addext".into(),
        "keyUsage=critical,keyCertSign,cRLSign".into(),
    ])?;
    ensure_ok(out, "Creating the local certificate authority")?;
    chmod(&key, 0o600);
    Ok(())
}

pub fn ensure_ca(trust: bool) -> Result<(), String> {
    fs::create_dir_all(ca_dir()).map_err(|e| e.to_string())?;
    // Keys are per-user: owner-only directories and key files.
    chmod(&tls_dir(), 0o700);
    chmod(&ca_dir(), 0o700);
    let ca = ca_dir().join("rootCA.pem");
    if !ca.exists() || !ca_dir().join("rootCA-key.pem").exists() {
        create_ca()?;
    }
    if trust && !ca_trusted() {
        install_system_trust(&ca)?;
        install_nss_trust(&ca);
    }
    if !ca.exists() {
        return Err("Set up trusted HTTPS first.".into());
    }
    Ok(())
}

/// Issues a leaf certificate signed by the local CA into `cert_out`/`key_out`.
fn issue_leaf(cert_out: &std::path::Path, key_out: &std::path::Path, names: &[String]) -> Result<(), String> {
    let primary = names.first().ok_or("No hostnames for certificate")?;
    let csr = cert_out.with_extension("csr");
    let ext = cert_out.with_extension("ext");
    let san = names
        .iter()
        .map(|n| {
            if n.parse::<std::net::IpAddr>().is_ok() {
                format!("IP:{n}")
            } else {
                format!("DNS:{n}")
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    fs::write(
        &ext,
        format!("basicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName={san}\n"),
    )
    .map_err(|e| e.to_string())?;
    let result = (|| {
        let out = openssl_run(&[
            "req".into(),
            "-new".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-keyout".into(),
            key_out.to_string_lossy().into_owned(),
            "-out".into(),
            csr.to_string_lossy().into_owned(),
            "-subj".into(),
            format!("/O=LinuxDevStackBox/CN={primary}"),
        ])?;
        ensure_ok(out, "Creating the certificate request")?;
        chmod(key_out, 0o600);
        let out = openssl_run(&[
            "x509".into(),
            "-req".into(),
            "-in".into(),
            csr.to_string_lossy().into_owned(),
            "-CA".into(),
            ca_dir().join("rootCA.pem").to_string_lossy().into_owned(),
            "-CAkey".into(),
            ca_dir().join("rootCA-key.pem").to_string_lossy().into_owned(),
            "-CAcreateserial".into(),
            "-out".into(),
            cert_out.to_string_lossy().into_owned(),
            "-days".into(),
            "397".into(),
            "-sha256".into(),
            "-extfile".into(),
            ext.to_string_lossy().into_owned(),
        ])?;
        ensure_ok(out, "Signing the site certificate")
    })();
    let _ = fs::remove_file(&csr);
    let _ = fs::remove_file(&ext);
    result
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
        && openssl(&["x509", "-checkend", "2592000", "-noout", "-in"], &cert)
            .is_ok_and(|o| o.status.success())
        && create_hidden_command(&get_openssl_exe(&get_installation_path()).to_string_lossy())
            .args(["verify", "-CAfile"])
            .arg(ca_dir().join("rootCA.pem"))
            .arg(&cert)
            .output()
            .is_ok_and(|o| o.status.success());
    if fresh {
        return Ok(());
    }
    let pending = cert.with_extension("pending.pem");
    let pending_key = leaf_key(host).with_extension("pending.pem");
    issue_leaf(&pending, &pending_key, &names)?;
    fs::rename(&pending, &cert).map_err(|e| e.to_string())?;
    fs::rename(&pending_key, leaf_key(host)).map_err(|e| e.to_string())?;
    chmod(&leaf_key(host), 0o600);
    fs::write(manifest, wanted).map_err(|e| e.to_string())?;
    Ok(())
}
fn key_matches_certificate(host: &str) -> bool {
    let cert = openssl(&["x509", "-pubkey", "-noout", "-in"], &leaf(host));
    let key = openssl(&["pkey", "-pubout", "-in"], &leaf_key(host));
    matches!((cert, key), (Ok(c), Ok(k)) if c.status.success() && k.status.success() && c.stdout == k.stdout)
}
pub fn https_port() -> u16 {
    fs::read_to_string(user_config_dir().join("https-port"))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|p| *p > 0)
        .unwrap_or_else(crate::utils::config_ports::default_https_port)
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
    let modules = get_apache_modules_dir(&get_installation_path());
    if let Some(dir) = &modules {
        if !dir.join("mod_ssl.so").exists() {
            return Err("Apache's mod_ssl is not installed (Fedora/RHEL: install the mod_ssl package).".into());
        }
    }
    let load_ssl = if modules.is_some() { "LoadModule ssl_module modules/mod_ssl.so\n" } else { "" };
    let mut conf = format!("# Managed LinuxDevStackBox HTTPS\n{load_ssl}Listen 127.0.0.1:443\nListen [::1]:443\nSSLSessionCache none\nSSLProtocol -all +TLSv1.2 +TLSv1.3\n");
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
    if let Ok(o) = openssl(&["x509", "-noout", "-enddate", "-in"], &leaf(&site.domain)) {
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
        &get_apache_exe(&get_installation_path()),
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
            let expected = openssl(&["x509", "-outform", "DER", "-in"], &leaf(&site.domain))?;
            if !expected.status.success()
                || Sha256::digest(peer) != Sha256::digest(&expected.stdout)
            {
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
    let busy = || -> String {
        "Another site or HTTPS operation is running. Wait for it to finish, then retry.".into()
    };
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(false);
    let file = options
        .open(user_config_dir().join("https-operation.lock"))
        .map_err(|_| busy())?;
    // Advisory exclusive lock (flock); released when the File is dropped.
    file.try_lock().map_err(|_| busy())?;
    Ok(file)
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
        &get_apache_exe(&get_installation_path()),
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
pub fn repair_ssl_conf() { /* Configuration is reconciled explicitly, never silently re-enabled. */
}

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
}
