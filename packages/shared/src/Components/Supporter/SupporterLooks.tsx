import { useState } from "react";
import { Lock, RefreshCw } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Button } from "../ui/button";
import { useT, type BaseTKey } from "../../i18n/context";
import { cn } from "../../lib/utils";
import {
  PACK_COLORWAYS,
  PACK_SWATCH,
  parseAccent,
  useTheme,
  type PackColorway,
} from "../../Context/Theme";
import { recheckSupporter, useSupporter } from "../../api/supporter";
import { SUPPORT_URL } from "../../api/support";

const PACK_LABEL: Record<PackColorway, BaseTKey> = {
  night: "colorway.night",
  works: "colorway.works",
  lime: "colorway.lime",
};

/**
 * The supporter pack's looks: three extra colorways and a custom accent colour.
 *
 * Shown to everybody, so the pack can be found; usable with the pack. Nothing here replaces or
 * locks the free colorways, which each app draws on its own.
 */
export default function SupporterLooks() {
  const t = useT();
  const { supporter } = useSupporter();
  const { colorway, setColorway, accent, setAccent } = useTheme();
  const [checking, setChecking] = useState(false);

  const recheck = async () => {
    setChecking(true);
    try {
      await recheckSupporter();
    } catch {
      // Offline or signed out: the cached answer stands.
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="mt-3 space-y-3">
      <div className="flex items-start justify-between gap-4">
        <span className="flex items-center gap-1.5 text-[12.5px] text-foreground/85">
          {t("supporter.looks")}
          {!supporter && <Lock className="size-3 text-muted-foreground" />}
        </span>
        <div className="flex gap-1.5">
          {PACK_COLORWAYS.map((c) => {
            const [tint, chrome] = PACK_SWATCH[c];
            const label = t(PACK_LABEL[c]);
            return (
              <button
                key={c}
                type="button"
                title={label}
                aria-label={label}
                aria-pressed={colorway === c}
                disabled={!supporter}
                onClick={() => setColorway(c)}
                className={cn(
                  "size-6 cursor-default rounded-full border transition-shadow disabled:opacity-40",
                  colorway === c && supporter
                    ? "border-primary ring-2 ring-primary/40"
                    : "border-input hover:border-foreground/30",
                )}
                style={{ background: `linear-gradient(135deg, ${tint} 0 50%, ${chrome} 50% 100%)` }}
              />
            );
          })}
        </div>
      </div>

      <div className="flex items-center justify-between gap-4">
        <span className="flex items-center gap-1.5 text-[12.5px] text-foreground/85">
          {t("supporter.accent")}
          {!supporter && <Lock className="size-3 text-muted-foreground" />}
        </span>
        <div className="flex items-center gap-2">
          <input
            type="color"
            aria-label={t("supporter.accent")}
            disabled={!supporter}
            value={accent ?? "#0a84ff"}
            onChange={(e) => setAccent(parseAccent(e.target.value))}
            className="h-6 w-9 cursor-default rounded border border-input bg-transparent p-0 disabled:opacity-40"
          />
          {accent && supporter && (
            <Button size="sm" variant="ghost" onClick={() => setAccent(null)}>
              {t("supporter.accentReset")}
            </Button>
          )}
        </div>
      </div>

      {!supporter && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-border px-3 py-2">
          <span className="text-[12px] text-muted-foreground">{t("supporter.locked")}</span>
          <div className="flex gap-1.5">
            <Button size="sm" variant="ghost" disabled={checking} onClick={() => void recheck()}>
              <RefreshCw className={cn("size-3.5", checking && "animate-spin")} />
              {t("supporter.recheck")}
            </Button>
            <Button size="sm" onClick={() => void openUrl(SUPPORT_URL).catch(() => {})}>
              {t("supporter.get")}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
