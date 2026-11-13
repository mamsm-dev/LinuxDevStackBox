# LinuxDevStackBox - Security Policy

## Supported versions

| Version | Security fixes |
| ------- | -------------- |
| v0.1.11+ | Yes |
| Older   | Update to latest release |

## Report a vulnerability

Email **hello@devstackbox.com**. Do not use public GitHub issues for security reports.

Include version, OS, steps to reproduce, and impact. We respond within 7 days.

## What LinuxDevStackBox does locally

- Starts and stops Apache, MySQL, and PHP
- Reads and writes config files on your PC
- Detects PHP versions installed by your package manager (no PHP downloads)
- May edit `/etc/hosts` (via pkexec) for virtual hosts (with your consent)
- Opens `localhost` in your browser

## What it does not do

- No telemetry or usage analytics
- No background cloud sync
- No account system

## PHP and server packages

PHP, Apache and MySQL come from your distribution's signed packages.

## AppImage

Verify that AppImage files come from the official release page and check their checksums before running them.

## Binary integrity

Run the AppImage from a folder you control. Do not run it from untrusted network shares.
