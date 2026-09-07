import { useMemo } from "react";
import {
  FEATURE_COLOUR,
  featureSpan,
  lapLength,
  pathAlong,
  positionAt,
  type TrackProgram,
} from "../../../api/trackgen";
import { useT } from "../../../i18n/context";
import { cn } from "@/lib/utils";

/**
 * The lap from above, drawn from the program itself.
 *
 * The 3D preview costs a build; this costs nothing and is always current, so it is what the
 * studio shows while a track is being shaped. It is the same walk the synthesiser does —
 * `positionAt` — so a corner here is the corner the terrain will be cut around.
 *
 * World z runs up the picture. The game's plan views are all north-up and a lap that flips
 * vertically between this and the 3D view is a lap nobody can match to the list.
 */
export default function LapPlan({
  program,
  scope,
  hover,
  onPick,
  className,
}: {
  program: TrackProgram;
  /** The stretch the list has selected: where it starts and how long it runs. */
  scope?: { at: number; length: number } | null;
  /** The stretch under the cursor, from the list or the height strip. */
  hover?: { at: number; length: number } | null;
  onPick?: (at: number) => void;
  className?: string;
}) {
  const t = useT();
  const lap = lapLength(program);

  const view = useMemo(() => {
    // Enough points that a 12 m berm is more than one segment, few enough that dragging a
    // slider doesn't cost a thousand of them.
    const pts = pathAlong(program, 0, lap);
    let loX = Infinity;
    let hiX = -Infinity;
    let loZ = Infinity;
    let hiZ = -Infinity;
    for (const p of pts) {
      loX = Math.min(loX, p.x);
      hiX = Math.max(hiX, p.x);
      loZ = Math.min(loZ, p.z);
      hiZ = Math.max(hiZ, p.z);
    }
    // Room for the corridor's own width — the line is the middle of a track, not its edge —
    // and a little more so a jump marker at the outside of a turn isn't clipped.
    const pad = program.width * 1.5 + 6;
    loX -= pad;
    hiX += pad;
    loZ -= pad;
    hiZ += pad;
    // The picture is drawn in metres and fitted by `preserveAspectRatio`, so one scale
    // serves both axes without the lap being squared off into a box it doesn't fill: an
    // oval 210 m by 90 m in a square frame is mostly frame.
    const w = Math.max(hiX - loX, 1);
    const h = Math.max(hiZ - loZ, 1);
    const to = (p: { x: number; z: number }) => ({
      // SVG y grows downward and world z grows away, so the picture is flipped once here
      // and nowhere else.
      x: p.x - loX,
      y: hiZ - p.z,
    });
    const d = (from: number, length: number) =>
      pathAlong(program, from, length)
        .map(to)
        .map((q, i) => `${i === 0 ? "M" : "L"}${q.x.toFixed(1)} ${q.y.toFixed(1)}`)
        .join(" ");
    return { to, d, w, h };
  }, [program, lap]);

  // Stroke widths are metres, because the picture is. The ribbon is the track's real width
  // rather than a thickness someone liked the look of.
  const corridor = Math.max(program.width, 1);
  const start = view.to(positionAt(program, 0));
  const nudge = view.to(positionAt(program, Math.min(6, lap)));
  // The gate line stands across the track, so it is drawn along the normal to the heading.
  const dx = nudge.x - start.x;
  const dy = nudge.y - start.y;
  const mag = Math.hypot(dx, dy) || 1;
  const nx = (-dy / mag) * corridor * 0.9;
  const ny = (dx / mag) * corridor * 0.9;
  // A grid in round metres, so the squares mean something: 10 m on a small plot, 50 on a
  // big one, and never so fine that it reads as a texture.
  const grid = [10, 20, 25, 50, 100].find((g) => Math.max(view.w, view.h) / g < 26) ?? 200;
  // The one piece of type on the picture, sized against the picture rather than the track:
  // at a corridor's width it read as a banner laid across the start straight.
  const label = Math.max(view.w, view.h) / 42;

  return (
    <svg
      viewBox={`0 0 ${view.w.toFixed(1)} ${view.h.toFixed(1)}`}
      preserveAspectRatio="xMidYMid meet"
      className={cn("h-full w-full", onPick && "cursor-default", className)}
    >
      <defs>
        <pattern id="lapgrid" width={grid} height={grid} patternUnits="userSpaceOnUse">
          <path
            d={`M${grid} 0H0V${grid}`}
            stroke="currentColor"
            strokeWidth={Math.max(view.w, view.h) / 900}
            className="text-foreground/[0.07]"
            fill="none"
          />
        </pattern>
      </defs>
      <rect width={view.w} height={view.h} fill="url(#lapgrid)" />

      {/* The riding surface, then the line down the middle of it. Two passes, because a
          dashed centreline over a solid ribbon is how every track map is drawn. */}
      <path
        d={view.d(0, lap)}
        className="stroke-primary/[0.16]"
        strokeWidth={corridor}
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
      <path
        d={view.d(0, lap)}
        className="stroke-primary/50"
        strokeWidth={corridor / 9}
        strokeDasharray={`${corridor / 3} ${corridor / 3}`}
        fill="none"
      />

      {/* Every jump, in the colour its row carries, so a lump on the map and a line in the
          list are obviously the same thing. */}
      {program.features.map((f, i) => {
        const span = featureSpan(f);
        return (
          <path
            key={i}
            d={view.d(span.at, span.length)}
            stroke={FEATURE_COLOUR[f.kind]}
            strokeWidth={corridor * 0.85}
            strokeLinecap="butt"
            fill="none"
            opacity={0.95}
          />
        );
      })}

      {hover && (
        <path
          d={view.d(hover.at, hover.length)}
          className="stroke-foreground/40"
          strokeWidth={corridor * 1.05}
          strokeLinecap="round"
          fill="none"
        />
      )}
      {scope && (
        <path
          d={view.d(scope.at, scope.length)}
          className="stroke-primary"
          strokeWidth={corridor}
          strokeLinecap="round"
          fill="none"
          opacity={0.9}
        />
      )}

      {/* Where the lap begins. Drawn last so nothing paints over it. */}
      <path
        d={`M${(start.x + nx).toFixed(1)} ${(start.y + ny).toFixed(1)} L${(start.x - nx).toFixed(1)} ${(start.y - ny).toFixed(1)}`}
        className="stroke-foreground"
        strokeWidth={corridor / 4}
        strokeLinecap="round"
      />
      <text
        x={start.x + corridor * 0.9}
        y={start.y + label * 0.35}
        fontSize={label}
        className="fill-foreground font-cond tracking-[0.18em]"
      >
        {t("track.start")}
      </text>

      {/* One transparent hit area over the lap: clicking the map picks the step you clicked
          on, which is the only way to select something you can see but can't name. */}
      {onPick && (
        <path
          d={view.d(0, lap)}
          stroke="transparent"
          strokeWidth={corridor * 2}
          fill="none"
          onClick={(e) => {
            const svg = e.currentTarget.ownerSVGElement;
            if (!svg) return;
            const box = svg.getBoundingClientRect();
            // `meet` fits the whole viewBox at one scale and centres what is left over.
            const k = Math.min(box.width / view.w, box.height / view.h);
            const px = (e.clientX - box.left - (box.width - view.w * k) / 2) / k;
            const py = (e.clientY - box.top - (box.height - view.h * k) / 2) / k;
            // Nearest point on the lap, walked at the resolution the picture is drawn at.
            let best = 0;
            let bestD = Infinity;
            const n = 400;
            for (let i = 0; i < n; i++) {
              const s = (lap * i) / n;
              const q = view.to(positionAt(program, s));
              const d = (q.x - px) ** 2 + (q.y - py) ** 2;
              if (d < bestD) {
                bestD = d;
                best = s;
              }
            }
            onPick(best);
          }}
        />
      )}
    </svg>
  );
}
