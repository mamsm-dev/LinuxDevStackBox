// LinuxDevStackBox - Tauri application entry point.
//
// The binary doubles as the privileged helper for two narrowly scoped
// operations that must run as root: editing /etc/hosts (per-site hostname
// mappings) and installing the local HTTPS CA anchor into the distribution
// trust store. Both are invoked via pkexec with fixed argument shapes; the
// graphical application itself never runs with elevated privileges.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--hosts-operation") {
        let result = if args.len() == 4 { linuxdevstackbox_lib::commands::vhosts::elevated_hosts_change(&args[3], &args[2]) } else { Err("Invalid arguments".into()) };
        std::process::exit(if result.is_ok() {0} else {1});
    }
    if args.get(1).map(String::as_str) == Some("--ca-trust-operation") {
        let result = if args.len() == 3 { linuxdevstackbox_lib::commands::ssl::elevated_ca_trust(&args[2]) } else { Err("Invalid arguments".into()) };
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
