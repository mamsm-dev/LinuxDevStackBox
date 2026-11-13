// Optional stored password for LinuxDevStackBox MySQL root (when set via the UI).

use serde::{Deserialize, Serialize};

use crate::utils::paths::user_config_dir;

const CREDENTIALS_FILE: &str = "mysql-root.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRootCredentials {
    #[serde(default)]
    pub password: Option<String>,
    /// Legacy field from the Windows (DPAPI) build; never written, ignored on read.
    #[serde(default, skip_serializing)]
    pub protected: Option<Vec<u8>>,
}

fn credentials_path() -> std::path::PathBuf {
    user_config_dir().join(CREDENTIALS_FILE)
}

pub fn read_stored_root_password() -> Option<String> {
    let path = credentials_path();
    let raw = std::fs::read_to_string(&path).ok()?;
    let creds: StoredRootCredentials = serde_json::from_str(&raw).ok()?;
    creds.password.filter(|p| !p.is_empty())
}

/// The credential file lives in the per-user config dir and is created with
/// mode 0600 inside a 0700 directory, so only the current user can read it.
pub fn write_stored_root_password(password: Option<String>) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let path = credentials_path();
    let creds = StoredRootCredentials { password, protected: None };
    let json = serde_json::to_string_pretty(&creds)
        .map_err(|e| format!("Failed to serialize MySQL credentials: {}", e))?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| format!("Failed to write MySQL credentials: {}", e))?;
    // The file may pre-exist with looser permissions.
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write MySQL credentials: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_protected_field_is_ignored() {
        let creds: StoredRootCredentials =
            serde_json::from_str(r#"{"protected":[1,2,3]}"#).unwrap();
        assert!(creds.password.is_none());
    }
}
