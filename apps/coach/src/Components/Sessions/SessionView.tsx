import { useCallback, useEffect, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { RefreshCw } from "lucide-react";
import { Badge } from "@frost/shared/Components/ui/badge";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import {
  coachLines,
  coachSession,
  coachSessions,
  onSessionsChanged,
  type Lines,
  type SessionDetail,
  type SessionSummary,
} from "@/api/coach";
import { gap, lapTime, started } from "@/lib/format";
import Page, { Label } from "../Page";

/** One session: every stint of the event, its laps, the lap they're compared with, and the
 *  ideal lap. */
export default function SessionView({
  path,
  onBack,
  onReview,
}: {
  path: string;
  onBack: () => void;
  /** The lap's own recording and number. `solo` reviews it with no faster lap to compare with;
   *  the track comes along because the review remembers its reference per track. */
  onReview: (path: string, lap: number, solo: boolean, trackId: string) => void;
}) {
  const t = useT();
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lines, setLines] = useState<Lines | null>(null);
  const [all, setAll] = useState<SessionSummary[]>([]);

  // Re-read rather than replace: a lap finished while this page is open should just appear.
  const load = useCallback(() => {
    coachSession(path).then(setDetail).catch((e) => setError(String(e)));
    coachLines(path).then(setLines).catch(() => {});
    coachSessions().then(setAll).catch(() => {});
  }, [path]);
  useEffect(() => load(), [load]);

  useEffect(() => {
    let live = true;
    let off: UnlistenFn | undefined;
    onSessionsChanged(load).then((stop) => {
      if (live) off = stop;
      else stop();
    });
    return () => {
      live = false;
      off?.();
    };
  }, [load]);

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
  const setups = compareSetups(all.filter((o) => o.trackId === s.trackId && o.bikeId === s.bikeId));
  // Several stints in one session: the lap numbers start again in each, so they're named.
  const multi = s.stints.length > 1;

  return (
    <Page
      title={s.trackName || s.trackId}
      sub={[s.bikeName || s.bikeId, started(s.started), multi ? t("session.stints", { n: s.stints.length }) : ""]
        .filter(Boolean)
        .join(" · ")}
      onBack={onBack}
      backLabel={t("sessions.title")}
      actions={
        <Button variant="outline" size="sm" onClick={load}>
          <RefreshCw className="size-3.5" />
          {t("common.refresh")}
        </Button>
      }
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
                key={`${l.path}-${l.num}`}
                className="grid grid-cols-[130px_110px_90px_1fr_auto] items-center gap-3 border-b border-border px-4 py-2 text-[12.5px] last:border-b-0"
              >
                <span className="text-muted-foreground">
                  {t("session.lap")} {l.num + 1}
                  {multi && <span className="ml-1.5 text-faint">{t("session.stintTag", { n: l.stint + 1 })}</span>}
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
                <Button size="sm" variant="outline" onClick={() => onReview(l.path, l.num, !comparable || !reference, s.trackId)}>
                  {t("session.review")}
                </Button>
              </div>
            );
          })}
        </div>
      </div>

      {setups.length > 1 && (
        <div className="mt-8">
          <Label>{t("session.setups")}</Label>
          <p className="-mt-1 mb-2 text-[12px] text-muted-foreground">{t("session.setupsHint")}</p>
          <div className="border border-border">
            {setups.map((r, i) => (
              <div
                key={r.name}
                className="grid grid-cols-[1fr_60px_100px_120px] items-center gap-3 border-b border-border px-4 py-2 text-[12.5px] last:border-b-0"
              >
                <span className="flex min-w-0 items-center gap-2">
                  <span className="truncate">{r.name || t("session.setupDefault")}</span>
                  {i === 0 && <Badge>{t("session.fastestSetup")}</Badge>}
                  {r.name === s.setup && <Badge>{t("session.thisSetup")}</Badge>}
                </span>
                <span className="text-muted-foreground">
                  {r.laps} {t("session.lapsShort")}
                </span>
                <span className="font-mono tabular-nums">{lapTime(r.best)}</span>
                <span className="font-mono tabular-nums text-muted-foreground">
                  {lapTime(r.avg3)}
                  {i > 0 && setups[0].avg3 && r.avg3 ? ` ${gap((r.avg3 - setups[0].avg3) / 1000)}` : ""}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}

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
            {ideal.sections.map((b, i) => {
              // The best is keyed by where the lap sits in this session, not by its number.
              const from = s.laps[b.lap];
              return (
                <div
                  key={b.name + i}
                  className="grid grid-cols-[1fr_100px_130px_110px] items-center gap-3 border-b border-border px-4 py-2 text-[12.5px] last:border-b-0"
                >
                  <span className="flex items-center gap-2">
                    {b.name}
                    {ideal.leastConsistent === i && <Badge>{t("session.leastConsistent")}</Badge>}
                  </span>
                  <span className="font-mono tabular-nums">{b.best.toFixed(3)}</span>
                  <span className="text-muted-foreground">
                    {t("session.lap")} {(from?.num ?? b.lap) + 1}
                    {multi && from && (
                      <span className="ml-1.5 text-faint">{t("session.stintTag", { n: from.stint + 1 })}</span>
                    )}
                  </span>
                  <span className="text-right text-muted-foreground">±{b.spread.toFixed(2)} s</span>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </Page>
  );
}

/** Every setup ridden on this track and bike: its whole laps, best, and the average of its best
 *  three, which one lucky lap can't carry. Fastest by that average first. */
function compareSetups(sessions: SessionSummary[]) {
  const times = new Map<string, number[]>();
  for (const x of sessions) {
    // Each stint of a session can be on its own setup, so every lap is counted under its own.
    const setupOf = new Map(x.stints.map((st) => [st.path, st.setup]));
    for (const l of x.laps) {
      if (!l.whole || l.invalid || l.timeMs <= 0) continue;
      const name = setupOf.get(l.path) ?? x.setup;
      times.set(name, [...(times.get(name) ?? []), l.timeMs]);
    }
  }
  return [...times.entries()]
    .filter(([, list]) => list.length > 0)
    .map(([name, list]) => {
      const top = [...list].sort((a, b) => a - b).slice(0, 3);
      return { name, laps: list.length, best: top[0], avg3: top.reduce((a, b) => a + b, 0) / top.length };
    })
    .sort((a, b) => a.avg3 - b.avg3);
}

function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="border border-border bg-card px-4 py-3">
      <div className="eyebrow">{label}</div>
      <div className="mt-1 font-mono text-[20px] tabular-nums">{value}</div>
      {note && <div className="mt-0.5 truncate text-[11.5px] text-muted-foreground">{note}</div>}
    </div>
  );
}
