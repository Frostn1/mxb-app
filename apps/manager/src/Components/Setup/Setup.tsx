import { useCallback, useEffect, useRef, useState } from "react";
import {
  FolderOpen,
  Gamepad2,
  Loader2,
  Check,
  ChevronRight,
} from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { createConfig, detectGamePath } from "@frost/shared/api/mods";
import { usePlatform } from "@frost/shared/lib/usePlatform";
import { Trans } from "@/i18n";
import { useT } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import type { GameInfo } from "@frost/shared/types";
import { Plate } from "../Shell/Brand";
import Progress from "./Progress";
import SteamStep from "./SteamStep";

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
 * Where the game keeps its user folder, per OS. Anywhere it runs as a Windows process —
 * Proton on Linux, a CrossOver/Whisky bottle on macOS — it writes inside the Wine prefix,
 * so pointing someone at `~/Documents` would send them looking in a folder the game has
 * never touched.
 */
function hintFor(platform: string | null, game: GameInfo): string {
  const appid = game.id === "gpb" ? "848050" : "655500";
  if (platform === "linux")
    return `steamapps/compatdata/${appid}/.../Documents/PiBoSo/${game.display}`;
  if (platform === "macos")
    return `Bottles/.../drive_c/users/crossover/Documents/PiBoSo/${game.display}`;
  return `Documents\\PiBoSo\\${game.display}`;
}

/**
 * First run, in the order the answers are needed:
 *
 *   1. which game — only when this build drives more than one, and only on a true first run;
 *   2. sign in with Steam — the identity every entitlement is keyed to;
 *   3. where the folders are — and only when detection couldn't work them out.
 *
 * Steam comes before the folders on purpose. The library is scanned the moment setup
 * finishes, and what that scan can open depends on the account: sealed tracks and gear
 * unlock for the account that owns them, and a store purchase is delivered to it. Asked
 * afterwards, the first library the rider ever sees is the wrong one, and every locked
 * mod in it has to be re-checked later from Settings.
 *
 * The folders step is skipped rather than shown pre-answered: detection either finds the
 * folder, in which case there was never a question, or it doesn't, in which case the step
 * has something real to ask. It used to appear either way, carrying a "Found" badge over a
 * path nobody had to do anything about.
 */
export default function Setup({ onComplete, game, games, firstRun }: SetupProps) {
  const t = useT();
  // The pick is held here rather than saved as it's made: writing a config before setup
  // finishes would make `create_config` treat a fresh install as an upgrade (and replay
  // the release showcase). It reaches the backend once, with the folders.
  const [picked, setPicked] = useState<GameInfo>(game);
  const askGame = firstRun && games.length > 1;
  // Steam is asked on a first run only. Arriving here by switching to a title whose folders
  // weren't found is not a first run: that account was linked the first time round, and a
  // step with no way past it is the wrong thing to put between a rider and a game switch.
  const askSteam = firstRun;
  const [phase, setPhase] = useState<"game" | "steam" | "detect" | "folders">(
    askGame ? "game" : askSteam ? "steam" : "detect",
  );
  // Asked once per run of setup. Someone who backs up to change the game they picked has
  // already signed in, and the account doesn't change with the title.
  const [steamDone, setSteamDone] = useState(false);
  /** Whether the silent "can detection answer the folders question?" attempt has been made. */
  const attempted = useRef(false);
  const goDetect = useCallback(() => {
    attempted.current = false;
    setPhase("detect");
  }, []);
  const defaultHint = hintFor(usePlatform(), picked);
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // MX Bikes install (Steam) folder — auto-detected on mount so the 3D rider
  // preview works out of the box, with a manual fallback when detection misses.
  const [detecting, setDetecting] = useState(true);
  const [gamePath, setGamePath] = useState<string | null>(null);
  const [gameAuto, setGameAuto] = useState(false);

  // Steps shown in the counter: the game question when it's asked, Steam when it's asked,
  // and the folders, which are counted even though they're often skipped — the rider is
  // told there are three, and finishing early is a pleasant surprise, not a missing step.
  const total = (askGame ? 1 : 0) + (askSteam ? 1 : 0) + 1;
  const current = phase === "game" ? 1 : phase === "steam" ? (askGame ? 2 : 1) : total;

  useEffect(() => {
    let cancelled = false;
    setDetecting(true);
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

  const finish = useCallback(
    async (modsPath: string) => {
      setBusy(true);
      setError(null);
      try {
        await createConfig({ modsPath, gamePath: gamePath ?? "", activeGame: picked.id });
        onComplete();
      } catch (e) {
        setError(String(e));
        setBusy(false);
      }
    },
    [gamePath, onComplete, picked.id],
  );

  // The folders question, asked of the backend first. `create_config` runs the same
  // detection the folder step's own default button runs, and refuses — without writing
  // anything — when it comes up empty. So a silent attempt is both the check and, when it
  // works, the end of setup; only a refusal puts the step on screen.
  useEffect(() => {
    if (phase !== "detect" || detecting) return;
    // Once per arrival at the step. The effect is re-run by StrictMode in development and by
    // a late `gamePath`, and this attempt writes a config when it succeeds.
    if (attempted.current) return;
    attempted.current = true;
    let cancelled = false;
    createConfig({ modsPath: "", gamePath: gamePath ?? "", activeGame: picked.id })
      .then(() => {
        if (!cancelled) onComplete();
      })
      .catch(() => {
        // Nothing to report: not finding the folder is exactly what the next step is for.
        if (!cancelled) setPhase("folders");
      });
    return () => {
      cancelled = true;
    };
  }, [phase, detecting, gamePath, picked.id, onComplete]);

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

  const progress = <Progress total={total} current={current} />;

  // Step one on a first run: which game are we setting up? Everything after this — the
  // folder we suggest, the install we scan for, the catalog we browse — depends on it,
  // so it's asked before anything else rather than inferred.
  if (phase === "game") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
          {progress}

          <div className="flex flex-col items-center gap-3.5">
            <Plate className="size-14" />
            <div className="flex flex-col items-center gap-1.5">
              <h1 className="font-cond text-[26px] font-bold leading-[1.05] tracking-[-0.045em]">
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
                  if (askSteam && !steamDone) setPhase("steam");
                  else goDetect();
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

  // Step two: the Steam account. Required — there is no way past it here.
  if (phase === "steam") {
    return (
      <SteamStep
        onDone={() => {
          setSteamDone(true);
          goDetect();
        }}
        progress={progress}
      />
    );
  }

  // Between the two: detection is being asked whether there is anything left to ask. It's
  // usually a blink, but it can be a slow disk, so it says what it's doing.
  if (phase === "detect") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
          {progress}
          <Plate className="size-14" />
          <div className="flex items-center gap-2.5 text-[13.5px] text-muted-foreground">
            <Loader2 className="size-4 flex-none animate-spin text-primary" />
            <span>{t("setup.finishing", { game: picked.display })}</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="grid min-h-0 flex-1 place-items-center px-10">
      <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
        {progress}

        <div className="flex flex-col items-center gap-3.5">
          <Plate className="size-14" />
          <div className="flex flex-col items-center gap-1.5">
            <h1 className="font-cond text-[26px] font-bold leading-[1.05] tracking-[-0.045em]">
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
            <div className="flex items-center gap-2.5 rounded-xl border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
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
            <div className="flex items-center gap-2.5 rounded-xl border border-input bg-card px-3.5 py-3 text-[12.5px] text-muted-foreground">
              <Loader2 className="size-4 flex-none animate-spin text-primary" />
              <span>{t("setup.detecting", { game: picked.display })}</span>
            </div>
          ) : gamePath ? (
            <>
              <div className="flex items-center gap-2.5 rounded-xl border border-input bg-card px-3.5 py-3 font-mono text-[12.5px] text-muted-foreground">
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
          onClick={() => void finish(chosen ?? "")}
        >
          {chosen ? t("setup.startBrowsing") : t("setup.detectAndStart")}
        </Button>
      </div>
    </div>
  );
}
