#!/usr/bin/env node
/**
 * Cross-checks the application version across every manifest that a release
 * touches, so a tag can never publish packages with inconsistent versions.
 *
 * Usage:
 *   node scripts/verify-packaging-metadata.mjs              # consistency only
 *   node scripts/verify-packaging-metadata.mjs v1.2.0       # also match tag
 *
 * Version sources checked:
 *   package.json, src-tauri/tauri.conf.json, src-tauri/Cargo.toml,
 *   src-tauri/Cargo.lock, packaging/arch/PKGBUILD,
 *   packaging/debian/changelog (upstream part of the <version>-<revision>),
 *   packaging/fedora/linuxdevstackbox.spec
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const errors = [];

function read(rel) {
  try {
    return fs.readFileSync(path.join(root, rel), "utf8");
  } catch (error) {
    errors.push(`Cannot read ${rel}: ${error.message}`);
    return null;
  }
}

function readJson(rel) {
  const text = read(rel);
  if (text === null) return null;
  try {
    return JSON.parse(text);
  } catch (error) {
    errors.push(`${rel} is not valid JSON: ${error.message}`);
    return null;
  }
}

function extractVersion(rel, pattern, description) {
  const text = read(rel);
  if (text === null) return null;
  const match = text.match(pattern);
  if (!match) {
    errors.push(`${rel}: could not find ${description}`);
    return null;
  }
  return match[1];
}

const pkg = readJson("package.json");
const appVersion = pkg?.version;

if (appVersion && !/^\d+\.\d+\.\d+$/.test(appVersion)) {
  errors.push(`package.json version "${appVersion}" is not a plain x.y.z version`);
}

for (const [rel, pattern, description] of [
  ["src-tauri/tauri.conf.json", /"version"\s*:\s*"([^"]+)"/, "the version field"],
  ["src-tauri/Cargo.toml", /^version\s*=\s*"([^"]+)"/m, "the [package] version"],
  ["src-tauri/Cargo.lock", /name = "linuxdevstackbox"\nversion = "([^"]+)"/, "the linuxdevstackbox package version"],
  ["packaging/arch/PKGBUILD", /^pkgver=([0-9.]+)$/m, "pkgver"],
  ["packaging/fedora/linuxdevstackbox.spec", /^Version:\s*([0-9.]+)\s*$/m, "the Version tag"],
]) {
  const version = extractVersion(rel, pattern, description);
  if (version !== null && version !== appVersion) {
    errors.push(`${rel} version ${version} disagrees with package.json version ${appVersion}`);
  }
}

// Debian changelog: "linuxdevstackbox (0.1.22-1) unstable; urgency=medium".
const changelogVersion = extractVersion(
  "packaging/debian/changelog",
  /^linuxdevstackbox \(([0-9.]+)(?:-[0-9]+)?\)\s+/m,
  "the latest changelog entry",
);
if (changelogVersion !== null && changelogVersion !== appVersion) {
  errors.push(`packaging/debian/changelog version ${changelogVersion} disagrees with package.json version ${appVersion}`);
}

// Sanity checks that would break package builds.
const pkgbuild = read("packaging/arch/PKGBUILD");
if (pkgbuild !== null && !pkgbuild.includes(`source=("$pkgname-$pkgver.tar.gz")`)) {
  errors.push("packaging/arch/PKGBUILD: unexpected source declaration");
}
const spec = read("packaging/fedora/linuxdevstackbox.spec");
if (spec !== null) {
  if (!/^Name:\s*linuxdevstackbox\s*$/m.test(spec)) {
    errors.push("packaging/fedora/linuxdevstackbox.spec: Name must be linuxdevstackbox");
  }
  if (!/^Source0:\s*%{name}-%{version}\.tar\.gz\s*$/m.test(spec)) {
    errors.push("packaging/fedora/linuxdevstackbox.spec: Source0 must be %{name}-%{version}.tar.gz");
  }
}
const control = read("packaging/debian/control");
if (control !== null) {
  if (!/^Package:\s*linuxdevstackbox\s*$/m.test(control)) {
    errors.push("packaging/debian/control: binary package must be named linuxdevstackbox");
  }
  if (!/^Architecture:\s*amd64\s*$/m.test(control)) {
    errors.push("packaging/debian/control: Architecture must be amd64");
  }
}

// Optional expected version (e.g. the pushed release tag "v1.2.0").
const expected = process.argv[2];
if (expected) {
  const tagVersion = expected.replace(/^v/, "");
  if (!/^\d+\.\d+\.\d+$/.test(tagVersion)) {
    errors.push(`Release tag "${expected}" is not in v<major>.<minor>.<patch> form`);
  } else if (tagVersion !== appVersion) {
    errors.push(`Release tag ${tagVersion} disagrees with application version ${appVersion}`);
  }
}

if (errors.length) {
  console.error("Packaging metadata verification FAILED:");
  for (const error of errors) console.error(`  - ${error}`);
  process.exit(1);
}
console.log(`Packaging metadata verified: version ${appVersion}${expected ? ` matches tag ${expected}` : ""}`);
