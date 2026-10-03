import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import { FolderOpen, Monitor } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { revealInExplorer } from "@frost/shared/api/mods";
import SurveySetting from "@frost/shared/Components/Survey/SurveySetting";
import UninstallSetting from "@frost/shared/Components/Uninstall/UninstallSetting";
import { Button } from "@frost/shared/Components/ui/button";
import { Switch } from "@frost/shared/Components/ui/switch";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { cn } from "@frost/shared/lib/utils";
import { useTheme, type ThemeMode } from "@frost/shared/Context/Theme";
import HotkeyField from "@frost/shared/Components/HotkeyField";
import {
  getOverlayState,
  overlayToggle,
  setOverlayEnabled,
  setOverlayHotkey,
  type OverlayState,
} from "@frost/shared/api/overlay";
import { betaUpdates, setBetaUpdates, useUpdate } from "@/Context/Update";
import { useConfig } from "@frost/shared/Context/Config";
import { useT, type TKey } from "@/i18n";
import {
  coachStatus,
  installRecorder,
  openFolder,
  refreshRecorder,
  removeRecorder,
  setGameDir,
  type CoachStatus,
} from "@/api/coach";
import HudPanel from "../Review/HudPanel";
import { SpokenCues } from "../Review/LiveCues";
import LineLookSettings, { LINE_PARTS } from "./LineLook";
import { FieldRow, Rule, Section, ToggleRow } from "./parts";

/** The folder a file sits in. */
const folderOf = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

export type SectionId = "general" | "line" | "hud" | "recording" | "keybinds" | "about";

/**
 * The nav, and with it the page: one section on screen at a time, laid out as MXB App's
 * Settings are so the two apps read alike. It used to be one long column, where the game's
 * line, the recorder and the version number shared a scrollbar.
 *
 * Grouped by where a setting lives: the app itself, what shows in the game, what makes the
 * coaching work at all, and the app's own details.
 */
const GROUPS: { label: TKey; sections: { id: SectionId; label: TKey }[] }[] = [
  { label: "coachSettings.groupApp", sections: [{ id: "general", label: "coachSettings.general" }] },
  {
    label: "coachSettings.groupGame",
    sections: [
      { id: "line", label: "coachSettings.line" },
      { id: "hud", label: "coachSettings.hud" },
    ],
  },
  {
    label: "coachSettings.groupSetup",
    sections: [
      { id: "recording", label: "coachSettings.recording" },
      { id: "keybinds", label: "coachSettings.keybinds" },
    ],
  },
  { label: "coachSettings.groupAbout", sections: [{ id: "about", label: "coachSettings.about" }] },
];

function Row({ label, value, onOpen }: { label: string; value: string; onOpen?: () => void }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border pb-3 last:border-b-0 last:pb-0">
      <div className="min-w-0">
        <div className="eyebrow">{label}</div>
        <div className="mt-1 break-all font-mono text-[12px] text-muted-foreground">{value || "—"}</div>
      </div>
      {onOpen && value && (
        <Button size="sm" variant="outline" onClick={onOpen}>
          <FolderOpen className="size-3.5" />
          {t("coachSettings.open")}
        </Button>
      )}
    </div>
  );
}

const OVERLAY_POLL_MS = 5000;

/** The overlay: on or off, its shortcut, and who holds the shortcut right now. */
function Overlay() {
  const t = useT();
  const [state, setState] = useState<OverlayState | null>(null);
  const refresh = useCallback(() => {
    getOverlayState().then(setState).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const id = setInterval(refresh, OVERLAY_POLL_MS);
    return () => clearInterval(id);
  }, [refresh]);

  const run = async (job: () => Promise<void>, fail: TKey, done?: TKey) => {
    try {
      await job();
      if (done) toast.success(t(done));
    } catch (e) {
      toast.error(t(fail), { description: String(e) });
    } finally {
      refresh();
    }
  };

  const enabled = state?.enabled ?? true;
  return (
    <>
      <ToggleRow
        label={t("overlay.enable")}
        desc={t("overlay.enableDesc")}
        checked={enabled}
        disabled={!state}
        onChange={(on) => void run(() => setOverlayEnabled(on), "overlay.registerFailed")}
      />
      <Rule />
      <div className="flex items-start justify-between gap-6">
        <div className="flex flex-col gap-0.5">
          <span className="text-[12.5px] text-foreground/85">{t("overlay.shortcut")}</span>
          <span className="text-[11.5px] leading-relaxed text-muted-foreground">{t("overlay.shortcutDesc")}</span>
        </div>
        <HotkeyField
          value={state?.hotkey ?? "CommandOrControl+Shift+X"}
          disabled={!enabled}
          onCapture={(combo) => void run(() => setOverlayHotkey(combo), "overlay.shortcutRejected", "overlay.shortcutUpdated")}
        />
      </div>
      {state?.deferred && (
        <p className="text-[12px] text-muted-foreground">{t(`overlay.deferred.${state.deferred}` as TKey)}</p>
      )}
      {/* Said plainly rather than left to look like a hotkey fault: with no link Coach keeps
          its own key, so the shortcut works — it is the sharing that doesn't. */}
      {state?.linkDown && <p className="text-[12px] text-warning">{t("overlay.linkDown")}</p>}
      {state?.hotkeyError && (
        <div className="text-[12px] text-warning">
          <div className="font-semibold">{t("overlay.hotkeyTaken")}</div>
          <div>{t("overlay.hotkeyTakenDesc")}</div>
        </div>
      )}
      <div>
        <Button size="sm" variant="outline" disabled={!enabled} onClick={() => void run(overlayToggle, "overlay.showFailed")}>
          <Monitor className="size-3.5" />
          {t("overlay.showNow")}
        </Button>
      </div>
    </>
  );
}

/** The recorder plugin, updates, where the coach looks, and what it shows in the game. */
export default function Settings({ initialSection }: { initialSection?: SectionId } = {}) {
  const t = useT();
  const { game } = useConfig();
  const [active, setActive] = useState<SectionId>(initialSection ?? "general");
  const pane = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<CoachStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState("");
  const [beta, setBeta] = useState(betaUpdates);
  const { theme, setTheme } = useTheme();
  const update = useUpdate();
  useEffect(() => {
    getVersion().then(setVersion).catch(() => {});
  }, []);

  const load = useCallback(() => {
    coachStatus().then(setStatus).catch(() => {});
  }, []);
  useEffect(() => load(), [load]);

  const run = async (job: () => Promise<unknown>, done: string) => {
    setBusy(true);
    try {
      await job();
      toast.success(done);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
      load();
    }
  };

  const pickGame = async () => {
    const dir = await open({ directory: true });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      setStatus(await setGameDir(dir));
      toast.success(t("recorder.gameFolderSet"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const fromFile = async () => {
    const f = await open({ filters: [{ name: "MX Bikes plugin", extensions: ["dlo"] }] });
    if (typeof f === "string") await run(() => installRecorder(f), t("recorder.installed"));
  };

  const show = (job: Promise<void>) => job.catch((e) => toast.error(String(e)));
  const plugin = status?.pluginPath ?? "";

  // Keep the recorder current without being asked. Nothing has ever refreshed `mxbcoach.dlo`,
  // so a rider installed it once and kept it — and a recorder older than the app it serves
  // draws nothing and, before 0.23, could not even say its own version.
  useEffect(() => {
    void refreshRecorder()
      .then((v) => {
        if (v) {
          toast.success(t("recorder.refreshed", { version: v }));
          load();
        }
      })
      .catch(() => {});
    // Once per visit to Settings; the button beside it is there for any other time. `load` and
    // `t` are deliberately not dependencies: re-running this would re-download the recorder.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const goto = (id: SectionId) => {
    setActive(id);
    pane.current?.scrollTo({ top: 0 });
  };

  return (
    <div className="flex h-full">
      <nav className="flex w-[170px] flex-none flex-col gap-4 overflow-y-auto px-4 pb-5 pt-8">
        <h2 className="headline px-3 text-[24px]">{t("coachSettings.title")}</h2>
        {GROUPS.map((g) => (
          <div key={g.label} className="flex flex-col gap-0.5">
            <span className="px-3 pb-1 text-[10.5px] font-semibold uppercase tracking-wide text-faint">{t(g.label)}</span>
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

      <div ref={pane} className="min-h-0 flex-1 overflow-y-auto px-2 py-8 pr-8">
        <div className="flex min-w-0 max-w-[820px] flex-col gap-[18px]">
          {active === "general" && (
            <>
              <Section title={t("settings.appearance")}>
                <FieldRow label={t("settings.theme")}>
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
                </FieldRow>
              </Section>
              {/* Coach has one dictionary so far (i18n/index.ts serves it to every locale), so
                  this says so rather than offering a picker that changes nothing. */}
              <Section title={t("coachSettings.language")}>
                <FieldRow label={t("coachSettings.language")} desc={t("coachSettings.languageBody")}>
                  <span className="text-[12.5px] font-semibold">English</span>
                </FieldRow>
              </Section>
            </>
          )}

          {active === "line" && <LineLookSettings />}

          {/* The same panel the in-game tab shows, reading and writing the same hud.ini and
              voice.ini; the line's own parts are under In-game line. */}
          {active === "hud" && (
            <>
              <Section title={t("hud.title")}>
                <HudPanel bare exclude={LINE_PARTS} />
              </Section>
              <Section title={t("coachSettings.spoken")}>
                <SpokenCues />
              </Section>
            </>
          )}

          {active === "recording" && (
            <>
              <Section title={t("recorder.title")}>
                <div>
                  <div className="text-[13px] font-semibold">
                    {status?.pluginInstalled ? t("recorder.on") : t("recorder.off")}
                  </div>
                  <p className="mt-1 text-[12.5px] text-muted-foreground">{t("recorder.body")}</p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    disabled={busy || !status?.gameDir}
                    onClick={() => run(() => installRecorder(), t("recorder.installed"))}
                  >
                    {status?.pluginInstalled ? t("recorder.update") : t("recorder.install")}
                  </Button>
                  <Button size="sm" variant="outline" disabled={busy || !status?.gameDir} onClick={() => void fromFile()}>
                    {t("recorder.fromFile")}
                  </Button>
                  {status?.pluginInstalled && (
                    <Button size="sm" variant="outline" disabled={busy} onClick={() => run(removeRecorder, t("recorder.removed"))}>
                      {t("recorder.remove")}
                    </Button>
                  )}
                  {status?.recorderVersion && (
                    <span className="self-center font-mono text-[12px] text-muted-foreground">
                      {t("recorder.version", { version: status.recorderVersion })}
                    </span>
                  )}
                </div>
                {status && !status.recorderVersion && (
                  <p className="text-[12px] text-muted-foreground">{t("recorder.versionUnknown")}</p>
                )}
                {status?.recorderOutdated && <p className="text-[12px] text-warning">{t("recorder.updateIt")}</p>}
                <Rule />
                <div>
                  <div className="text-[12.5px] font-semibold">{t("recorder.gameFolder")}</div>
                  <p className="mt-0.5 font-mono text-[12px] text-muted-foreground">
                    {status?.gameDir || t("recorder.gameFolderNone")}
                  </p>
                  <div className="mt-2 flex flex-wrap gap-2">
                    <Button size="sm" variant="outline" disabled={busy} onClick={() => void pickGame()}>
                      {t("recorder.gameFolderPick")}
                    </Button>
                  </div>
                  {status && !status.gameDir && <p className="mt-2 text-[12px] text-warning">{t("recorder.noGame")}</p>}
                </div>
              </Section>

              <Section title={t("coachSettings.where")}>
                <Row label={t("coachSettings.game")} value={game.display} />
                <Row
                  label={t("coachSettings.gameFolder")}
                  value={status?.gameDir ?? ""}
                  onOpen={() => void show(openFolder(status?.gameDir ?? ""))}
                />
                <Row
                  label={t("coachSettings.plugin")}
                  value={plugin}
                  onOpen={() => void show(status?.pluginInstalled ? revealInExplorer(plugin) : openFolder(folderOf(plugin)))}
                />
                {(status?.sessionDirs.length ? status.sessionDirs : [""]).map((dir, k) => (
                  <Row key={dir || k} label={t("coachSettings.sessions")} value={dir} onOpen={() => void show(openFolder(dir))} />
                ))}
              </Section>
            </>
          )}

          {active === "keybinds" && (
            <Section title={t("overlay.section")}>
              <Overlay />
            </Section>
          )}

          {active === "about" && (
            <>
              <Section title={t("coachSettings.updates")}>
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <div className="text-[12.5px] text-foreground/85">{t("coachSettings.beta")}</div>
                    <p className="mt-0.5 text-[11.5px] text-muted-foreground">{t("coachSettings.betaBody")}</p>
                  </div>
                  <Switch
                    checked={beta}
                    onCheckedChange={(on) => {
                      setBetaUpdates(on);
                      setBeta(on);
                    }}
                  />
                </div>
                <div className="flex items-center gap-3">
                  <Button size="sm" variant="outline" onClick={() => void update.check({ silent: false })}>
                    {t("coachSettings.check")}
                  </Button>
                  {version && <span className="font-mono text-[12px] text-muted-foreground">v{version}</span>}
                </div>
              </Section>
              {/* The same rows MXB App and the Studio show; each carries its own label. */}
              <div className="rounded-xl bg-card p-[18px]">
                <SurveySetting />
              </div>
              <div className="rounded-xl bg-card p-[18px]">
                <UninstallSetting />
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
