import { useCallback, useEffect, useRef, useState } from "react";
import {
  FolderOpen,
  Gamepad2,
  Loader2,
  Check,
  ChevronRight,
} from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import {
  createConfig,
  completeSetup,
  detectGamePath,
  normalizeGameFolder,
  setGamePath as saveGamePath,
  type GameFolderCorrection,
} from "@frost/shared/api/mods";
import { usePlatform } from "@frost/shared/lib/usePlatform";
import { Trans } from "@/i18n";
import { useT } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import type { GameInfo } from "@frost/shared/types";
import { useFrostmod } from "@/Context/FrostmodContext";
import { LoadingMark } from "../Shell/LoadingMark";
import Progress from "./Progress";
import { Mxbmrp3Prompt } from "../Mxbmrp3/Mxbmrp3Suggestion";

const GAME_LOGOS: Record<string, string> = {
  mxb: "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/655500/logo.png",
  gpb: "https://shared.fastly.steamstatic.com/store_item_assets/steam/apps/848050/logo.png",
};

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
 *   2. where the folders are — and only when detection couldn't work them out;
 *   3. whether to enable the optional in-game integration.
 *
 * Steam is deliberately absent here. The app-level sign-in gate already settled identity
 * before setup became usable; asking again would contradict that required first screen.
 *
 * The folders step is skipped rather than shown pre-answered: detection either finds the
 * folder, in which case there was never a question, or it doesn't, in which case the step
 * has something real to ask. It used to appear either way, carrying a "Found" badge over a
 * path nobody had to do anything about.
 */
export default function Setup({ onComplete, game, games, firstRun }: SetupProps) {
  const t = useT();
  const {
    enableIntegration,
    useAppOnly: chooseAppOnly,
  } = useFrostmod();
  // The pick is held here until the folder step. `create_config` then saves the paths as an
  // explicitly incomplete setup so the integration choice can update that same config;
  // `complete_setup` is the only thing that opens the dashboard afterwards.
  const [picked, setPicked] = useState<GameInfo>(game);
  const askGame = firstRun && games.length > 1;
  // A first run always asks explicitly. The provider may infer "enabled" for an existing
  // FrostMod install as a backwards-compatibility measure, but that is not consent for a
  // new setup (and a stale localStorage choice should not silently skip this screen).
  const needsIntegration = firstRun;
  const [phase, setPhase] = useState<
    "game" | "detect" | "folders" | "integration"
  >(
    askGame ? "game" : "detect",
  );
  // Keep the fourth step in the counter while enabling it. The provider records the
  // choice before its install finishes, but the screen is still step four until we leave.
  const askIntegration = needsIntegration || phase === "integration";
  /** Whether the silent "can detection answer the folders question?" attempt has been made. */
  const attempted = useRef(false);
  const goDetect = useCallback(() => {
    attempted.current = false;
    setPhase("detect");
  }, []);
  const defaultHint = hintFor(usePlatform(), picked);
  const [chosen, setChosen] = useState<string | null>(null);
  const [folderCorrection, setFolderCorrection] = useState<GameFolderCorrection | null>(null);
  const [busy, setBusy] = useState(false);
  const [integrationBusy, setIntegrationBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // MX Bikes install (Steam) folder — auto-detected on mount so the 3D rider
  // preview works out of the box, with a manual fallback when detection misses.
  const [detecting, setDetecting] = useState(true);
  const [gamePath, setGamePath] = useState<string | null>(null);
  const [gameAuto, setGameAuto] = useState(false);

  // Steps shown in the counter: game selection when needed, the folder, and optional
  // Game Integration consent. Steam was already handled by the entry gate.
  const folderStep = (askGame ? 1 : 0) + 1;
  const total = folderStep + (askIntegration ? 1 : 0);
  const current =
    phase === "game"
      ? 1
      : phase === "integration"
          ? total
          : folderStep;

  useEffect(() => {
    let cancelled = false;
    // A path belongs to one game. Never carry a detected install or a manual mods
    // destination across the game picker into the other title.
    setGamePath(null);
    setGameAuto(false);
    setChosen(null);
    setFolderCorrection(null);
    setError(null);
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
        if (askIntegration) setPhase("integration");
        else onComplete();
      } catch (e) {
        setError(String(e));
        setBusy(false);
      }
    },
    [askIntegration, gamePath, onComplete, picked.id],
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
        if (!cancelled) {
          if (askIntegration) setPhase("integration");
          else onComplete();
        }
      })
      .catch(() => {
        // Nothing to report: not finding the folder is exactly what the next step is for.
        if (!cancelled) setPhase("folders");
      });
    return () => {
      cancelled = true;
    };
  }, [phase, detecting, gamePath, picked.id, askIntegration, onComplete]);

  const choose = async () => {
    const folder = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickModsFolder", { game: picked.display }),
    });
    if (typeof folder === "string") {
      const normalized = await normalizeGameFolder(folder).catch(() => ({
        path: folder,
        correction: null,
      }));
      setChosen(normalized.path);
      setFolderCorrection(normalized.correction);
      setError(null);
    }
  };

  const chooseGame = async (persist = false): Promise<string | null> => {
    const folder = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickInstallFolder", { game: picked.display }),
    });
    if (typeof folder === "string") {
      try {
        if (persist) await saveGamePath(folder);
        setGamePath(folder);
        setGameAuto(false);
        setError(null);
        return folder;
      } catch (e) {
        setError(String(e));
      }
    }
    return null;
  };

  const progress = <Progress total={total} current={current} />;

  const finishIntegration = async (enabled: boolean) => {
    // Integration attaches to the installed game, not its Documents/PiBoSo data folder.
    // If Steam detection missed that install, keep the player inside setup: choosing it
    // here saves it to the config we created on the previous step and then continues.
    if (enabled && !gamePath) {
      const selected = await chooseGame(true);
      if (!selected) return;
    }
    setIntegrationBusy(true);
    setError(null);
    try {
      if (enabled) await enableIntegration();
      else await chooseAppOnly();
      await completeSetup();
      onComplete();
    } catch (e) {
      setError(String(e));
    } finally {
      setIntegrationBusy(false);
    }
  };

  // Step one on a first run: which game are we setting up? Everything after this — the
  // folder we suggest, the install we scan for, the catalog we browse — depends on it,
  // so it's asked before anything else rather than inferred.
  if (phase === "game") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
          {progress}

          <div className="flex flex-col items-center gap-3.5">
            <div className="flex flex-col items-center gap-1.5">
              <h1 className="font-cond text-[26px] font-bold leading-[1.05] tracking-[-0.045em]">
                {t("setup.title")}
              </h1>
              <p className="max-w-[380px] text-center text-[13.5px] leading-relaxed text-muted-foreground">
                {t("setup.whichGame")}
              </p>
            </div>
          </div>

          <div className="grid w-full grid-cols-2 border-y border-border">
            {games.map((g) => (
              <button
                key={g.id}
                onClick={() => {
                  setPicked(g);
                  goDetect();
                }}
                className="group flex min-h-32 cursor-default flex-col items-center justify-center gap-4 px-6 py-6 text-center transition-colors even:border-l even:border-border hover:bg-foreground/[0.03]"
              >
                <img
                  src={GAME_LOGOS[g.id]}
                  alt=""
                  className="h-12 w-full max-w-36 object-contain"
                />
                <span className="flex items-center gap-1.5 text-[12px] font-semibold text-muted-foreground transition-colors group-hover:text-foreground">
                  {g.display}
                  <ChevronRight className="size-3.5" />
                </span>
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

  if (phase === "integration") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col gap-8 pb-16">
          <div className="self-center">{progress}</div>

          <div className="flex flex-col gap-2.5">
            <span className="font-cond text-[11px] font-bold uppercase tracking-[0.12em] text-primary">
              {t("integration.optional")}
            </span>
            <h1 className="font-cond text-[30px] font-bold leading-[1.05] tracking-[-0.045em]">
              {t("setup.integrationTitle")}
            </h1>
            <p className="max-w-[450px] text-[13.5px] leading-relaxed text-muted-foreground">
              {t("setup.integrationIntro")}
            </p>
            <p className="max-w-[450px] text-[13px] font-medium leading-relaxed text-foreground/85">
              {t("setup.integrationCrashFixes", { game: picked.display })}
            </p>
            <p className="max-w-[450px] text-[12px] leading-relaxed text-faint">
              {t("setup.integrationInstallDisclosure")}
            </p>
          </div>

          {!gamePath && (
            <div className="border-l-2 border-primary py-1 pl-4">
              <p className="text-[12px] font-semibold text-foreground">
                {t("setup.integrationInstallRequired", { game: picked.display })}
              </p>
              <p className="mt-1 text-[12px] leading-relaxed text-muted-foreground">
                {t("setup.integrationInstallRequiredDesc", { game: picked.display })}
              </p>
            </div>
          )}

          <div className="flex items-center gap-5">
            <Button
              className="h-12 flex-1 text-[14px]"
              disabled={integrationBusy}
              onClick={() => void finishIntegration(true)}
            >
              {integrationBusy && <Loader2 className="animate-spin" />}
              {gamePath
                ? t("setup.integrationEnable")
                : t("setup.integrationChooseInstall")}
            </Button>
            <button
              type="button"
              disabled={integrationBusy}
              onClick={() => void finishIntegration(false)}
              className="flex-none cursor-default text-[12px] font-semibold text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
            >
              {t("setup.integrationUseAppOnly")}
            </button>
          </div>
          {/* Beside the choice, not part of it: nothing here waits on it. */}
          <Mxbmrp3Prompt />
          {error && (
            <p className="select-text text-center text-[12px] text-destructive">
              {error}
            </p>
          )}
        </div>
      </div>
    );
  }

  // Between the two: detection is being asked whether there is anything left to ask. It's
  // usually a blink, but it can be a slow disk, so it says what it's doing.
  if (phase === "detect") {
    return (
      <div className="grid min-h-0 flex-1 place-items-center px-10">
        <div className="flex w-full max-w-[480px] flex-col items-center gap-7 pb-16">
          {progress}
          <div className="flex items-center gap-2.5 text-[13.5px] text-muted-foreground">
            <LoadingMark
              className="flex-none text-primary"
              label={t("setup.finishing", { game: picked.display })}
            />
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
          <div className="flex flex-col items-center gap-1.5">
            <h1 className="sr-only">{picked.display}</h1>
            <img
              src={GAME_LOGOS[picked.id]}
              alt=""
              className="mb-1 h-11 w-full max-w-44 object-contain"
            />
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
          <div className="flex items-center justify-between gap-4">
            <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-foreground/70">
              {t("setup.modsFolder", { game: picked.display })}
            </span>
            <span className="text-[10.5px] font-bold uppercase tracking-[0.08em] text-primary">
              {t("setup.required")}
            </span>
          </div>
          {chosen ? (
            <div className="flex items-center gap-2.5 border-l-2 border-primary py-1 pl-3 font-mono text-[12.5px] text-foreground/80">
              <FolderOpen className="size-4 flex-none text-primary" />
              <span className="flex-1 truncate" title={chosen}>
                {chosen}
              </span>
            </div>
          ) : (
            <p className="text-[13px] leading-relaxed text-foreground/85">
              <Trans
                k="setup.autoDetect"
                values={{
                  game: picked.display,
                  hint: (
                    <span className="font-mono text-foreground/80">{defaultHint}</span>
                  ),
                }}
              />
            </p>
          )}
          {chosen && (
            <>
              {folderCorrection && (
                <p className="border-l-2 border-warning/70 py-0.5 pl-3 text-[12px] leading-relaxed text-foreground/80">
                  {t(
                    folderCorrection === "mods-subfolder"
                      ? "setup.correctedModsFolder"
                      : "setup.correctedProfilesFolder",
                    { game: picked.display },
                  )}
                </p>
              )}
              <button
                onClick={choose}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                {t("setup.chooseDifferent")}
              </button>
            </>
          )}
        </div>

        <div className="flex w-full flex-col gap-2.5 border-t border-border pt-6">
          <div className="flex items-center justify-between gap-4">
            <span className="text-[11.5px] font-bold uppercase tracking-[1px] text-foreground/70">
              {t("setup.gameInstall", { game: picked.display })}
            </span>
            <span className="text-[10.5px] font-bold uppercase tracking-[0.08em] text-muted-foreground">
              {t("integration.optional")}
            </span>
          </div>
          {detecting ? (
            <div className="flex items-center gap-2.5 py-1 text-[12.5px] text-muted-foreground">
              <Loader2 className="size-4 flex-none animate-spin text-primary" />
              <span>{t("setup.detecting", { game: picked.display })}</span>
            </div>
          ) : gamePath ? (
            <>
              <div className="flex items-center gap-2.5 border-l-2 border-primary py-1 pl-3 font-mono text-[12.5px] text-foreground/80">
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
                onClick={() => void chooseGame()}
                className="cursor-default self-start text-[12px] font-semibold text-primary hover:brightness-110"
              >
                {t("setup.chooseDifferent")}
              </button>
            </>
          ) : (
            <>
              <p className="text-[13px] leading-relaxed text-foreground/85">
                {t("setup.installNotFound")}
              </p>
              <button
                onClick={() => void chooseGame()}
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
          onClick={() => (chosen ? void finish(chosen) : void choose())}
        >
          {!chosen && <FolderOpen className="size-4" />}
          {chosen ? t("setup.startBrowsing") : t("setup.chooseGameFolder")}
        </Button>
      </div>
    </div>
  );
}
