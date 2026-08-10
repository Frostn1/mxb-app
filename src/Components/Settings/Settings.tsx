import { useEffect, useRef, useState } from "react";
import {
  Check,
  RefreshCw,
  ExternalLink,
  Play,
  Square,
  Compass,
  MessagesSquare,
  Monitor,
  TriangleAlert,
  Sparkles,
} from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";
import {
  countProfilesIn,
  detectGamePath,
  getModsRoot,
  getOverlayState,
  overlayToggle,
  presetsListProfiles,
  setAutoRunFrostmod,
  setGamePath,
  setInstantRefresh,
  setLaunchAtStartup,
  setModsPath,
  setOverlayEnabled,
  setOverlayHotkey,
  setProfilesPath,
  setRunInBackground,
  setWatchModsReload,
  setWineRunner,
  wineHostInfo,
  type WineHostInfo,
  type OverlayState,
  experimentalState as experimentalStateApi,
  setExperimental,
  type ExperimentalState,
} from "../../api/mods";
import { useUpdate } from "../../Context/Update";
import { usePlatform } from "../../lib/usePlatform";
import { useConfig } from "../../Context/Config";
import GameSwitcher from "../Shell/GameSwitcher";
import ReshadeCard from "./ReshadeCard";
import SupportersCard from "./SupportersCard";
import { useTheme, type ThemeMode } from "../../Context/Theme";
import { Trans } from "../../i18n";
import { useI18n, type LocalePref, type TKey } from "../../i18n/context";
import { LOCALE_OPTIONS } from "../../i18n/core";
import { useFrostmod } from "../../Context/FrostmodContext";
import { prettyHotkey } from "../../lib/hotkey";
import { useTour } from "../Tour/Tour";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import { Segmented } from "@/Components/ui/segmented";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/Components/ui/select";
import { Switch } from "@/Components/ui/switch";
import { cn } from "@/lib/utils";

const REPO_URL = "https://github.com/Frostn1/mxb-app";
// Permanent invite (no expiry, no use cap) — a link that dies leaves a dead button
// in a shipped build, and the app can't be told about a new one without an update.
const DISCORD_URL = "https://discord.gg/3994Rr3ywb";

export type SectionId =
  | "game"
  | "folder"
  | "general"
  | "overlay"
  | "appearance"
  | "frostmod"
  | "reshade"
  | "supporters"
  | "about";
const SECTIONS: { id: SectionId; label: TKey }[] = [
  { id: "game", label: "game.label" },
  { id: "folder", label: "settings.gameFolder" },
  { id: "general", label: "settings.general" },
  { id: "overlay", label: "overlay.section" },
  { id: "appearance", label: "settings.appearance" },
  { id: "frostmod", label: "settings.frostmod" },
  { id: "reshade", label: "settings.reshade" },
  { id: "supporters", label: "settings.supporters" },
  { id: "about", label: "settings.about" },
];

/** Default shown before the backend answers, so the field is never blank. */
const FALLBACK_HOTKEY = "CommandOrControl+Shift+X";

/** How often the overlay's live state (game up? screen owned?) is re-read while
 *  Settings is on screen. Slow enough to be free, quick enough that alt-tabbing out of
 *  a race and looking here shows what the game is actually doing. */
const OVERLAY_POLL_MS = 4000;

/** Turn a `KeyboardEvent.code` into the token Tauri's accelerator parser expects. */
function acceleratorKey(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return null;
}

/** Modifier-plus-key capture field for the overlay hotkey.
 *
 * A modifier is required: a bare `M` would be swallowed globally, including while the
 * player is typing a server chat message. */
function HotkeyField({
  value,
  onCapture,
  disabled,
}: {
  value: string;
  onCapture: (accelerator: string) => void;
  disabled?: boolean;
}) {
  const { t } = useI18n();
  const [recording, setRecording] = useState(false);
  const isMac = usePlatform() === "macos";

  const pretty = prettyHotkey(value, isMac);

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.preventDefault();
    if (e.code === "Escape") {
      setRecording(false);
      return;
    }
    const key = acceleratorKey(e.code);
    if (!key) return; // a modifier on its own — keep waiting for the real key
    const mods: string[] = [];
    // Cmd on macOS and Ctrl elsewhere are the same accelerator token. The Windows key
    // is its own thing, so it must not be folded into it.
    if (e.ctrlKey || (isMac && e.metaKey)) mods.push("CommandOrControl");
    if (!isMac && e.metaKey) mods.push("Super");
    if (e.altKey) mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (mods.length === 0) {
      toast.error(t("overlay.needModifier"), {
        description: t("overlay.needModifierDesc"),
      });
      return;
    }
    setRecording(false);
    onCapture([...mods, key].join("+"));
  };

  return (
    <button
      disabled={disabled}
      onClick={(e) => {
        // WebKit doesn't focus a button on click, and an unfocused button never sees
        // the keydown we're about to wait for.
        e.currentTarget.focus();
        setRecording(true);
      }}
      onBlur={() => setRecording(false)}
      onKeyDown={recording ? onKeyDown : undefined}
      className={cn(
        "min-w-[148px] cursor-default rounded-lg border px-3 py-1.5 text-center font-mono text-[12px] transition-colors disabled:opacity-50",
        recording
          ? "border-primary text-primary"
          : "border-white/[0.09] text-foreground/85 hover:bg-foreground/[0.05]",
      )}
    >
      {recording ? t("overlay.pressKeys") : pretty}
    </button>
  );
}

interface SettingsProps {
  /** Section to scroll to on open — set when something sent the player here for a
   *  specific setting (the release showcase, the empty Presets tab). */
  initialSection?: SectionId;
  /** Re-open the release showcase for the current version, from About. */
  onShowWhatsNew?: () => void;
}

export default function Settings({ initialSection, onShowWhatsNew }: SettingsProps) {
  const { t, locale, setLocale } = useI18n();
  const { config, reloadConfig, game, games } = useConfig();
  const caps = game.caps;
  // A build that only knows one title has nothing to switch between — see `GameSwitcher`.
  const multiGame = games.length > 1;
  const platform = usePlatform();
  const isWindows = platform === "windows";
  const isMac = platform === "macos";
  const { theme, setTheme } = useTheme();
  const { running, reload, status, installing, checking, statusError, install, start, stop, refreshStatus, missingRuntime, installRuntime, installingRuntime } =
    useFrostmod();
  const { check: checkForUpdates } = useUpdate();
  const { startTour } = useTour();
  const [version, setVersion] = useState("");
  const [experimental, setExperimentalState] = useState<ExperimentalState | null>(null);
  // The packaged version is always a plain `x.y.z` — the release tag's pre-release suffix
  // only survives in what the backend reports (see `release_version`), so a beta names
  // itself here. `getVersion()` covers the moment before that call lands.
  const shownVersion = experimental?.version || version;
  const [active, setActive] = useState<SectionId>(initialSection ?? "folder");
  // FrostMod has no GP Bikes build, so its section isn't there to jump to either — and
  // neither is the game picker when there's only one game to pick.
  const sections = SECTIONS.filter(
    (s) =>
      (s.id !== "frostmod" || caps.frostmod) && (s.id !== "game" || multiGame),
  );
  const [busy, setBusy] = useState(false);
  const refs = useRef<Record<SectionId, HTMLDivElement | null>>({
    game: null,
    folder: null,
    general: null,
    overlay: null,
    appearance: null,
    frostmod: null,
    reshade: null,
    supporters: null,
    about: null,
  });

  // The folder the backend *actually* reads profiles from when there's no override.
  // Usually `<modsPath>/profiles`, but it falls back to `Documents\PiBoSo\MX Bikes\
  // profiles` when that one doesn't exist — show the resolved path so a fallback is
  // visible here rather than something the player has to infer.
  const [resolvedProfilesPath, setResolvedProfilesPath] = useState("");
  useEffect(() => {
    if (config.profilesPath) {
      // An override is shown verbatim; nothing to resolve.
      setResolvedProfilesPath("");
      return;
    }
    presetsListProfiles()
      .then((scan) => setResolvedProfilesPath(scan.dir))
      .catch(() => setResolvedProfilesPath(""));
  }, [config.modsPath, config.profilesPath]);

  // Where content is really read from. `modsPath` no longer answers it on its own: it can
  // be the game folder (the root is its `mods` child) or a relocated tree that *is* the
  // root. Showing it turns "my paints are missing" into something a player can see.
  const [modsRoot, setModsRoot] = useState<{
    path: string;
    exists: boolean;
    relocated: boolean;
  } | null>(null);
  useEffect(() => {
    getModsRoot()
      .then(setModsRoot)
      .catch(() => setModsRoot(null));
  }, [config.modsPath]);

  const profilesSep = config.modsPath.includes("\\") ? "\\" : "/";
  const defaultProfilesPath =
    resolvedProfilesPath ||
    (config.modsPath
      ? `${config.modsPath}${profilesSep}profiles`
      : t("settings.insideModsFolder"));

  const runInBackground = config.runInBackground ?? true;
  const launchAtStartup = config.launchAtStartup ?? true;
  const autoRunFrostmod = config.autoRunFrostmod ?? true;
  const instantRefresh = config.instantRefresh ?? true;
  const watchModsReload = config.watchModsReload ?? true;

  const overlayEnabled = config.overlayEnabled ?? true;
  const overlayHotkey = config.overlayHotkey || FALLBACK_HOTKEY;

  // What the overlay is doing right now: is the game up, does something else own the
  // screen, and did the hotkey actually bind. A shortcut that never registered has
  // nothing to say at the moment it isn't pressed, so this is the only place the
  // player can find out why "nothing happens".
  const [overlayLive, setOverlayLive] = useState<OverlayState | null>(null);
  useEffect(() => {
    let alive = true;
    const poll = () =>
      getOverlayState()
        .then((s) => {
          if (alive) setOverlayLive(s);
        })
        .catch(() => {});
    void poll();
    const id = setInterval(poll, OVERLAY_POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [config.overlayEnabled, config.overlayHotkey]);

  const showOverlayNow = async () => {
    try {
      await overlayToggle();
    } catch (e) {
      toast.error(t("overlay.showFailed"), { description: String(e) });
    }
  };

  const toggleOverlay = async (v: boolean) => {
    try {
      await setOverlayEnabled(v);
      await reloadConfig();
    } catch (e) {
      // The setting saved but the hotkey didn't register — say so rather than
      // leaving a switch that looks on and does nothing.
      toast.error(t("overlay.registerFailed"), { description: String(e) });
      await reloadConfig();
    }
  };

  const rebindOverlay = async (accelerator: string) => {
    try {
      await setOverlayHotkey(accelerator);
      await reloadConfig();
      toast.success(t("overlay.shortcutUpdated"));
    } catch (e) {
      toast.error(t("overlay.shortcutRejected"), { description: String(e) });
    }
  };

  const toggleInstantRefresh = async (v: boolean) => {
    try {
      await setInstantRefresh(v);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const toggleWatchModsReload = async (v: boolean) => {
    try {
      await setWatchModsReload(v);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const toggleAutoRun = async (v: boolean) => {
    try {
      await setAutoRunFrostmod(v);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const toggleBackground = async (v: boolean) => {
    try {
      await setRunInBackground(v);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const toggleStartup = async (v: boolean) => {
    try {
      await setLaunchAtStartup(v);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.startupUpdateFailed"), { description: String(e) });
    }
  };

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion(""));
    experimentalStateApi().then(setExperimentalState).catch(() => {});
    // Re-check FrostMod against GitHub whenever Settings opens — the provider
    // only fetches once at launch, so this catches releases cut since then.
    void refreshStatus();
  }, [refreshStatus]);

  const goto = (id: SectionId) => {
    setActive(id);
    refs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  // Sent here for one setting in particular — scroll to it rather than making them
  // find it. Runs after the first paint, when the section refs exist.
  useEffect(() => {
    if (!initialSection) return;
    setActive(initialSection);
    refs.current[initialSection]?.scrollIntoView({ block: "start" });
  }, [initialSection]);

  const changeFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickModsFolder"),
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      const adopted = await setModsPath(picked);
      await reloadConfig();
      // Picking the `mods` folder is the common slip, and the backend quietly corrects it
      // to the folder above. Say so — a path that isn't the one they chose looks like a bug.
      const corrected = adopted && adopted !== picked;
      toast.success(t("settings.folderUpdated"), {
        description: corrected
          ? t("settings.folderUsedParent", { folder: adopted })
          : t("settings.folderUpdatedDesc"),
      });
    } catch (e) {
      toast.error(t("settings.setFolderFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const detectAgain = async () => {
    setBusy(true);
    try {
      await setModsPath("");
      await reloadConfig();
      toast.success(t("settings.reDetected"));
    } catch (e) {
      toast.error(t("settings.detectFolderFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const changeGameFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: t("settings.pickInstallFolder"),
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await setGamePath(picked);
      await reloadConfig();
      toast.success(t("settings.installSet"), {
        description: t("settings.installSetDesc"),
      });
    } catch (e) {
      toast.error(t("settings.setInstallFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const detectGameFolder = async () => {
    setBusy(true);
    try {
      const found = await detectGamePath();
      if (!found) {
        toast.info(t("settings.installNotFound"), {
          description: t("settings.installNotFoundDesc"),
        });
        return;
      }
      await setGamePath(found);
      await reloadConfig();
      toast.success(t("settings.installFound"), { description: found });
    } catch (e) {
      toast.error(t("settings.detectInstallFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  // macOS: MX Bikes is a Windows binary, so Play has to go through a Wine wrapper. What
  // we found is shown rather than assumed — "no runner" is the difference between Play
  // working and Play failing, and the player should see it before they press it.
  const [wineHost, setWineHost] = useState<WineHostInfo | null>(null);
  useEffect(() => {
    if (!isMac) return;
    wineHostInfo()
      .then(setWineHost)
      .catch(() => setWineHost(null));
  }, [isMac]);

  const changeWineRunner = async () => {
    const picked = await pickFolder({
      multiple: false,
      title: t("settings.pickWineRunner"),
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      setWineHost(await setWineRunner(picked));
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.wineRunnerFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const resetWineRunner = async () => {
    setBusy(true);
    try {
      setWineHost(await setWineRunner(""));
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.wineRunnerFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const changeProfilesFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: t("settings.pickProfilesFolder"),
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await setProfilesPath(picked);
      await reloadConfig();
      const count = await countProfilesIn(picked).catch(() => 0);
      if (count > 0) {
        toast.success(t("settings.profilesSet"), {
          description: t("settings.profilesFound", { count }),
        });
      } else {
        // Warn but keep the pick (per design) — they may be mid-setup.
        toast.warning(t("settings.noProfilesThere"), {
          description: t("settings.noProfilesThereDesc"),
        });
      }
    } catch (e) {
      toast.error(t("settings.setProfilesFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const resetProfilesFolder = async () => {
    setBusy(true);
    try {
      await setProfilesPath("");
      await reloadConfig();
      toast.success(t("settings.profilesReverted"));
    } catch (e) {
      toast.error(t("settings.resetProfilesFailed"), { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const reloadGame = async () => {
    const outcome = await reload();
    if (outcome === "signaled") toast.success(t("frostmod.reloadedGame"));
    else if (outcome === "not_running")
      toast.info(t("settings.frostmodNotRunningHint"));
    else toast.info(t("settings.reloadUnavailable"));
  };

  return (
    <div className="flex h-full">
      <nav className="flex w-[170px] flex-none flex-col gap-0.5 px-4 pt-[70px]">
        {sections.map((s) => (
          <button
            key={s.id}
            onClick={() => goto(s.id)}
            className={cn(
              "cursor-default rounded-md px-3 py-1.5 text-left text-[12.5px] transition-colors",
              active === s.id
                ? "bg-foreground/[0.07] font-semibold text-foreground"
                : "text-muted-foreground hover:text-foreground",
            )}
          >
            {t(s.label)}
          </button>
        ))}
      </nav>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-5">
        <div className="flex max-w-[640px] flex-col gap-[18px]">
          <div className="flex items-center gap-1.5">
            <h1 className="text-[21px] font-bold tracking-[-0.2px]">
              {t("nav.settings")}
            </h1>
            <HelpHint
              title={t("nav.settings")}
              description={t("settings.help")}
            />
          </div>

          {/* game — which title the app is driving. Its own card, above the folders it
              scopes: everything below belongs to whatever is picked here, so it isn't a
              property of the folder setting it used to sit inside. */}
          {multiGame && (
            <Section
              title={t("game.label")}
              desc={t("settings.gameDesc")}
              innerRef={(el) => (refs.current.game = el)}
            >
              <GameSwitcher />
            </Section>
          )}

          {/* game folder */}
          <Section
            title={t("setup.modsFolder", { game: game.display })}
            desc={t("settings.modsFolderDesc")}
            innerRef={(el) => (refs.current.folder = el)}
          >
            <div className="flex gap-2">
              <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                {/* Named rather than a bare "Not set": switching to a title the player
                    hasn't installed lands here, and "Not set" says neither what to set
                    nor which game it's for. */}
                <span className="flex-1 truncate" title={config.modsPath}>
                  {config.modsPath || t("settings.selectFolderFor")}
                </span>
                {config.modsPath && (
                  <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                    <Check className="size-3" strokeWidth={3} /> Set
                  </span>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={changeFolder} disabled={busy}>
                {config.modsPath ? t("settings.change") : t("settings.set")}
              </Button>
            </div>
            <button
              onClick={detectAgain}
              disabled={busy}
              className="cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110 disabled:opacity-50"
            >
              Detect automatically
            </button>

            {/* The resolved content root. Only worth a line when it isn't the obvious
                `<picked>/mods` — a relocated tree, or a folder that isn't there yet, are
                exactly the two cases where an empty library needs explaining. */}
            {config.modsPath && modsRoot && (modsRoot.relocated || !modsRoot.exists) && (
              <p
                className={cn(
                  "-mt-0.5 text-[11.5px] leading-relaxed",
                  modsRoot.exists ? "text-muted-foreground" : "text-warning",
                )}
              >
                {modsRoot.exists ? "Reading mods from " : "No mods folder at "}
                <span className="font-mono">{modsRoot.path}</span>
                {!modsRoot.exists && " — nothing will show up until it's there."}
              </p>
            )}

            {/* Profiles folder — a customization nested under the mods folder. It
                normally lives at <mods>/profiles; override only for the split case. */}
            <div className="ml-1.5 mt-0.5 border-l border-border pl-4">
              <div className="flex items-center gap-2">
                <span className="text-[11.5px] font-semibold text-foreground/80">
                  Profiles subfolder
                </span>
                <span className="rounded-full bg-foreground/[0.06] px-1.5 py-[1px] text-[10px] font-medium text-muted-foreground">
                  Customization
                </span>
              </div>
              <p className="mt-1 text-[11.5px] leading-relaxed text-muted-foreground">
                <Trans
                  k="settings.profilesDesc"
                  values={{
                    profiles: <span className="font-mono">profiles</span>,
                    documents: (
                      <span className="font-mono">{`Documents\\PiBoSo\\${game.display}`}</span>
                    ),
                  }}
                />
              </p>
              <div className="mt-2 flex gap-2">
                <div
                  className={cn(
                    "flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px]",
                    config.profilesPath ? "text-muted-foreground" : "text-faint",
                  )}
                >
                  <span
                    className="flex-1 truncate"
                    title={config.profilesPath || defaultProfilesPath}
                  >
                    {config.profilesPath || defaultProfilesPath}
                  </span>
                  {config.profilesPath ? (
                    <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                      <Check className="size-3" strokeWidth={3} /> Custom
                    </span>
                  ) : (
                    <span className="flex-none font-sans text-[11px] font-medium text-faint">
                      Default
                    </span>
                  )}
                </div>
                <Button variant="outline" size="sm" onClick={changeProfilesFolder} disabled={busy}>
                  {config.profilesPath ? t("settings.change") : t("settings.set")}
                </Button>
              </div>
              {config.profilesPath && (
                <button
                  onClick={resetProfilesFolder}
                  disabled={busy}
                  className="mt-2 cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110 disabled:opacity-50"
                >
                  {t("settings.resetToDefault")}
                </button>
              )}
            </div>

            <div className="mt-1 h-px bg-border" />

            {/* Optional game *install* folder (holds core rider.pkz) — powers the
                real 3D rider body in the preset preview. */}
            <p className="text-[12px] text-muted-foreground">
              <Trans
                k="settings.gameInstallDesc"
                values={{ file: <span className="font-mono">rider.pkz</span> }}
              />
            </p>
            <div className="flex gap-2">
              <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                <span className="flex-1 truncate" title={config.gamePath}>
                  {config.gamePath || t("settings.notSet")}
                </span>
                {config.gamePath && (
                  <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                    <Check className="size-3" strokeWidth={3} /> Set
                  </span>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={changeGameFolder} disabled={busy}>
                {config.gamePath ? t("settings.change") : t("settings.set")}
              </Button>
            </div>
            <button
              onClick={detectGameFolder}
              disabled={busy}
              className="cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110 disabled:opacity-50"
            >
              Detect automatically
            </button>

            {/* macOS only: the Wine wrapper Play launches through. */}
            {isMac && (
              <>
                <div className="mt-1 h-px bg-border" />
                <p className="text-[12px] text-muted-foreground">
                  {t("settings.wineRunnerDesc", { game: game.display })}
                </p>
                <div className="flex gap-2">
                  <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                    <span className="flex-1 truncate" title={wineHost?.runner}>
                      {wineHost?.runner || t("settings.wineRunnerNone")}
                    </span>
                    {wineHost?.runner && (
                      <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                        <Check className="size-3" strokeWidth={3} /> {wineHost.via}
                      </span>
                    )}
                  </div>
                  <Button variant="outline" size="sm" onClick={changeWineRunner} disabled={busy}>
                    {config.wineRunner ? t("settings.change") : t("settings.set")}
                  </Button>
                </div>
                <p className="text-[11.5px] text-muted-foreground">
                  {wineHost?.bottles.length
                    ? t("settings.wineBottlesFound", { count: wineHost.bottles.length })
                    : t("settings.wineBottlesNone", { game: game.display })}
                </p>
                {config.wineRunner && (
                  <button
                    onClick={resetWineRunner}
                    disabled={busy}
                    className="cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110 disabled:opacity-50"
                  >
                    {t("settings.resetToDefault")}
                  </button>
                )}
              </>
            )}
          </Section>

          {/* general / background */}
          <Section title={t("settings.general")} innerRef={(el) => (refs.current.general = el)}>
            <ToggleRow
              label={t("settings.runInBackground")}
              desc={t("settings.runInBackgroundDesc")}
              checked={runInBackground}
              onChange={toggleBackground}
            />
            <div className="h-px bg-border" />
            <ToggleRow
              label={t("settings.launchAtStartup")}
              desc={t("settings.launchAtStartupDesc")}
              checked={launchAtStartup}
              onChange={toggleStartup}
            />
            <div className="h-px bg-border" />
            <ToggleRow
              label={t("settings.instantRefresh")}
              desc={
                !caps.instantRefresh
                  ? t("settings.instantRefreshMxOnly", { game: game.display })
                  : isWindows
                    ? t("settings.instantRefreshDesc")
                    : t("settings.instantRefreshWindowsOnly")
              }
              checked={instantRefresh && caps.instantRefresh}
              disabled={!caps.instantRefresh}
              onChange={toggleInstantRefresh}
            />
          </Section>

          {/* in-game overlay */}
          <Section
            title={t("overlay.section")}
            innerRef={(el) => (refs.current.overlay = el)}
          >
            <ToggleRow
              label={t("overlay.enable")}
              desc={t("overlay.enableDesc")}
              checked={overlayEnabled}
              onChange={toggleOverlay}
            />
            <div className="h-px bg-border" />
            <div className="flex items-start justify-between gap-6">
              <div className="flex flex-col gap-1">
                <span className="text-[12.5px] text-foreground/85">
                  {t("overlay.shortcut")}
                </span>
                <span className="text-[11.5px] leading-relaxed text-muted-foreground">
                  {t("overlay.shortcutDesc")}
                </span>
              </div>
              <HotkeyField
                value={overlayHotkey}
                onCapture={rebindOverlay}
                disabled={!overlayEnabled}
              />
            </div>
            <div className="h-px bg-border" />

            {/* Live state, and a way to see the thing without launching a race. */}
            <div className="flex items-center justify-between gap-4">
              <span className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
                <span
                  className={cn(
                    "size-[7px] rounded-full",
                    overlayLive?.gameRunning ? "bg-success" : "bg-muted-foreground/50",
                  )}
                />
                {overlayLive?.gameRunning
                  ? t("overlay.gameRunning")
                  : t("overlay.gameNotRunning")}
              </span>
              <Button
                variant="outline"
                size="sm"
                onClick={showOverlayNow}
                disabled={!overlayEnabled}
              >
                <Monitor className="size-3.5" /> {t("overlay.showNow")}
              </Button>
            </div>

            {/* The hotkey never bound — almost always another app holding the combo.
                Named here because the overlay can't report it when it fails to open. */}
            {overlayLive?.hotkeyError && (
              <Callout tone="warning" title={t("overlay.hotkeyTaken")}>
                {t("overlay.hotkeyTakenDesc")}
              </Callout>
            )}

            {/* Right now, not in general — worth its own line, because it means the
                overlay is already open behind the game. */}
            {overlayLive?.fullscreenBlocked && (
              <Callout tone="warning" title={t("overlay.fullscreenNow")}>
                {t("overlay.fullscreenNowDesc")}
              </Callout>
            )}

            {/* Not macOS: there's no MX Bikes to run borderless there. Linux gets it —
                Proton hands the game the same exclusive swapchain Windows does. */}
            {!isMac && (
              <Callout tone="info" title={t("overlay.borderlessTitle")}>
                {t("overlay.borderlessNote")}
              </Callout>
            )}

            <p className="text-[11.5px] leading-relaxed text-muted-foreground">
              <span className="font-semibold text-foreground/80">
                {t("overlay.notWorking")}
              </span>{" "}
              {t("overlay.notWorkingDesc")}
            </p>
          </Section>

          {/* appearance */}
          <Section
            title={t("settings.appearance")}
            innerRef={(el) => (refs.current.appearance = el)}
          >
            <div className="flex items-center justify-between">
              <span className="text-[12.5px] text-foreground/85">
                {t("settings.theme")}
              </span>
              <Segmented
                size="sm"
                value={theme}
                onChange={(v) => setTheme(v as ThemeMode)}
                options={[
                  { value: "light", label: t("settings.themeLight") },
                  { value: "dark", label: t("settings.themeDark") },
                  { value: "system", label: t("settings.themeSystem") },
                ]}
              />
            </div>

            {/* A Select, not a Segmented control — seven options don't fit the
                segmented track, and each is named in its own language so someone
                who lands in a script they can't read can still get back out. */}
            <div className="mt-3 flex items-center justify-between">
              <span className="text-[12.5px] text-foreground/85">
                {t("settings.language")}
              </span>
              <Select
                value={locale}
                onValueChange={(v) => setLocale(v as LocalePref)}
              >
                <SelectTrigger className="h-8 w-[180px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {LOCALE_OPTIONS.map((opt) => (
                    <SelectItem key={opt.value} value={opt.value}>
                      {opt.value === "system"
                        ? t("settings.languageSystem")
                        : opt.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          </Section>

          {/* frostmod — a Win32 DLL injected into the game, so it has nothing to do
              anywhere else. Hidden rather than shown-and-disabled: every control in it
              would fail, including one that downloads two Windows binaries. */}
          {isWindows && caps.frostmod && (
          <Section
            title={t("settings.frostmod")}
            innerRef={(el) => (refs.current.frostmod = el)}
            titleRight={
              <span
                className={cn(
                  "flex items-center gap-1.5 text-[11.5px]",
                  running ? "text-success" : "text-muted-foreground",
                )}
              >
                <span
                  className={cn(
                    "size-[7px] rounded-full",
                    running ? "bg-success" : "bg-muted-foreground/50",
                  )}
                />
                {running === null
                  ? t("settings.checking")
                  : running
                    ? t("settings.runningConnected")
                    : t("settings.notRunning")}
              </span>
            }
          >
            <p className="text-[12px] leading-relaxed text-muted-foreground">
              Live-reloads MX Bikes when mods change, so you don&apos;t restart the game.
              MXB App installs it, keeps it updated, and runs it for you.
            </p>

            <div className="flex items-center justify-between rounded-lg border border-input bg-background px-3 py-2.5">
              <div className="flex flex-col">
                <span className="text-[12.5px] text-foreground/85">
                  {status?.installed
                    ? t("settings.frostmodInstalled", {
                        suffix: status.version ? ` · ${status.version}` : "",
                      })
                    : t("settings.notInstalled")}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  {checking
                    ? t("settings.checkingGitHub")
                    : statusError
                      ? t("settings.updateCheckFailed")
                      : // Above everything else: a missing Visual C++ runtime stops
                        // FrostMod attaching at all, and no amount of repairing or
                        // updating FrostMod puts one on the machine.
                        missingRuntime
                        ? t("settings.frostmodRuntimeMissing")
                        : status?.needsRepair
                        ? t("settings.frostmodNeedsRepair")
                        : // Ranked above "latest version" on purpose: a build too old for
                          // this game can't run at all, which the version line alone
                          // wouldn't explain.
                          status?.installed && !status.supportedForGame
                          ? t("settings.frostmodUnsupportedForGame", {
                              game: game.display,
                            })
                          : status?.latest
                            ? t("settings.latestVersion", { version: status.latest })
                            : null}
                </span>
              </div>
              <div className="flex items-center gap-1.5">
                {/* Its own button rather than a mode of the one below: the FrostMod
                    install and the Windows component are separate things to fix, and
                    someone can genuinely need both. */}
                {missingRuntime && (
                  <Button
                    size="sm"
                    onClick={() => void installRuntime(missingRuntime)}
                    disabled={installingRuntime}
                  >
                    {installingRuntime
                      ? t("runtime.installing")
                      : t("runtime.fixIt")}
                  </Button>
                )}
                {status?.installed && (
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={() => refreshStatus()}
                    disabled={checking || installing}
                    title={t("settings.checkNewer")}
                  >
                    <RefreshCw className={cn("size-3.5", checking && "animate-spin")} />
                  </Button>
                )}
                {(() => {
                  const updatable =
                    status?.installed &&
                    status.latest &&
                    status.version !== status.latest;
                  // An install carrying the right tag over the wrong binaries reads as
                  // current on version alone; without this it'd sit on a disabled "Up to
                  // date" with no way to put the missing half in place.
                  const repairable = Boolean(status?.needsRepair);
                  // Same idea for a build that's too old for the active title: it reads as
                  // current on version alone, but can't be started, so the button has to
                  // stay live to offer the update that fixes it.
                  const unsupported = Boolean(
                    status?.installed && !status.supportedForGame,
                  );
                  // "Up to date" only when we actually confirmed the latest tag.
                  const confirmedCurrent =
                    status?.installed &&
                    !updatable &&
                    !repairable &&
                    !unsupported &&
                    !statusError &&
                    status?.latest;
                  return (
                    <Button
                      variant={confirmedCurrent ? "outline" : "default"}
                      size="sm"
                      onClick={install}
                      disabled={installing || checking || Boolean(confirmedCurrent)}
                    >
                      {installing
                        ? t("settings.working")
                        : !status?.installed
                          ? t("settings.installFrostmod")
                          : updatable
                            ? t("settings.updateTo", { version: status.latest ?? "" })
                            : repairable
                              ? t("settings.frostmodRepair")
                              : // Already on the newest tag, but that tag is still too
                                // old for this game — "Up to date" would be a lie.
                                unsupported
                                ? t("settings.frostmodUpdateRequired")
                                : statusError || !status?.latest
                                  ? t("settings.reinstallLatest")
                                  : t("settings.upToDate")}
                    </Button>
                  );
                })()}
              </div>
            </div>

            <ToggleRow
              label={t("settings.autoRunFrostmod")}
              desc={t("settings.autoRunFrostmodDesc")}
              checked={autoRunFrostmod}
              onChange={toggleAutoRun}
            />

            <ToggleRow
              label={t("settings.watchModsReload")}
              desc={t("settings.watchModsReloadDesc")}
              checked={watchModsReload}
              onChange={toggleWatchModsReload}
            />

            <div className="flex gap-2">
              {/* Stop is offered whenever FrostMod is running, installed by us or not —
                  `frostmod_stop` kills a hand-launched `frostmod.exe` too, and gating it
                  on `installed` left the one case that needs it most (something running
                  that we didn't put there) with no button at all. Start still needs an
                  install to start. */}
              {running ? (
                <Button variant="outline" size="sm" onClick={stop}>
                  <Square className="size-3.5" /> {t("frostmod.stop")}
                </Button>
              ) : (
                status?.installed && (
                  <Button variant="default" size="sm" onClick={start}>
                    <Play className="size-3.5" /> {t("frostmod.start")}
                  </Button>
                )
              )}
              <Button variant="outline" size="sm" onClick={reloadGame} disabled={!running}>
                <RefreshCw className="size-3.5" /> Reload game now
              </Button>
            </div>
          </Section>
          )}

          {/* reshade — post-processing presets. Not gated on a capability: both titles are
              OpenGL, so ReShade attaches to either one the same way. */}
          <Section
            title={t("settings.reshade")}
            desc={t("settings.reshadeDesc")}
            innerRef={(el) => (refs.current.reshade = el)}
          >
            <ReshadeCard />
          </Section>

          {/* experimental */}
          <Section title={t("settings.experimental")} innerRef={() => {}}>
            <ToggleRow
              label={t("settings.experimentalServers")}
              desc={
                experimental?.forcedByEnv
                  ? t("settings.experimentalForced")
                  : t("settings.experimentalServersDesc")
              }
              checked={experimental?.enabled ?? false}
              onChange={(v) => {
                // The env override wins in the backend, so flipping the switch would look
                // like it did nothing. Say so instead of pretending.
                if (experimental?.forcedByEnv) {
                  toast.info(t("settings.experimentalForced"));
                  return;
                }
                setExperimental(v)
                  .then(() => experimentalStateApi())
                  .then(setExperimentalState)
                  .catch((e) => toast.error(String(e)));
              }}
            />
          </Section>

          {/* supporters — who's buying the coffees that pay for the app. Above About
              rather than inside it: a thank-you buried under the version number and the
              update button is one nobody reads. */}
          <Section
            title={t("settings.supporters")}
            desc={t("settings.supportersDesc")}
            innerRef={(el) => (refs.current.supporters = el)}
          >
            <SupportersCard />
          </Section>

          {/* about */}
          <Section title={t("settings.about")} innerRef={(el) => (refs.current.about = el)}>
            <div className="flex items-center gap-3 text-[12px] text-muted-foreground">
              <span>{shownVersion ? `mxb-app v${shownVersion}` : "mxb-app"}</span>
              {experimental?.prerelease && (
                <span className="rounded-full bg-primary/15 px-2 py-0.5 text-[10.5px] font-semibold uppercase tracking-wide text-primary">
                  {t("settings.betaBadge")}
                </span>
              )}
              <button
                onClick={() => openUrl(REPO_URL)}
                className="flex cursor-default items-center gap-1 font-semibold text-primary hover:brightness-110"
              >
                GitHub <ExternalLink className="size-3" />
              </button>
              <button
                onClick={() => openUrl(`${REPO_URL}/blob/main/CHANGELOG.md`)}
                className="cursor-default hover:text-foreground"
              >
                Changelog
              </button>
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  void checkForUpdates();
                  void refreshStatus();
                }}
              >
                <RefreshCw className="size-3.5" /> Check for updates
              </Button>
              {onShowWhatsNew && (
                <Button variant="outline" size="sm" onClick={onShowWhatsNew}>
                  <Sparkles className="size-3.5" /> {t("settings.whatsNew")}
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={startTour}>
                <Compass className="size-3.5" /> Replay tour
              </Button>
              <Button variant="outline" size="sm" onClick={() => openUrl(DISCORD_URL)}>
                <MessagesSquare className="size-3.5" /> Join the Discord
              </Button>
            </div>
            <div className="flex flex-col gap-1 pt-1 text-[11.5px] text-faint">
              <div className="flex items-center gap-1.5">
                <span>{t("settings.madeWith")}</span>
                <span className="text-primary">❄</span>
                <span>by</span>
                <button
                  onClick={() => openUrl("https://github.com/Frostn1")}
                  className="cursor-default font-semibold text-primary hover:brightness-110"
                >
                  Frost
                </button>
              </div>
            </div>
          </Section>
        </div>
      </div>
    </div>
  );
}

function ToggleRow({
  label,
  desc,
  checked,
  onChange,
  /** Shown but not operable — for a setting the active game can't support, where
   *  hiding it would leave the player wondering where it went. */
  disabled = false,
}: {
  label: string;
  desc: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4", disabled && "opacity-60")}>
      <div className="flex flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        <span className="text-[11.5px] leading-relaxed text-muted-foreground">
          {desc}
        </span>
      </div>
      <div className="pt-0.5">
        <Switch checked={checked} onCheckedChange={onChange} disabled={disabled} />
      </div>
    </div>
  );
}

/** A boxed note inside a section — for the things that decide whether a feature works
 *  at all, which read as optional when they're set in the same grey as everything else. */
function Callout({
  tone,
  title,
  children,
}: {
  tone: "info" | "warning";
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className={cn(
        "flex items-start gap-2.5 rounded-lg border p-3",
        tone === "warning"
          ? "border-warning/30 bg-warning/[0.08]"
          : "border-input bg-foreground/[0.03]",
      )}
    >
      {tone === "warning" ? (
        <TriangleAlert className="mt-[1px] size-4 flex-none text-warning" />
      ) : (
        <Monitor className="mt-[1px] size-4 flex-none text-muted-foreground" />
      )}
      <div className="flex flex-col gap-0.5">
        <span className="text-[12px] font-semibold text-foreground/85">{title}</span>
        <span className="text-[11.5px] leading-relaxed text-muted-foreground">
          {children}
        </span>
      </div>
    </div>
  );
}

function Section({
  title,
  desc,
  titleRight,
  innerRef,
  children,
}: {
  title: string;
  desc?: string;
  titleRight?: React.ReactNode;
  innerRef: (el: HTMLDivElement | null) => void;
  children: React.ReactNode;
}) {
  return (
    <div
      ref={innerRef}
      className="flex scroll-mt-4 flex-col gap-3 rounded-xl border border-input bg-card p-[18px]"
    >
      <div className="flex items-center gap-2">
        <span className="flex-1 text-[14px] font-bold">{title}</span>
        {titleRight}
      </div>
      {desc && <span className="-mt-1.5 text-[12px] text-muted-foreground">{desc}</span>}
      {children}
    </div>
  );
}
