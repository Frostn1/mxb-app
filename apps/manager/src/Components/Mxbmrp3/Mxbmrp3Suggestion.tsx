import { useEffect, useSyncExternalStore } from "react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { Check, ExternalLink, Gauge, X } from "lucide-react";
import { useConfig } from "@frost/shared/Context/Config";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { mxbmrp3Store, setMxbmrp3Dismissed, shouldSuggest } from "@/lib/mxbmrp3";

/**
 * The shared status (see `mxbmrp3Store`), asked of Rust again whenever the game folder or the
 * game changes: a rider who points the app at another install, or switches title, may have it
 * on one and not the other. Rust answers rather than the config's `gamePath`, which is blank
 * when the install is found through Steam.
 */
export function useMxbmrp3() {
  const { config } = useConfig();
  const { status, snoozed } = useSyncExternalStore(mxbmrp3Store.subscribe, mxbmrp3Store.get);
  useEffect(() => {
    void mxbmrp3Store.refresh();
  }, [config?.gamePath, config?.activeGame]);

  const setDismissed = async (dismissed: boolean) => {
    await setMxbmrp3Dismissed(dismissed).catch(() => {});
    if (!dismissed) mxbmrp3Store.unsnooze();
    await mxbmrp3Store.refresh();
  };
  return { status, suggest: shouldSuggest(status, snoozed), notNow: mxbmrp3Store.snooze, setDismissed };
}

interface SuggestionProps {
  downloadUrl: string;
  onNotNow: () => void;
  onNever: () => void;
}

/**
 * What MXBMRP3 is, why it's worth having, and where to get it, with a way to say not now and
 * a way to say never. Only ever a link: the rider installs it with its own installer.
 */
export function Mxbmrp3Suggestion({ downloadUrl, onNotNow, onNever }: SuggestionProps) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-card/60 p-3.5">
      <div className="flex items-start gap-2.5">
        <Gauge className="mt-0.5 size-4 flex-none text-primary" />
        <div className="flex min-w-0 flex-col gap-1">
          <p className="text-[12.5px] font-semibold text-foreground">{t("mxbmrp3.title")}</p>
          <p className="text-[12px] leading-relaxed text-muted-foreground">{t("mxbmrp3.what")}</p>
          <p className="text-[11.5px] leading-relaxed text-faint">{t("mxbmrp3.how")}</p>
        </div>
        <button
          type="button"
          aria-label={t("mxbmrp3.notNow")}
          onClick={onNotNow}
          className="ml-auto flex-none cursor-default text-muted-foreground hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </div>
      <div className="flex items-center gap-3 pl-[26px]">
        <Button size="sm" className="h-7 px-2.5 text-xs" onClick={() => void openUrl(downloadUrl)}>
          <ExternalLink className="size-3" />
          {t("mxbmrp3.get")}
        </Button>
        <button
          type="button"
          onClick={onNotNow}
          className="cursor-default text-[11.5px] font-semibold text-muted-foreground hover:text-foreground"
        >
          {t("mxbmrp3.notNow")}
        </button>
        <button
          type="button"
          onClick={onNever}
          className="cursor-default text-[11.5px] font-semibold text-muted-foreground hover:text-foreground"
        >
          {t("mxbmrp3.never")}
        </button>
      </div>
    </div>
  );
}

/** The suggestion where it's shown unasked: the last setup step, and the top of the app. */
export function Mxbmrp3Prompt({ paused = false, className }: { paused?: boolean; className?: string }) {
  const { status, suggest, notNow, setDismissed } = useMxbmrp3();
  if (paused || !suggest || !status) return null;
  return (
    <div className={className}>
      <Mxbmrp3Suggestion
        downloadUrl={status.downloadUrl}
        onNotNow={notNow}
        onNever={() => void setDismissed(true)}
      />
    </div>
  );
}

/**
 * The Settings row: whether it's installed, the link either way, and "suggest it again" for a
 * rider who said never. It shows whatever the rider chose, because Settings is where they come
 * to look.
 */
export function Mxbmrp3SettingsRow() {
  const t = useT();
  const { status, setDismissed } = useMxbmrp3();
  if (!status) return null;
  const state =
    status.installed === true
      ? t("mxbmrp3.installed")
      : status.installed === false
        ? t("mxbmrp3.missing")
        : t("mxbmrp3.unknown");
  return (
    <div className="flex flex-col gap-1.5">
      <p className="text-[12px] text-muted-foreground">{t("mxbmrp3.settingsDesc")}</p>
      <div className="flex items-center gap-3 text-[12px]">
        <span className="flex items-center gap-1 font-semibold text-foreground">
          {status.installed === true && <Check className="size-3 text-success" strokeWidth={3} />}
          {state}
        </span>
        <button
          type="button"
          onClick={() => void openUrl(status.downloadUrl)}
          className="flex cursor-default items-center gap-1 text-[11.5px] font-semibold text-primary hover:brightness-110"
        >
          <ExternalLink className="size-3" />
          {t("mxbmrp3.get")}
        </button>
        {status.dismissed && status.installed !== true && (
          <button
            type="button"
            onClick={() => void setDismissed(false)}
            className="cursor-default text-[11.5px] font-semibold text-muted-foreground hover:text-foreground"
          >
            {t("mxbmrp3.askAgain")}
          </button>
        )}
      </div>
    </div>
  );
}
