import { useMemo, useRef } from "react";
import { bounds, edges, OUTCOME_LABEL, pointAt, segment, svgPoints, type CutTrack, type CutZone, type RecentCut } from "@/lib/cuts";

const ZONE_COLOR = "#d97706";

/** What a zone looks like: amber when it penalises, blue for warning only, grey and dashed when
 *  cuts there are allowed. */
const zoneColor = (z: CutZone) => (!z.enable ? "var(--muted-foreground)" : z.seconds === 0 ? "var(--primary)" : ZONE_COLOR);

/** The track from above (world x right, world z up): the corridor from the centre line and half
 *  widths, named zones, and where cuts happened. With `onPoint` a click gives the world point. */
export function CutMap({
  track,
  zones,
  cuts = [],
  selected = null,
  draft = [],
  draftClosed = false,
  draftRange = null,
  onPoint,
  height = 320,
}: {
  track: CutTrack;
  zones: CutZone[];
  cuts?: RecentCut[];
  /** Index into `zones` drawn heavier. */
  selected?: number | null;
  /** Points clicked so far while picking. */
  draft?: [number, number][];
  draftClosed?: boolean;
  draftRange?: [number, number] | null;
  onPoint?: (x: number, z: number) => void;
  height?: number;
}) {
  const svg = useRef<SVGSVGElement>(null);
  const { left, right, view, unit } = useMemo(() => {
    const e = edges(track.points);
    const extra: [number, number][] = cuts.flatMap((c) => [c.exit, c.rejoin].filter((p): p is [number, number] => !!p));
    const b = bounds(track, [...extra, ...zones.flatMap((z) => z.area ?? [])]);
    const pad = Math.max(b.maxX - b.minX, b.maxZ - b.minZ) * 0.04 + 4;
    const w = b.maxX - b.minX + pad * 2;
    const h = b.maxZ - b.minZ + pad * 2;
    // A world-metre length that is a few pixels on screen, for dots.
    return { ...e, view: `${b.minX - pad} ${-b.maxZ - pad} ${w} ${h}`, unit: Math.max(w, h) / 160 };
  }, [track, cuts, zones]);

  if (track.points.length < 2) return <p className="text-sm text-muted-foreground">No outline for this track yet.</p>;

  const click = (e: React.MouseEvent<SVGSVGElement>) => {
    if (!onPoint || !svg.current) return;
    const ctm = svg.current.getScreenCTM();
    if (!ctm) return;
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    onPoint(p.x, -p.y);
  };

  const cutDot = (c: RecentCut): [number, number] | null => c.exit ?? pointAt(track.points, c.route_metres);

  return (
    <svg
      ref={svg}
      viewBox={view}
      role="img"
      aria-label={`${track.track} track map`}
      style={{ height, cursor: onPoint ? "crosshair" : undefined }}
      className="w-full rounded-lg border bg-muted"
      onClick={click}
    >
      <polygon points={svgPoints([...left, ...[...right].reverse()])} fill="var(--card)" stroke="none" />
      <polyline points={svgPoints(left)} fill="none" stroke="var(--muted-foreground)" strokeWidth={1.2} vectorEffect="non-scaling-stroke" />
      <polyline points={svgPoints(right)} fill="none" stroke="var(--muted-foreground)" strokeWidth={1.2} vectorEffect="non-scaling-stroke" />
      <polyline points={svgPoints(track.points.map((p) => [p[0], p[1]]))} fill="none" stroke="var(--border)" strokeWidth={1} strokeDasharray="4 4" vectorEffect="non-scaling-stroke" />

      {zones.map((z, i) => {
        const color = zoneColor(z);
        const heavy = selected === i;
        const dash = z.enable ? undefined : "5 4";
        return (
          <g key={i} opacity={heavy || selected == null ? 1 : 0.6}>
            {z.area && (
              <polygon points={svgPoints(z.area)} fill={color} fillOpacity={0.22} stroke={color} strokeWidth={heavy ? 3 : 1.6} strokeDasharray={dash} vectorEffect="non-scaling-stroke">
                <title>{z.name}</title>
              </polygon>
            )}
            {z.from_m != null && z.to_m != null && (
              <polyline points={svgPoints(segment(track.points, z.from_m, z.to_m))} fill="none" stroke={color} strokeOpacity={0.85} strokeWidth={heavy ? 9 : 6} strokeLinecap="round" strokeDasharray={dash} vectorEffect="non-scaling-stroke">
                <title>{z.name}</title>
              </polyline>
            )}
          </g>
        );
      })}

      {draftRange && (
        <polyline points={svgPoints(segment(track.points, draftRange[0], draftRange[1]))} fill="none" stroke="var(--primary)" strokeWidth={8} strokeLinecap="round" strokeOpacity={0.7} vectorEffect="non-scaling-stroke" />
      )}
      {draft.length > 1 &&
        (draftClosed ? (
          <polygon points={svgPoints(draft)} fill="var(--primary)" fillOpacity={0.15} stroke="var(--primary)" strokeWidth={2} vectorEffect="non-scaling-stroke" />
        ) : (
          <polyline points={svgPoints(draft)} fill="none" stroke="var(--primary)" strokeWidth={2} strokeDasharray="4 3" vectorEffect="non-scaling-stroke" />
        ))}
      {draft.map(([x, z], i) => (
        <circle key={i} cx={x} cy={-z} r={unit * 1.1} fill="var(--primary)" stroke="var(--card)" strokeWidth={1} vectorEffect="non-scaling-stroke" />
      ))}

      {cuts.map((c, i) => {
        const dot = cutDot(c);
        if (!dot) return null;
        return (
          <g key={i}>
            {c.path && c.path.length > 1 && (
              <polyline points={svgPoints(c.path)} fill="none" stroke="var(--destructive)" strokeOpacity={0.5} strokeWidth={1.4} vectorEffect="non-scaling-stroke" />
            )}
            <circle cx={dot[0]} cy={-dot[1]} r={unit * 1.3} fill="var(--destructive)" stroke="var(--card)" strokeWidth={1} vectorEffect="non-scaling-stroke">
              <title>
                {`#${c.race} ${c.name}${c.zone ? ` · ${c.zone}` : ""}${c.outcome ? ` · ${OUTCOME_LABEL[c.outcome] ?? c.outcome}` : ""}${c.skipped_m != null ? ` · skipped ${c.skipped_m.toFixed(0)} m` : ""}`}
              </title>
            </circle>
          </g>
        );
      })}
    </svg>
  );
}

/** What the colours on the map mean. */
export function CutMapLegend() {
  const item = (color: string, label: string, dashed = false) => (
    <span className="inline-flex items-center gap-1.5">
      <span className="h-1.5 w-4 rounded-full" style={{ background: dashed ? "transparent" : color, border: dashed ? `1.5px dashed ${color}` : undefined, height: dashed ? 0 : undefined }} aria-hidden />
      {label}
    </span>
  );
  return (
    <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
      {item(ZONE_COLOR, "Zone with a penalty")}
      {item("var(--primary)", "Warning only")}
      {item("var(--muted-foreground)", "Disabled: cuts allowed", true)}
      <span className="inline-flex items-center gap-1.5">
        <span className="size-2 rounded-full bg-destructive" aria-hidden />
        Cut
      </span>
    </div>
  );
}
