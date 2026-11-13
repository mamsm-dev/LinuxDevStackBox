#!/usr/bin/env bash
# LinuxDevStackBox development environment
set -euo pipefail
cd "$(dirname "$0")/.."
command -v node >/dev/null || { echo "Node.js 18+ is required"; exit 1; }
command -v cargo >/dev/null || { echo "Rust (cargo) is required: https://rustup.rs"; exit 1; }
[ -d node_modules ] || npm install
npm run tauri:dev
