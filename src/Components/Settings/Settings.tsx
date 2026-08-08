import { useEffect, useRef, useState } from "react";
import { Check, RefreshCw, ExternalLink, Play, Compass, MessagesSquare } from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";
import {
  countProfilesIn,
  detectGamePath,
  presetsListProfiles,
  setAutoRunFrostmod,
  setGamePath,
  setInstantRefresh,
  setLaunchAtStartup,
  setModsPath,
  setProfilesPath,
  setRunInBackground,
  setWatchModsReload,
} from "../../api/mods";
import { useUpdate } from "../../Context/Update";
import { usePlatform } from "../../lib/usePlatform";
import { useConfig } from "../../Context/Config";
import { useTheme, type ThemeMode } from "../../Context/Theme";
import { Trans } from "../../i18n";
import { useI18n, type LocalePref, type TKey } from "../../i18n/context";
import { LOCALE_OPTIONS } from "../../i18n/core";
import { useFrostmod } from "../../Context/FrostmodContext";
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

type SectionId = "folder" | "general" | "appearance" | "frostmod" | "about";
const SECTIONS: { id: SectionId; label: TKey }[] = [
  { id: "folder", label: "settings.gameFolder" },
  { id: "general", label: "settings.general" },
  { id: "appearance", label: "settings.appearance" },
  { id: "frostmod", label: "settings.frostmod" },
  { id: "about", label: "settings.about" },
];

export default function Settings() {
  const { t, locale, setLocale } = useI18n();
  const { config, reloadConfig } = useConfig();
  const isWindows = usePlatform() === "windows";
  const { theme, setTheme } = useTheme();
  const { running, reload, status, installing, checking, statusError, install, start, refreshStatus } =
    useFrostmod();
  const { check: checkForUpdates } = useUpdate();
  const { startTour } = useTour();
  const [version, setVersion] = useState("");
  const [active, setActive] = useState<SectionId>("folder");
  const [busy, setBusy] = useState(false);
  const refs = useRef<Record<SectionId, HTMLDivElement | null>>({
    folder: null,
    general: null,
    appearance: null,
    frostmod: null,
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
    // Re-check FrostMod against GitHub whenever Settings opens — the provider
    // only fetches once at launch, so this catches releases cut since then.
    void refreshStatus();
  }, [refreshStatus]);

  const goto = (id: SectionId) => {
    setActive(id);
    refs.current[id]?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const changeFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: t("setup.pickModsFolder"),
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await setModsPath(picked);
      await reloadConfig();
      toast.success(t("settings.folderUpdated"), {
        description: t("settings.folderUpdatedDesc"),
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
        {SECTIONS.map((s) => (
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

          {/* game folder */}
          <Section
            title={t("setup.modsFolder")}
            desc={t("settings.modsFolderDesc")}
            innerRef={(el) => (refs.current.folder = el)}
          >
            <div className="flex gap-2">
              <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                <span className="flex-1 truncate" title={config.modsPath}>
                  {config.modsPath || t("settings.notSet")}
                </span>
                {config.modsPath && (
                  <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                    <Check className="size-3" strokeWidth={3} /> Set
                  </span>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={changeFolder} disabled={busy}>
                Change…
              </Button>
            </div>
            <button
              onClick={detectAgain}
              disabled={busy}
              className="cursor-default self-start text-[11.5px] font-semibold text-primary hover:brightness-110 disabled:opacity-50"
            >
              Detect automatically
            </button>

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
                      <span className="font-mono">Documents\PiBoSo\MX Bikes</span>
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
                isWindows
                  ? t("settings.instantRefreshDesc")
                  : t("settings.instantRefreshWindowsOnly")
              }
              checked={instantRefresh}
              onChange={toggleInstantRefresh}
            />
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
          {isWindows && (
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
                      : status?.needsRepair
                        ? t("settings.frostmodNeedsRepair")
                        : status?.latest
                          ? t("settings.latestVersion", { version: status.latest })
                          : null}
                </span>
              </div>
              <div className="flex items-center gap-1.5">
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
                  // "Up to date" only when we actually confirmed the latest tag.
                  const confirmedCurrent =
                    status?.installed &&
                    !updatable &&
                    !repairable &&
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
              {status?.installed && !running && (
                <Button variant="default" size="sm" onClick={start}>
                  <Play className="size-3.5" /> Start FrostMod
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={reloadGame} disabled={!running}>
                <RefreshCw className="size-3.5" /> Reload game now
              </Button>
            </div>
          </Section>
          )}

          {/* about */}
          <Section title={t("settings.about")} innerRef={(el) => (refs.current.about = el)}>
            <div className="flex items-center gap-3 text-[12px] text-muted-foreground">
              <span>mxb-app {version && `v${version}`}</span>
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
}: {
  label: string;
  desc: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex flex-col gap-0.5">
        <span className="text-[12.5px] text-foreground/85">{label}</span>
        <span className="text-[11.5px] leading-relaxed text-muted-foreground">
          {desc}
        </span>
      </div>
      <div className="pt-0.5">
        <Switch checked={checked} onCheckedChange={onChange} />
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
