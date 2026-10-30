#!/usr/bin/env node
/**
 * Builds the Linux release bundles: verifies inputs, then produces the
 * Debian .deb and Fedora .rpm via the Tauri bundler. The Arch package is
 * built separately with makepkg from packaging/arch/PKGBUILD.
 */
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriCli = resolve(root, "node_modules/@tauri-apps/cli/tauri.js");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    env: process.env,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run(process.execPath, [resolve(root, "scripts/verify-release.mjs")]);
run(process.execPath, [tauriCli, "build", "--bundles", "deb", "rpm"]);
console.log("Bundles written to src-tauri/target/release/bundle/{deb,rpm}");
