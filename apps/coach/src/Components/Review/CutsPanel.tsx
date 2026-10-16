import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { Copy } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import { coachCuts, type CutSession } from "@/api/coach";
import { copyText } from "@/lib/clipboard";

/** `20261013-101500-123` as `13/10 10:15`. */
function when(stamp: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})/.exec(stamp);
  return m ? `${m[3]}/${m[2]} ${m[4]}:${m[5]}` : stamp;
}

/** The text a rider sends their track maker: the track and the markers they were cut at. */
export function cutsText(s: CutSession): string {
  const rows = s.byMarker.filter((m) => m.confirmed > 0 || m.possible > 0);
  const parts = rows.map((m) => {
    const bits = [];
    if (m.confirmed) bits.push(`${m.confirmed}x`);
    if (m.possible) bits.push(`${m.possible} possible`);
    return `marker ${m.marker}: ${bits.join(", ")}`;
  });
  return `Cuts on ${s.track} (${when(s.session)}): ${parts.length ? parts.join("; ") : "none at a known marker"}`;
}

/** The track's markers with the rider's cuts on them, north up. */
function CutMap({ s }: { s: CutSession }) {
  const box = useMemo(() => {
    const pts = s.markers.length ? s.markers : [];
    if (!pts.length) return null;
    let x0 = Infinity, x1 = -Infinity, z0 = Infinity, z1 = -Infinity;
    for (const p of pts) {
      x0 = Math.min(x0, p.x);
      x1 = Math.max(x1, p.x);
      z0 = Math.min(z0, p.z);
      z1 = Math.max(z1, p.z);
    }
    const pad = Math.max(x1 - x0, z1 - z0, 1) * 0.06;
    return { x0: x0 - pad, x1: x1 + pad, z0: z0 - pad, z1: z1 + pad };
  }, [s.markers]);
  if (!box) return null;
  const w = box.x1 - box.x0, h = box.z1 - box.z0;
  const r = Math.max(w, h) / 140;
  const count = new Map(s.byMarker.map((m) => [m.marker, m.confirmed + m.possible]));
  const every = s.markers.length > 60 ? 5 : 1;
  const px = (x: number) => x - box.x0;
  const pz = (z: number) => box.z1 - z;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-[320px] w-full rounded-md bg-foreground/[0.03]" preserveAspectRatio="xMidYMid meet">
      <polyline
        points={[...s.markers, s.markers[0]].map((m) => `${px(m.x)},${pz(m.z)}`).join(" ")}
        fill="none"
        className="stroke-muted-foreground/40"
        strokeWidth={r * 0.6}
      />
      {s.markers.map((m) => {
        const n = count.get(m.id) ?? 0;
        return (
          <g key={m.id}>
            <circle
              cx={px(m.x)}
              cy={pz(m.z)}
              r={n ? r * (1.6 + Math.min(n, 5) * 0.4) : r}
              className={n ? "fill-red-500" : "fill-muted-foreground"}
            />
            {(n > 0 || m.id % every === 0) && (
              <text
                x={px(m.x) + r * 1.8}
                y={pz(m.z) - r * 1.2}
                fontSize={r * (n ? 4 : 3)}
                className={n ? "fill-red-500 font-semibold" : "fill-muted-foreground"}
              >
                {m.id}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

/** The rider's track cuts, ride by ride: which marker each was at, on a map of the markers, and
 *  a line to send the track maker. */
export default function CutsPanel() {
  const t = useT();
  const [sessions, setSessions] = useState<CutSession[] | null>(null);
  const [pick, setPick] = useState(0);
  useEffect(() => {
    coachCuts()
      .then(setSessions)
      .catch(() => setSessions([]));
  }, []);
  if (!sessions) return <p className="text-[12.5px] text-muted-foreground">{t("common.loading")}</p>;
  if (!sessions.length) return <p className="text-[12.5px] text-muted-foreground">{t("cuts.empty")}</p>;
  const s = sessions[Math.min(pick, sessions.length - 1)];
  const unknown = s.cuts.filter((c) => c.own && c.marker < 0).length;
  return (
    <div className="flex flex-col gap-3 py-2">
      <div className="flex items-center justify-between gap-3">
        <Select value={String(pick)} onValueChange={(v) => setPick(Number(v))}>
          <SelectTrigger className="h-8 w-[300px] text-[12px]">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {sessions.map((x, i) => (
              <SelectItem key={`${x.session}-${x.track}`} value={String(i)}>
                {x.track} · {when(x.session)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          size="sm"
          variant="outline"
          onClick={() => void copyText(cutsText(s)).then((ok) => ok && toast.success(t("cuts.copied")))}
        >
          <Copy className="size-3.5" />
          {t("cuts.copy")}
        </Button>
      </div>
      <CutMap s={s} />
      <div className="divide-y divide-border/60">
        {s.byMarker.map((m) => (
          <div key={m.marker} className="flex items-center justify-between py-1.5 text-[12.5px]">
            <span className="font-semibold">{t("cuts.marker", { id: m.marker })}</span>
            <span className="text-muted-foreground">
              {m.confirmed > 0 && (m.confirmed === 1 ? t("cuts.countOne") : t("cuts.count", { n: m.confirmed }))}
              {m.confirmed > 0 && m.possible > 0 && " · "}
              {m.possible > 0 && t("cuts.possible", { n: m.possible })}
            </span>
          </div>
        ))}
        {unknown > 0 && (
          <div className="flex items-center justify-between py-1.5 text-[12.5px]">
            <span className="font-semibold">{t("cuts.unknown")}</span>
            <span className="text-muted-foreground">{unknown === 1 ? t("cuts.countOne") : t("cuts.count", { n: unknown })}</span>
          </div>
        )}
      </div>
    </div>
  );
}
