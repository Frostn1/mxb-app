import { useMemo } from "react";
import { useT } from "@/i18n";
import type { Channels } from "@/api/coach";
import { Label } from "../Page";

/**
 * What the lap did to the suspension: the share of each end's travel it used on average, and
 * how deep the hardest hit went.
 *
 * Numbers only. It used to draw them on a picture of a bike, which was a picture of *a* bike
 * and not the rider's; the real one is rendered above with the lap's own travel on it, so the
 * picture was standing in for something now on the same screen.
 */
export default function BikeSuspension({ channels }: { channels: Channels }) {
  const t = useT();
  const stats = useMemo(() => {
    const of = (v: number[]) => {
      const real = v.filter((x) => Number.isFinite(x));
      if (real.length === 0) return null;
      const mean = real.reduce((a, b) => a + b, 0) / real.length / 100;
      const deep = Math.max(...real) / 100;
      return { mean, deep };
    };
    return { fork: of(channels.fork.lap), shock: of(channels.shock.lap) };
  }, [channels]);

  // All zero is how the recorder says it couldn't work the travel out, rather than a lap that
  // never moved the suspension.
  const known = stats.fork && stats.shock && (stats.fork.deep > 0 || stats.shock.deep > 0);
  if (!known) return null;
  const pct = (v: number) => `${Math.round(v * 100)}%`;
  return (
    <div>
      <Label>{t("susp.title")}</Label>
      <div className="border border-border bg-card px-4 py-3">
        <p className="text-[12.5px] text-muted-foreground">{t("susp.body")}</p>
        <dl className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-[12.5px]">
          <dt className="text-muted-foreground">{t("susp.front")}</dt>
          <dd className="font-mono">{t("susp.reading", { used: pct(stats.fork!.mean), deep: pct(stats.fork!.deep) })}</dd>
          <dt className="text-muted-foreground">{t("susp.rear")}</dt>
          <dd className="font-mono">{t("susp.reading", { used: pct(stats.shock!.mean), deep: pct(stats.shock!.deep) })}</dd>
        </dl>
      </div>
    </div>
  );
}
