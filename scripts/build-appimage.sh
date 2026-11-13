#!/usr/bin/env bash
# Builds LinuxDevStackBox as an AppImage.
# Output: src-tauri/target/release/bundle/appimage/LinuxDevStackBox_<version>_amd64.AppImage
set -euo pipefail
cd "$(dirname "$0")/.."

need() { command -v "$1" >/dev/null 2>&1 || { echo "Missing: $1 (see scripts/install-deps.sh)"; exit 1; }; }
need node; need npm; need cargo
for pkg in webkit2gtk-4.1 gtk+-3.0 librsvg-2.0; do
  pkg-config --exists "$pkg" || { echo "Missing dev package for $pkg (see scripts/install-deps.sh build)"; exit 1; }
done

[ -d node_modules ] || npm ci
npm run build:release

echo
ls -1 src-tauri/target/release/bundle/appimage/*.AppImage
