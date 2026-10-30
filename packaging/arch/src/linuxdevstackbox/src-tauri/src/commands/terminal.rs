// Embedded terminal session management.
//
// Spawns the user's login shell (or a PHP/MySQL interactive client) in a PTY
// (via portable-pty) and streams output to the frontend via Tauri events.
//
// Event emitted:  "terminal-output"  payload: { session_id: String, data: String }

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

use crate::utils::paths::{
    get_mysql_client_binary, get_php_binary, user_config_dir, user_www_dir,
};

// ── shared state ─────────────────────────────────────────────────────────────

pub struct TerminalSessions {
    pub inner: Mutex<HashMap<String, TerminalSession>>,
}

impl TerminalSessions {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for TerminalSessions {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TerminalSession {
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Emit a chunk of terminal output to the frontend.
fn emit_output(app: &AppHandle, session_id: &str, data: &str) {
    let _ = app.emit(
        "terminal-output",
        serde_json::json!({ "session_id": session_id, "data": data }),
    );
}

/// Read PTY output in a background thread and batch-emits to the frontend.
fn spawn_pty_output_forwarder(app: AppHandle, session_id: String, reader: Box<dyn Read + Send>) {
    use std::sync::mpsc;
    use std::time::Duration;

    let (tx, rx) = mpsc::channel::<String>();

    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut reader = reader;
        while let Ok(n @ 1..) = reader.read(&mut buf) {
            let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
            if tx.send(chunk).is_err() {
                break;
            }
        }
    });

    std::thread::spawn(move || {
        let window = Duration::from_millis(16);
        while let Ok(first) = rx.recv() {
            let mut batch = first;
            while let Ok(chunk) = rx.recv_timeout(window) {
                batch.push_str(&chunk);
            }
            emit_output(&app, &session_id, &batch);
        }
        emit_output(&app, &session_id, "\r\n[session closed]\r\n");
    });
}

/// Prefer the shell that launched the app so the terminal matches the user's
/// interactive environment; fall back through the common Linux shells.
fn login_shell() -> String {
    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.is_empty() && std::path::Path::new(&shell).is_file() {
            return shell;
        }
    }
    for candidate in ["/bin/bash", "/usr/bin/bash", "/bin/zsh", "/bin/sh"] {
        if std::path::Path::new(candidate).is_file() {
            return candidate.to_string();
        }
    }
    "/bin/sh".to_string()
}

fn build_shell_env() -> String {
    let php_dir = crate::utils::paths::active_php_dir();
    let mysql_bin = get_mysql_client_binary()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));
    let existing_path = std::env::var("PATH").unwrap_or_default();
    let mut prefix = vec![php_dir.display().to_string()];
    if let Some(dir) = mysql_bin {
        prefix.push(dir.display().to_string());
    }
    format!("{}:{}", prefix.join(":"), existing_path)
}

// ── commands ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn spawn_terminal(
    app: AppHandle,
    state: tauri::State<'_, Arc<TerminalSessions>>,
    session_id: String,
    shell_type_id: String,
) -> Result<(), String> {
    let new_path = build_shell_env();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {}", e))?;

    let mut cmd = match shell_type_id.as_str() {
        "shell" | "bash" => {
            let mut command = CommandBuilder::new(login_shell());
            command.args(["-i"]);
            command
        }
        "php-cli" => {
            let executable = get_php_binary()?;
            let mut command = CommandBuilder::new(executable);
            command.arg("-a");
            command
        }
        "mysql-cli" => {
            crate::utils::mysql_connection::prepare_mysql_client()?;
            let executable = get_mysql_client_binary()?;
            let conn = crate::utils::mysql_connection::build_mysql_connection()?;
            let mut command = CommandBuilder::new(executable);
            command.arg(format!("--defaults-file={}", conn.defaults_file.display()));
            command.args(["--protocol=TCP", "--host=127.0.0.1", "--user=root"]);
            command.arg(format!("--port={}", conn.port));
            if let Some(password) = conn.password {
                command.env("MYSQL_PWD", password);
            }
            command
        }
        _ => return Err("Unsupported terminal type".into()),
    };
    cmd.env("PATH", &new_path);
    cmd.env("PHPRC", user_config_dir());
    cmd.cwd(user_www_dir());

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn shell: {}", e))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;
    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

    spawn_pty_output_forwarder(app.clone(), session_id.clone(), reader);

    let session = TerminalSession {
        writer,
        master: pair.master,
        child,
    };

    let mut sessions = state
        .inner
        .lock()
        .map_err(|_| "State lock poisoned".to_string())?;
    if let Some(mut previous) = sessions.insert(session_id, session) {
        let _ = previous.child.kill();
    }

    Ok(())
}

#[tauri::command]
pub async fn send_terminal_input(
    state: tauri::State<'_, Arc<TerminalSessions>>,
    session_id: String,
    data: String,
) -> Result<(), String> {
    let mut sessions = state
        .inner
        .lock()
        .map_err(|_| "State lock poisoned".to_string())?;

    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("Terminal session not found: {}", session_id))?;

    session
        .writer
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed to write to PTY: {}", e))?;
    session
        .writer
        .flush()
        .map_err(|e| format!("Failed to flush PTY: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn resize_terminal(
    state: tauri::State<'_, Arc<TerminalSessions>>,
    session_id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let sessions = state
        .inner
        .lock()
        .map_err(|_| "State lock poisoned".to_string())?;

    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("Terminal session not found: {}", session_id))?;

    session
        .master
        .resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to resize PTY: {}", e))?;

    Ok(())
}

#[tauri::command]
pub async fn kill_terminal_session(
    state: tauri::State<'_, Arc<TerminalSessions>>,
    session_id: String,
) -> Result<(), String> {
    let mut sessions = state
        .inner
        .lock()
        .map_err(|_| "State lock poisoned".to_string())?;

    if let Some(mut session) = sessions.remove(&session_id) {
        let _ = session.child.kill();
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn login_shell_executes_commands() {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(login_shell());
        command.args(["-i"]);
        command.cwd(std::env::temp_dir());
        let mut child = pair.slave.spawn_command(command).unwrap();
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0; 4096];
            while let Ok(n @ 1..) = reader.read(&mut buf) {
                if tx
                    .send(String::from_utf8_lossy(&buf[..n]).to_string())
                    .is_err()
                {
                    break;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut output = String::new();
        let mut sent = false;
        while Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                output.push_str(&chunk);
                if !sent && (output.contains('$') || output.contains('%') || output.contains('#')) {
                    writer.write_all(b"echo LDSB_TERMINAL_OK\r\n").unwrap();
                    writer.flush().unwrap();
                    sent = true;
                }
                if output.contains("LDSB_TERMINAL_OK") {
                    break;
                }
            }
        }
        let _ = child.kill();
        assert!(
            output.contains("LDSB_TERMINAL_OK"),
            "shell did not execute: {output:?}"
        );
    }
}
