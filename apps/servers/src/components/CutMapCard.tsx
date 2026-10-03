import { useState } from "react";
import { serverCuts, serverRecentCuts, type ServerView } from "@/lib/api";
import { OUTCOME_LABEL } from "@/lib/cuts";
import { usePoll } from "@/lib/usePoll";
import { CutMap, CutMapLegend } from "./CutMap";
import { Card } from "./ui";

/** The Status tab's cut map: the current track's outline, its zones and the cuts so far. Shows
 *  nothing unless the server has cut detection on (or is too old to know about it). */
export function CutMapCard({ server }: { server: ServerView }) {
  const ready = server.kind === "native" && (!server.local || (server.adminPort != null && server.hasToken));
  const cuts = usePoll(() => (ready ? serverCuts(server.id) : Promise.resolve(null)), ready ? 15_000 : 0, `${server.id}-cuts`);
  const recent = usePoll(() => (ready ? serverRecentCuts(server.id) : Promise.resolve([])), ready ? 5_000 : 0, `${server.id}-cuts-recent`);
  const [chosen, setChosen] = useState<string | null>(null);

  const info = cuts.data;
  if (!ready || !info || !info.enabled || info.tracks.length === 0) return null;
  const trackName = chosen && info.tracks.some((t) => t.track === chosen) ? chosen : (info.current_track ?? info.tracks[0].track);
  const track = info.tracks.find((t) => t.track === trackName) ?? info.tracks[0];
  const here = (recent.data ?? []).filter((c) => c.track === track.track);

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h3 className="font-heading text-sm font-bold uppercase tracking-wide text-muted-foreground">Cut map</h3>
        <div className="flex items-center gap-3 text-xs text-muted-foreground">
          <span>
            {track.length_m.toFixed(0)} m · {track.source === "file" ? "from a limits file" : "auto outline"} · {here.length} {here.length === 1 ? "cut" : "cuts"}
          </span>
          {info.tracks.length > 1 && (
            <select
              className="h-8 rounded-md border border-input bg-background px-2 text-sm text-foreground"
              value={track.track}
              onChange={(e) => setChosen(e.target.value)}
              aria-label="Track"
            >
              {info.tracks.map((t) => (
                <option key={t.track} value={t.track}>
                  {t.track}
                  {t.track === info.current_track ? " (now)" : ""}
                </option>
              ))}
            </select>
          )}
        </div>
      </div>
      <CutMap track={track} zones={track.zones} cuts={here} />
      <CutMapLegend />
      {here.length > 0 && (
        <ul className="flex flex-col gap-1 text-sm">
          {here.slice(-5).reverse().map((c, i) => (
            <li key={i} className="truncate">
              <span className="font-mono text-xs text-muted-foreground">#{c.race}</span> {c.name}
              <span className="text-muted-foreground">
                {c.zone ? ` · ${c.zone}` : ""}
                {c.outcome ? ` · ${OUTCOME_LABEL[c.outcome] ?? c.outcome}` : ""}
                {c.outcome === "penalty" && c.penalty_seconds != null ? ` +${c.penalty_seconds} s` : ""}
                {c.skipped_m != null ? ` · skipped ${c.skipped_m.toFixed(0)} m` : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
