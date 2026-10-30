# LinuxDevStackBox

![Release](https://img.shields.io/github/v/release/mamsm-dev/LinuxDevStackBox?style=flat-square)
![License: MIT](https://img.shields.io/github/license/mamsm-dev/LinuxDevStackBox?style=flat-square)
![Linux](https://img.shields.io/badge/Linux-Arch%20%7C%20Debian%20%7C%20Fedora-FCC624?style=flat-square&logo=linux&logoColor=black)

**A local PHP development environment for Linux** - Apache, MariaDB/MySQL, phpMyAdmin and PHP managed from one MIT-licensed desktop app, using the runtimes your distribution already ships.

![LinuxDevStackBox v0.1.17 Services](docs/images/services-v0.1.17-hd.jpg)

Screenshot from an earlier development build. See [CHANGELOG.md](CHANGELOG.md) for the latest release notes.

## How it works

LinuxDevStackBox does not bundle web servers. It drives the Apache, MariaDB/MySQL and PHP packages installed by your distribution, generates per-user configuration, and runs everything as your user - no root, no system services touched:

| Component | Source | Default |
| --------- | ------ | ------- |
| Apache | `apache` / `apache2` / `httpd` package | `http://localhost:8080/` |
| MySQL or MariaDB | `mariadb` / `mariadb-server` package | `127.0.0.1:3306` |
| PHP + php-cgi | `php` package (8.2 or newer) | CGI handler + `php` CLI |
| phpMyAdmin | bundled inside the app | `http://localhost:8080/phpmyadmin/` |

Configuration lives in `~/.config/linuxdevstackbox/` (httpd.conf, my.cnf, php.ini, site records) and runtime data in `~/.local/share/linuxdevstackbox/` (logs, database data, your `www/` projects). Both follow the XDG Base Directory Specification and can be redirected with `LINUXDEVSTACKBOX_DATA_DIR`.

## Install

Pick the package for your distribution family from [GitHub Releases](https://github.com/mamsm-dev/LinuxDevStackBox/releases/latest):

```sh
# Arch Linux and derivatives
sudo pacman -U ./linuxdevstackbox-*.pkg.tar.zst
sudo pacman -S --needed apache mariadb php php-cgi   # runtime dependencies

# Debian and derivatives
sudo apt install ./linuxdevstackbox_*.deb

# Fedora and derivatives
sudo dnf install ./linuxdevstackbox-*.rpm
```

The `.deb` and `.rpm` declare their runtime dependencies automatically; on Arch install them once as shown. Then:

1. Launch **LinuxDevStackBox** from your application menu.
2. Start Apache and MySQL from the Services page.
3. Open `http://localhost:8080/` and `http://localhost:8080/phpmyadmin/`.

Uninstall with `sudo pacman -R linuxdevstackbox`, `sudo apt remove linuxdevstackbox` or `sudo dnf remove linuxdevstackbox`; your projects and data are kept.

## Features

- One desktop UI for services, configs, virtual hosts, trusted local HTTPS, database tooling, backups and projects
- Per-user configuration generated for the distribution Apache/MySQL/PHP; system services are never modified
- `.test` hostnames mapped in `/etc/hosts` (single polkit prompt per change) and a private local CA installed into the system trust store for trusted `https://` sites
- phpMyAdmin with upload/save directories and import limits tuned per user
- Embedded terminal with your login shell, `php -a` and a MySQL CLI session
- Full backups of `www/`, configuration and databases; import from XAMPP (`/opt/lampp`)
- Update checks that point you at the matching package-manager upgrade
- No telemetry; everything works offline after installation

## Docs

The user and developer documentation lives in [docs/](docs/index.mdx).

Using an AI coding tool? Give it [LINUXDEVSTACKBOX.md](LINUXDEVSTACKBOX.md) for runtime discovery, project setup and database guidance.

## Project workspaces

Connect existing folders, manage `.test` hosts and trusted HTTPS, and set PHP handling per website. See the [project guide](docs/getting-started/projects.mdx), the [project roadmap](PROJECT-ROADMAP.md), and [release validation](docs/RELEASE-VALIDATION.md).

## Building from source

Requires Node.js 20+, Rust and the WebKitGTK development libraries of your distribution (for example `webkit2gtk-4.1` on Arch):

```sh
npm ci
npm run build
cargo build --release --manifest-path src-tauri/Cargo.toml
# or build installable packages directly:
npx tauri build --bundles deb rpm
```

The Arch package is built from [packaging/arch/PKGBUILD](packaging/arch/PKGBUILD). See the [building guide](docs/development/building.mdx) for details.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Links

- Repository: [github.com/mamsm-dev/LinuxDevStackBox](https://github.com/mamsm-dev/LinuxDevStackBox)
- Issues: [GitHub Issues](https://github.com/mamsm-dev/LinuxDevStackBox/issues)
- Security: [SECURITY.md](SECURITY.md) - Privacy: [PRIVACY.md](PRIVACY.md)
