import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { FolderOpen, Upload } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import {
  coachImportLaps,
  coachImports,
  coachSessions,
  type LapRef,
  type LapSummary,
  type SessionSummary,
} from "@/api/coach";
import { lapTime, started } from "@/lib/format";
import { ALONE, BEST, IDEAL, lapRef, type Reference } from "@/lib/reference";

/** How far two centrelines can differ and still be the same build of a track; `coach.rs`
 *  refuses the comparison past this, so the picker says so before it's picked. */
const LENGTH_SLACK_M = 1;

const comparable = (l: LapSummary) => l.whole && !l.invalid;

/** A session is every stint of one event, so a lap's date is its own stint's. */
const stintStart = (s: SessionSummary, file: string) => s.stints.find((x) => x.path === file)?.started ?? s.started;

/** The fastest lap on this track, the same bike winning over a faster one on another — what
 *  "your best ever here" resolves to, the same way `best_reference` does in `coach.rs`. */
function bestEver(sessions: SessionSummary[], bikeId: string, not: { path: string; lap: number }) {
  return sessions
    .flatMap((s) => s.laps.filter(comparable).map((l) => ({ s, l })))
    .filter(({ l }) => !(l.path === not.path && l.num === not.lap))
    .sort((a, b) => Number(a.s.bikeId !== bikeId) - Number(b.s.bikeId !== bikeId) || a.l.timeMs - b.l.timeMs)[0];
}

/**
 * What this lap is held against, and where another rider's lap is brought in.
 *
 * Its own component rather than part of the review page: the same choice drives the review, the
 * live cues and the in-game HUD, and it is the one thing on the page that reaches outside this
 * session — every lap on the track, the ideal lap, and laps that aren't the rider's at all.
 */
export default function RefPicker({
  trackId,
  path,
  lap,
  bikeId,
  value,
  onChange,
}: {
  trackId: string;
  /** The lap being reviewed: the picker never offers it against itself. */
  path: string;
  lap: number;
  /** The bike this lap was ridden on, so a reference on another one says which. */
  bikeId: string;
  value: Reference;
  onChange: (value: Reference) => void;
}) {
  const t = useT();
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [imports, setImports] = useState<SessionSummary[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    coachSessions().then(setSessions).catch(() => {});
    coachImports().then(setImports).catch(() => {});
  }, []);
  useEffect(() => load(), [load]);

  // This track's length as this lap's own recording measured it. Another build of the track
  // has a different centreline, and two laps on different centrelines can't be lined up.
  const length = sessions.find((s) => s.stints.some((x) => x.path === path))?.trackLength ?? 0;

  const bring = async (folder: boolean) => {
    const picked = await open(
      folder
        ? { directory: true, multiple: false }
        : { multiple: true, filters: [{ name: "Coach recording", extensions: ["mxbc"] }] },
    );
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (paths.length === 0) return;
    setBusy(true);
    try {
      const out = await coachImportLaps(paths);
      if (out.added > 0) toast.success(out.added === 1 ? t("imports.addedOne") : t("imports.addedMany", { n: out.added }));
      // Each recording says for itself why it couldn't come in; a long pile of them doesn't help.
      for (const s of out.skipped.slice(0, 3)) toast.error(`${s.file} — ${s.why}`);
      if (out.added === 0 && out.skipped.length === 0) toast.error(t("imports.none"));
      load();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const choices = useMemo(() => {
    const mine = sessions.filter((s) => s.trackId === trackId);
    const best = bestEver(mine, bikeId, { path, lap });
    const out: { value: Reference; label: string }[] = [
      {
        value: BEST,
        label: best
          ? [
              t("review.bestEver"),
              lapTime(best.l.timeMs),
              started(stintStart(best.s, best.l.path)),
              best.s.bikeId !== bikeId ? best.s.bikeName : "",
            ]
              .filter(Boolean)
              .join(" · ")
          : t("review.bestEver"),
      },
      { value: IDEAL, label: t("review.idealLap") },
      { value: ALONE, label: t("review.alone") },
    ];
    for (const s of imports.filter((x) => x.trackId === trackId)) {
      const version = length > 0 && Math.abs(s.trackLength - length) > LENGTH_SLACK_M ? t("review.otherVersion") : "";
      for (const l of s.laps.filter(comparable)) {
        out.push({
          value: lapRef(l.path, l.num),
          label: [t("review.imported"), s.rider || t("review.someone"), lapTime(l.timeMs), s.bikeName, version]
            .filter(Boolean)
            .join(" · "),
        });
      }
    }
    // This session's own laps first, then every other lap of theirs on the track.
    const here = (s: SessionSummary) => s.stints.some((x) => x.path === path);
    for (const s of [...mine].sort((a, b) => Number(here(b)) - Number(here(a)))) {
      for (const l of s.laps.filter(comparable)) {
        if (l.path === path && l.num === lap) continue;
        out.push({
          value: lapRef(l.path, l.num),
          label: [
            `${t("session.lap")} ${l.num + 1}`,
            lapTime(l.timeMs),
            l.path === path ? "" : started(stintStart(s, l.path)),
            s.bikeId !== bikeId ? s.bikeName : "",
          ]
            .filter(Boolean)
            .join(" · "),
        });
      }
    }
    return out;
  }, [sessions, imports, trackId, path, lap, bikeId, length, t]);

  // A remembered lap can be gone: an import removed, a recording deleted. Fall back rather
  // than asking the backend for a file that isn't there any more.
  useEffect(() => {
    if (sessions.length > 0 && !choices.some((c) => c.value === value)) onChange(BEST);
  }, [choices, sessions.length, value, onChange]);

  return (
    <div className="flex items-center gap-2">
      <span className="text-[11.5px] text-muted-foreground">{t("review.compareWith")}</span>
      <Select value={value} onValueChange={onChange}>
        <SelectTrigger className="h-8 w-[300px] text-[12px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {choices.map((c) => (
            <SelectItem key={c.value} value={c.value}>
              {c.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <Button size="sm" variant="outline" disabled={busy} onClick={() => void bring(false)} title={t("imports.hint")}>
        <Upload className="size-3.5" />
        {t("imports.add")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        disabled={busy}
        onClick={() => void bring(true)}
        aria-label={t("imports.folder")}
        title={t("imports.folder")}
      >
        <FolderOpen className="size-3.5" />
      </Button>
    </div>
  );
}

/** The reference in a line: whose lap it is, when it was, and which bike — because a lap of
 *  somebody else's, or one on another bike, doesn't mean what one of your own means. */
export function ReferenceLine({
  reference: r,
  lapPath,
  lapBikeId,
  idealFrom,
}: {
  reference: LapRef;
  /** The recording the reviewed lap is in: a reference from the same one needs no date. */
  lapPath?: string;
  /** The bike the reviewed lap was on, when the difference is worth saying. */
  lapBikeId?: string;
  /** How many laps the ideal was stitched from. */
  idealFrom?: number | null;
}) {
  const t = useT();
  if (r.kind === "ideal") {
    const from = idealFrom ? t("review.idealFrom", { n: idealFrom }) : "";
    return <span>{[t("review.idealLap"), lapTime(r.timeMs), from].filter(Boolean).join(" · ")}</span>;
  }
  const who =
    r.kind === "imported"
      ? [t("review.imported"), r.rider || t("review.someone")]
      : [`${t("session.lap")} ${r.lap + 1}`];
  const when = r.started && r.path !== lapPath ? started(r.started) : "";
  const bike = lapBikeId && r.bikeId && r.bikeId !== lapBikeId ? t("review.otherBike", { bike: r.bikeName }) : "";
  return <span>{[...who, lapTime(r.timeMs), when, bike].filter(Boolean).join(" · ")}</span>;
}
