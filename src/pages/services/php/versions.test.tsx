import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { PhpVersionsPage } from "./versions";
import "@/lib/i18n";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@/lib/tauri", () => ({ safeInvoke: mocks.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
afterEach(() => vi.clearAllMocks());

async function render(versions: object[]) {
  mocks.invoke.mockImplementation(async (command: string) => command === "get_php_versions" ? versions : "");
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("__TAURI_INTERNALS__", { transformCallback: () => 1, invoke: async () => 1 });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  await act(async () => root.render(<PhpVersionsPage />));
  return { host, dispose: async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); } };
}

describe("PHP version manager", () => {
  it("shows the system runtime with branch and measured patch separately", async () => {
    const view = await render([{ version: "8.4", installed: true, is_active: true, installed_version: "8.4.27", path: "/usr/bin/php" }]);
    expect(view.host.textContent).toContain("PHP 8.4");
    expect(view.host.textContent).toContain("8.4.27");
    expect(view.host.textContent).toContain("/usr/bin/php");
    expect(view.host.textContent).toContain("System default");
    await view.dispose();
  });

  it("offers a PHP terminal for an installed runtime", async () => {
    const view = await render([{ version: "8.4", installed: true, is_active: true, installed_version: "8.4.27" }]);
    const terminal = [...view.host.querySelectorAll("button")].find(b => b.textContent?.includes("Open PHP terminal"))!;
    await act(async () => terminal.click());
    expect(mocks.invoke).toHaveBeenCalledWith("open_php_terminal", { version: "8.4" });
    await view.dispose();
  });

  it("flags a missing PHP runtime with the package install hint", async () => {
    const view = await render([{ version: "system", installed: false, release_error: "PHP is not installed." }]);
    expect(view.host.textContent).toContain("Not installed");
    expect(view.host.textContent).toContain("PHP is not installed.");
    expect([...view.host.querySelectorAll("button")].find(b => b.textContent?.includes("Install the php package"))?.disabled).toBe(true);
    await view.dispose();
  });
});
