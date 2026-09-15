import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { RefreshCw } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { coachSessions, coachStatus, type CoachStatus, type SessionSummary } from "@/api/coach";
import { lapTime, started } from "@/lib/format";
import Page from "../Page";

export default function SessionList({
  onOpen,
  onSettings,
}: {
  onOpen: (path: string) => void;
  onSettings: () => void;
}) {
  const t = useT();
  const [sessions, setSessions] = useState<SessionSummary[] | null>(null);
  const [status, setStatus] = useState<CoachStatus | null>(null);

  const load = useCallback(() => {
    setSessions(null);
    coachSessions()
      .then(setSessions)
      .catch((e) => {
        toast.error(String(e));
        setSessions([]);
      });
    coachStatus().then(setStatus).catch(() => {});
  }, []);
  useEffect(() => load(), [load]);

  return (
    <Page
      title={t("sessions.title")}
      sub={t("sessions.sub")}
      actions={
        <Button variant="outline" size="sm" onClick={load}>
          <RefreshCw className="size-3.5" />
          {t("common.refresh")}
        </Button>
      }
    >
      {status && !status.pluginInstalled && (
        <div className="mb-6 flex items-center justify-between gap-4 border border-warning/40 bg-warning/10 px-4 py-3">
          <div className="text-[12.5px]">
            <div className="font-semibold">{t("recorder.missingTitle")}</div>
            <div className="mt-0.5 text-muted-foreground">{t("recorder.missingBody")}</div>
          </div>
          <Button size="sm" onClick={onSettings}>
            {t("recorder.setUp")}
          </Button>
        </div>
      )}

      {sessions === null ? (
        <p className="text-[13px] text-muted-foreground">{t("common.loading")}</p>
      ) : sessions.length === 0 ? (
        <div className="border border-border px-5 py-6 text-[13px] text-muted-foreground">
          <div className="font-semibold text-foreground">{t("sessions.emptyTitle")}</div>
          <p className="mt-1">{t("sessions.emptyBody")}</p>
        </div>
      ) : (
        <div className="border border-border">
          <div className="grid grid-cols-[150px_1fr_1fr_60px_90px] gap-3 border-b border-border px-4 py-2 text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">
            <span>{t("sessions.when")}</span>
            <span>{t("sessions.track")}</span>
            <span>{t("sessions.bike")}</span>
            <span className="text-right">{t("sessions.laps")}</span>
            <span className="text-right">{t("sessions.best")}</span>
          </div>
          {sessions.map((s) => (
            <button
              key={s.path}
              onClick={() => onOpen(s.path)}
              className="grid w-full grid-cols-[150px_1fr_1fr_60px_90px] items-center gap-3 border-b border-border px-4 py-2.5 text-left text-[12.5px] last:border-b-0 hover:bg-accent"
            >
              <span className="text-muted-foreground">{started(s.started)}</span>
              <span className="truncate font-medium">{s.trackName || s.trackId}</span>
              <span className="truncate text-muted-foreground">{s.bikeName || s.bikeId}</span>
              <span className="text-right tabular-nums">{s.laps.length}</span>
              <span className="text-right font-mono tabular-nums">{lapTime(s.bestMs)}</span>
            </button>
          ))}
        </div>
      )}
    </Page>
  );
}
