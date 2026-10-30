#!/usr/bin/env node
/**
 * Verifies the Linux release inputs before a build: version agreement across
 * manifests, the staged phpMyAdmin payload, bundled resources and packaging
 * metadata. Run by scripts/build-release.mjs.
 */
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { stagePhpMyAdmin } from "./stage-phpmyadmin.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json")));
const config = JSON.parse(fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json")));
const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");
const pkgbuild = fs.readFileSync(path.join(root, "packaging/arch/PKGBUILD"), "utf8");

// The bundle must ship only the staged phpMyAdmin payload as a resource.
if (config.bundle.resources["../packaging/phpmyadmin-runtime"] !== "phpmyadmin" || config.bundle.resources["../phpmyadmin"]) {
  throw new Error("Bundle must use only the staged phpMyAdmin payload");
}
if (!config.bundle.targets.includes("deb") || !config.bundle.targets.includes("rpm")) {
  throw new Error("Bundle targets must include deb and rpm");
}
if (config.bundle.linux?.deb?.depends?.length && !config.bundle.linux.deb.depends.includes("apache2")) {
  throw new Error("Debian dependencies must declare apache2");
}

// Version agreement across every manifest users and packagers compare.
if (config.version !== pkg.version) throw new Error("package.json and tauri.conf.json versions disagree");
if (!cargo.includes(`version = "${pkg.version}"`)) throw new Error("Cargo.toml version disagrees");
if (!pkgbuild.includes(`pkgver=${pkg.version}`)) throw new Error("PKGBUILD pkgver disagrees");

// Staged phpMyAdmin payload.
stagePhpMyAdmin();
for (const rel of ["packaging/phpmyadmin-runtime/index.php", "packaging/phpmyadmin-runtime/config.inc.php"]) {
  if (!fs.existsSync(path.join(root, rel))) throw new Error(`Missing resource: ${rel}`);
}
const pmaConfig = fs.readFileSync(path.join(root, "phpmyadmin/config.inc.php"), "utf8");
if (!/AllowNoPassword'\]\s*=\s*true/.test(pmaConfig)) throw new Error("phpmyadmin/config.inc.php must set AllowNoPassword = true for local empty root login");

// Application resources and identity.
for (const rel of ["docs/images/logo/icon-512.png", "src-tauri/icons/icon.png", "packaging/arch/linuxdevstackbox.desktop", "LICENSE"]) {
  if (!fs.existsSync(path.join(root, rel))) throw new Error(`Missing resource: ${rel}`);
}
if (config.identifier !== "com.mam.linuxdevstackbox") throw new Error("Unexpected application identifier");
if (config.productName !== "LinuxDevStackBox") throw new Error("Unexpected product name");

// The Arch package must not declare bundled runtimes.
for (const banned of ["apache-bin", "mysql", "php83"]) {
  if (pkgbuild.includes(banned)) throw new Error(`PKGBUILD unexpectedly references ${banned}`);
}

console.log(`Release inputs verified for ${pkg.version} (deb + rpm + pkg.tar.zst)`);
