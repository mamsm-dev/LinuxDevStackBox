# LinuxDevStackBox

![License: MIT](https://img.shields.io/github/license/DevStackBox/DevStackBox?style=flat-square)
![Linux](https://img.shields.io/badge/Linux-AppImage-FCC624?style=flat-square&logo=linux&logoColor=black)

**A local PHP development environment for Linux** - Apache, MySQL/MariaDB, phpMyAdmin and PHP in one MIT-licensed desktop app, distributed as an AppImage.

LinuxDevStackBox is a Linux port of [DevStackBox](https://github.com/DevStackBox/DevStackBox) by Nomad Programmer.

## Stack

Server binaries are resolved in this order: a copy bundled with the app, then the system installation.

| Component   | Linux package examples |
| ----------- | ---------------------- |
| Apache 2.4  | `apache2` (Debian/Ubuntu), `httpd` (Fedora) |
| MySQL / MariaDB | `mysql-server` or `mariadb-server` |
| PHP (CGI)   | `php8.3-cgi php8.3-cli` (more versions from your distribution or the Ondrej PPA) |
| phpMyAdmin  | bundled |

Extra PHP versions are not downloaded by the app; install them with your package manager and they are detected automatically.

## Run

1. Install the packages above (`sudo apt install apache2 mariadb-server php-cgi php-cli` on Debian/Ubuntu).
2. Start Apache and MySQL from the app.
3. Open `http://localhost:8080/` and `http://localhost:8080/phpmyadmin/` (the app shows the actual port; ports below 1024 need root or `net.ipv4.ip_unprivileged_port_start`).

Web projects live in `~/LinuxDevStackBox/www`; configuration, logs and databases in `~/.local/share/linuxdevstackbox`. Virtual hosts edit `/etc/hosts` after authentication (pkexec).

## Build

```bash
npm install
npm run tauri build   # produces an AppImage when the AppImage bundle target is enabled
```

Requires Rust, Node.js and the Tauri Linux prerequisites (webkit2gtk-4.1, libayatana-appindicator, librsvg2).

## Features

- One desktop UI for services, configs, SSL, backups, and projects
- Import from XAMPP/LAMPP (`Settings → Import from XAMPP`)
- No telemetry

## Project workspaces

Connect existing folders, manage `.test` hosts and trusted HTTPS, and select an installed PHP version per website. Install extra PHP runtimes with your package manager first. The terminal currently uses global PHP.

See the [project guide](docs/getting-started/projects.mdx), [project roadmap](PROJECT-ROADMAP.md), and [release validation](docs/RELEASE-VALIDATION.md). Framework installers, database linking and Git/SSH are future work. Frameworks will download on demand; AI integration is deferred.

## v0.1.17

Released on 23 October 2026, v0.1.17 fixes Apache ancestor `.htaccess` handling and adds configuration/log diagnostics. Published artifact verification and remaining installed-site checks are tracked in [release validation](docs/RELEASE-VALIDATION.md). See the [documentation and cleanup audit](docs/development/v0.1.17-docs-audit.md) for readiness gates and proposed cleanup; no development files were deleted.

## Releasing

Maintainers: follow [Release process](docs/development/release-process.mdx) and the [Release runbook](docs/development/release-runbook.mdx) for signed builds, `finalize-release`, and GitHub publication. Download links for users always resolve to the [latest published GitHub release](https://github.com/DevStackBox/DevStackBox/releases/latest).

## Diagnostics and configuration safety

Read Apache and the selected PHP runtime error log from project details. Site changes preserve recovery copies and stop before overwriting unrelated hosts missing from saved records. See [release notes](CHANGELOG.md) for scope and limitations.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Links

- Website: [devstackbox.com](https://www.devstackbox.com)
- Issues: [GitHub Issues](https://github.com/DevStackBox/DevStackBox/issues)
- Security: [SECURITY.md](SECURITY.md) · Privacy: [PRIVACY.md](PRIVACY.md)
