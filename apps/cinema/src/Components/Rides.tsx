import { useEffect, useMemo, useState } from "react";
import { Activity, Film, Loader2, Plane, RefreshCw, Search } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { getLocale } from "@frost/shared/i18n/core";
import { cn } from "@frost/shared/lib/utils";
import { listSessions, type SessionSummary, type Source } from "@/api/cinema";
import { useT } from "@/i18n";
import { duration, when } from "@/lib/ride";

/**
 * Pick a ride. MXB Coach's recordings today: every one the recorder wrote, newest first, with
 * what makes a shot worth looking for — how many jumps, the longest airtime, the top speed.
 * Game replays are the other source; until their format reads, the tab says so.
 */
export default function Rides({ onPick }: { onPick: (s: SessionSummary) => void }) {
  const t = useT();
  const [source, setSource] = useState<Source>("coach");
  const [rides, setRides] = useState<SessionSummary[] | null>(null);
  const [failed, setFailed] = useState(false);
  const [q, setQ] = useState("");
  const [jumpsOnly, setJumpsOnly] = useState(false);

  const load = () => {
    setFailed(false);
    listSessions()
      .then(setRides)
      .catch(() => setFailed(true));
  };
  useEffect(load, []);

  const shown = useMemo(() => {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    return (rides ?? []).filter(
      (r) =>
        (!jumpsOnly || r.jumps > 0) &&
        words.every((w) => `${r.trackName} ${r.trackId} ${r.bikeName} ${r.bikeId} ${r.rider}`.toLowerCase().includes(w)),
    );
  }, [rides, q, jumpsOnly]);

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-[1100px] px-8 py-8">
        <div className="flex items-end justify-between gap-4">
          <div>
            <h2 className="headline text-[24px]">{t("rides.title")}</h2>
            <div className="mt-1 text-[12.5px] text-muted-foreground">{t("rides.sub")}</div>
          </div>
          <Segmented
            value={source}
            onChange={(v) => setSource(v as Source)}
            options={[
              { value: "coach", label: t("rides.sourceCoach") },
              { value: "replay", label: t("rides.sourceReplay") },
            ]}
          />
        </div>

        {source === "replay" ? (
          <div className="mt-8 rounded-xl bg-card p-8 text-center">
            <Film className="mx-auto size-7 text-muted-foreground" />
            <div className="mt-3 text-[14px] font-semibold">{t("rides.replaySoon")}</div>
            <div className="mx-auto mt-1 max-w-md text-[12.5px] text-muted-foreground">{t("rides.replaySoonBody")}</div>
          </div>
        ) : (
          <>
            <div className="mt-6 flex items-center gap-2">
              <div className="flex h-8 flex-1 items-center gap-2 rounded-lg border border-input bg-card px-2.5">
                <Search className="size-3.5 text-muted-foreground" />
                <input
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                  placeholder={t("rides.search")}
                  className="h-full flex-1 bg-transparent text-[12.5px] outline-none placeholder:text-faint"
                />
              </div>
              <Button size="sm" variant={jumpsOnly ? "default" : "outline"} onClick={() => setJumpsOnly((v) => !v)}>
                <Plane className="size-3.5" />
                {t("rides.withJumps")}
              </Button>
              <Button size="sm" variant="ghost" onClick={load} aria-label={t("rides.refresh")}>
                <RefreshCw className="size-3.5" />
              </Button>
            </div>

            {rides == null && !failed && (
              <div className="mt-10 flex items-center justify-center gap-2 text-[12.5px] text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                {t("rides.loading")}
              </div>
            )}
            {failed && <p className="mt-8 text-[13px] text-[#ff6961]">{t("rides.failed")}</p>}
            {rides != null && rides.length === 0 && (
              <div className="mt-8 rounded-xl bg-card p-8 text-center">
                <Activity className="mx-auto size-7 text-muted-foreground" />
                <div className="mt-3 text-[14px] font-semibold">{t("rides.none")}</div>
                <div className="mx-auto mt-1 max-w-md text-[12.5px] text-muted-foreground">{t("rides.noneBody")}</div>
              </div>
            )}
            {rides != null && rides.length > 0 && (
              <div className="mt-4 overflow-hidden rounded-xl bg-card">
                <div className="grid grid-cols-[minmax(0,2.2fr)_minmax(0,1.6fr)_150px_70px_60px_80px_70px] gap-3 border-b border-border px-4 py-2 text-[10.5px] font-semibold uppercase tracking-wide text-faint">
                  <span>{t("rides.colTrack")}</span>
                  <span>{t("rides.colBike")}</span>
                  <span>{t("rides.colWhen")}</span>
                  <span className="text-right">{t("rides.colLength")}</span>
                  <span className="text-right">{t("rides.colJumps")}</span>
                  <span className="text-right">{t("rides.colAir")}</span>
                  <span className="text-right">{t("rides.colTop")}</span>
                </div>
                {shown.map((r, i) => (
                  <button
                    key={r.path}
                    onClick={() => onPick(r)}
                    className={cn(
                      "grid w-full grid-cols-[minmax(0,2.2fr)_minmax(0,1.6fr)_150px_70px_60px_80px_70px] items-center gap-3 px-4 py-2.5 text-left text-[12.5px] hover:bg-foreground/[0.05]",
                      i > 0 && "border-t border-border/60",
                    )}
                  >
                    <span className="truncate font-medium">{r.trackName}</span>
                    <span className="truncate text-muted-foreground">{r.bikeName}</span>
                    <span className="truncate text-muted-foreground">{when(r.started, getLocale())}</span>
                    <span className="text-right font-mono tabular-nums text-muted-foreground">{duration(r.duration)}</span>
                    <span className="text-right font-mono tabular-nums">{r.jumps}</span>
                    <span className="text-right font-mono tabular-nums text-muted-foreground">{r.bestAir > 0 ? `${r.bestAir.toFixed(1)}s` : "–"}</span>
                    <span className="text-right font-mono tabular-nums text-muted-foreground">{Math.round(r.topSpeed * 3.6)}</span>
                  </button>
                ))}
                {shown.length === 0 && <div className="px-4 py-6 text-center text-[12.5px] text-muted-foreground">{t("rides.noMatch")}</div>}
              </div>
            )}
            {rides != null && rides.length > 0 && (
              <div className="mt-2 text-right text-[11px] text-faint">{t("rides.count", { n: shown.length, of: rides.length })}</div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
