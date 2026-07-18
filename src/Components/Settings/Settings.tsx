import { useEffect, useRef, useState } from "react";
import { Check, RefreshCw, ExternalLink, Play } from "lucide-react";
import { open as pickFolder } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { getVersion } from "@tauri-apps/api/app";
import { toast } from "sonner";
import {
  createConfig,
  setAutoRunFrostmod,
  setGamePath,
  setInstantRefresh,
  setLaunchAtStartup,
  setRunInBackground,
} from "../../api/mods";
import { checkForUpdates } from "../../lib/updater";
import { useConfig } from "../../Context/Config";
import { useTheme, type ThemeMode } from "../../Context/Theme";
import { useFrostmod } from "../../Context/FrostmodContext";
import { Button } from "@/Components/ui/button";
import { Segmented } from "@/Components/ui/segmented";
import { Switch } from "@/Components/ui/switch";
import { cn } from "@/lib/utils";

const REPO_URL = "https://github.com/Frostn1/mxb-app";

type SectionId = "folder" | "general" | "appearance" | "frostmod" | "about";
const SECTIONS: { id: SectionId; label: string }[] = [
  { id: "folder", label: "Game folder" },
  { id: "general", label: "General" },
  { id: "appearance", label: "Appearance" },
  { id: "frostmod", label: "FrostMod" },
  { id: "about", label: "About & updates" },
];

export default function Settings() {
  const { config, reloadConfig } = useConfig();
  const { theme, setTheme } = useTheme();
  const { running, reload, status, installing, checking, statusError, install, start, refreshStatus } =
    useFrostmod();
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

  const runInBackground = config.runInBackground ?? true;
  const launchAtStartup = config.launchAtStartup ?? true;
  const autoRunFrostmod = config.autoRunFrostmod ?? true;
  const instantRefresh = config.instantRefresh ?? true;

  const toggleInstantRefresh = async (v: boolean) => {
    try {
      await setInstantRefresh(v);
      await reloadConfig();
    } catch (e) {
      toast.error("Couldn't update setting", { description: String(e) });
    }
  };

  const toggleAutoRun = async (v: boolean) => {
    try {
      await setAutoRunFrostmod(v);
      await reloadConfig();
    } catch (e) {
      toast.error("Couldn't update setting", { description: String(e) });
    }
  };

  const toggleBackground = async (v: boolean) => {
    try {
      await setRunInBackground(v);
      await reloadConfig();
    } catch (e) {
      toast.error("Couldn't update setting", { description: String(e) });
    }
  };

  const toggleStartup = async (v: boolean) => {
    try {
      await setLaunchAtStartup(v);
      await reloadConfig();
    } catch (e) {
      toast.error("Couldn't update startup setting", { description: String(e) });
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
      title: "Select your MX Bikes folder",
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await createConfig({ modsPath: picked });
      await reloadConfig();
      toast.success("Game folder updated", { description: "Your library will re-scan." });
    } catch (e) {
      toast.error("Couldn't set folder", { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const detectAgain = async () => {
    setBusy(true);
    try {
      await createConfig({ modsPath: "" });
      await reloadConfig();
      toast.success("Re-detected your MX Bikes folder");
    } catch (e) {
      toast.error("Couldn't detect folder", { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const changeGameFolder = async () => {
    const picked = await pickFolder({
      directory: true,
      multiple: false,
      title: "Select your MX Bikes install folder (contains rider.pkz)",
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    try {
      await setGamePath(picked);
      await reloadConfig();
      toast.success("Game install set", {
        description: "The 3D rider preview can now load the real body model.",
      });
    } catch (e) {
      toast.error("Couldn't set install folder", { description: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const reloadGame = async () => {
    const outcome = await reload();
    if (outcome === "signaled") toast.success("FrostMod reloaded the game.");
    else if (outcome === "not_running")
      toast.info("FrostMod isn't running — start it to hot-reload mods.");
    else toast.info("Reload isn't available on this platform.");
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
            {s.label}
          </button>
        ))}
      </nav>

      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-5">
        <div className="flex max-w-[640px] flex-col gap-[18px]">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">Settings</h1>

          {/* game folder */}
          <Section
            title="MX Bikes folder"
            desc="Where mods are installed. Changing it re-scans your library."
            innerRef={(el) => (refs.current.folder = el)}
          >
            <div className="flex gap-2">
              <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                <span className="flex-1 truncate" title={config.modsPath}>
                  {config.modsPath || "Not set"}
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

            <div className="mt-1 h-px bg-border" />

            {/* Optional game *install* folder (holds core rider.pkz) — powers the
                real 3D rider body in the preset preview. */}
            <p className="text-[12px] text-muted-foreground">
              Game install folder (optional) — where MX Bikes is installed (holds{" "}
              <span className="font-mono">rider.pkz</span>). Set it to load the real
              rider body in the 3D preview.
            </p>
            <div className="flex gap-2">
              <div className="flex flex-1 items-center gap-2 rounded-lg border border-input bg-background px-3 py-2.5 font-mono text-[12px] text-muted-foreground">
                <span className="flex-1 truncate" title={config.gamePath}>
                  {config.gamePath || "Not set"}
                </span>
                {config.gamePath && (
                  <span className="flex flex-none items-center gap-1 font-sans text-[11px] font-semibold text-success">
                    <Check className="size-3" strokeWidth={3} /> Set
                  </span>
                )}
              </div>
              <Button variant="outline" size="sm" onClick={changeGameFolder} disabled={busy}>
                {config.gamePath ? "Change…" : "Set…"}
              </Button>
            </div>
          </Section>

          {/* general / background */}
          <Section title="General" innerRef={(el) => (refs.current.general = el)}>
            <ToggleRow
              label="Keep running in the background"
              desc="Closing the window hides MXB App to the tray so FrostMod stays connected. Quit from the tray icon."
              checked={runInBackground}
              onChange={toggleBackground}
            />
            <div className="h-px bg-border" />
            <ToggleRow
              label="Launch at startup"
              desc="Start MXB App automatically when you log in."
              checked={launchAtStartup}
              onChange={toggleStartup}
            />
            <div className="h-px bg-border" />
            <ToggleRow
              label="Instant preset refresh"
              desc="When you apply a preset while MX Bikes is running, refresh the look in-game instantly — no restart or profile reselect. Windows only; if it can't, you'll be told to reselect your profile."
              checked={instantRefresh}
              onChange={toggleInstantRefresh}
            />
          </Section>

          {/* appearance */}
          <Section
            title="Appearance"
            innerRef={(el) => (refs.current.appearance = el)}
          >
            <div className="flex items-center justify-between">
              <span className="text-[12.5px] text-foreground/85">Theme</span>
              <Segmented
                size="sm"
                value={theme}
                onChange={(v) => setTheme(v as ThemeMode)}
                options={[
                  { value: "light", label: "Light" },
                  { value: "dark", label: "Dark" },
                  { value: "system", label: "System" },
                ]}
              />
            </div>
          </Section>

          {/* frostmod */}
          <Section
            title="FrostMod"
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
                  ? "Checking…"
                  : running
                    ? "Running · game connected"
                    : "Not running"}
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
                    ? `Installed${status.version ? ` · ${status.version}` : ""}`
                    : "Not installed"}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  {checking
                    ? "Checking GitHub for the latest release…"
                    : statusError
                      ? "Couldn't check for updates — offline or GitHub unavailable."
                      : status?.latest
                        ? `Latest: ${status.latest}`
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
                    title="Check for a newer FrostMod"
                  >
                    <RefreshCw className={cn("size-3.5", checking && "animate-spin")} />
                  </Button>
                )}
                {(() => {
                  const updatable =
                    status?.installed &&
                    status.latest &&
                    status.version !== status.latest;
                  // "Up to date" only when we actually confirmed the latest tag.
                  const confirmedCurrent =
                    status?.installed && !updatable && !statusError && status?.latest;
                  return (
                    <Button
                      variant={confirmedCurrent ? "outline" : "default"}
                      size="sm"
                      onClick={install}
                      disabled={installing || checking || Boolean(confirmedCurrent)}
                    >
                      {installing
                        ? "Working…"
                        : !status?.installed
                          ? "Install FrostMod"
                          : updatable
                            ? `Update to ${status.latest}`
                            : statusError || !status?.latest
                              ? "Reinstall latest"
                              : "Up to date"}
                    </Button>
                  );
                })()}
              </div>
            </div>

            <ToggleRow
              label="Run FrostMod automatically"
              desc="Start FrostMod in the background whenever MXB App opens."
              checked={autoRunFrostmod}
              onChange={toggleAutoRun}
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

          {/* about */}
          <Section title="About & updates" innerRef={(el) => (refs.current.about = el)}>
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
            <div>
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
            </div>
            <div className="flex flex-col gap-1 pt-1 text-[11.5px] text-faint">
              <div className="flex items-center gap-1.5">
                <span>Made with</span>
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
