# LinuxDevStackBox

[English](README.md) · [فارسی](README.fa.md)

![License: MIT](https://img.shields.io/github/license/mamsm-dev/LinuxDevStackBox?style=flat-square)
![Linux](https://img.shields.io/badge/Linux-AppImage%20%7C%20deb%20%7C%20rpm%20%7C%20Arch-FCC624?style=flat-square&logo=linux&logoColor=black)
![Release](https://img.shields.io/github/v/release/mamsm-dev/LinuxDevStackBox?style=flat-square)

**A local PHP development environment for Linux.** Apache, MySQL/MariaDB, phpMyAdmin and PHP, managed from one desktop app and distributed as an AppImage, `.deb`, `.rpm` and Arch package. A free alternative to XAMPP/LAMPP with a modern UI.

LinuxDevStackBox is a Linux port of [DevStackBox](https://github.com/DevStackBox/DevStackBox) (originally for Windows) by Nomad Programmer, maintained by [mamsm-dev](https://github.com/mamsm-dev). It is built with Tauri (Rust) and React.

> **Status:** early port. The app runs and its services work on Arch Linux; the `.deb`, `.rpm` and update flow have had limited testing on real systems. Please report problems in [Issues](https://github.com/mamsm-dev/LinuxDevStackBox/issues).

## Features

- Start, stop and monitor Apache, MySQL/MariaDB and PHP from one window, with a system tray icon
- Multiple PHP versions (8.2 to 8.5), selectable per project
- Project workspaces and virtual hosts (`.test` domains via `/etc/hosts`)
- Local HTTPS with a generated local CA
- Database manager (create, import, export, backup, users and grants) and bundled phpMyAdmin
- Config editor with backups, log viewer, full backups and a built-in terminal
- Import from XAMPP/LAMPP (`/opt/lampp`)
- Persian (RTL) and English interface; no telemetry

## Install

Download the package for your distribution from the [latest release](https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest):

| Distribution | File | Install |
| --- | --- | --- |
| Debian / Ubuntu / Mint | `.deb` | `sudo apt install ./LinuxDevStackBox_*.deb` |
| Fedora / openSUSE / RHEL | `.rpm` | `sudo dnf install ./LinuxDevStackBox-*.rpm` |
| Arch / Manjaro / EndeavourOS | `.pkg.tar.zst` | `sudo pacman -U linuxdevstackbox-*.pkg.tar.zst` |
| Any distribution | `.AppImage` | `chmod +x LinuxDevStackBox_*.AppImage && ./LinuxDevStackBox_*.AppImage` |

The `.deb`, `.rpm` and Arch packages install Apache, MariaDB/MySQL and `php-cgi` as dependencies. The **AppImage does not bundle them**; install them yourself:

| Component | Debian/Ubuntu | Fedora | Arch |
| --- | --- | --- | --- |
| Apache | `apache2` | `httpd` | `apache` |
| Database | `mariadb-server` or `mysql-server` | `mariadb-server` | `mariadb` |
| PHP | `php-cgi php-cli php-mysql` | `php-cgi php-cli php-mysqlnd` | `php php-cgi` |

Or run `bash scripts/install-deps.sh`, which also disables the distribution's own Apache/MariaDB services so they do not clash with the app (they would otherwise compete for port 3306).

AppImage notes: if it does not start, install FUSE (`sudo apt install libfuse2`). If the window is blank, start it with `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

## Updates

On startup the app checks the [latest GitHub release](https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest). If a newer version exists it tells you and asks whether to update. With a native package (`.deb`, `.rpm`, Arch) the update is downloaded, verified against `SHA256SUMS.txt` and installed through `pkexec`; an AppImage replaces itself. You can decline and keep working. The only request the app makes is to the public GitHub API.

## Usage

- Start Apache and MySQL from the dashboard, then open `http://localhost:8080/` and `http://localhost:8080/phpmyadmin/`. The app shows the actual port.
- Apache uses port 8080 (HTTPS 8443) unless your system allows unprivileged ports below 1024 (`sudo sysctl net.ipv4.ip_unprivileged_port_start=80`).
- Web projects live in `~/LinuxDevStackBox/www`. Configuration, logs and databases live in `~/.local/share/linuxdevstackbox`.
- Editing `/etc/hosts` and trusting the HTTPS certificate ask for your password through `pkexec`.
- Extra PHP versions are not downloaded by the app. Install them with your package manager (for example `php8.4-cgi` from the Ondřej PPA) and they are detected automatically.

## Build from source

```bash
bash scripts/install-deps.sh build   # Tauri build dependencies + runtime packages
npm ci
bash scripts/build-appimage.sh       # AppImage: src-tauri/target/release/bundle/appimage/
BUNDLES=appimage,deb,rpm node scripts/build-release.mjs   # all bundles
```

For development use `npm run tauri:dev`. Requires Rust, Node.js 18+, webkit2gtk-4.1, libayatana-appindicator and librsvg2.

## Releases

Releases are built by GitHub Actions. Push a tag such as `v0.2.0`, or run the **Release packages** workflow manually, and the AppImage, `.deb`, `.rpm`, Arch package and `SHA256SUMS.txt` are attached to the release. Each version has its own release and tag, so older versions stay available in [Releases](https://github.com/mamsm-dev/LinuxDevStackBox/releases); re-running a tag replaces only that release's files.

## Website

The product page lives on the `gh-pages` branch (plain HTML/CSS/JS, English and Persian, responsive). Enable it under **Settings → Pages → Deploy from a branch → `gh-pages` / root**.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Security reports: [SECURITY.md](SECURITY.md). Privacy: [PRIVACY.md](PRIVACY.md). Changes: [CHANGELOG.md](CHANGELOG.md).

## License

MIT. Copyright (c) 2026 DevStackBox (original project) and mamsm-dev (Linux port). See [LICENSE](LICENSE).
