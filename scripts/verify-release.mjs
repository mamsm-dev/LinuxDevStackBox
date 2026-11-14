// Pre-build sanity checks for the Linux/AppImage build.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const config = JSON.parse(fs.readFileSync(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
const cargo = fs.readFileSync(path.join(root, "src-tauri/Cargo.toml"), "utf8");

if (config.productName !== "LinuxDevStackBox") throw new Error("productName must be LinuxDevStackBox");
if (config.version !== pkg.version || !cargo.includes(`version = "${pkg.version}"`)) throw new Error("Release versions disagree");
for (const target of ["appimage", "deb", "rpm"]) if (!config.bundle.targets.includes(target)) throw new Error(`bundle.targets must include ${target}`);
if (config.bundle.resources["../packaging/phpmyadmin-runtime"] !== "phpmyadmin") throw new Error("Installer must use only the staged phpMyAdmin payload");
for (const rel of ["phpmyadmin/index.php", "phpmyadmin/config.inc.php", "src-tauri/icons/icon.png", "src-tauri/icons/128x128.png"]) {
  if (!fs.existsSync(path.join(root, rel))) throw new Error(`Missing resource: ${rel}`);
}
const pmaConfig = fs.readFileSync(path.join(root, "phpmyadmin/config.inc.php"), "utf8");
if (!/AllowNoPassword'\]\s*=\s*true/.test(pmaConfig)) throw new Error("phpmyadmin/config.inc.php must set AllowNoPassword = true for local empty root login");
console.log(`Release inputs verified: ${pkg.version}`);
