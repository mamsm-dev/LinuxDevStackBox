// Optional stored password for LinuxDevStackBox MySQL/MariaDB root
// (when set via the UI).
//
// The credential file is created with 0600 permissions inside the user's XDG
// config directory, so only the owning user can read it.

use serde::{Deserialize, Serialize};

use crate::utils::paths::user_config_dir;

const CREDENTIALS_FILE: &str = "mysql-root.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRootCredentials {
    #[serde(default)]
    pub password: Option<String>,
}

fn credentials_path() -> std::path::PathBuf {
    user_config_dir().join(CREDENTIALS_FILE)
}

pub fn read_stored_root_password() -> Option<String> {
    let path = credentials_path();
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    let creds: StoredRootCredentials = serde_json::from_str(&raw).ok()?;
    creds.password.filter(|p| !p.is_empty())
}

pub fn write_stored_root_password(password: Option<String>) -> Result<(), String> {
    let path = credentials_path();
    if password.is_none() && !path.exists() {
        return Ok(());
    }
    let creds = StoredRootCredentials { password };
    let json = serde_json::to_string_pretty(&creds)
        .map_err(|e| format!("Failed to serialize MySQL credentials: {}", e))?;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write MySQL credentials: {}", e))?;
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write MySQL credentials: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_round_trip_and_are_user_private() {
        let path = credentials_path();
        let _ = std::fs::remove_file(&path);
        assert!(read_stored_root_password().is_none());
        write_stored_root_password(Some("linuxdevstackbox-test-password".into())).unwrap();
        assert_eq!(
            read_stored_root_password().as_deref(),
            Some("linuxdevstackbox-test-password")
        );
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        write_stored_root_password(None).unwrap();
        assert!(read_stored_root_password().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
