import { useEffect, useState } from "react";
import {
  Snowflake,
  FolderOpen,
  Gamepad2,
  Loader2,
  Check,
  ChevronRight,
} from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { createConfig, detectGamePath } from "../../api/mods";
import { usePlatform } from "../../lib/usePlatform";
import { Trans } from "../../i18n";
import { useT } from "../../i18n/context";
import { Button } from "@/Components/ui/button";
import type { GameInfo } from "../../types";

interface SetupProps {
  onComplete: () => void;
  /** The title being set up. Drives the folder hint and is carried into the config, so
   *  finishing setup after a game switch doesn't drop you back on MX Bikes. */
  game: GameInfo;
  /** Every title this build can drive, for the "which game?" step. */
  games: GameInfo[];
  /** No config exists yet — a genuine first run, so ask which game before anything else.
   *  False when we got here by switching to a game whose folders weren't found: that
   *  choice has already been made. */
  firstRun: boolean;
}

/**
 * Where the game keeps its user folder, per OS. On Linux it runs under Proton and writes
 * inside the Wine prefix, so pointing someone at `~/Documents` would send them looking in
 * a folder the game has never touched.
 */
function hintFor(platform: string | null, game: GameInfo): string {
  const appid = game.id === "gpb" ? "848050" : "655500";
  if (platform === "linux")
    return `steamapps/compatdata/${appid}/.../Documents/PiBoSo/${game.display}`;
  if (platform === "macos") return `Documents/PiBoSo/${game.display}`;
  return `Documents\\PiBoSo\\${game.display}`;
}

export default function Setup({ onComplete, game, games, firstRun }: SetupProps) {
  const t = useT();
  // The pick is held here rather than saved as it's made: writing a config before setup
  // finishes would make `create_config` treat a fresh install as an upgrade (and replay
  // the release showcase). It reaches the backend once, with the folders.
  const [picked, setPicked] = useState<GameInfo>(game);
  const [phase, setPhase] = useState<"game" | "folders">(
    firstRun && games.length > 1 ? "game" : "folders",
  );
  const defaultHint = hintFor(usePlatform(), picked);
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // MX Bikes install (Steam) folder — auto-detected on mount so the 3D rider
  // preview works out of the box, with a manual fallback when detection misses.
  const [detecting, setDetecting] = useState(true);
  const [gamePath, setGamePath] = useState<string | null>(null);
  const [gameAuto, setGameAuto] = useState(false);

  useEffect(() => {
    let cancelled = false;
    detectGamePath(picked.id)
      .then((found) => {
        if (cancelled) return;
        if (found) {
          setGamePath(found);
          setGameAuto(true);
        }
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setDetecting(false);
      });
    return () => {
      cancelled = true;
    };
  }, [picked.id]);

  const finish = async (modsPath: string) => {
    setBusy(true);
    setError(null);
    try {
      await createConfig({ modsPath, gamePath: gamePath ?? "", activeGame: picked.id });
      onComplete();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const choose = async () => {
    const folder = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickModsFolder", { game: picked.display }),
    });
    if (typeof folder === "string") setChosen(folder);
  };

  const chooseGame = async () => {
    const folder = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickInstallFolder", { game: picked.display }),
    });
    if (typeof folder === "string") {
      setGamePath(folder);
      setGameAuto(false);
    }
  };

  // Step one on a first run: which game are we setting up? Everything after this — the
  // folder we suggest, the install we scan for, the catalog we browse — depends on it,
  // so it's asked before anything else rather than inferred.
  if (phase === "game") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
          <div className="flex flex-col items-center gap-3.5">
            <div className="grid size-14 place-items-center rounded-[15px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
              <Snowflake className="size-7" strokeWidth={2.5} />
            </div>
            <div className="flex flex-col items-center gap-1.5">
              <h1 className="text-[26px] font-extrabold tracking-[-0.4px]">
                {t("setup.title")}
              </h1>
              <p className="max-w-[380px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
                {t("setup.whichGame")}
              </p>
            </div>
          </div>

          <div className="flex w-full flex-col gap-2.5">
            {games.map((g) => (
              <button
                key={g.id}
                onClick={() => {
                  setPicked(g);
                  setPhase("folders");
                }}
                className="flex cursor-default items-center gap-3 rounded-xl border border-input bg-card px-4 py-4 text-left transition-colors hover:border-primary/50 hover:bg-foreground/[0.03]"
              >
                <Gamepad2 className="size-5 flex-none text-primary" />
                <span className="flex-1 text-[14.5px] font-semibold">{g.display}</span>
                <ChevronRight className="size-4 flex-none text-muted-foreground" />
              </button>
            ))}
          </div>

          <p className="text-center text-[12px] text-muted-foreground">
            {t("setup.switchLater")}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="grid min-h-0 flex-1 place-items-center px-10">
      <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
        <div className="flex flex-col items-center gap-3.5">
          <div className="grid size-14 place-items-center rounded-[15px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
            <Snowflake className="size-7" strokeWidth={2.5} />
          </div>
          <div className="flex flex-col items-center gap-1.5">
            <h1 className="text-[26px] font-extrabold tracking-[-0.4px]">
              {picked.display}
            </h1>
            <p className="max-w-[380px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
              {t("setup.tagline")}
            </p>
            {games.length > 1 && (
              <button
                onClick={() => setPhase("game")}
                className="mt-1 cursor-default text-[12px] font-semibold text-primary hover:brightness-110"
              >
                {t("setup.chooseDifferentGame")}
              </button>
            )}
          </div>
        </div>

        <div className="flex w-full flex-col gap-2.5">
          <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-faint">
            {t("setup.modsFolder", { game: picked.display })}
          </span>
          {chosen ? (
            <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
              <FolderOpen className="size-4 flex-none text-primary" />
              <span className="flex-1 truncate" title={chosen}>
                {chosen}
              </span>
            </div>
          ) : (
            <p className="text-[12.5px] text-muted-foreground">
              <Trans
                k="setup.autoDetect"
                values={{
                  hint: (
                    <span className="font-mono text-foreground/80">{defaultHint}</span>
                  ),
                }}
              />
            </p>
          )}
          <button
            onClick={choose}
            className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
          >
            {chosen ? t("setup.chooseDifferent") : t("setup.chooseManually")}
          </button>
        </div>

        <div className="flex w-full flex-col gap-2.5">
          <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-faint">
            {t("setup.gameInstall", { game: picked.display })}
          </span>
          {detecting ? (
            <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 text-[12.5px] text-muted-foreground">
              <Loader2 className="size-4 flex-none animate-spin text-primary" />
              <span>{t("setup.detecting", { game: picked.display })}</span>
            </div>
          ) : gamePath ? (
            <>
              <div className="flex items-center gap-2.5 rounded-[10px] border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
                <Gamepad2 className="size-4 flex-none text-primary" />
                <span className="flex-1 truncate" title={gamePath}>
                  {gamePath}
                </span>
                {gameAuto && (
                  <span
                    className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-primary"
                    title={t("setup.detectedAutomatically")}
                  >
                    <Check className="size-3.5" strokeWidth={2.5} />
                    {t("setup.found")}
                  </span>
                )}
              </div>
              <button
                onClick={chooseGame}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                {t("setup.chooseDifferent")}
              </button>
            </>
          ) : (
            <>
              <p className="text-[12.5px] text-muted-foreground">
                {t("setup.installNotFound")}
              </p>
              <button
                onClick={chooseGame}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                {t("setup.chooseInstallManually")}
              </button>
            </>
          )}
        </div>

        {error && (
          <p className="w-full select-text text-center text-[12px] text-destructive">
            {error}
          </p>
        )}

        <Button
          className="h-12 w-full text-[14.5px]"
          disabled={busy}
          onClick={() => finish(chosen ?? "")}
        >
          {chosen ? t("setup.startBrowsing") : t("setup.detectAndStart")}
        </Button>
      </div>
    </div>
  );
}
