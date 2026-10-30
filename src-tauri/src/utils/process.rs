// Process and executable utilities (Linux).
//
// All service discovery reads /proc directly: the exe symlink gives the
// binary path and cmdline gives the arguments. A service is considered "ours"
// only when its executable basename matches AND its command line references
// our generated configuration file, so system Apache/MySQL instances (started
// by systemd, XAMPP in /opt/lampp, Docker, ...) are never touched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn create_hidden_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

/// Creates a service command that runs in its own process group so stop/kill
/// can address the whole tree (Apache workers, InnoDB helpers) without
/// touching unrelated processes.
pub fn create_service_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.process_group(0);
    cmd
}

fn kill_signal(pid: u32, signal: i32, negative: bool) -> bool {
    let target = if negative { -(pid as i32) } else { pid as i32 };
    unsafe { libc::kill(target, signal) == 0 }
}

/// Kill a process tree: SIGTERM to the process group, then SIGKILL.
pub fn kill_pid(pid: u32) -> Result<(), String> {
    // -pid targets the process group created by create_service_command; when
    // the process is not a group leader, fall back to the single PID.
    if kill_signal(pid, libc::SIGTERM, true) || kill_signal(pid, libc::SIGTERM, false) {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    if kill_signal(pid, libc::SIGKILL, true) || kill_signal(pid, libc::SIGKILL, false) {
        return Ok(());
    }
    let errno = std::io::Error::last_os_error();
    if errno.kind() == std::io::ErrorKind::NotFound {
        return Ok(());
    }
    Err(format!("kill failed for pid {pid}: {errno}"))
}

// =====================================================================
// /proc scanning
// =====================================================================

struct ProcEntry {
    pid: u32,
    exe: PathBuf,
    cmdline: String,
}

/// Read /proc and return every accessible process with its exe path and
/// command line. Processes of other users (and short-lived kernel threads)
/// are simply skipped.
fn scan_processes() -> Vec<ProcEntry> {
    let mut entries = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return entries;
    };
    for entry in dir.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else { continue };
        let Ok(pid) = name.parse::<u32>() else { continue };
        let proc_dir = entry.path();
        let exe = std::fs::read_link(proc_dir.join("exe")).unwrap_or_default();
        let cmdline = std::fs::read_to_string(proc_dir.join("cmdline"))
            .map(|raw| raw.split('\0').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        if exe.as_os_str().is_empty() && cmdline.is_empty() {
            continue;
        }
        entries.push(ProcEntry { pid, exe, cmdline });
    }
    entries
}

fn image_name(entry: &ProcEntry) -> String {
    entry
        .exe
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_lowercase())
        .unwrap_or_default()
}

fn is_diagnostic_process(image: &str, command_line: &str) -> bool {
    let tokens: Vec<&str> = command_line.split_whitespace().collect();
    tokens.iter().skip(1).any(|arg| match image {
        "httpd" | "apache2" => matches!(*arg, "-v" | "-V" | "-t" | "-S" | "-M" | "-l" | "-L" | "-h" | "-?"),
        "mysqld" | "mariadbd" => matches!(*arg, "--version" | "-V" | "--help" | "-?" | "--validate-config" | "--initialize" | "--initialize-insecure"),
        _ => false,
    })
}

fn sanitize_image_name(process_name: &str) -> bool {
    !process_name.is_empty()
        && process_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

// Returns PIDs grouped by process image name (lowercase), one /proc scan.
pub fn batch_find_processes_by_image(process_names: &[&str]) -> HashMap<String, Vec<(u32, PathBuf)>> {
    let mut map: HashMap<String, Vec<(u32, PathBuf)>> = HashMap::new();
    for name in process_names {
        map.insert(name.to_lowercase(), Vec::new());
    }
    if process_names.is_empty() || process_names.iter().any(|n| !sanitize_image_name(n)) {
        return map;
    }
    for entry in scan_processes() {
        let image = image_name(&entry);
        if let Some(bucket) = map.get_mut(&image) {
            if !is_diagnostic_process(&image, &entry.cmdline) {
                bucket.push((entry.pid, entry.exe));
            }
        }
    }
    map
}

pub fn find_processes_by_image(process_name: &str) -> Vec<(u32, PathBuf)> {
    batch_find_processes_by_image(&[process_name])
        .remove(&process_name.to_lowercase())
        .unwrap_or_default()
}

pub fn is_process_running(process_name: &str) -> bool {
    !find_processes_by_image(process_name).is_empty()
}

pub fn get_process_pid(process_name: &str) -> Option<u32> {
    find_processes_by_image(process_name).into_iter().map(|(pid, _)| pid).next()
}

/// Returns true only when a process with the given image name references
/// `marker` (our generated config path) on its command line. This is the
/// canonical "is OUR service running?" check.
pub fn is_our_process_running(process_name: &str, marker: &Path) -> bool {
    !find_our_processes(process_name, marker).is_empty()
}

/// Returns the PIDs of `process_name` processes whose command line references
/// `marker`. Pass the service's generated config file (httpd.conf / my.cnf)
/// rather than the binary path, because the binary is shared with the system.
pub fn find_our_processes(process_name: &str, marker: &Path) -> Vec<u32> {
    if !sanitize_image_name(process_name) {
        return Vec::new();
    }
    let marker = marker.to_string_lossy().into_owned();
    let mut pids = Vec::new();
    for entry in scan_processes() {
        if image_name(&entry) != process_name.to_lowercase() {
            continue;
        }
        if !entry.cmdline.contains(&marker) {
            continue;
        }
        if is_diagnostic_process(&process_name.to_lowercase(), &entry.cmdline) {
            continue;
        }
        pids.push(entry.pid);
    }
    pids
}

/// Returns Ok(()) if `port` on 127.0.0.1 is free; otherwise returns a
/// human-readable error mentioning the service name.
pub fn ensure_port_available(port: u16, service: &str) -> Result<(), String> {
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    match TcpListener::bind(addr) {
        Ok(listener) => {
            // Release the port immediately so the real service can grab it.
            drop(listener);
            Ok(())
        }
        Err(e) => Err(format!(
            "Port {port} is already in use, so {service} cannot start. \
             Close the program holding that port (often a system Apache or \
             MySQL service, Docker, or a previous LinuxDevStackBox instance) \
             and try again. Underlying error: {e}"
        )),
    }
}

/// True when a TCP port accepts connections on 127.0.0.1.
pub fn is_port_listening(port: u16) -> bool {
    use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    TcpStream::connect_timeout(&addr.into(), std::time::Duration::from_millis(250)).is_ok()
}

#[cfg(test)]
mod service_probe_tests {
    use super::{is_diagnostic_process, sanitize_image_name};

    #[test]
    fn probes_are_not_servers() {
        for flag in ["-v", "-V", "-t", "-S", "-M"] {
            assert!(is_diagnostic_process("httpd", &format!("/usr/bin/httpd {flag}")));
        }
        for flag in ["--version", "--help", "--initialize-insecure", "--validate-config"] {
            assert!(is_diagnostic_process("mariadbd", &format!("mariadbd {flag}")));
        }
    }

    #[test]
    fn real_servers_and_quoted_paths_remain_visible() {
        assert!(!is_diagnostic_process("httpd", "httpd -DFOREGROUND -f /etc/linuxdevstackbox/httpd.conf"));
        assert!(!is_diagnostic_process("mariadbd", "mariadbd --defaults-file=/home/u/.config/linuxdevstackbox/my.cnf"));
        assert!(!is_diagnostic_process("httpd", ""));
    }

    #[test]
    fn image_names_are_sanitized() {
        assert!(sanitize_image_name("httpd"));
        assert!(sanitize_image_name("mariadbd"));
        assert!(!sanitize_image_name(""));
        assert!(!sanitize_image_name("httpd; rm -rf /"));
    }
}
