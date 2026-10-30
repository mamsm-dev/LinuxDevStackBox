import { useEffect, useState, useCallback } from "react";
import { motion } from "framer-motion";
import { useTranslation } from "react-i18next";
import {
  Check,
  RefreshCw,
  PackageCheck,
  TriangleAlert,
  Terminal,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { safeInvoke, isTauri } from "@/lib/tauri";
import { TAURI_COMMANDS } from "@/lib/commands";
import { useToast } from "@/hooks/use-toast";
import type { PHPVersionInfo } from "@/types/services";

export function PhpVersionsPage() {
  const { t } = useTranslation();
  const { toast } = useToast();
  const [versions, setVersions] = useState<PHPVersionInfo[]>([]);
  const [loading, setLoading] = useState(false);

  const loadVersions = useCallback(async (fresh = false) => {
    setLoading(true);
    try {
      if (!isTauri()) {
        // Browser mode fallback - show the system runtime placeholder.
        setVersions([
          {
            version: "system",
            status: "missing",
            path: "",
            is_active: false,
            installed: false,
            download_url: "",
          },
        ]);
        return;
      }
      const result = await safeInvoke<PHPVersionInfo[]>(
        TAURI_COMMANDS.php.getVersions, undefined, { fresh },
      );
      if (result) {
        setVersions(result);
      }
    } catch (err) {
      toast({
        title: t("php.loadFailed", "Failed to load PHP versions"),
        description: String(err),
        variant: "destructive",
      });
    } finally {
      setLoading(false);
    }
  }, [t, toast]);

  // Load versions on mount.
  useEffect(() => {
    loadVersions();
  }, [loadVersions]);

  const openPhpTerminal = async (version: string) => {
    try {
      await safeInvoke<string>(TAURI_COMMANDS.php.openTerminal, { version });
      toast({
        title: t("php.terminalOpened", "PHP terminal opened"),
        description: t(
          "php.terminalOpenedHint",
          "A desktop terminal running the interactive PHP shell was opened.",
        ),
      });
    } catch (err) {
      toast({
        title: t("php.terminalFailed", "Could not open a PHP terminal"),
        description: String(err),
        variant: "destructive",
      });
    }
  };

  const currentVersion = versions.find((v) => v.is_active)?.version
    ?? versions.find((v) => v.installed)?.version;
  const currentFullVersion = versions.find((v) => v.installed)?.installed_version;

  const renderStatusBadge = (v: PHPVersionInfo) => {
    if (v.is_active) {
      return (
        <Badge className="bg-primary text-primary-foreground hover:bg-primary/90">
          {t("php.active", "System default")}
        </Badge>
      );
    }
    if (v.installed) {
      return (
        <Badge variant="secondary">
          <PackageCheck className="mr-1 h-3 w-3" />
          {t("php.installed", "Installed")}
        </Badge>
      );
    }
    return <Badge variant="outline">{t("php.missing", "Not installed")}</Badge>;
  };

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.2 }}
    >
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle className="flex items-center gap-2">
                {t("php.title", "PHP Version Manager")}
                {currentVersion && <Badge variant="outline" className="font-mono">
                  {currentVersion}
                </Badge>}
                {currentFullVersion && <span className="text-sm font-normal text-muted-foreground">{currentFullVersion}</span>}
              </CardTitle>
              <CardDescription>
                {t(
                  "php.versionDescription",
                  "PHP comes from your distribution's packages. Upgrade it with your package manager (pacman, apt or dnf); LinuxDevStackBox picks up the new runtime automatically.",
                )}
              </CardDescription>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void loadVersions(true)}
              disabled={loading}
            >
              <RefreshCw
                className={`h-4 w-4 mr-2 ${loading ? "animate-spin" : ""}`}
              />
              {t("actions.refresh", "Refresh")}
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          <p className="mb-3 text-sm text-muted-foreground">{t("php.defaultScope", "The system PHP is used by the php command and by projects set to inherit it. Apache must be restarted after a PHP package upgrade so the CGI handler reloads.")}</p>
          <div className="flex flex-col gap-1">
            {versions.map((v, index) => {
              return (
                <motion.div
                  key={v.version}
                  initial={{ opacity: 0, x: -8 }}
                  animate={{ opacity: 1, x: 0 }}
                  transition={{ duration: 0.15, delay: index * 0.04 }}
                  className={`flex items-center gap-3 rounded-lg border px-3 py-2.5 ${
                    v.is_active
                      ? "border-primary bg-primary/5"
                      : "bg-card"
                  }`}
                >
                  {/* Left: version + description */}
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                      <span className="font-semibold text-sm">
                        PHP {v.version}
                      </span>
                      {v.installed_version && (
                        <Badge variant="outline" className="font-mono text-xs">
                          {v.installed_version}
                        </Badge>
                      )}
                      {v.end_of_life && <Badge variant="destructive">{t("php.endOfLife", "End of life")}</Badge>}
                      {renderStatusBadge(v)}
                    </div>
                    {v.path && (
                      <p className="text-xs text-muted-foreground mt-0.5 font-mono leading-tight break-all">
                        {v.path}
                      </p>
                    )}
                    {v.installed && !v.installed_version && <p className="text-xs text-destructive mt-1">{t("php.versionUnknown", "Installed patch version could not be read.")}</p>}
                    {v.end_of_life && (
                      <p className="text-xs text-destructive mt-1 flex items-center gap-1">
                        <TriangleAlert className="h-3 w-3" />
                        {t("php.endOfLifeHint", "This PHP branch no longer receives upstream security releases. Plan an upgrade through your package manager.")}
                      </p>
                    )}
                    {v.release_error && <div className="text-xs text-muted-foreground mt-1"><p className="mt-1 break-words select-text">{v.release_error}</p></div>}
                  </div>

                  {/* Right: action button */}
                  <div className="shrink-0">
                    {v.installed ? (
                      <Button size="sm" variant="outline" onClick={() => void openPhpTerminal(v.version)}>
                        <Terminal className="w-3.5 h-3.5 mr-1.5" />
                        {t("php.openTerminal", "Open PHP terminal")}
                      </Button>
                    ) : (
                      <Button size="sm" variant="outline" disabled>
                        <Check className="w-3.5 h-3.5 mr-1.5" />
                        {t("php.installHint", "Install the php package")}
                      </Button>
                    )}
                  </div>
                </motion.div>
              );
            })}
          </div>
        </CardContent>
      </Card>
    </motion.div>
  );
}

export default PhpVersionsPage;
