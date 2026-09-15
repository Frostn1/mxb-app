import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@/i18n";
import { coachStatus, installRecorder, removeRecorder, type CoachStatus } from "@/api/coach";
import Page, { Label } from "../Page";

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="border-b border-border py-3 last:border-b-0">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</div>
      <div className="mt-1 break-all font-mono text-[12px] text-muted-foreground">{value || "—"}</div>
    </div>
  );
}

/** The recorder plugin, and where the coach looks. */
export default function Settings() {
  const t = useT();
  const { game } = useConfig();
  const [status, setStatus] = useState<CoachStatus | null>(null);
  const [busy, setBusy] = useState(false);

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
        <Label>{t("coachSettings.where")}</Label>
        <Row label={t("coachSettings.game")} value={game.display} />
        <Row label={t("coachSettings.gameFolder")} value={status?.gameDir ?? ""} />
        <Row label={t("coachSettings.plugin")} value={status?.pluginPath ?? ""} />
        <Row label={t("coachSettings.sessions")} value={(status?.sessionDirs ?? []).join("\n")} />
      </div>
    </Page>
  );
}
