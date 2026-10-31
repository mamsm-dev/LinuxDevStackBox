# LinuxDevStackBox - Security Policy

## Supported versions

| Version | Security fixes |
| ------- | -------------- |
| v0.1.24+ (Linux) | Yes |
| Older, Windows builds | Not supported; the Windows platform is end-of-life |

## Report a vulnerability

Open a private security advisory via [GitHub Security Advisories](https://github.com/mamsm-dev/LinuxDevStackBox/security/advisories/new) for this repository. Do not use public GitHub issues for security reports.

Include version, distribution, steps to reproduce, and impact.

## What LinuxDevStackBox does locally

- Starts and stops the distribution Apache and MySQL/MariaDB binaries as your user, with generated configuration under your XDG config directory
- Reads and writes config files in `~/.config/linuxdevstackbox` and data in `~/.local/share/linuxdevstackbox`
- May edit `/etc/hosts` for `.test` hostnames - one fixed-argument `pkexec` prompt per change, performed by the app binary acting as a narrow helper
- May install the local HTTPS CA anchor into the system trust store - also via a single `pkexec` prompt, followed by the distribution's trust-bundle refresh tool
- Opens `localhost` in your browser

The graphical application itself never runs with root privileges.

## What it does not do

- No telemetry or usage analytics
- No background cloud sync
- No account system
- No download of server runtimes: Apache, MySQL/MariaDB and PHP come exclusively from your distribution's signed packages

## Credential storage

An optional MySQL root password set through the UI is stored in a `0600` file in `~/.config/linuxdevstackbox/`, readable only by your user.

## Local HTTPS

The app generates a private per-user certificate authority with the system `openssl` and stores its key with `0600` permissions under your config directory. Installing that CA into the system trust store makes sites it signs trusted by browsers on this machine only.

## Package integrity

Install packages only from the official [GitHub Releases](https://github.com/mamsm-dev/LinuxDevStackBox/releases) page or a repository you trust. SHA-256 digests for release packages are published in the release notes; verify them before installing.
