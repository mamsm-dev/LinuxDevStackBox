#!/usr/bin/env bash
# Installs what LinuxDevStackBox needs.
#   scripts/install-deps.sh           runtime: Apache, MariaDB/MySQL, PHP (CGI), openssl, libnss3-tools
#   scripts/install-deps.sh build     additionally the Tauri build dependencies
# The AppImage does NOT bundle Apache, MySQL/MariaDB or PHP: it uses the system ones.
set -euo pipefail
MODE="${1:-runtime}"
SUDO=""; [ "$(id -u)" -eq 0 ] || SUDO="sudo"

if command -v apt-get >/dev/null; then
  PKGS="apache2 mariadb-server php-cgi php-cli php-mysql php-mbstring php-xml php-curl php-zip php-gd openssl libnss3-tools policykit-1 xdg-utils"
  BUILD="build-essential curl wget file libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev libxdo-dev"
  $SUDO apt-get update
  $SUDO apt-get install -y $PKGS
  [ "$MODE" = build ] && $SUDO apt-get install -y $BUILD
  # The distro-managed services would fight over ports; LinuxDevStackBox runs its own instances.
  $SUDO systemctl disable --now apache2 mariadb 2>/dev/null || true
elif command -v dnf >/dev/null; then
  $SUDO dnf install -y httpd mariadb-server php-cgi php-cli php-mysqlnd php-mbstring php-xml php-gd openssl nss-tools polkit xdg-utils
  [ "$MODE" = build ] && $SUDO dnf install -y gcc gcc-c++ curl wget file webkit2gtk4.1-devel gtk3-devel librsvg2-devel libappindicator-gtk3-devel openssl-devel libxdo-devel
  $SUDO systemctl disable --now httpd mariadb 2>/dev/null || true
elif command -v pacman >/dev/null; then
  $SUDO pacman -S --needed --noconfirm apache mariadb php php-cgi openssl nss polkit xdg-utils
  [ "$MODE" = build ] && $SUDO pacman -S --needed --noconfirm base-devel curl wget file webkit2gtk-4.1 gtk3 librsvg libayatana-appindicator openssl xdotool
  $SUDO systemctl disable --now httpd mariadb 2>/dev/null || true
else
  echo "Unsupported package manager. Install Apache, MariaDB/MySQL and php-cgi manually." >&2
  exit 1
fi
echo "Done."
