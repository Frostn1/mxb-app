import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronLeft, Pause, Play, Plane } from "lucide-react";
import type { ViewerCamera } from "@frost/shared/Components/Viewer/TrackViewer";
import { getLocale } from "@frost/shared/i18n/core";
import { cn } from "@frost/shared/lib/utils";
import { useReplayClock } from "@coach/Components/Review/useReplayClock";
import { loadRide, type Ride, type SessionSummary } from "@/api/cinema";
import { useT } from "@/i18n";
import { clock, momentAt, rideLength, when } from "@/lib/ride";
import ExportPanel from "./ExportPanel";
import Preview from "./Preview";
import Timeline from "./Timeline";

/** Seconds across the scrubber. */
const SPAN = 15;

/**
 * One ride: scrub to the moment, move the camera like Blender, export that frame from that view.
 * The preview is Coach's replay: its clock, the shared track viewer, the bike and the rider posed
 * from the recording.
 */
export default function Shot({
  session,
  onBack,
  onBusy,
}: {
  session: SessionSummary;
  onBack: () => void;
  /** An export is running: the rest of the app holds still until it ends. */
  onBusy?: (busy: boolean) => void;
}) {
  const t = useT();
  const [ride, setRide] = useState<Ride | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const cameraProbe = useRef<(() => ViewerCamera | null) | null>(null);
  // Dev builds: the camera readable from a test page (the headless checks drive the view).
  useEffect(() => {
    if (import.meta.env.DEV) (window as unknown as Record<string, unknown>).__cinemaCamera = () => cameraProbe.current?.() ?? null;
  }, []);

  useEffect(() => {
    let live = true;
    setRide(null);
    setFailed(null);
    loadRide(session.path)
      .then((r) => live && setRide(r))
      .catch((e) => live && setFailed(String(e)));
    return () => {
      live = false;
    };
  }, [session.path]);

  const length = ride ? rideLength(ride) : 0;
  const clockState = useReplayClock(length, false, "stop");
  const at = clockState.at;
  const now = useMemo(() => (ride ? momentAt(ride, at) : null), [ride, at]);

  // The first moment worth looking at: the first jump, rather than the start gate.
  useEffect(() => {
    if (ride?.jumps.length) clockState.seek(ride.jumps[0].apex);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ride]);

  const nextJump = () => {
    if (!ride) return;
    const j = ride.jumps.find((x) => x.start > at + 0.05) ?? ride.jumps[0];
    if (j) clockState.seek(j.apex);
  };

  // Space plays, arrows step a frame (Shift: a second), J goes to the next jump.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!ride || busy || (e.target as HTMLElement)?.closest("input, textarea")) return;
      if (e.code === "Space") {
        e.preventDefault();
        clockState.toggle();
      } else if (e.key === "ArrowRight") clockState.seek(at + (e.shiftKey ? 1 : 1 / 50));
      else if (e.key === "ArrowLeft") clockState.seek(at - (e.shiftKey ? 1 : 1 / 50));
      else if (e.key === "j" || e.key === "J") nextJump();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const setBusyBoth = (b: boolean) => {
    setBusy(b);
    onBusy?.(b);
  };

  const bigJumps = useMemo(
    () => (ride ? [...ride.jumps].sort((a, b) => b.end - b.start - (a.end - a.start)).slice(0, 10).sort((a, b) => a.start - b.start) : []),
    [ride],
  );

  return (
    <div className="h-full overflow-y-auto">
      <div className="mx-auto max-w-[1400px] px-8 py-6">
        <button
          onClick={onBack}
          disabled={busy}
          title={t("shot.backTip")}
          className="mb-3 flex items-center gap-1 text-[12px] text-muted-foreground hover:text-foreground disabled:opacity-40"
        >
          <ChevronLeft className="size-3.5" />
          {t("shot.back")}
        </button>
        <h2 className="headline truncate text-[24px]">{session.trackName}</h2>
        <div className="mt-1 text-[12.5px] text-muted-foreground">
          {session.bikeName} · {when(session.started, getLocale())} · {session.rider}
        </div>

        {failed && <p className="mt-6 text-[13px] text-[#ff6961]">{t("shot.failed")}</p>}

        {ride && (
          <div className="mt-5 grid grid-cols-[minmax(0,1fr)_340px] gap-5">
            <div className="min-w-0 space-y-3">
              <Preview ride={ride} now={now} range={null} cameraProbe={cameraProbe} className="h-[480px]" />
              <div className="text-[12px] text-muted-foreground">{t("shot.help")}</div>

              <div className="flex items-center gap-3">
                <button
                  onClick={clockState.toggle}
                  disabled={busy}
                  className="flex size-8 shrink-0 items-center justify-center rounded-full border border-border text-foreground hover:bg-secondary disabled:opacity-40"
                  title={clockState.playing ? t("shot.pauseTip") : t("shot.playTip")}
                  aria-label={clockState.playing ? t("shot.pause") : t("shot.play")}
                >
                  {clockState.playing ? <Pause className="size-4" /> : <Play className="size-4" />}
                </button>
                <span className="whitespace-nowrap font-mono text-[13px] tabular-nums" title={t("shot.timeTip")}>
                  {clock(at)} <span className="text-muted-foreground">/ {clock(length)}</span>
                </span>
              </div>

              <div title={t("shot.scrubTip")} className={cn(busy && "pointer-events-none opacity-50")}>
                <Timeline ride={ride} at={at} span={SPAN} range={null} onSeek={clockState.seek} onRange={() => {}} />
              </div>
              <div className={cn("flex flex-wrap items-center gap-1.5", busy && "pointer-events-none opacity-50")}>
                <span className="mr-1 flex items-center gap-1 text-[11.5px] text-muted-foreground">
                  <Plane className="size-3.5" />
                  {t("shot.jumpsLabel")}
                </span>
                {bigJumps.map((j) => {
                  const on = at >= j.start && at <= j.end;
                  return (
                    <button
                      key={j.start}
                      onClick={() => clockState.seek(j.apex)}
                      title={t("shot.jumpTip", { air: (j.end - j.start).toFixed(1), h: j.height.toFixed(1) })}
                      className={cn(
                        "rounded-full border px-2.5 py-0.5 font-mono text-[11px] tabular-nums",
                        on ? "border-[#ff9f0a] text-[#ff9f0a]" : "border-border text-muted-foreground hover:text-foreground",
                      )}
                    >
                      {clock(j.apex).replace(/\.\d+$/, "")} · {(j.end - j.start).toFixed(1)}s
                    </button>
                  );
                })}
              </div>
            </div>

            <ExportPanel path={session.path} mark={at} view={() => cameraProbe.current?.() ?? null} onBusy={setBusyBoth} />
          </div>
        )}
      </div>
    </div>
  );
}
