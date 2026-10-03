import { useMemo, useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { serverCuts, serverRecentCuts, type ServerView } from "@/lib/api";
import { projectToLine, zoneWhere, type CutZone } from "@/lib/cuts";
import { usePoll } from "@/lib/usePoll";
import { CutMap, CutMapLegend } from "./CutMap";
import { Button, Input, Toggle } from "./ui";

type Pick = { index: number; kind: "range" | "area" };

const round1 = (n: number) => Math.round(n * 10) / 10;

/** Named cut zones, per track: where (pick on the map or type the distances), how many seconds
 *  (0 = warning only) and on/off (off: cuts there are allowed). The list is the file's
 *  `[[cuts.zones]]`; saving goes through the same review as every other setting. */
export function CutZones({ server, zones, onChange }: { server: ServerView; zones: CutZone[]; onChange: (zones: CutZone[]) => void }) {
  const adminReady = !server.local || (server.adminPort != null && server.hasToken);
  const cuts = usePoll(() => (adminReady ? serverCuts(server.id) : Promise.resolve(null)), 0, `${server.id}-cuts-editor`);
  const recent = usePoll(() => (adminReady ? serverRecentCuts(server.id) : Promise.resolve([])), 0, `${server.id}-cuts-editor-recent`);
  const [chosen, setChosen] = useState<string | null>(null);
  const [pick, setPick] = useState<Pick | null>(null);
  const [first, setFirst] = useState<number | null>(null);
  const [draft, setDraft] = useState<[number, number][]>([]);

  const names = useMemo(() => {
    const out = new Set<string>();
    for (const t of cuts.data?.tracks ?? []) out.add(t.track);
    for (const z of zones) out.add(z.track);
    return [...out];
  }, [cuts.data, zones]);
  const trackName = chosen ?? cuts.data?.current_track ?? names[0] ?? "";
  const track = cuts.data?.tracks.find((t) => t.track === trackName) ?? null;
  const rows = zones.map((zone, index) => ({ zone, index })).filter((r) => r.zone.track === trackName);

  const update = (index: number, patch: Partial<CutZone>) => onChange(zones.map((z, i) => (i === index ? { ...z, ...patch } : z)));
  const stop = () => {
    setPick(null);
    setFirst(null);
    setDraft([]);
  };

  const onPoint = (x: number, z: number) => {
    if (!pick || !track) return;
    if (pick.kind === "range") {
      const at = projectToLine(track.points, x, z);
      if (!at) return;
      if (first == null) {
        setFirst(at.s);
        return;
      }
      update(pick.index, { from_m: round1(Math.min(first, at.s)), to_m: round1(Math.max(first, at.s)) });
      stop();
    } else {
      setDraft((d) => [...d, [round1(x), round1(z)]]);
    }
  };

  const add = () => {
    if (!trackName.trim()) return;
    stop();
    onChange([...zones, { track: trackName.trim(), name: "New zone", from_m: null, to_m: null, area: null, seconds: 10, enable: true }]);
  };

  const live = pick?.kind === "range" && first != null ? ([first, first] as [number, number]) : null;
  const picked = pick && pick.index < zones.length ? pick.index : null;

  return (
    <div className="mt-6 flex flex-col gap-4 border-t pt-5" data-testid="cut-zones">
      <div className="flex flex-col gap-0.5">
        <h4 className="font-heading text-base font-extrabold tracking-tight">Cut zones</h4>
        <p className="text-sm text-muted-foreground">
          Name a stretch of a track and say what a cut there costs. A cut that no zone covers uses the penalty for a cut. A switched-off zone means cuts there are allowed.
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-sm">
          <span className="font-medium">Track</span>
          <Input
            list="cut-track-names"
            className="w-64"
            value={trackName}
            onChange={(e) => {
              stop();
              setChosen(e.target.value);
            }}
            placeholder="Track name"
          />
          <datalist id="cut-track-names">
            {names.map((n) => (
              <option key={n} value={n} />
            ))}
          </datalist>
        </label>
        <Button size="sm" onClick={add} disabled={!trackName.trim()}>
          <Plus className="size-4" /> Add zone
        </Button>
      </div>

      {cuts.data === null && cuts.at != null && (
        <p className="text-sm text-muted-foreground">
          This server can&apos;t send track outlines (an older version, or cut detection is off), so there is no map to pick on. Type the distances instead.
        </p>
      )}
      {cuts.error && <p className="text-sm text-muted-foreground">No map: {cuts.error}</p>}
      {cuts.data && cuts.data.tracks.length === 0 && (
        <p className="text-sm text-muted-foreground">The server has no outline for any track yet. Turn on cut detection and ride the track once; type the distances meanwhile.</p>
      )}

      {track && (
        <div className="flex flex-col gap-2">
          <CutMap track={track} zones={rows.map((r) => r.zone)} cuts={(recent.data ?? []).filter((c) => c.track === trackName)} selected={picked == null ? null : rows.findIndex((r) => r.index === picked)} draft={pick?.kind === "range" ? [] : draft} draftRange={live} onPoint={pick ? onPoint : undefined} height={300} />
          <CutMapLegend />
          {pick && (
            <div className="flex flex-wrap items-center gap-2 text-sm">
              <span className="font-medium text-primary">
                {pick.kind === "range" ? (first == null ? "Click the start of the stretch on the track." : "Now click the end of the stretch.") : `Click corners around the area (${draft.length} so far).`}
              </span>
              {pick.kind === "area" && (
                <>
                  <Button size="sm" variant="primary" disabled={draft.length < 3} onClick={() => { update(pick.index, { area: draft }); stop(); }}>
                    Finish area
                  </Button>
                  <Button size="sm" disabled={draft.length === 0} onClick={() => setDraft((d) => d.slice(0, -1))}>
                    Undo point
                  </Button>
                </>
              )}
              <Button size="sm" variant="ghost" onClick={stop}>
                Cancel
              </Button>
            </div>
          )}
        </div>
      )}

      {rows.length === 0 && <p className="text-sm text-muted-foreground">{trackName ? "No zones on this track." : "Choose a track to see its zones."}</p>}
      <ul className="flex flex-col divide-y rounded-lg border bg-card">
        {rows.map(({ zone, index }) => (
          <ZoneRow
            key={index}
            zone={zone}
            canPick={!!track}
            picking={pick?.index === index ? pick.kind : null}
            onPick={(kind) => {
              stop();
              setPick({ index, kind });
            }}
            onChange={(patch) => update(index, patch)}
            onDelete={() => {
              stop();
              onChange(zones.filter((_, i) => i !== index));
            }}
          />
        ))}
      </ul>
    </div>
  );
}

function ZoneRow({
  zone,
  canPick,
  picking,
  onPick,
  onChange,
  onDelete,
}: {
  zone: CutZone;
  canPick: boolean;
  picking: "range" | "area" | null;
  onPick: (kind: "range" | "area") => void;
  onChange: (patch: Partial<CutZone>) => void;
  onDelete: () => void;
}) {
  const number = (text: string) => (text === "" ? null : Number(text));
  const missing = zone.from_m == null && !zone.area;
  return (
    <li className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-3">
        <Input className="min-w-48 flex-1" value={zone.name} onChange={(e) => onChange({ name: e.target.value })} aria-label="Zone name" placeholder="Zone name" />
        <label className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground">Penalty</span>
          <Input
            type="number"
            min={0}
            max={3600}
            step={1}
            className="w-20 text-right font-mono tabular-nums"
            value={zone.seconds}
            onChange={(e) => onChange({ seconds: Math.max(0, Math.round(Number(e.target.value) || 0)) })}
            aria-label="Penalty seconds (0 is a warning only)"
          />
          <span className="text-muted-foreground">s</span>
        </label>
        <span className="text-xs text-muted-foreground">{zone.seconds === 0 ? "warning only" : ""}</span>
        <div className="ml-auto flex items-center gap-3">
          <span className="text-sm text-muted-foreground">{zone.enable ? "On" : "Cuts allowed"}</span>
          <Toggle label={`${zone.name} on`} checked={zone.enable} onChange={(on) => onChange({ enable: on })} />
          <Button size="sm" variant="danger" onClick={onDelete} aria-label={`Delete ${zone.name}`}>
            <Trash2 className="size-4" />
          </Button>
        </div>
      </div>
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <span className="text-muted-foreground">From</span>
        <Input type="number" min={0} step={1} className="w-24 text-right font-mono tabular-nums" value={zone.from_m ?? ""} onChange={(e) => onChange({ from_m: number(e.target.value) })} aria-label="From metres" />
        <span className="text-muted-foreground">to</span>
        <Input type="number" min={0} step={1} className="w-24 text-right font-mono tabular-nums" value={zone.to_m ?? ""} onChange={(e) => onChange({ to_m: number(e.target.value) })} aria-label="To metres" />
        <span className="text-muted-foreground">m</span>
        <Button size="sm" disabled={!canPick} variant={picking === "range" ? "primary" : "secondary"} onClick={() => onPick("range")}>
          Pick range on map
        </Button>
        <Button size="sm" disabled={!canPick} variant={picking === "area" ? "primary" : "secondary"} onClick={() => onPick("area")}>
          {zone.area ? "Redraw area" : "Draw area"}
        </Button>
        {zone.area && (
          <Button size="sm" variant="ghost" onClick={() => onChange({ area: null })}>
            Remove area
          </Button>
        )}
        <span className={`text-xs ${missing ? "text-destructive" : "text-muted-foreground"}`}>{zoneWhere(zone)}</span>
      </div>
    </li>
  );
}
