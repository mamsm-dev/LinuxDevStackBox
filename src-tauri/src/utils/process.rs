// Process and executable utilities (Linux, /proc based).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// ELF 32-bit check is irrelevant on Linux; kept so callers compile.
pub fn is_32bit_executable(_path: &Path) -> Result<bool, String> {
    Ok(false)
}

/// Creates a plain `Command`. (The name is a leftover from the Windows port,
/// where it suppressed console windows.)
pub fn create_hidden_command(program: &str) -> Command {
    Command::new(program)
}

struct ProcEntry {
    pid: u32,
    /// Resolved path of the executable (`/proc/<pid>/exe`).
    exe: PathBuf,
    /// argv, split on NUL.
    args: Vec<String>,
}

fn read_proc_entries() -> Vec<ProcEntry> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return out };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let base = entry.path();
        // readlink fails for kernel threads and processes we may not inspect.
        let Ok(mut exe) = std::fs::read_link(base.join("exe")) else { continue };
        // A replaced binary shows up as "<path> (deleted)".
        if let Some(s) = exe.to_str() {
            if let Some(stripped) = s.strip_suffix(" (deleted)") {
                exe = PathBuf::from(stripped);
            }
        }
        let args = std::fs::read(base.join("cmdline"))
            .map(|raw| {
                raw.split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .collect()
            })
            .unwrap_or_default();
        out.push(ProcEntry { pid, exe, args });
    }
    out
}

fn exe_basename(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Returns running processes whose executable file name equals one of
/// `process_names` (e.g. "apache2", "httpd", "mysqld", "mariadbd"), as
/// `(pid, executable path)` grouped by the requested name. Diagnostic probes
/// such as `httpd -t` or `mysqld --version` are ignored.
pub fn batch_find_processes_by_image(process_names: &[&str]) -> HashMap<String, Vec<(u32, PathBuf)>> {
    let mut map: HashMap<String, Vec<(u32, PathBuf)>> = HashMap::new();
    for name in process_names {
        map.insert(name.to_string(), Vec::new());
    }
    if process_names.is_empty() {
        return map;
    }
    for p in read_proc_entries() {
        let base = exe_basename(&p.exe);
        if let Some(key) = process_names.iter().find(|n| **n == base) {
            if is_diagnostic_process(&base, &p.args) {
                continue;
            }
            map.entry(key.to_string()).or_default().push((p.pid, p.exe.clone()));
        }
    }
    map
}

pub fn find_processes_by_image(process_name: &str) -> Vec<(u32, PathBuf)> {
    batch_find_processes_by_image(&[process_name])
        .remove(process_name)
        .unwrap_or_default()
}

/// True if any process with this executable name runs (not path-aware; prefer
/// `is_our_process_running`).
pub fn is_process_running(process_name: &str) -> bool {
    !find_processes_by_image(process_name).is_empty()
}

pub fn get_process_pid(process_name: &str) -> Option<u32> {
    find_processes_by_image(process_name).first().map(|(pid, _)| *pid)
}

// Returns true only for processes whose executable equals the path we expect.
// This is the canonical "is OUR service running?" check: a system-wide Apache
// or MySQL (systemd) must never be mistaken for ours, nor be killed by us.
pub fn is_our_process_running(process_name: &str, expected_path: &Path) -> bool {
    !find_our_processes(process_name, expected_path).is_empty()
}

// Returns the PIDs of processes whose executable path matches `expected_path`.
// `process_name` is accepted for API symmetry; the match is path based.
pub fn find_our_processes(_process_name: &str, expected_path: &Path) -> Vec<u32> {
    let expected = normalize_path(expected_path);
    read_proc_entries()
        .into_iter()
        .filter(|p| normalize_path(&p.exe) == expected)
        .filter(|p| !is_diagnostic_process(&exe_basename(&p.exe), &p.args))
        .map(|p| p.pid)
        .collect()
}

// Version/configuration probes use the same executable as the server, but
// their normal exit must never establish a running-service/crash baseline.
fn is_diagnostic_process(image: &str, args: &[String]) -> bool {
    let is_apache = image == "httpd" || image == "apache2";
    let is_mysql = image == "mysqld" || image == "mariadbd";
    args.iter().skip(1).any(|arg| {
        if is_apache {
            matches!(arg.as_str(), "-v" | "-V" | "-t" | "-S" | "-M" | "-l" | "-L" | "-h" | "-?")
        } else if is_mysql {
            matches!(
                arg.as_str(),
                "--version" | "-V" | "--help" | "-?" | "--validate-config" | "--initialize"
                    | "--initialize-insecure" | "--verbose"
            )
        } else {
            false
        }
    })
}

fn normalize_path(p: &Path) -> String {
    // Canonicalize so symlinks (alternatives, /usr/sbin -> /sbin) collapse.
    std::fs::canonicalize(p)
        .map(|c| c.to_string_lossy().into_owned())
        .unwrap_or_else(|_| p.to_string_lossy().into_owned())
}

fn pid_alive(pid: u32) -> bool {
    // Zombies keep /proc/<pid> but have no exe; treat state Z as dead.
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => stat
            .rsplit(')')
            .next()
            .map(|rest| !rest.trim_start().starts_with('Z'))
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn send_signal(pid: u32, signal: &str) -> Result<(), String> {
    let output = Command::new("kill")
        .args([signal, &pid.to_string()])
        .output()
        .map_err(|e| format!("Failed to execute kill: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "kill {signal} failed for pid {pid}: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

// Stops a single PID: SIGTERM first (lets Apache/MySQL shut down cleanly and
// flush), then SIGKILL if it is still alive after a short grace period.
// Used instead of killing by name so processes of other stacks stay untouched.
pub fn kill_pid(pid: u32) -> Result<(), String> {
    if !pid_alive(pid) {
        return Ok(());
    }
    send_signal(pid, "-TERM")?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if !pid_alive(pid) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    send_signal(pid, "-KILL")?;
    std::thread::sleep(Duration::from_millis(200));
    if pid_alive(pid) {
        Err(format!("Process {pid} did not exit after SIGKILL"))
    } else {
        Ok(())
    }
}

/// Port conflict detection.
///
/// Returns Ok(()) if `port` on 127.0.0.1 is free; otherwise returns a
/// human-readable error mentioning the service name. Ports below 1024 need
/// root (or a lowered `net.ipv4.ip_unprivileged_port_start`), which is
/// reported explicitly instead of as a generic "port in use".
pub fn ensure_port_available(port: u16, service: &str) -> Result<(), String> {
    use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};

    let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
    match TcpListener::bind(addr) {
        Ok(listener) => {
            // Release the port immediately so the real service can grab it.
            drop(listener);
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(format!(
            "{service} cannot use port {port}: ports below 1024 require root on Linux. \
             Choose a port such as 8080 in the settings, or run \
             `sudo sysctl net.ipv4.ip_unprivileged_port_start=80` to allow it."
        )),
        Err(e) => Err(format!(
            "Port {port} is already in use, so {service} cannot start. \
             Close the program holding that port (often a system apache2/mysql \
             service, another web server, or a previous LinuxDevStackBox \
             instance) and try again. Underlying error: {e}"
        )),
    }
}

/// Lists TCP listeners as `(port, pid)` from `ss -ltnpH`, used to report which
/// process holds a port.
pub fn listening_pid_for_port(port: u16) -> Option<u32> {
    let out = Command::new("ss").args(["-ltnpH"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let suffix = format!(":{port}");
    for line in text.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || !cols[3].ends_with(&suffix) {
            continue;
        }
        if let Some(idx) = line.find("pid=") {
            let digits: String = line[idx + 4..].chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(pid) = digits.parse() {
                return Some(pid);
            }
        }
    }
    None
}

#[cfg(test)]
mod service_probe_tests {
    use super::is_diagnostic_process;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn probes_are_not_servers() {
        for flag in ["-v", "-V", "-t", "-S", "-M"] {
            assert!(is_diagnostic_process("apache2", &argv(&["/usr/sbin/apache2", flag])));
            assert!(is_diagnostic_process("httpd", &argv(&["/usr/sbin/httpd", flag])));
        }
        for flag in ["--version", "--help", "--initialize-insecure", "--validate-config"] {
            assert!(is_diagnostic_process("mysqld", &argv(&["mysqld", flag])));
        }
    }

    #[test]
    fn real_servers_remain_visible() {
        assert!(!is_diagnostic_process("apache2", &argv(&["apache2", "-d", "/srv/apache", "-f", "/x -v/httpd.conf"])));
        assert!(!is_diagnostic_process("apache2", &argv(&["apache2", "-k", "start"])));
        assert!(!is_diagnostic_process("mysqld", &argv(&["mysqld", "--defaults-file=/x/my.cnf"])));
        assert!(!is_diagnostic_process("apache2", &[]));
    }
}
