import { useCallback, useEffect, useRef, useState } from "react";
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
  FolderOpen,
  Download,
  Share2,
  Copy,
  Loader2,
} from "lucide-react";
import { open as pickFolder, save as pickSavePath } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";
import {
  countProfilesIn,
  detectGamePath,
  exportLogs,
  getModsRoot,
  getOverlayState,
  logsInfo,
  onLogsShareProgress,
  openLogsFolder,
  shareLogs,
  type LogGroup,
  type LogsInfo,
  type LogsKind,
  type LogsShare,
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
  voiceDevices,
  voiceMute,
  voiceStatus,
  setVoiceEnabled,
  setVoiceInputDevice,
  setVoiceOutputDevice,
  setVoicePttHotkey,
  setVoiceLevels,
  voiceMeterStart,
  voiceMeterStop,
  voiceTestOutput,
  onVoiceStatus,
  onVoiceInputLevel,
  onVoicePtt,
  setVoiceToggleToTalk,
  type VoiceDevices,
  type VoiceStatus,
} from "../../api/mods";
import { useUpdate } from "../../Context/Update";
import { usePlatform } from "../../lib/usePlatform";
import { useConfig } from "../../Context/Config";
import GameSwitcher from "../Shell/GameSwitcher";
import { ReplayCameraKeys } from "./ReplayCameraKeys";
import ReshadeCard from "./ReshadeCard";
import SupportersCard from "./SupportersCard";
import { useTheme, type ThemeMode } from "../../Context/Theme";
import { Trans } from "../../i18n";
import { useI18n, type LocalePref, type TKey } from "../../i18n/context";
import { getLocale, LOCALE_OPTIONS } from "../../i18n/core";
import { useFrostmod } from "../../Context/FrostmodContext";
import { prettyHotkey } from "../../lib/hotkey";
import { formatBytes, formatDateShort } from "../../lib/mods";
import { copyText } from "../../lib/clipboard";
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
  | "voice"
  | "appearance"
  | "frostmod"
  | "reshade"
  | "logs"
  | "experimental"
  | "supporters"
  | "about";

/**
 * The nav, and with it the page: exactly one of these sections is on screen at a time.
 *
 * It used to be one column with all twelve rendered into it and a nav that only scrolled
 * you to an anchor — which meant the folder settings and the version number shared a
 * scrollbar, and finding anything in the middle meant reading past everything else.
 *
 * Grouped because twelve flat entries is its own kind of list. The groups are about where
 * a setting *lives* — the game, the app, the things you only touch when something's wrong
 * — not about how often they're used.
 */
const GROUPS: { label: TKey; sections: { id: SectionId; label: TKey }[] }[] = [
  {
    label: "settings.groupSetup",
    sections: [
      { id: "game", label: "game.label" },
      { id: "folder", label: "settings.gameFolder" },
      { id: "frostmod", label: "settings.frostmod" },
      { id: "reshade", label: "settings.reshade" },
    ],
  },
  {
    label: "settings.groupApp",
    sections: [
      { id: "general", label: "settings.general" },
      { id: "appearance", label: "settings.appearance" },
      { id: "overlay", label: "overlay.section" },
      { id: "voice", label: "voice.section" },
    ],
  },
  {
    label: "settings.groupAdvanced",
    sections: [
      { id: "logs", label: "settings.logs" },
      // Had no nav entry at all before this, and rendered in the middle of the scroll
      // with nothing pointing at it.
      { id: "experimental", label: "settings.experimental" },
    ],
  },
  {
    label: "settings.groupAbout",
    sections: [
      { id: "supporters", label: "settings.supporters" },
      { id: "about", label: "settings.about" },
    ],
  },
];

/** Default shown before the backend answers, so the field is never blank. */
const FALLBACK_HOTKEY = "CommandOrControl+Shift+X";

/** Default push-to-talk combo shown before the backend answers. Mirrors
 *  `DEFAULT_PTT_HOTKEY` in config.rs. */
const FALLBACK_PTT_HOTKEY = "CommandOrControl+Shift+V";

/** Stands in for the empty string in the device pickers.
 *
 * The config stores `""` to mean "follow the system default", but Radix reserves the empty
 * string for *no selection* — an item valued `""` never reads as selected and can't be told
 * apart from the placeholder. The sentinel only exists inside the Select; it is mapped back
 * to `""` before anything is saved. */
const DEVICE_DEFAULT = "__system_default__";

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
  // FrostMod is a Win32 DLL injected into the game — which is exactly what the game is
  // everywhere it runs: natively on Windows, under Proton on Linux, in a CrossOver/Whisky
  // bottle on macOS. The app starts FrostMod in whichever prefix holds the game.
  const hasFrostmod = isWindows || platform === "linux" || isMac;
  const { theme, setTheme } = useTheme();
  const { running, reload, status, installing, checking, statusError, install, start, stop, refreshStatus, missingRuntime, installRuntime, installingRuntime, repairRuntimes, repairingRuntimes, strayMsvcr90, clearingStray, clearStrayMsvcr90 } =
    useFrostmod();
  const { check: checkForUpdates } = useUpdate();
  const { startTour } = useTour();
  const [version, setVersion] = useState("");
  const [experimental, setExperimentalState] = useState<ExperimentalState | null>(null);
  // The packaged version is always a plain `x.y.z` — the release tag's pre-release suffix
  // only survives in what the backend reports (see `release_version`), so a beta names
  // itself here. `getVersion()` covers the moment before that call lands.
  const shownVersion = experimental?.version || version;
  const [wanted, setActive] = useState<SectionId>(initialSection ?? "folder");
  // FrostMod is a Win32 DLL injected into the game and has no GP Bikes build, so its
  // section isn't there to open either — and neither is the game picker when there's only
  // one game to pick. A group left with nothing in it drops out of the nav entirely.
  const groups = GROUPS.map((g) => ({
    ...g,
    sections: g.sections.filter(
      (s) =>
        (s.id !== "frostmod" || (hasFrostmod && caps.frostmod)) &&
        (s.id !== "game" || multiGame),
    ),
  })).filter((g) => g.sections.length > 0);
  // Only one section is on screen, so being sent to one this build doesn't have would
  // leave the page empty rather than merely missing a card the way the old scroll did —
  // `initialSection="frostmod"` on a Mac, say. Fall back to the first section there is.
  const shown = groups.flatMap((g) => g.sections).map((s) => s.id);
  const active = shown.includes(wanted) ? wanted : (shown[0] ?? "folder");
  const [busy, setBusy] = useState(false);
  // Each pane starts at the top rather than wherever the last one was left.
  const pane = useRef<HTMLDivElement | null>(null);

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

  // Where the logs are and what's in them. Re-read whenever the Logs section is opened
  // (and after an export) rather than once on mount: the reason anyone comes here is that
  // something just went wrong, and a count from ten minutes ago answers the wrong question.
  const [logs, setLogs] = useState<LogsInfo | null>(null);
  const [exportingLogs, setExportingLogs] = useState(false);
  // The link the last share came back with, kept on screen for the rest of the session:
  // it lands on the clipboard by itself, and a clipboard that has since been used for
  // something else is the one way an uploaded bundle is lost for good.
  const [sharedLogs, setSharedLogs] = useState<LogsShare | null>(null);
  const [sharingLogs, setSharingLogs] = useState<string | null>(null);
  const [copiedLogsLink, setCopiedLogsLink] = useState(false);
  const refreshLogs = useCallback(() => {
    logsInfo()
      .then(setLogs)
      .catch(() => setLogs(null));
  }, []);
  const logsOpen = active === "logs";
  useEffect(() => {
    refreshLogs();
  }, [refreshLogs, config.modsPath, config.gamePath, logsOpen]);

  const openLogs = (which: LogsKind) => {
    openLogsFolder(which).catch((e) => toast.error(String(e)));
  };

  const saveLogs = async () => {
    // A date rather than a clock time: the file is named for the day it was collected,
    // which is what a support thread refers back to.
    const stamp = new Date().toISOString().slice(0, 10);
    const dest = await pickSavePath({
      defaultPath: `mxb-app-logs-${stamp}.zip`,
      filters: [{ name: "Zip", extensions: ["zip"] }],
    }).catch(() => null);
    if (!dest) return;
    setExportingLogs(true);
    try {
      const written = await exportLogs(dest);
      toast.success(t("logs.saved"), {
        description: t("logs.savedDesc", {
          count: written.files,
          size: formatBytes(written.bytes),
        }),
      });
      refreshLogs();
    } catch (e) {
      toast.error(t("logs.saveFailed"), { description: String(e) });
    } finally {
      setExportingLogs(false);
    }
  };

  /** Zip the same logs and put them on the file host, handing back one link.
   *
   * Saving to disk is only half of "send me your logs" — the file still has to get to
   * whoever asked, and that is where a bug report usually stalls. This ends with a link
   * on the clipboard. It goes to a public host, hence the warning that sits under it. */
  const shareLogsNow = async () => {
    setSharingLogs(t("logs.sharePacking"));
    setSharedLogs(null);
    setCopiedLogsLink(false);
    const unlisten = await onLogsShareProgress((p) => {
      // "done" arrives just before the call returns; letting it through would flash the
      // button back to "Packing…" for a frame on the way out.
      if (p.phase === "done") return;
      setSharingLogs(
        p.phase === "uploading" ? p.message || t("logs.sharing") : t("logs.sharePacking"),
      );
    });
    try {
      const share = await shareLogs();
      setSharedLogs(share);
      // Straight to the clipboard: the link exists to be pasted somewhere, and someone
      // who has just waited out an upload shouldn't have to click again to collect it.
      const copied = await copyText(shareLinkText(share));
      setCopiedLogsLink(copied);
      toast.success(t("logs.shared"), {
        description: copied
          ? t("logs.sharedCopied", { size: formatBytes(share.size) })
          : t("logs.sharedDesc", { size: formatBytes(share.size) }),
      });
      refreshLogs();
    } catch (e) {
      toast.error(t("logs.shareFailed"), {
        description: String(e).replace(/^Error:\s*/, ""),
      });
    } finally {
      unlisten();
      setSharingLogs(null);
    }
  };

  const copyLogsLink = async () => {
    if (!sharedLogs) return;
    if (await copyText(shareLinkText(sharedLogs))) {
      setCopiedLogsLink(true);
      toast.success(t("logs.linkCopied"));
    }
  };

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
  // Same shape as the overlay pair above: the config's fields are optional (an install
  // predating voice has none), so every read is defaulted here rather than at each use.
  const voiceEnabled = config.voiceEnabled ?? false;
  const voiceInput = config.voiceInputDevice ?? "";
  const voiceOutput = config.voiceOutputDevice ?? "";
  const voicePtt = config.voicePttHotkey || FALLBACK_PTT_HOTKEY;
  const voiceGain = config.voiceInputGain ?? 1;
  const voiceToggle = config.voiceToggleToTalk ?? false;
  const voiceVolume = config.voiceOutputVolume ?? 1;

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

  // ---- voice ------------------------------------------------------------------
  // Devices are re-read on mount and whenever the section is opened rather than cached:
  // the headset a player is asking about is usually the one they just plugged in.
  const [devices, setDevices] = useState<VoiceDevices | null>(null);
  const [micTesting, setMicTesting] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  // Whether the mic key currently has the mic open. Shown because in toggle mode there is
  // nothing physical to tell you — you can walk away from a latched-open microphone.
  const [micOpen, setMicOpen] = useState(false);
  // Who else is in voice. Pushed from the engine, so this is live without polling.
  const [voice, setVoice] = useState<VoiceStatus>({
    joined: false,
    server: "",
    peers: [],
    error: null,
  });

  const refreshDevices = useCallback(async () => {
    try {
      setDevices(await voiceDevices());
    } catch {
      // A machine with no audio stack at all. The section shows the empty state.
      setDevices({ inputs: [], outputs: [], error: null });
    }
  }, []);

  useEffect(() => {
    void refreshDevices();
  }, [refreshDevices]);

  // The meter is a live mic. It must never outlive the page that opened it — leaving it
  // running would keep the microphone open behind a settings screen nobody is looking at.
  useEffect(() => {
    if (!micTesting) return;
    let un: (() => void) | undefined;
    void onVoiceInputLevel(({ rms }) => setMicLevel(rms)).then((f) => (un = f));
    return () => {
      un?.();
      void voiceMeterStop();
    };
  }, [micTesting]);

  useEffect(() => {
    return () => {
      void voiceMeterStop();
    };
  }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    void onVoicePtt(setMicOpen).then((f) => (un = f));
    return () => un?.();
  }, []);

  // The room, as it changes. Asked once for the first paint, then pushed — a rider joining
  // mid-session should appear without the page having to ask.
  useEffect(() => {
    let un: (() => void) | undefined;
    void voiceStatus().then(setVoice).catch(() => {});
    void onVoiceStatus(setVoice).then((f) => (un = f));
    return () => un?.();
  }, []);

  const toggleMute = useCallback((peerId: string, muted: boolean) => {
    // Optimistic: the engine is authoritative, but its next status push is up to 20 ms away
    // and a mute button that lags is a mute button people press twice.
    setVoice((v) => ({
      ...v,
      peers: v.peers.map((p) => (p.peerId === peerId ? { ...p, muted } : p)),
    }));
    void voiceMute(peerId, muted);
  }, []);

  // Navigating to another section closes the meter with it. The state that drives it lives
  // up here rather than in the section, so without this a mic left testing would keep
  // recording behind a pane that isn't even on screen.
  useEffect(() => {
    if (active !== "voice") setMicTesting(false);
  }, [active]);

  const toggleVoice = async (v: boolean) => {
    try {
      await setVoiceEnabled(v);
      if (!v) setMicTesting(false);
      await reloadConfig();
    } catch (e) {
      toast.error(t("voice.registerFailed"), { description: String(e) });
      await reloadConfig();
    }
  };

  const changeMicMode = async (mode: string) => {
    try {
      await setVoiceToggleToTalk(mode === "toggle");
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const rebindPtt = async (accelerator: string) => {
    try {
      await setVoicePttHotkey(accelerator);
      await reloadConfig();
      toast.success(t("voice.pttUpdated"));
    } catch (e) {
      toast.error(t("overlay.shortcutRejected"), { description: String(e) });
    }
  };

  const pickInput = async (name: string) => {
    try {
      await setVoiceInputDevice(name === DEVICE_DEFAULT ? "" : name);
      await reloadConfig();
      // A running meter is listening to the old device; restart it on the new one.
      if (micTesting) {
        await voiceMeterStop();
        await voiceMeterStart();
      }
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const pickOutput = async (name: string) => {
    try {
      await setVoiceOutputDevice(name === DEVICE_DEFAULT ? "" : name);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
  };

  const toggleMicTest = async () => {
    if (micTesting) {
      setMicTesting(false);
      setMicLevel(0);
      await voiceMeterStop();
      return;
    }
    try {
      const warning = await voiceMeterStart();
      setMicTesting(true);
      // The saved headset is gone and we fell back. Silently going mute is the failure
      // that reads as "voice chat is broken", so it gets said out loud.
      if (warning) toast.warning(t("voice.deviceGone"), { description: warning });
    } catch (e) {
      toast.error(t("voice.micFailed"), { description: String(e) });
    }
  };

  const testOutput = async () => {
    try {
      const warning = await voiceTestOutput();
      if (warning) toast.warning(t("voice.deviceGone"), { description: warning });
    } catch (e) {
      toast.error(t("voice.outputFailed"), { description: String(e) });
    }
  };

  const changeLevels = async (gain: number, volume: number) => {
    try {
      await setVoiceLevels(gain, volume);
      await reloadConfig();
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
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
    pane.current?.scrollTo({ top: 0 });
  };

  // Sent here for one setting in particular — open it rather than making them find it.
  useEffect(() => {
    if (!initialSection) return;
    setActive(initialSection);
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
      <nav className="flex w-[170px] flex-none flex-col gap-4 overflow-y-auto px-4 pb-5 pt-[70px]">
        {groups.map((g) => (
          <div key={g.label} className="flex flex-col gap-0.5">
            <span className="px-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wide text-faint">
              {t(g.label)}
            </span>
            {g.sections.map((s) => (
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
          </div>
        ))}
      </nav>

      <div ref={pane} className="min-h-0 flex-1 overflow-y-auto px-2 py-5">
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
          {multiGame && active === "game" && (
            <Section title={t("game.label")} desc={t("settings.gameDesc")}>
              <GameSwitcher />
            </Section>
          )}

          {/* game folder */}
          {active === "folder" && (
          <Section
            title={t("setup.modsFolder", { game: game.display })}
            desc={t("settings.modsFolderDesc")}
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
          )}

          {/* general / background */}
          {active === "general" && (
          <Section title={t("settings.general")}>
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
          )}

          {/* in-game overlay */}
          {active === "overlay" && (
          <Section title={t("overlay.section")}>
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
          )}

          {/* voice — devices, levels and the mic key. The transport that carries any of
              this to other riders isn't built yet, which the callout says plainly rather
              than leaving a page that looks finished and does nothing.

              The labels carry this section on their own: "Microphone" beside a device
              picker doesn't need a line under it explaining that it picks a microphone. */}
          {active === "voice" && (
          <Section title={t("voice.section")}>
            <ToggleRow
              label={t("voice.enable")}
              checked={voiceEnabled}
              onChange={toggleVoice}
            />

            {devices?.error && (
              <Callout tone="warning" title={t("voice.noDevices")}>
                {devices.error}
              </Callout>
            )}

            {/* The room. Nothing here is a control except mute: joining happens because
                the rider is on a server, which is the whole point of the feature. */}
            {voiceEnabled && (
              <div className="space-y-2 rounded-md border border-border/60 p-3">
                <div className="flex items-center gap-2">
                  <span
                    className={cn(
                      "size-1.5 rounded-full",
                      voice.joined ? "bg-success" : "bg-foreground/25",
                    )}
                  />
                  <span className="text-[12.5px] text-foreground/85">
                    {voice.joined ? t("voice.inRoom", { server: voice.server }) : t("voice.notConnected")}
                  </span>
                </div>

                {voice.error && (
                  <Callout tone="warning" title={t("voice.stopped")}>
                    {voice.error}
                  </Callout>
                )}

                {voice.peers.length === 0 ? (
                  <p className="text-[12px] leading-relaxed text-muted-foreground">
                    {t("voice.notConnectedDesc")}
                  </p>
                ) : (
                  <ul className="space-y-1">
                    {voice.peers.map((peer) => (
                      <li key={peer.peerId} className="flex items-center gap-2">
                        {/* Talking is the one thing worth seeing at a glance, so it is a
                            colour change on the row rather than an icon to look for. */}
                        <span
                          className={cn(
                            "size-1.5 shrink-0 rounded-full",
                            !peer.connected
                              ? "bg-foreground/25"
                              : peer.talking
                                ? "bg-success"
                                : "bg-foreground/40",
                          )}
                        />
                        <span
                          className={cn(
                            "flex-1 truncate text-[12.5px]",
                            peer.muted ? "text-muted-foreground line-through" : "text-foreground/85",
                          )}
                        >
                          {peer.riderName || t("voice.unnamedRider")}
                          {peer.raceNum > 0 && (
                            <span className="ml-1.5 text-muted-foreground">#{peer.raceNum}</span>
                          )}
                        </span>
                        {!peer.connected && (
                          <span className="text-[11px] text-muted-foreground">
                            {t("voice.connecting")}
                          </span>
                        )}
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 px-2 text-[11px]"
                          onClick={() => toggleMute(peer.peerId, !peer.muted)}
                        >
                          {peer.muted ? t("voice.unmute") : t("voice.mute")}
                        </Button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}

            <div className="h-px bg-border" />

            {/* Microphone. "" is a real, selectable value — it means "follow whatever
                Windows is set to", which keeps tracking a default the player changes
                later. Storing the resolved name would freeze it instead. */}
            <div className="flex items-center justify-between gap-6">
              <span className="text-[12.5px] text-foreground/85">{t("voice.microphone")}</span>
              <Select
                value={voiceInput || DEVICE_DEFAULT}
                onValueChange={pickInput}
                disabled={!voiceEnabled}
              >
                <SelectTrigger className="h-8 w-[220px]" onClick={() => void refreshDevices()}>
                  <SelectValue placeholder={t("voice.systemDefault")} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={DEVICE_DEFAULT}>{t("voice.systemDefault")}</SelectItem>
                  {devices?.inputs.map((d) => (
                    <SelectItem key={d.name} value={d.name}>
                      {d.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* The single most useful control here: "can it hear me" is most of what
                anyone needs answered, and a meter answers it without a second person. */}
            <div className="flex items-center gap-3">
              <div className="h-2 flex-1 overflow-hidden rounded-full bg-foreground/[0.08]">
                <div
                  className={cn(
                    "h-full rounded-full transition-[width] duration-75",
                    micLevel > 0.85 ? "bg-warning" : "bg-success",
                  )}
                  style={{ width: `${Math.min(100, Math.round(micLevel * 140))}%` }}
                />
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={toggleMicTest}
                disabled={!voiceEnabled}
              >
                {micTesting ? (
                  <>
                    <Square className="size-3.5" /> {t("voice.stopTest")}
                  </>
                ) : (
                  <>
                    <Play className="size-3.5" /> {t("voice.testMic")}
                  </>
                )}
              </Button>
            </div>
            {micTesting && (
              <span className="-mt-1 text-[11.5px] text-muted-foreground">
                {t("voice.speakNow")}
              </span>
            )}

            <div className="h-px bg-border" />

            {/* Output, deliberately separate from game audio: voice on the headset with
                the game on speakers is a setup people actually run. */}
            <div className="flex items-center justify-between gap-6">
              <span className="text-[12.5px] text-foreground/85">{t("voice.output")}</span>
              <Select
                value={voiceOutput || DEVICE_DEFAULT}
                onValueChange={pickOutput}
                disabled={!voiceEnabled}
              >
                <SelectTrigger className="h-8 w-[220px]" onClick={() => void refreshDevices()}>
                  <SelectValue placeholder={t("voice.systemDefault")} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={DEVICE_DEFAULT}>{t("voice.systemDefault")}</SelectItem>
                  {devices?.outputs.map((d) => (
                    <SelectItem key={d.name} value={d.name}>
                      {d.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-center justify-between gap-4">
              <span className="text-[11.5px] text-muted-foreground">
                {t("voice.testOutputDesc")}
              </span>
              <Button
                variant="outline"
                size="sm"
                onClick={testOutput}
                disabled={!voiceEnabled}
              >
                <Play className="size-3.5" /> {t("voice.testOutput")}
              </Button>
            </div>

            <div className="h-px bg-border" />

            <LevelSlider
              label={t("voice.micGain")}
              value={voiceGain}
              min={0}
              max={2}
              disabled={!voiceEnabled}
              onCommit={(v) => changeLevels(v, voiceVolume)}
            />
            <LevelSlider
              label={t("voice.volume")}
              value={voiceVolume}
              min={0}
              max={1}
              disabled={!voiceEnabled}
              onCommit={(v) => changeLevels(voiceGain, v)}
            />

            <div className="h-px bg-border" />

            {/* Hold or latch. Push-to-talk is the default because it cannot leave a
                microphone open by accident; toggle can, which is why the live indicator
                below exists. */}
            <div className="flex items-start justify-between gap-6">
              <div className="flex flex-col gap-1">
                <span className="text-[12.5px] text-foreground/85">{t("voice.micMode")}</span>
                <span className="text-[11.5px] leading-relaxed text-muted-foreground">
                  {voiceToggle ? t("voice.toggleDesc") : t("voice.pttDesc")}
                </span>
              </div>
              <Segmented
                size="sm"
                value={voiceToggle ? "toggle" : "ptt"}
                onChange={changeMicMode}
                options={[
                  { value: "ptt", label: t("voice.modePush") },
                  { value: "toggle", label: t("voice.modeToggle") },
                ]}
              />
            </div>

            <div className="flex items-center justify-between gap-6">
              <span className="flex items-center gap-2 text-[12.5px] text-foreground/85">
                {t("voice.micKey")}
                {/* Live, and deliberately loud in toggle mode: a latched mic has nothing
                    physical to remind you it is open. */}
                {voiceEnabled && micOpen && (
                  <span className="flex items-center gap-1.5 rounded-full bg-success/15 px-2 py-0.5 text-[11px] font-semibold text-success">
                    <span className="size-[6px] rounded-full bg-success" />
                    {t("voice.micOpen")}
                  </span>
                )}
              </span>
              <HotkeyField
                value={voicePtt}
                onCapture={rebindPtt}
                disabled={!voiceEnabled}
              />
            </div>
          </Section>
          )}

          {/* appearance */}
          {active === "appearance" && (
          <Section title={t("settings.appearance")}>
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
          )}

          {/* frostmod — a Win32 DLL injected into the game, so it has nothing to do where
              the game isn't a Windows process. It is one on all three platforms (Proton on
              Linux, a Wine bottle on macOS), and hidden anywhere else rather than
              shown-and-disabled: every control would fail, including one that downloads two
              Windows binaries. The nav drops its entry on the same condition. */}
          {hasFrostmod && caps.frostmod && active === "frostmod" && (
          <Section
            title={t("settings.frostmod")}
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
                      : // Above everything else: a file in the game folder that aborts
                        // MX Bikes with R6034 the moment anything loads the VC9 CRT. The
                        // game not starting at all outranks FrostMod not attaching.
                        strayMsvcr90
                        ? t("settings.frostmodStrayMsvcr90")
                        : // Then a missing Visual C++ runtime, which stops FrostMod
                          // attaching at all — and no amount of repairing or updating
                          // FrostMod puts one on the machine.
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
                {/* The banner carries this too, but a dismissed bar shouldn't take the
                    only explanation with it — and this is the one press that moves a file
                    out of somebody's game folder, so it stays somewhere they can find it
                    again. */}
                {strayMsvcr90 && (
                  <Button
                    size="sm"
                    onClick={() => void clearStrayMsvcr90()}
                    disabled={clearingStray || repairingRuntimes}
                    title={t("runtime.strayFixHint")}
                  >
                    {clearingStray
                      ? t("runtime.strayClearing")
                      : t("runtime.strayFix")}
                  </Button>
                )}
                {/* Its own button rather than a mode of the one below: the FrostMod
                    install and the Windows component are separate things to fix, and
                    someone can genuinely need both. */}
                {missingRuntime && (
                  <Button
                    size="sm"
                    onClick={() => void installRuntime(missingRuntime)}
                    disabled={installingRuntime || repairingRuntimes}
                  >
                    {installingRuntime
                      ? t("runtime.installing")
                      : t("runtime.fixIt")}
                  </Button>
                )}
                {/* Always offered, never gated on `missingRuntime`. Detection can say a PC
                    has everything and be right about that while the game still won't
                    start: the redistributable registers a side-by-side assembly and leaves
                    the plain DLL search path alone. A repair reachable only when we'd
                    already spotted a problem would never run on the machines that need it
                    most — the ones where we spotted nothing. */}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void repairRuntimes()}
                  disabled={repairingRuntimes || installingRuntime}
                  title={t("settings.repairRuntimesHint")}
                >
                  {repairingRuntimes
                    ? t("runtime.repairing")
                    : t("settings.repairRuntimes")}
                </Button>
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

            {/* Only once FrostMod is on disk: without an install there is no config to
                edit, and the keys would be an offer that quietly does nothing. */}
            {status?.installed && <ReplayCameraKeys />}

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
          {active === "reshade" && (
          <Section title={t("settings.reshade")} desc={t("settings.reshadeDesc")}>
            <ReshadeCard />
          </Section>
          )}

          {/* experimental */}
          {active === "experimental" && (
          <Section title={t("settings.experimental")}>
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
          )}

          {/* logs — the first thing any bug report asks for, and the one thing a player
              has no way to find on their own: MXB App's log dir is buried in AppData, and
              the game writes its own beside the executable. Both are named here, either
              can be opened, and the pair zips into one file to attach. */}
          {active === "logs" && (
          <Section title={t("settings.logs")} desc={t("logs.desc")}>
            <LogRow
              label={t("logs.appLogs")}
              hint={t("logs.appLogsDesc")}
              group={logs?.app}
              onOpen={() => openLogs("app")}
            />
            {/* FrostMod's folder is ours — we install it and run it from there — so it
                is offered on the same terms as the rest, on the same condition as the
                FrostMod section above: a Win32 DLL injected into the game has no folder
                to open anywhere else, and a permanently empty row would be the only
                mention of it on the page. */}
            {hasFrostmod && caps.frostmod && (
              <LogRow
                label="FrostMod"
                hint={t("logs.frostmodLogsDesc")}
                group={logs?.frostmod}
                onOpen={() => openLogs("frostmod")}
              />
            )}
            <LogRow
              label={game.display}
              hint={t("logs.gameLogsDesc")}
              group={logs?.game}
              onOpen={() => openLogs("game")}
            />
            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={saveLogs}
                disabled={exportingLogs || !!sharingLogs}
              >
                <Download className="size-3.5" />
                {exportingLogs ? t("logs.saving") : t("logs.save")}
              </Button>
              {/* The same zip, uploaded — for the far more common case where the logs are
                  wanted by someone who isn't sitting at this machine. */}
              <Button
                variant="outline"
                size="sm"
                onClick={shareLogsNow}
                disabled={exportingLogs || !!sharingLogs}
              >
                {sharingLogs ? (
                  <Loader2 className="size-3.5 animate-spin" />
                ) : (
                  <Share2 className="size-3.5" />
                )}
                {sharingLogs || t("logs.share")}
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={refreshLogs}
                disabled={exportingLogs || !!sharingLogs}
              >
                <RefreshCw className="size-3.5" /> {t("logs.refresh")}
              </Button>
            </div>
            {sharedLogs && (
              <div className="flex flex-col gap-1.5">
                <div className="flex gap-2">
                  {/* One link is a field; a sliced bundle is a numbered list, and an
                      `input` would eat the newlines that keep the parts apart. */}
                  {sharedLogs.parts.length > 1 ? (
                    <textarea
                      readOnly
                      value={shareLinkText(sharedLogs)}
                      onFocus={(e) => e.currentTarget.select()}
                      className="h-20 min-w-0 flex-1 resize-none rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px] leading-snug text-muted-foreground"
                    />
                  ) : (
                    <input
                      readOnly
                      value={sharedLogs.url}
                      onFocus={(e) => e.currentTarget.select()}
                      className="min-w-0 flex-1 rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px] text-muted-foreground"
                    />
                  )}
                  <Button variant="outline" size="sm" onClick={copyLogsLink}>
                    {copiedLogsLink ? (
                      <Check className="size-3.5" />
                    ) : (
                      <Copy className="size-3.5" />
                    )}
                    {copiedLogsLink ? t("logs.linkCopiedShort") : t("logs.copyLink")}
                  </Button>
                </div>
                <span className="text-[11.5px] text-muted-foreground">
                  {t("logs.sharedSummary", {
                    count: sharedLogs.files,
                    size: formatBytes(sharedLogs.size),
                  })}
                </span>
                {/* Anonymous public host, no expiry — the one thing worth saying out loud
                    before a link goes into a Discord thread. */}
                <span className="text-[11.5px] text-warning">{t("logs.shareWarning")}</span>
              </div>
            )}
            <p className="text-[11.5px] leading-relaxed text-faint">
              {t("logs.privacy")}
            </p>
          </Section>
          )}

          {/* supporters — who's buying the coffees that pay for the app. Its own entry
              rather than a footnote under About: a thank-you buried under the version
              number and the update button is one nobody reads. */}
          {active === "supporters" && (
          <Section
            title={t("settings.supporters")}
            desc={t("settings.supportersDesc")}
          >
            <SupportersCard />
          </Section>
          )}

          {/* about */}
          {active === "about" && (
          <Section title={t("settings.about")}>
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
          )}
        </div>
      </div>
    </div>
  );
}

/** A labelled 0..max slider that saves on release rather than on every pixel.
 *
 * Dragging fires a change per frame; committing each one would rewrite the config file
 * dozens of times for one gesture. The displayed value tracks the thumb regardless, so
 * the deferred save is invisible. */
function LevelSlider({
  label,
  value,
  min,
  max,
  disabled,
  onCommit,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  disabled?: boolean;
  onCommit: (v: number) => void;
}) {
  const [local, setLocal] = useState(value);
  const [dragging, setDragging] = useState(false);
  // Follow the config while idle, so an external change (or a failed save that reverted)
  // is reflected instead of being masked by stale local state.
  useEffect(() => {
    if (!dragging) setLocal(value);
  }, [value, dragging]);

  const commit = () => {
    setDragging(false);
    onCommit(local);
  };

  return (
    <div className={cn("flex items-center gap-4", disabled && "opacity-60")}>
      <span className="w-[130px] flex-none text-[12.5px] text-foreground/85">{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={0.01}
        value={local}
        disabled={disabled}
        onChange={(e) => {
          setDragging(true);
          setLocal(Number(e.target.value));
        }}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={() => dragging && commit()}
        className="h-1.5 flex-1 cursor-default appearance-none rounded-full bg-foreground/[0.12] accent-primary disabled:opacity-50"
      />
      <span className="w-[42px] flex-none text-right font-mono text-[11.5px] text-muted-foreground">
        {Math.round((local / max) * 100)}%
      </span>
    </div>
  );
}

/** One log location: where it is, what's in it, and a way into the folder.
 *
 * The line under the path is the point of the row. "3 files, newest 12 minutes ago" is
 * what tells someone the log covers the run that just went wrong — a path on its own
 * can't say whether there's anything there worth sending. */
function LogRow({
  label,
  hint,
  group,
  onOpen,
}: {
  label: string;
  hint: string;
  /** Undefined until the backend answers. */
  group?: LogGroup;
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const newest = group?.files[0];
  const status = !group
    ? t("logs.loading")
    : !group.exists
      ? t("logs.folderMissing")
      : group.files.length === 0
        ? t("logs.empty")
        : t("logs.summary", {
            count: group.files.length,
            size: formatBytes(group.files.reduce((sum, f) => sum + f.bytes, 0)),
            when: formatWhen(newest?.modified ?? null),
          });

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center gap-2">
        <span className="text-[12.5px] font-semibold text-foreground/85">{label}</span>
        <span className="text-[11.5px] text-muted-foreground">{hint}</span>
      </div>
      <div className="flex gap-2">
        <div className="flex min-w-0 flex-1 items-center rounded-lg border border-input bg-background px-3 py-2 font-mono text-[12px] text-muted-foreground">
          <span className="flex-1 truncate" title={group?.dir || undefined}>
            {group?.dir || t("settings.notSet")}
          </span>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={onOpen}
          // Nothing to open until we know the folder is there. The button going quiet is
          // itself the answer: the game hasn't written anything here yet.
          disabled={!group?.exists}
        >
          <FolderOpen className="size-3.5" /> {t("logs.open")}
        </Button>
      </div>
      <span
        className={cn(
          "text-[11.5px]",
          group && !group.exists ? "text-warning" : "text-muted-foreground",
        )}
      >
        {status}
      </span>
    </div>
  );
}

/** The link text a share is copied and shown as.
 *
 * One line for the single upload a log bundle almost always is. A bundle big enough to
 * have been sliced needs every part, in order, or it can't be put back together — so
 * they go out as a numbered list rather than a first link that quietly loses the rest. */
function shareLinkText(share: LogsShare): string {
  if (share.parts.length < 2) return share.url;
  return share.parts.map((url, i) => `${i + 1}/${share.parts.length} ${url}`).join("\n");
}

/** "today at 14:32" / "Aug 11 at 14:32" for a log's mtime — the age is what matters,
 *  so the time of day is always shown and the year never is. */
function formatWhen(ms: number | null): string {
  if (!ms) return "";
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "";
  const time = d.toLocaleTimeString(getLocale(), { hour: "2-digit", minute: "2-digit" });
  const sameDay = new Date().toDateString() === d.toDateString();
  return sameDay ? time : `${formatDateShort(d.toISOString())} ${time}`;
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
  /** Optional — a label that already says it doesn't need a line under it repeating it. */
  desc?: string;
  checked: boolean;
  onChange: (v: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <div className={cn("flex items-start justify-between gap-4", disabled && "opacity-60")}>
      <div className="flex flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        {desc && (
          <span className="text-[11.5px] leading-relaxed text-muted-foreground">
            {desc}
          </span>
        )}
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
  children,
}: {
  title: string;
  desc?: string;
  titleRight?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-input bg-card p-[18px]">
      <div className="flex items-center gap-2">
        <span className="flex-1 text-[14px] font-bold">{title}</span>
        {titleRight}
      </div>
      {desc && <span className="-mt-1.5 text-[12px] text-muted-foreground">{desc}</span>}
      {children}
    </div>
  );
}
