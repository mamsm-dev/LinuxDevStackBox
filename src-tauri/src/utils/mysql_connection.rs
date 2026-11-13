// Centralized MySQL client connection settings for LinuxDevStackBox.
//
// A bare `mysql -u root` can hit a system-wide MySQL/MariaDB through its unix
// socket. Always pin to 127.0.0.1 / TCP and the user my.cnf defaults file.

use std::path::PathBuf;
use std::process::Command;

use crate::utils::mysql_credentials::read_stored_root_password;
use crate::utils::paths::{
    get_installation_path, get_mysql_client_exe, get_mysqladmin_exe, get_mysqld_exe,
    get_mysqldump_exe, user_config_dir, user_mysql_data_dir,
};
use crate::utils::process::{create_hidden_command, is_our_process_running};

pub struct MysqlConnection {
    pub defaults_file: PathBuf,
    pub host: String,
    pub port: u16,
    pub protocol: String,
    pub user: String,
    pub password: Option<String>,
}

pub fn build_mysql_connection() -> Result<MysqlConnection, String> {
    Ok(MysqlConnection {
        defaults_file: user_config_dir().join("my.cnf"),
        host: "127.0.0.1".to_string(),
        port: crate::utils::config_ports::mysql_port(),
        protocol: "TCP".to_string(),
        user: "root".to_string(),
        password: read_stored_root_password(),
    })
}

fn apply_connection(cmd: &mut Command, conn: &MysqlConnection) {
    cmd.arg(format!("--defaults-file={}", conn.defaults_file.display()));
    cmd.arg("-h").arg(&conn.host);
    cmd.arg("--protocol=TCP");
    cmd.arg("-P").arg(conn.port.to_string());
    cmd.arg("-u").arg(&conn.user);
    if let Some(ref password) = conn.password {
        cmd.env("MYSQL_PWD", password);
    }
}

pub fn mysql_command() -> Result<Command, String> {
    let base = get_installation_path();
    let mysql_path = get_mysql_client_exe(&base);
    if !mysql_path.exists() {
        return Err(format!("mysql not found at {}", mysql_path.display()));
    }
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(&mysql_path.to_string_lossy());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn mysqldump_command() -> Result<Command, String> {
    let base = get_installation_path();
    let dump_path = get_mysqldump_exe(&base);
    if !dump_path.exists() {
        return Err(format!("mysqldump not found at {}", dump_path.display()));
    }
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(&dump_path.to_string_lossy());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn mysqladmin_command() -> Result<Command, String> {
    let base = get_installation_path();
    let admin_path = get_mysqladmin_exe(&base);
    if !admin_path.exists() {
        return Err(format!("mysqladmin not found at {}", admin_path.display()));
    }
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(&admin_path.to_string_lossy());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn ensure_mysql_running() -> Result<(), String> {
    let base = get_installation_path();
    let mysqld = get_mysqld_exe(&base);
    if !is_our_process_running(&exe_name(&mysqld), &mysqld) {
        return Err(
            "LinuxDevStackBox MySQL is not running. Start MySQL from the Services page.".to_string(),
        );
    }
    Ok(())
}

fn exe_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Normalize MySQL datadir strings for comparison.
/// Paths are never hardcoded per user; `verify_mysql_datadir` compares MySQL's
/// `@@datadir` against `user_mysql_data_dir()` which resolves the XDG data dir
/// (or `LINUXDEVSTACKBOX_DATA_DIR`) at runtime.
fn normalize_mysql_path(path: &str) -> String {
    let mut s = path.trim().trim_matches('"').to_string();
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    s
}

/// Mandatory check that the pinned client reached LinuxDevStackBox's datadir.
pub fn verify_mysql_datadir() -> Result<(), String> {
    let output = mysql_command()?
        .args(["--raw", "-N", "-B", "-e", "SELECT @@datadir"])
        .output()
        .map_err(|e| format!("Failed to verify MySQL datadir: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "MySQL datadir check failed: {}",
            stderr.trim()
        ));
    }

    let actual = normalize_mysql_path(&String::from_utf8_lossy(&output.stdout));
    let expected = normalize_mysql_path(&user_mysql_data_dir().to_string_lossy());
    // Compare real paths too, so a symlinked home/data dir does not trip the check.
    let canonical_matches = std::fs::canonicalize(&actual)
        .ok()
        .zip(std::fs::canonicalize(&expected).ok())
        .map(|(a, b)| a == b)
        .unwrap_or(false);

    if actual != expected && !canonical_matches {
        return Err(format!(
            "Connected to the wrong MySQL instance (datadir: {}). \
             Expected LinuxDevStackBox data at {}. Stop other MySQL/MariaDB servers \
             (system service, Docker) or ensure LinuxDevStackBox MySQL is running.",
            String::from_utf8_lossy(&output.stdout).trim(),
            user_mysql_data_dir().display()
        ));
    }

    Ok(())
}

/// Guard + datadir verify before data-bearing client queries.
pub fn prepare_mysql_client() -> Result<(), String> {
    ensure_mysql_running()?;
    verify_mysql_datadir()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_mysql_path;

    #[test]
    fn strips_trailing_slashes() {
        assert_eq!(
            normalize_mysql_path("/home/u/.local/share/linuxdevstackbox/mysql-data/"),
            normalize_mysql_path("/home/u/.local/share/linuxdevstackbox/mysql-data")
        );
    }

    #[test]
    fn strips_quotes_and_whitespace() {
        assert_eq!(normalize_mysql_path("  \"/data/mysql\"\n"), "/data/mysql");
    }

    #[test]
    fn root_is_preserved() {
        assert_eq!(normalize_mysql_path("/"), "/");
    }
}
