import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { FolderOpen, Monitor } from "lucide-react";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-dialog";
import { revealInExplorer } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { Switch } from "@frost/shared/Components/ui/switch";
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
import { coachStatus, installRecorder, openFolder, removeRecorder, type CoachStatus } from "@/api/coach";
import Page, { Label } from "../Page";

/** The folder a file sits in. */
const folderOf = (path: string) => path.replace(/[\\/][^\\/]*$/, "");

function Row({ label, value, onOpen }: { label: string; value: string; onOpen?: () => void }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border py-3 last:border-b-0">
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
    <div className="border border-border bg-card px-4 py-4">
      <div className="flex items-center justify-between gap-4">
        <div>
          <div className="text-[13px] font-semibold">{t("overlay.enable")}</div>
          <p className="mt-0.5 text-[12.5px] text-muted-foreground">{t("overlay.enableDesc")}</p>
        </div>
        <Switch
          checked={enabled}
          disabled={!state}
          onCheckedChange={(on) => void run(() => setOverlayEnabled(on), "overlay.registerFailed")}
        />
      </div>
      <div className="mt-4 flex items-start justify-between gap-6 border-t border-border pt-4">
        <div>
          <div className="text-[13px] font-semibold">{t("overlay.shortcut")}</div>
          <p className="mt-0.5 text-[12.5px] text-muted-foreground">{t("overlay.shortcutDesc")}</p>
        </div>
        <HotkeyField
          value={state?.hotkey ?? "CommandOrControl+Shift+X"}
          disabled={!enabled}
          onCapture={(combo) => void run(() => setOverlayHotkey(combo), "overlay.shortcutRejected", "overlay.shortcutUpdated")}
        />
      </div>
      {state?.deferred && (
        <p className="mt-3 text-[12px] text-muted-foreground">{t(`overlay.deferred.${state.deferred}` as TKey)}</p>
      )}
      {state?.hotkeyError && (
        <div className="mt-3 text-[12px] text-warning">
          <div className="font-semibold">{t("overlay.hotkeyTaken")}</div>
          <div>{t("overlay.hotkeyTakenDesc")}</div>
        </div>
      )}
      <div className="mt-4">
        <Button size="sm" variant="outline" disabled={!enabled} onClick={() => void run(overlayToggle, "overlay.showFailed")}>
          <Monitor className="size-3.5" />
          {t("overlay.showNow")}
        </Button>
      </div>
    </div>
  );
}

/** The recorder plugin, updates, and where the coach looks. */
export default function Settings() {
  const t = useT();
  const { game } = useConfig();
  const [status, setStatus] = useState<CoachStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [version, setVersion] = useState("");
  const [beta, setBeta] = useState(betaUpdates);
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

  const fromFile = async () => {
    const f = await open({ filters: [{ name: "MX Bikes plugin", extensions: ["dlo"] }] });
    if (typeof f === "string") await run(() => installRecorder(f), t("recorder.installed"));
  };

  const show = (job: Promise<void>) => job.catch((e) => toast.error(String(e)));
  const plugin = status?.pluginPath ?? "";

  return (
    <Page title={t("coachSettings.title")}>
      <Label>{t("recorder.title")}</Label>
      <div className="border border-border bg-card px-4 py-4">
        <div className="text-[13px] font-semibold">
          {status?.pluginInstalled ? t("recorder.on") : t("recorder.off")}
        </div>
        <p className="mt-1 text-[12.5px] text-muted-foreground">{t("recorder.body")}</p>
        <div className="mt-4 flex flex-wrap gap-2">
          <Button size="sm" disabled={busy || !status?.gameDir} onClick={() => run(() => installRecorder(), t("recorder.installed"))}>
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
        </div>
        {status && !status.gameDir && <p className="mt-3 text-[12px] text-warning">{t("recorder.noGame")}</p>}
      </div>

      <div className="mt-8">
        <Label>{t("overlay.section")}</Label>
        <Overlay />
      </div>

      <div className="mt-8">
        <Label>{t("coachSettings.updates")}</Label>
        <div className="border border-border bg-card px-4 py-4">
          <div className="flex items-center justify-between gap-4">
            <div>
              <div className="text-[13px] font-semibold">{t("coachSettings.beta")}</div>
              <p className="mt-0.5 text-[12.5px] text-muted-foreground">{t("coachSettings.betaBody")}</p>
            </div>
            <Switch
              checked={beta}
              onCheckedChange={(on) => {
                setBetaUpdates(on);
                setBeta(on);
              }}
            />
          </div>
          <div className="mt-4 flex items-center gap-3">
            <Button size="sm" variant="outline" onClick={() => void update.check({ silent: false })}>
              {t("coachSettings.check")}
            </Button>
            {version && <span className="font-mono text-[12px] text-muted-foreground">v{version}</span>}
          </div>
        </div>
      </div>

      <div className="mt-8">
        <Label>{t("coachSettings.where")}</Label>
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
      </div>
    </Page>
  );
}
