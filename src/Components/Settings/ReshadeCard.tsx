import { useCallback, useEffect, useState } from "react";
import {
  Check,
  ExternalLink,
  FolderOpen,
  RefreshCw,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import {
  applyReshadePreset,
  deleteReshadePreset,
  reshadeStatus,
  setReshadePath,
  RESHADE_OFF,
} from "../../api/mods";
import type { ReshadePreset, ReshadeStatus } from "../../types";
import { useConfig } from "../../Context/Config";
import { useGameRunning } from "../../lib/useGameRunning";
import { useI18n } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";

/** ReShade's own site. The app never mirrors the installer — reshade.me asks that people be
 *  linked here rather than handed the binaries, so this button is the whole install path. */
const RESHADE_URL = "https://reshade.me/";

/**
 * Pick which ReShade preset the game renders with.
 *
 * Two jobs, in order of what the player needs: say whether ReShade is there at all — and if
 * it's there under the wrong graphics API, say *that*, because these games are OpenGL and a
 * DirectX install simply never loads — then let them switch between the presets they have.
 *
 * Every answer comes from one folder, normally the game's install dir. When the app's idea of
 * that folder is wrong or missing, the player can name it themselves — see `reshadePath` in
 * the config, and `Choose folder…` below, which is offered in every state where the card has
 * bad news.
 */
export default function ReshadeCard() {
  const { t } = useI18n();
  const { config, reloadConfig, game } = useConfig();
  const { running } = useGameRunning();
  const [status, setStatus] = useState<ReshadeStatus | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setStatus(await reshadeStatus());
    } catch {
      setStatus(null);
    }
  }, []);

  // Re-read on mount and whenever either folder changes — the override when it's set, the
  // game folder when it isn't.
  useEffect(() => {
    void refresh();
  }, [refresh, config.gamePath, config.reshadePath]);

  /** Name the folder ReShade is in. Kept whether or not ReShade turns out to be there:
   *  the card re-reads immediately and says which, and a rejected pick would leave the
   *  player guessing what the app disagreed with. */
  const chooseFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: t("reshade.pickFolder"),
    });
    if (typeof picked !== "string") return;
    try {
      await setReshadePath(picked);
      await reloadConfig();
      const next = await reshadeStatus();
      setStatus(next);
      if (next.installed) {
        toast.success(t("reshade.folderSet"), { description: picked });
      } else {
        toast.warning(t("reshade.notThere"), { description: picked });
      }
    } catch (e) {
      toast.error(String(e));
    }
  };

  const revertToGameFolder = async () => {
    try {
      await setReshadePath("");
      await reloadConfig();
      setStatus(await reshadeStatus());
    } catch (e) {
      toast.error(String(e));
    }
  };

  const chooseButton = (
    <Button size="sm" variant="outline" onClick={() => void chooseFolder()}>
      <FolderOpen className="size-3.5" />
      {t("reshade.browse")}
    </Button>
  );

  const gameFolderLink = status?.custom ? (
    <button
      onClick={() => void revertToGameFolder()}
      className="cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110"
    >
      {t("reshade.resetFolder")}
    </button>
  ) : null;

  const activate = async (name: string) => {
    setBusy(name);
    try {
      const outcome = await applyReshadePreset(name);
      await refresh();
      toast.success(
        outcome.gameRunning
          ? t("reshade.appliedNextLaunch", { name })
          : t("reshade.applied", { name }),
      );
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (preset: ReshadePreset) => {
    setBusy(preset.name);
    try {
      await deleteReshadePreset(preset.name);
      await refresh();
      toast.success(t("reshade.deleted", { name: preset.name }));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const openSite = () => void openUrl(RESHADE_URL);

  // No folder at all, so nothing below can be answered. Saying "not installed" here would
  // send someone off to install what they may well already have — but they can name the
  // folder themselves rather than being sent to the game-folder setting and back.
  if (!status || !status.gameDir) {
    return (
      <div className="flex flex-col items-start gap-3">
        <p className="text-[12px] leading-relaxed text-muted-foreground">
          {t("reshade.needsGameFolder")}
        </p>
        {chooseButton}
      </div>
    );
  }

  // A folder was picked and has since moved. Distinct from the case above: the path is the
  // whole answer, and "set your game folder" would be the wrong thing to say.
  if (status.folderMissing) {
    return (
      <div className="flex flex-col items-start gap-3">
        <div className="flex gap-2 rounded-lg border border-warning/40 bg-warning/10 p-3">
          <TriangleAlert className="mt-px size-4 shrink-0 text-warning" />
          <p className="text-[12px] leading-relaxed text-foreground/85">
            {t("reshade.folderMissing")}
            <br />
            <span className="font-mono text-[11.5px]">{status.gameDir}</span>
          </p>
        </div>
        {chooseButton}
        {gameFolderLink}
      </div>
    );
  }

  if (!status.installed) {
    return (
      <div className="flex flex-col gap-3">
        {status.wrongApi ? (
          <div className="flex gap-2 rounded-lg border border-warning/40 bg-warning/10 p-3">
            <TriangleAlert className="mt-px size-4 shrink-0 text-warning" />
            <p className="text-[12px] leading-relaxed text-foreground/85">
              {t("reshade.wrongApi", { dll: status.wrongApi })}
            </p>
          </div>
        ) : (
          <p className="text-[12px] leading-relaxed text-muted-foreground">
            {t("reshade.intro")}
          </p>
        )}

        <ol className="flex list-decimal flex-col gap-1 pl-4 text-[12px] leading-relaxed text-muted-foreground">
          <li>{t("reshade.step1")}</li>
          <li>{t("reshade.step2", { exe: game.exe })}</li>
          {/* The step everyone gets wrong: the installer offers DirectX first. */}
          <li className="text-foreground/85">{t("reshade.step3")}</li>
        </ol>

        {/* Which folder we looked in. Without it "not installed" is unarguable — someone
            who has ReShade sitting right there has no way to see that we're looking
            somewhere else, and no reason to think the button below applies to them. */}
        <FolderLine dir={status.gameDir} custom={status.custom} />

        <div className="flex flex-wrap gap-2">
          <Button size="sm" onClick={openSite}>
            <ExternalLink className="size-3.5" />
            {t("reshade.getIt")}
          </Button>
          {chooseButton}
          <Button size="sm" variant="ghost" onClick={() => void refresh()}>
            <RefreshCw className="size-3.5" />
            {t("reshade.recheck")}
          </Button>
        </div>
        {gameFolderLink}
      </div>
    );
  }

  const rows: ReshadePreset[] = [
    {
      name: RESHADE_OFF,
      path: "",
      active: status.active === RESHADE_OFF || status.active === "",
      managed: false,
      missingEffects: [],
    },
    ...status.presets.filter((p) => p.name !== RESHADE_OFF),
  ];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
          <span className="size-[7px] rounded-full bg-success" />
          {status.version
            ? t("reshade.installedVersion", { version: status.version })
            : t("reshade.installed")}
        </span>
        <Button size="sm" variant="ghost" onClick={() => void refresh()}>
          <RefreshCw className="size-3.5" />
          {t("reshade.recheck")}
        </Button>
      </div>

      {!status.hasShaders && (
        <div className="flex gap-2 rounded-lg border border-warning/40 bg-warning/10 p-3">
          <TriangleAlert className="mt-px size-4 shrink-0 text-warning" />
          <p className="text-[12px] leading-relaxed text-foreground/85">
            {t("reshade.noShaders")}
          </p>
        </div>
      )}

      <div className="flex flex-col overflow-hidden rounded-lg border border-input">
        {rows.map((p) => (
          <div
            key={p.name}
            className="flex items-center gap-2 border-b border-input px-3 py-2.5 last:border-b-0"
          >
            <button
              type="button"
              disabled={busy !== null}
              onClick={() => void activate(p.name)}
              className="flex flex-1 items-center gap-2 text-left disabled:opacity-60"
            >
              <Check
                className={cn(
                  "size-4 shrink-0",
                  p.active ? "text-success" : "text-transparent",
                )}
              />
              <span className="flex flex-col">
                <span className="text-[12.5px] text-foreground/85">
                  {p.name === RESHADE_OFF ? t("reshade.off") : p.name}
                </span>
                {p.missingEffects.length > 0 && (
                  <span className="text-[11px] text-warning">
                    {t("reshade.missingEffects", {
                      count: p.missingEffects.length,
                      list: p.missingEffects.slice(0, 3).join(", "),
                    })}
                  </span>
                )}
                {p.name !== RESHADE_OFF && !p.managed && (
                  <span className="text-[11px] text-muted-foreground">
                    {t("reshade.loosePreset")}
                  </span>
                )}
              </span>
            </button>
            {p.managed && (
              <Button
                size="icon"
                variant="ghost"
                disabled={busy !== null}
                aria-label={t("reshade.delete")}
                onClick={() => void remove(p)}
              >
                <Trash2 className="size-3.5" />
              </Button>
            )}
          </div>
        ))}
      </div>

      <p className="text-[11px] leading-relaxed text-muted-foreground">
        {status.presets.length === 0
          ? t("reshade.noPresets")
          : running
            ? t("reshade.nextLaunchHint")
            : t("reshade.browseHint")}
      </p>

      {/* Only worth saying once ReShade is found, and only when it isn't where the app
          would have looked on its own — otherwise it's the game folder, which the setting
          above this section already names. */}
      {status.custom && (
        <div className="flex flex-col items-start gap-1.5">
          <FolderLine dir={status.gameDir} custom />
          {gameFolderLink}
        </div>
      )}
    </div>
  );
}

/** The folder every answer on this card came from. */
function FolderLine({ dir, custom }: { dir: string; custom?: boolean }) {
  const { t } = useI18n();
  return (
    <p className="text-[11.5px] leading-relaxed text-muted-foreground">
      {custom ? t("reshade.customFolder") : t("reshade.folder")}{" "}
      <span className="break-all font-mono">{dir}</span>
    </p>
  );
}
