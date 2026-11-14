// The repository's phpMyAdmin copy comes from the git sources, which do not
// contain the compiled JavaScript (js/dist, ignored by `dist/` in .gitignore).
// Without it phpMyAdmin renders a blank page (all ajax.js/common.js... are 404).
// Fetch the official release archive of the same version and extract js/dist.
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pma = path.join(root, "phpmyadmin");
const dist = path.join(pma, "js", "dist");

if (fs.existsSync(path.join(dist, "ajax.js"))) {
  console.log("phpMyAdmin js/dist already present.");
  process.exit(0);
}

const versionFile = fs.readFileSync(path.join(pma, "libraries/classes/Version.php"), "utf8");
const version = /VERSION = '(\d+\.\d+\.\d+)'/.exec(versionFile)?.[1];
if (!version) throw new Error("Cannot determine the bundled phpMyAdmin version");

const name = `phpMyAdmin-${version}-all-languages`;
const base = `https://files.phpmyadmin.net/phpMyAdmin/${version}/${name}.tar.gz`;
console.log(`Fetching ${base}`);

async function get(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

const archive = await get(base);
const expected = (await get(`${base}.sha256`)).toString("utf8").trim().split(/\s+/)[0].toLowerCase();
const actual = createHash("sha256").update(archive).digest("hex");
if (expected !== actual) throw new Error(`phpMyAdmin archive checksum mismatch: ${actual} != ${expected}`);

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "pma-"));
try {
  const file = path.join(tmp, "pma.tar.gz");
  fs.writeFileSync(file, archive);
  execFileSync("tar", ["-xzf", file, "-C", tmp, `${name}/js/dist`], { stdio: "inherit" });
  fs.mkdirSync(path.dirname(dist), { recursive: true });
  fs.cpSync(path.join(tmp, name, "js", "dist"), dist, { recursive: true });
} finally {
  fs.rmSync(tmp, { recursive: true, force: true });
}
if (!fs.existsSync(path.join(dist, "ajax.js"))) throw new Error("js/dist/ajax.js missing after extraction");
console.log(`phpMyAdmin ${version} js/dist installed.`);
