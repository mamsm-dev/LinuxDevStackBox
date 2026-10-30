// Centralized MySQL/MariaDB client connection settings for LinuxDevStackBox.
//
// Bare `mysql -u root` can hit a system MariaDB instance via its socket.
// Always pin to 127.0.0.1 / TCP and the user my.cnf defaults file.

use std::path::PathBuf;
use std::process::Command;

use crate::utils::mysql_credentials::read_stored_root_password;
use crate::utils::paths::{
    get_mysql_client_binary, get_mysqladmin_binary, get_mysqldump_binary, user_config_dir,
    user_mysql_data_dir,
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
    let mysql_path = get_mysql_client_binary()?;
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(mysql_path.as_os_str());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn mysqldump_command() -> Result<Command, String> {
    let dump_path = get_mysqldump_binary()?;
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(dump_path.as_os_str());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn mysqladmin_command() -> Result<Command, String> {
    let admin_path = get_mysqladmin_binary()?;
    let conn = build_mysql_connection()?;
    let mut cmd = create_hidden_command(admin_path.as_os_str());
    apply_connection(&mut cmd, &conn);
    Ok(cmd)
}

pub fn ensure_mysql_running() -> Result<(), String> {
    let marker = user_config_dir().join("my.cnf");
    if !is_our_process_running("mysqld", &marker)
        && !is_our_process_running("mariadbd", &marker)
    {
        return Err(
            "LinuxDevStackBox MySQL is not running. Start MySQL from the Services page.".to_string(),
        );
    }
    Ok(())
}

/// Normalize MySQL datadir strings for comparison.
/// Paths are never hardcoded per user; `verify_mysql_datadir` compares MySQL's
/// `@@datadir` against `user_mysql_data_dir()` which resolves the XDG data
/// root (or `LINUXDEVSTACKBOX_DATA_DIR`) at runtime.
fn normalize_mysql_path(path: &str) -> String {
    let mut s = path.trim().trim_matches('"').to_string();
    while s.ends_with('/') {
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

    if actual != expected {
        return Err(format!(
            "Connected to the wrong MySQL instance (datadir: {}). \
             Expected LinuxDevStackBox data at {}. Stop other MySQL installations \
             (system services, XAMPP in /opt/lampp, Docker) or ensure the \
             LinuxDevStackBox MySQL is running.",
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
    use crate::utils::paths::user_mysql_data_dir;

    #[test]
    fn trailing_slashes_are_ignored() {
        assert_eq!(
            normalize_mysql_path("/home/u/.local/share/linuxdevstackbox/mysql-data/"),
            normalize_mysql_path("/home/u/.local/share/linuxdevstackbox/mysql-data")
        );
    }

    #[test]
    fn user_data_dir_matches_forward_slash_mysql_output() {
        let expected = user_mysql_data_dir();
        let forward = format!("{}/", expected.to_string_lossy());
        assert_eq!(
            normalize_mysql_path(&forward),
            normalize_mysql_path(&expected.to_string_lossy())
        );
    }
}
