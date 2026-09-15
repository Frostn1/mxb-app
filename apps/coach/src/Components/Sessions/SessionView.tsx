import { useEffect, useState } from "react";
import { Badge } from "@frost/shared/Components/ui/badge";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { coachLines, coachSession, type Lines, type SessionDetail } from "@/api/coach";
import { gap, lapTime, started } from "@/lib/format";
import Page, { Label } from "../Page";

/** One stint on track: its laps, the lap they're compared with, and the ideal lap. */
export default function SessionView({
  path,
  onBack,
  onReview,
}: {
  path: string;
  onBack: () => void;
  /** `solo` reviews the lap on its own, with no faster lap to compare with. */
  onReview: (lap: number, solo: boolean) => void;
}) {
  const t = useT();
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lines, setLines] = useState<Lines | null>(null);

  useEffect(() => {
    coachSession(path).then(setDetail).catch((e) => setError(String(e)));
    coachLines(path).then(setLines).catch(() => {});
  }, [path]);

  if (error || !detail) {
    return (
      <Page title={t("sessions.title")} onBack={onBack} backLabel={t("sessions.title")}>
        <p className="text-[13px] text-muted-foreground">{error ?? t("common.loading")}</p>
      </Page>
    );
  }

  const { summary: s, reference, ideal } = detail;
  const best = s.bestMs;
  const idealMs = ideal ? ideal.time * 1000 : null;

  return (
    <Page
      title={s.trackName || s.trackId}
      sub={`${s.bikeName || s.bikeId} · ${started(s.started)}`}
      onBack={onBack}
      backLabel={t("sessions.title")}
    >
      <div className="grid gap-3 sm:grid-cols-3">
        <Stat label={t("session.best")} value={lapTime(best)} />
        <Stat
          label={t("session.ideal")}
          value={lapTime(idealMs)}
          note={best && idealMs ? `${gap((idealMs - best) / 1000)} ${t("session.vsBest")}` : undefined}
        />
        <Stat
          label={t("session.reference")}
          value={lapTime(reference?.timeMs)}
          note={reference ? `${reference.bikeName} · ${started(reference.started)}` : t("session.noReference")}
        />
      </div>

      <div className="mt-8">
        <Label>{t("session.laps")}</Label>
        <div className="border border-border">
          {s.laps.length === 0 && (
            <p className="px-4 py-3 text-[12.5px] text-muted-foreground">{t("session.noLaps")}</p>
          )}
          {s.laps.map((l) => {
            const comparable = l.whole && !l.invalid;
            return (
              <div
                key={l.num}
                className="grid grid-cols-[60px_110px_90px_1fr_auto] items-center gap-3 border-b border-border px-4 py-2 text-[12.5px] last:border-b-0"
              >
                <span className="text-muted-foreground">
                  {t("session.lap")} {l.num + 1}
                </span>
                <span className={l.timeMs ? "font-mono tabular-nums" : "font-mono tabular-nums text-muted-foreground"}>
                  {lapTime(l.timeMs || l.riddenMs)}
                </span>
                <span className="font-mono tabular-nums text-muted-foreground">
                  {best && comparable ? (l.timeMs === best ? t("session.bestTag") : gap((l.timeMs - best) / 1000)) : ""}
                </span>
                <span className="flex gap-1.5">
                  {l.invalid && <Badge>{t("session.invalid")}</Badge>}
                  {l.issue && <Badge>{l.issue[0].toUpperCase() + l.issue.slice(1)}</Badge>}
                  {l.crashed && <Badge>{t("session.crashed")}</Badge>}
                </span>
                {/* A lap that can't be held against another is still worth a look on its own. */}
                <Button size="sm" variant="outline" onClick={() => onReview(l.num, !comparable || !reference)}>
                  {t("session.review")}
                </Button>
              </div>
            );
          })}
        </div>
      </div>

      {lines && lines.notes.length > 0 && (
        <div className="mt-8">
          <Label>{t("review.linesTitle")}</Label>
          <div className="space-y-1">
            {lines.notes.map((n, k) => (
              <div key={k} className="border border-border bg-card px-4 py-3">
                <div className="text-[13px] font-semibold">{n.title}</div>
                <div className="mt-0.5 text-[12.5px] leading-snug text-muted-foreground">{n.detail}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {ideal && (
        <div className="mt-8">
          <Label>{t("session.sections")}</Label>
          <div className="border border-border">
            {ideal.sections.map((b, i) => (
              <div
                key={b.name + i}
                className="grid grid-cols-[1fr_100px_70px_110px] items-center gap-3 border-b border-border px-4 py-2 text-[12.5px] last:border-b-0"
              >
                <span className="flex items-center gap-2">
                  {b.name}
                  {ideal.leastConsistent === i && <Badge>{t("session.leastConsistent")}</Badge>}
                </span>
                <span className="font-mono tabular-nums">{b.best.toFixed(3)}</span>
                <span className="text-muted-foreground">
                  {t("session.lap")} {b.lap + 1}
                </span>
                <span className="text-right text-muted-foreground">±{b.spread.toFixed(2)} s</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </Page>
  );
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="border border-border bg-card px-4 py-3">
      <div className="text-[10.5px] font-semibold uppercase tracking-[0.09em] text-faint">{label}</div>
      <div className="mt-1 font-mono text-[20px] tabular-nums">{value}</div>
      {note && <div className="mt-0.5 truncate text-[11.5px] text-muted-foreground">{note}</div>}
    </div>
  );
}
