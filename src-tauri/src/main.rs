/// The AppImage ships its own WebKitGTK (built on Ubuntu 22.04). On newer
/// distros/GPU drivers (Wayland, NVIDIA, recent Mesa) its web process crashes
/// with a blank window. These switches make WebKit fall back to the safe
/// renderer; users can override them by exporting the variables themselves.
fn apply_webkit_workarounds() {
    for (key, value) in [
        ("WEBKIT_DISABLE_DMABUF_RENDERER", "1"),
        ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
    ] {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
}

/// Inside an AppImage the bundled libwayland-client (from Ubuntu) can clash
/// with the host's Mesa/EGL ("Could not create default EGL display:
/// EGL_BAD_PARAMETER", blank window). LD_PRELOAD must be set before the
/// process starts, so preload the host's copy and re-exec once.
fn reexec_with_host_wayland() {
    use std::os::unix::process::CommandExt;
    if std::env::var_os("APPIMAGE").is_none()
        || std::env::var_os("LINUXDEVSTACKBOX_NO_PRELOAD").is_some()
        || std::env::var_os("LINUXDEVSTACKBOX_REEXEC").is_some()
    {
        return;
    }
    let host_lib = [
        "/usr/lib/libwayland-client.so",
        "/usr/lib/libwayland-client.so.0",
        "/usr/lib64/libwayland-client.so.0",
        "/usr/lib/x86_64-linux-gnu/libwayland-client.so.0",
        "/usr/lib/aarch64-linux-gnu/libwayland-client.so.0",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists());
    let (Some(lib), Ok(exe)) = (host_lib, std::env::current_exe()) else {
        return;
    };
    let preload = match std::env::var("LD_PRELOAD") {
        Ok(existing) if !existing.is_empty() => format!("{lib}:{existing}"),
        _ => lib.to_string(),
    };
    // exec only returns on failure; then just continue without the preload.
    let _ = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("LD_PRELOAD", preload)
        .env("LINUXDEVSTACKBOX_REEXEC", "1")
        .exec();
}

fn main() {
    reexec_with_host_wayland();
    apply_webkit_workarounds();
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--hosts-operation") {
        let result = if args.len() == 4 { linuxdevstackbox_lib::commands::vhosts::elevated_hosts_change(&args[3], &args[2]) } else { Err("Invalid arguments".into()) };
        std::process::exit(if result.is_ok() {0} else {1});
    }
    if args.iter().any(|a| a == "--setup-local-https") {
        let result = tauri::async_runtime::block_on(linuxdevstackbox_lib::commands::ssl::provision());
        if let Err(ref error) = result {
            let _ = std::fs::write(linuxdevstackbox_lib::utils::paths::user_config_dir().join("https-setup-error.txt"), error);
        }
        std::process::exit(if result.is_ok() {0} else {1});
    }
    if args.iter().any(|a| a == "--https-status") {
        let result = tauri::async_runtime::block_on(linuxdevstackbox_lib::commands::ssl::get_ssl_status());
        if let Ok(status) = result { println!("{}", serde_json::to_string_pretty(&status).unwrap()); }
        else { std::process::exit(1); }
        return;
    }
    if args.iter().any(|a| a == "--start-apache") {
        let result = tauri::async_runtime::block_on(linuxdevstackbox_lib::commands::apache::start_apache());
        std::process::exit(if result.is_ok() {0} else {1});
    }

    if std::env::args().any(|arg| arg == "--stop-services") {
        let result = tauri::async_runtime::block_on(linuxdevstackbox_lib::commands::system::stop_all_services());
        std::process::exit(if result.is_ok() { 0 } else { 1 });
    }
    linuxdevstackbox_lib::run()
}
