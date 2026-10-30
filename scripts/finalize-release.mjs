#!/usr/bin/env node
/**
 * Finalizes release metadata: hashes the built Linux packages, injects the
 * SHA-256 lines into the matching CHANGELOG section and writes release notes.
 *
 * Usage:
 *   node scripts/finalize-release.mjs [--write-changelog] [--notes-only]
 */

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriConf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const version = tauriConf.version;
const changelogPath = join(root, "CHANGELOG.md");
const SHA_LINE = /^\*\*Package SHA-256:\*\* `[0-9A-Fa-f]{64}`\s*$/m;

function artifactPaths() {
  const base = join(root, "src-tauri/target/release/bundle");
  return [
    join(base, "deb", `LinuxDevStackBox_${version}_amd64.deb`),
    join(base, "rpm", `LinuxDevStackBox-${version}-1.x86_64.rpm`),
  ];
}

function hashFile(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex").toUpperCase();
}

function parseArgs(argv) {
  const out = { writeChangelog: false, notesOnly: false };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--write-changelog") out.writeChangelog = true;
    else if (a === "--notes-only") out.notesOnly = true;
    else throw new Error(`Unknown argument: ${a}`);
  }
  return out;
}

function extractSection(changelog, ver) {
  const header = `## [${ver}]`;
  const start = changelog.indexOf(header);
  if (start === -1) throw new Error(`CHANGELOG missing section ${header}`);
  const afterHeader = changelog.indexOf("\n", start) + 1;
  const next = changelog.indexOf("\n## [", afterHeader);
  const body = changelog.slice(afterHeader, next === -1 ? undefined : next);
  if (/Unreleased/i.test(changelog.slice(start, start + 80))) {
    throw new Error(`CHANGELOG ${header} is still Unreleased`);
  }
  return body.trimEnd();
}

function stripShaFromSection(section) {
  return section.replace(SHA_LINE, "").trimEnd();
}

function injectSha(section, shas) {
  const base = stripShaFromSection(section);
  const lines = shas.map(([name, sha]) => `- **${name} SHA-256:** \`${sha}\``);
  return `${base}\n\n**Package SHA-256:**\n\n${lines.join("\n")}`;
}

const args = parseArgs(process.argv);
const section = extractSection(readFileSync(changelogPath, "utf8"), version);

const artifacts = artifactPaths().filter((p) => existsSync(p));
if (!artifacts.length) {
  console.error("No built packages found. Run scripts/build-release.mjs first.");
  process.exit(1);
}
const hashes = artifacts.map((p) => [path.basename(p), hashFile(p)]);

const notes = injectSha(section, hashes);
if (!args.notesOnly) {
  console.log(`Release ${version} finalized\n`);
  for (const [name, sha] of hashes) console.log(`${name}\n  SHA-256: ${sha}`);
}
if (args.writeChangelog) {
  const changelog = readFileSync(changelogPath, "utf8");
  const header = `## [${version}]`;
  const start = changelog.indexOf(header);
  const next = changelog.indexOf("\n## [", start + header.length);
  const updated =
    changelog.slice(0, start) +
    header + "\n\n" + notes + "\n" +
    (next === -1 ? "" : changelog.slice(next));
  writeFileSync(changelogPath, updated);
  console.log("CHANGELOG.md updated with package hashes.");
}

// Updater manifest consumed by the in-app release check.
const latest = {
  version,
  notes: stripShaFromSection(section),
  pub_date: new Date().toISOString(),
};
writeFileSync(join(root, "latest.json"), JSON.stringify(latest, null, 2) + "\n");
console.log("latest.json written for the in-app release check.");
