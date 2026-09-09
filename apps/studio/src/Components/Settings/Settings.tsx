import { ExternalLink, FolderOpen } from "lucide-react";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { revealInExplorer } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { useConfig } from "@frost/shared/Context/Config";
import { APP_NAME, LOCALE_OPTIONS, useI18n, useT } from "@/i18n";

const MANAGER_RELEASES = "https://github.com/Frostn1/mxb-app/releases/latest";

function Row({ label, value, onOpen }: { label: string; value: string; onOpen?: () => void }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border py-3 last:border-b-0">
      <div className="min-w-0">
        <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
          {label}
        </div>
        <div className="mt-1 truncate font-mono text-[12px] text-muted-foreground">
          {value || "—"}
        </div>
      </div>
      {onOpen && value && (
        <Button size="sm" variant="outline" onClick={onOpen}>
          <FolderOpen className="size-3.5" />
        </Button>
      )}
    </div>
  );
}

/**
 * The studio's settings, and deliberately short.
 *
 * The folders are shown, not edited: both apps read one config, and the mod manager is what
 * finds the game and owns first-run setup. Two setup screens that can disagree about where
 * MX Bikes lives is a support thread waiting to happen, so this says where it is looking and
 * points at the app that decides.
 */
export default function Settings() {
  const t = useT();
  const { config, game } = useConfig();
  const { locale, setLocale } = useI18n();

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl px-8 py-10">
        <h2 className="font-cond text-[18px] font-semibold tracking-[0.04em]">
          {t("studioSettings.title")}
        </h2>

        <div className="mt-6">
          <Row label={t("studioSettings.game")} value={game.display} />
          <Row
            label={t("studioSettings.mods")}
            value={config.modsPath}
            onOpen={() => void revealInExplorer(config.modsPath)}
          />
          <Row label={t("studioSettings.gameFolder")} value={config.gamePath ?? ""} />
        </div>

        <p className="mt-4 text-[12px] leading-relaxed text-muted-foreground">
          {t("studioSettings.foldersWhy")}
        </p>
        <Button
          size="sm"
          variant="outline"
          className="mt-3"
          onClick={() => void openUrl(MANAGER_RELEASES)}
        >
          <ExternalLink className="size-3.5" />
          {t("studioSettings.getManager")}
        </Button>

        <div className="mt-10">
          <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("studioSettings.language")}
          </div>
          <select
            value={locale}
            onChange={(e) => setLocale(e.target.value as typeof locale)}
            className="mt-2 w-full cursor-default border border-border bg-transparent px-3 py-2 text-[13px]"
          >
            {LOCALE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value} className="bg-background">
                {o.label}
              </option>
            ))}
          </select>
        </div>

        <p className="mt-10 text-[11.5px] text-faint">{APP_NAME}</p>
      </div>
    </div>
  );
}
