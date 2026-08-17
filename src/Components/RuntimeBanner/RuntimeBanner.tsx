import { AlertTriangle, Download, Loader2, X } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { RUNTIME_NAME_KEY } from "@/api/mods";
import { useFrostmod } from "@/Context/FrostmodContext";
import { Trans } from "@/i18n";
import { useT } from "@/i18n/context";

/**
 * Slim bar shown when Windows is missing a Visual C++ runtime the FrostMod chain needs.
 *
 * This is the one problem the app can't fix behind the player's back: installing a
 * Microsoft redistributable needs admin rights, so it has to be a button they press. It
 * earns a banner rather than living only in Settings because the symptom — FrostMod
 * silently failing to attach, or a bare "…dll was not found" box over the game — gives no
 * hint that Settings is where to look.
 *
 * Renders nothing when there's nothing missing, which is the overwhelmingly common case.
 */
export default function RuntimeBanner() {
  const t = useT();
  const { runtimeWarning, installingRuntime, installRuntime, dismissRuntimeWarning } =
    useFrostmod();
  if (!runtimeWarning) return null;

  // vc90 is the *game's* runtime and vc140 is FrostMod's own, so they need different
  // sentences — "MX Bikes needs this" and "FrostMod needs this" aren't interchangeable
  // when one of the two is visibly working.
  const isGameRuntime = runtimeWarning === "vc90";
  const bodyKey = isGameRuntime ? "runtime.bannerGame" : "runtime.bannerFrostmod";
  // `vc140_x86` never reaches here — the backend keeps it out of `missingRuntimes` because
  // nothing we ship is 32-bit — but the lookup covers it rather than leaving a hole that
  // would render an empty name if that ever changed.
  const nameKey = RUNTIME_NAME_KEY[runtimeWarning];

  return (
    <div className="flex items-center gap-3 border-b border-amber-500/25 bg-amber-500/10 px-4 py-2 text-sm text-foreground">
      <AlertTriangle className="size-4 shrink-0 text-amber-500" />
      <span className="min-w-0 truncate">
        <Trans
          k={bodyKey}
          values={{
            what: <span className="font-semibold">{t(nameKey)}</span>,
          }}
        />
        <span className="ml-1 text-muted-foreground">{t("runtime.pitch")}</span>
      </span>

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        <Button
          size="sm"
          onClick={() => void installRuntime(runtimeWarning)}
          disabled={installingRuntime}
        >
          {installingRuntime ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Download className="size-3.5" />
          )}
          {installingRuntime ? t("runtime.installing") : t("runtime.fixIt")}
        </Button>
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          onClick={dismissRuntimeWarning}
          disabled={installingRuntime}
          aria-label={t("runtime.dismiss")}
        >
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}
