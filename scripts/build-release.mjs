import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const tauriCli = resolve(root, "node_modules/@tauri-apps/cli/tauri.js");
function run(command, args, env = process.env) {
  const result = spawnSync(command, args, { cwd: root, env, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
run(process.execPath, [resolve(root, "scripts/verify-release.mjs")]);
// linuxdeploy's bundled strip is too old for some modern distros (.relr.dyn).
run(process.execPath, [tauriCli, "build", "--ci", "--bundles", process.env.BUNDLES || "appimage,deb,rpm"], { NO_STRIP: "true", ...process.env });
