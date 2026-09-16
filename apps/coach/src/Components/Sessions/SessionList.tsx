import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { RefreshCw } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { coachSessions, coachStatus, onSessionsChanged, type CoachStatus, type SessionSummary } from "@/api/coach";
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

  // `quiet` keeps the list on screen while it re-reads. A live refresh that blanked it would
  // flash "Loading…" every time the recorder wrote another second of the lap being ridden.
  const load = useCallback((quiet = false) => {
    if (!quiet) setSessions(null);
    coachSessions()
      .then(setSessions)
      .catch((e) => {
        toast.error(String(e));
        setSessions([]);
      });
    coachStatus().then(setStatus).catch(() => {});
  }, []);
  useEffect(() => load(), [load]);

  // The recorder writes while the rider is out on track.
  useEffect(() => {
    let live = true;
    let off: UnlistenFn | undefined;
    onSessionsChanged(() => load(true)).then((stop) => {
      if (live) off = stop;
      else stop();
    });
    return () => {
      live = false;
      off?.();
    };
  }, [load]);

  return (
    <Page
      title={t("sessions.title")}
      sub={t("sessions.sub")}
      actions={
        <Button variant="outline" size="sm" onClick={() => load()}>
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
          <div className="grid grid-cols-[150px_1fr_1fr_60px_90px] gap-3 border-b border-border px-4 py-2 eyebrow">
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
