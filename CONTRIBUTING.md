# Contributing to LinuxDevStackBox

Thank you for your interest in contributing to LinuxDevStackBox.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Development Guidelines](#development-guidelines)
- [Submitting Changes](#submitting-changes)
- [Reporting Issues](#reporting-issues)

## Getting Started

LinuxDevStackBox is built with:

- **Backend**: Tauri 2 (Rust), driving distribution Apache/MariaDB/PHP packages
- **Frontend**: Vite + React + TypeScript
- **Styling**: Tailwind CSS + shadcn/ui
- **Animations**: Framer Motion
- **i18n**: i18next (English & Hindi)

Linux is the only supported platform. Development happens on Arch Linux; CI verifies on Ubuntu.

## Development Setup

### Prerequisites

- Node.js 20+ and npm (the app tracks package-lock.json)
- Current stable Rust
- The WebKitGTK libraries of your distribution, e.g. `webkit2gtk-4.1` on Arch, `libwebkit2gtk-4.1-dev` on Debian, `webkit2gtk4.1-devel` on Fedora
- The runtime stack for testing: `apache`/`apache2`/`httpd`, `mariadb`/`mariadb-server`, `php`, `php-cgi`

### Installation

```bash
git clone https://github.com/mamsm-dev/LinuxDevStackBox.git
cd LinuxDevStackBox
npm ci
npm run tauri:dev
```

### Build for Production

```bash
npm run build:release     # verifies inputs, builds .deb and .rpm
# Arch package:
git archive --prefix=linuxdevstackbox/ -o packaging/arch/linuxdevstackbox-$(version).tar.gz HEAD
cd packaging/arch && makepkg -f
```

See [the release runbook](docs/development/release-runbook.mdx).

## Project Structure

```
LinuxDevStackBox/
├── src/              # React frontend
│   ├── components/   # UI components (shadcn/ui)
│   ├── pages/        # Page components
│   ├── lib/          # Utilities & i18n
│   └── hooks/        # Custom React hooks
├── src-tauri/        # Rust backend
│   ├── src/          # Tauri commands & service management
│   └── capabilities/ # Tauri permissions
├── config/           # Server config templates (httpd.conf, my.cnf, etc.)
├── locales/          # i18next translations (en.json, hi.json)
├── packaging/        # Arch PKGBUILD, phpMyAdmin staging policy
├── docs/             # MDX documentation
└── scripts/          # Build & preparation scripts
```

## Development Guidelines

Documentation lives in `docs/` as MDX. When you add or rename a page, update `docs/navigation.ts` in the same pull request.

Follow [docs/development/coding-standards.mdx](docs/development/coding-standards.mdx) for user-facing copy.

### UI/UX Rules

- **Only use Tailwind CSS and shadcn/ui** - No custom CSS unless absolutely necessary
- **Dark/light mode required** - All components must support both themes
- **Use Framer Motion** for animations (not CSS keyframes)
- **Responsive design** - Test on different screen sizes
- **KDE Plasma and GNOME** are the reference desktops

### Code Style

- **TypeScript**: Use strict typing, no `any` unless necessary
- **Rust**: Follow standard Rust conventions, use `cargo fmt`; clippy must stay clean (`-D warnings`)
- **Platform code**: never reintroduce Windows-specific code; services run as the invoking user and elevation uses pkexec for the two whitelisted operations only
- **Components**: Keep components small and focused
- **i18n**: All user-facing text must use `t()` from i18next

### Key checks before pushing

```bash
npm test                                  # vitest
npx tsc --noEmit                          # typecheck
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
node scripts/validate-docs.mjs
```

## Submitting Changes

1. **Fork** the repository
2. **Create a branch**: `git checkout -b feature/your-feature-name`
3. **Make your changes** following the guidelines above
4. **Test thoroughly** on your distribution
5. **Commit**: Use clear, descriptive commit messages
6. **Push**: `git push origin feature/your-feature-name`
7. **Create a Pull Request** with detailed description

### PR Guidelines

- Reference related issues
- Include screenshots/videos for UI changes
- Ensure all tests pass
- Update documentation if needed

## Reporting Issues

Use [GitHub Issues](https://github.com/mamsm-dev/LinuxDevStackBox/issues) to report:

- Bugs
- Feature requests
- Documentation improvements

For security vulnerabilities, follow [SECURITY.md](SECURITY.md) - do not open a public issue.

### Bug Report Template

```markdown
**Environment:**

- LinuxDevStackBox Version:
- Distribution and desktop (e.g. Arch Linux, KDE Plasma):
- Installation Method: pacman / deb / rpm; include whether this was a clean install or upgrade

**Description:**
Clear description of the bug

**Steps to Reproduce:**

1. Step 1
2. Step 2
3. ...

**Expected Behavior:**
What should happen

**Actual Behavior:**
What actually happens

**Screenshots:**
If applicable
```

---

Thank you for contributing to LinuxDevStackBox!
