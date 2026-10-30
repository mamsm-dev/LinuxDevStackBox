/**
 * UpdaterContext
 *
 * Single source of truth for all app-update state.
 *
 * Linux packages are upgraded with the distribution package manager
 * (pacman/apt/dnf) - the Tauri in-place updater only supports AppImage
 * bundles. This context therefore performs update CHECKS only; when an
 * update is confirmed, the user is pointed at the release page that
 * documents the upgrade command for their distribution family.
 *
 * Rules enforced here:
 *  - ONLY this provider may call Tauri updater check APIs.
 *  - Polling: silent startup check after 2 s, then every 6 hours while the app is open.
 *  - `isDismissed` is NOT stored in context state; consumers compute it locally:
 *      localStorage.getItem("linuxdevstackbox.dismissedUpdate") === updateInfo?.version
 *  - `checkError` is set on network/server failure and cleared before each new check,
 *    ensuring the About page never shows "up to date" after a failed check.
 *  - `updateAvailable` is NOT persisted to localStorage - doing so creates stale
 *    "update available" state if the user upgrades manually. The 2 s startup delay
 *    is the accepted tradeoff (same as VS Code / GitHub Desktop).
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { APP_VERSION } from "@/lib/version";
import { isTauri, openExternalUrl, safeInvoke } from "@/lib/tauri";
import { URLS } from "@/lib/urls";
import { useToast } from "@/hooks/use-toast";
import { syncAppSettingsToBackend } from "@/lib/app-settings";
import { TAURI_COMMANDS } from "@/lib/commands";
import { useTranslation } from "react-i18next";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

// Minimal shape of the object returned by the backend update check
interface UpdateCheckMetadata {
  rid: number;
  currentVersion: string;
  version: string;
  body?: string;
  rawJson: Record<string, unknown>;
}

interface UpdateInfo {
  version: string;
  body?: string;
  available: boolean;
}

export interface UpdaterContextValue {
  /** True once an update is confirmed available from the server */
  updateAvailable: boolean;
  /** Available update metadata from the channel-aware check */
  updateInfo: UpdateInfo | null;
  /** True while checkForUpdates() is running */
  checking: boolean;
  /** Always false on Linux: there is no in-place download step. */
  downloading: boolean;
  /** Kept for interface compatibility; always 0 on Linux. */
  downloadProgress: number;
  /** Kept for interface compatibility; always false on Linux. */
  downloadIndeterminate: boolean;
  /** Kept for interface compatibility; always null on Linux. */
  downloadPhase: null;
  /** Kept for interface compatibility; always 0 on Linux. */
  downloadRetryAttempt: number;
  /** Kept for interface compatibility; always false on Linux. */
  downloadStalled: boolean;
  /** Kept for interface compatibility; always false on Linux. */
  readyToInstall: boolean;
  /** Timestamp of the last completed check (success or failure) */
  lastCheckedAt: Date | null;
  /** Non-null when the last check failed (network error, server unreachable, etc.) */
  checkError: string | null;
  /** Run an update check. Pass showNotification=true to toast "up to date". */
  checkForUpdates: (showNotification?: boolean) => Promise<void>;
  /** Open the update dialog (no re-check). Used by header badge and About button. */
  openUpdateDialog: () => void;
  /** Point the user at the release page for the package-manager upgrade. */
  downloadAndInstall: () => Promise<void>;
  /** Store dismissed version so dialog doesn't reopen for this release. */
  dismissUpdate: () => void;
  /** Whether the update dialog is currently open */
  showDialog: boolean;
  /** Internal setter - consumers should use openUpdateDialog / dismissUpdate */
  setShowDialog: (open: boolean) => void;
}

// ---------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------

const UpdaterContext = createContext<UpdaterContextValue | null>(null);

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

export function UpdaterProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const { toast } = useToast();

  const [updateAvailable, setUpdateAvailable] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [checking, setChecking] = useState(false);
  const [lastCheckedAt, setLastCheckedAt] = useState<Date | null>(null);
  const [checkError, setCheckError] = useState<string | null>(null);
  const [showDialog, setShowDialog] = useState(false);

  const checkForUpdates = useCallback(
    async (showNotification = false) => {
      if (!isTauri()) return;

      setChecking(true);
      setCheckError(null);

      try {
        await syncAppSettingsToBackend();

        const meta = await safeInvoke<UpdateCheckMetadata | null>(
          TAURI_COMMANDS.system.checkAppUpdate,
        );

        setLastCheckedAt(new Date());

        if (meta?.version) {
          setUpdateInfo({
            version: meta.version,
            body: meta.body,
            available: true,
          });
          setUpdateAvailable(true);
          const dismissedVersion = localStorage.getItem(
            "linuxdevstackbox.dismissedUpdate",
          );
          if (dismissedVersion !== meta.version) {
            setShowDialog(true);
          }
        } else {
          setUpdateInfo(null);
          setUpdateAvailable(false);
          if (showNotification)
            toast({
              title: t("updater.upToDate"),
              description: `v${APP_VERSION} ${t("updater.latestVersion")}`,
            });
        }
      } catch (err) {
        console.error("[UpdaterContext] checkForUpdates failed:", err);
        setLastCheckedAt(new Date());
        setCheckError(t("updater.checkFailed", "Could not check for updates"));
      } finally {
        setChecking(false);
      }
    },
    [t, toast],
  );

  const openUpdateDialog = useCallback(() => {
    setShowDialog(true);
  }, []);

  const downloadAndInstall = useCallback(async () => {
    if (!updateInfo || !isTauri()) return;

    localStorage.setItem("linuxdevstackbox.dismissedUpdate", updateInfo.version);
    setShowDialog(false);
    setUpdateAvailable(false);

    await openExternalUrl(URLS.download);
    toast({
      title: t("updater.linuxUpgradeTitle", "Update {{version}} available", {
        version: updateInfo.version,
      }),
      description: t(
        "updater.linuxUpgradeHint",
        "Upgrade the linuxdevstackbox package with your distribution's package manager, then restart the app.",
      ),
    });
  }, [updateInfo, t, toast]);

  const dismissUpdate = useCallback(() => {
    if (updateInfo?.version) {
      localStorage.setItem("linuxdevstackbox.dismissedUpdate", updateInfo.version);
    }
    setShowDialog(false);
  }, [updateInfo]);

  useEffect(() => {
    const autoCheck =
      localStorage.getItem("linuxdevstackbox.settings.autoCheckUpdates") !== "false";
    if (!autoCheck) return;

    const startupTimer = setTimeout(() => {
      void checkForUpdates(false);
    }, 2000);

    const pollingInterval = setInterval(
      () => void checkForUpdates(false),
      6 * 60 * 60 * 1000,
    );

    return () => {
      clearTimeout(startupTimer);
      clearInterval(pollingInterval);
    };
  }, [checkForUpdates]);

  return (
    <UpdaterContext.Provider
      value={{
        updateAvailable,
        updateInfo,
        checking,
        downloading: false,
        downloadProgress: 0,
        downloadIndeterminate: false,
        downloadPhase: null,
        downloadRetryAttempt: 0,
        downloadStalled: false,
        readyToInstall: false,
        lastCheckedAt,
        checkError,
        checkForUpdates,
        openUpdateDialog,
        downloadAndInstall,
        dismissUpdate,
        showDialog,
        setShowDialog,
      }}
    >
      {children}
    </UpdaterContext.Provider>
  );
}

export function useUpdater(): UpdaterContextValue {
  const ctx = useContext(UpdaterContext);
  if (!ctx) {
    throw new Error("useUpdater must be used inside <UpdaterProvider>");
  }
  return ctx;
}
