import { useEffect, useMemo, useState } from "react";
import { Pause, Play, RotateCcw } from "lucide-react";
import { ModelViewer, type BikePose } from "@frost/shared/Components/Viewer/ModelViewer";
import { riderPoseFrom } from "@frost/shared/lib/riderMotion";
import { coachRiderBody } from "@frost/shared/api/mods";
import { cn } from "@frost/shared/lib/utils";
import type { BikeModel, RiderPart } from "@frost/shared/types";
import { coachReplay, type Replay, type ReplayFrame } from "@/api/coach";
import { useT } from "@/i18n";
import { preloadBike } from "./BikeRender";
import { frameAt, lerp, useReplayClock } from "./useReplayClock";

/** The bike's own attitude, degrees, and how much of it to draw. */
const LEAN_SHOW = 0.85;
/**
 * A nominal stroke at each end, millimetres, for turning a travel share into a distance.
 *
 * The recording gives the share of the travel in use, not the travel. The real stroke comes off
 * the setup plan, which this step has not asked for — so these stand in, and they are a modern
 * 450's figures rather than this bike's. The share is the measurement and it is exact; the
 * distance it is drawn at is near enough to read and no claim is made about it anywhere.
 */
const NOMINAL_MM: [number, number] = [310, 130];
/** Half of it on screen, for the same reason the feel step halves its stroke. */
const SHOW = 0.5;

/**
 * Read a frame between two samples.
 *
 * Everything here is continuous except gear, stance and whether the wheels are down — those are
 * states, and a gear that reads 3.4 halfway through a shift is not a thing that happened.
 */
function at(f: ReplayFrame[], t: number): ReplayFrame | null {
  if (f.length === 0) return null;
  const { i, mix } = frameAt(
    f.map((x) => x.t),
    t,
  );
  const a = f[i];
  const b = f[i + 1] ?? a;
  return {
    ...a,
    t,
    dist: lerp(a.dist, b.dist, mix),
    v: lerp(a.v, b.v, mix),
    steer: lerp(a.steer, b.steer, mix),
    used: [lerp(a.used[0], b.used[0], mix), lerp(a.used[1], b.used[1], mix)],
    roll: lerp(a.roll, b.roll, mix),
    pitch: lerp(a.pitch, b.pitch, mix),
    spin: lerp(a.spin, b.spin, mix),
    throttle: lerp(a.throttle, b.throttle, mix),
    front: lerp(a.front, b.front, mix),
    rear: lerp(a.rear, b.rear, mix),
  };
}

/** The ghost's frame at the same point on the track, which is what a comparison means here. */
function atDistance(f: ReplayFrame[], dist: number): ReplayFrame | null {
  if (f.length === 0) return null;
  let lo = 0;
  let hi = f.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (f[mid].dist <= dist) lo = mid;
    else hi = mid - 1;
  }
  return f[lo];
}

/**
 * The corner, played back on the rider's own bike.
 *
 * Everything on screen came off their recording: the bars, both ends of the suspension, how far
 * the bike is leaned and how hard it is pitched, and the wheels turning at the speed they were
 * actually doing. Where the recorder read the rider's body controls, the body moves too;
 * where it did not, the body stays still and the panel says so, because a centred rider and a
 * rider we never saw are different facts and only one of them is knowledge.
 */
export default function SectionReplay({
  path,
  lap,
  sectionId,
  bikeId,
  rider,
  maxTravel,
  className,
}: {
  path: string;
  lap: number;
  sectionId: string;
  bikeId: string;
  /** The rider's own profile, as the recording names them. Falls back to the game's stock body. */
  rider?: string;
  /** Each end's real stroke in mm, when the caller knows it. Without it, {@link NOMINAL_MM}. */
  maxTravel?: [number, number] | null;
  className?: string;
}) {
  const t = useT();
  const [replay, setReplay] = useState<Replay | null>(null);
  const [failed, setFailed] = useState(false);
  const [model, setModel] = useState<BikeModel | null>(null);
  const [body, setBody] = useState<RiderPart[]>([]);

  useEffect(() => {
    setReplay(null);
    setFailed(false);
    let live = true;
    coachReplay(path, lap, sectionId)
      .then((r) => live && setReplay(r))
      .catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [path, lap, sectionId]);

  // The same shared promise the feel step warms, so stepping between corners never reloads it.
  useEffect(() => {
    const p = preloadBike(bikeId);
    if (!p) return;
    let live = true;
    p.then((m) => live && setModel(m)).catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [bikeId]);

  // The body is asked for once and left alone if it isn't installed: the bike is the subject
  // and a missing rider is a quieter answer, not an error.
  useEffect(() => {
    let live = true;
    coachRiderBody(rider ?? "")
      .then((parts) => live && setBody(parts))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [rider]);

  const frames = useMemo(() => replay?.frames ?? [], [replay]);
  const length = frames.length > 0 ? frames[frames.length - 1].t : 0;
  const clock = useReplayClock(length);
  const now = useMemo(() => at(frames, clock.at), [frames, clock.at]);
  const ghost = useMemo(
    () => (now && replay?.best.length ? atDistance(replay.best, now.dist) : null),
    [now, replay],
  );

  const pose = useMemo<Partial<BikePose> | null>(
    () =>
      now
        ? {
            steer: now.steer,
            spin: now.spin,
            forkUp: now.used[0] * (maxTravel?.[0] ?? NOMINAL_MM[0]) * SHOW,
            rearDrop: -now.used[1] * (maxTravel?.[1] ?? NOMINAL_MM[1]) * SHOW,
          }
        : null,
    [now, maxTravel],
  );

  const attitude = useMemo(
    () => (now ? { roll: now.roll * LEAN_SHOW, pitch: now.pitch * LEAN_SHOW } : null),
    [now],
  );

  /**
   * The body, from what the rider was asking for with the controls.
   *
   * An axis the recorder never read arrives as null and stays null all the way here, so the
   * bones it would have moved do not move. The bike leans either way: its attitude is measured,
   * the body is reported, and the two are not the same kind of fact.
   */
  const bones = useMemo(() => {
    if (!now || !replay) return undefined;
    const known = (v: number | null, ok: boolean) => (ok && v != null ? v : NaN);
    return riderPoseFrom({
      leanLR: known(now.lean[0], replay.leanKnown[0]),
      leanFB: known(now.lean[1], replay.leanKnown[1]),
      stance: !replay.stanceKnown || now.stance === 0 ? "unknown" : now.stance === 1 ? "stand" : "sit",
      bikeRoll: now.roll,
    });
  }, [now, replay]);

  if (failed) return <p className={cn("text-[12px] text-faint", className)}>{t("replay.noFrames")}</p>;

  const bodyBlind = replay != null && !replay.leanKnown[0] && !replay.leanKnown[1] && !replay.stanceKnown;
  const delta = now && ghost ? now.t - ghost.t : null;

  return (
    <div className={cn("space-y-2", className)}>
      <div className="relative h-[280px] overflow-hidden rounded-[var(--radius)] border border-border">
        <ModelViewer
          mode={body.length > 0 ? "onBike" : "bike"}
          nodes={model?.nodes ?? null}
          rig={model?.rig ?? null}
          textures={model?.base}
          riderParts={body.length > 0 ? body : null}
          riderPose={bones}
          bikePoseOffset={pose}
          bikeAttitude={attitude}
          loading={!model || !replay}
          hideHints
          className="absolute inset-0"
        />

        {/* The numbers, on the bike, instead of a paragraph about the corner. */}
        {now && (
          <div className="pointer-events-none absolute left-3 top-3 space-y-1">
            <div className="font-mono text-[22px] font-bold leading-none tabular-nums text-primary">
              {Math.round(now.v * 3.6)}
              <span className="ml-1 text-[11px] font-normal text-muted-foreground">km/h</span>
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t("replay.gear")} {now.gear > 0 ? now.gear : "N"}
            </div>
          </div>
        )}

        {now && (
          <div className="pointer-events-none absolute bottom-3 left-3 right-3 space-y-1.5">
            <Bar label={t("replay.throttle")} v={now.throttle} colour="var(--primary)" />
            <Bar label={t("replay.brake")} v={Math.max(now.front, now.rear)} colour="#ff6961" />
          </div>
        )}

        {delta != null && Math.abs(delta) > 0.01 && (
          <div className="pointer-events-none absolute right-3 top-3 text-right">
            <div
              className="font-mono text-[15px] font-bold tabular-nums"
              style={{ color: delta > 0 ? "#ff6961" : "#30d158" }}
            >
              {delta > 0
                ? t("replay.slower", { s: delta.toFixed(2) })
                : t("replay.faster", { s: (-delta).toFixed(2) })}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t("replay.best")}
              {replay?.bestLap != null ? ` ${replay.bestLap}` : ""}
            </div>
          </div>
        )}
      </div>

      <div className="flex items-center gap-2">
        <button
          onClick={clock.toggle}
          className="flex size-7 shrink-0 items-center justify-center rounded-full border border-border text-foreground hover:bg-secondary"
          aria-label={clock.playing ? t("replay.pause") : t("replay.play")}
        >
          {clock.playing ? <Pause className="size-3.5" /> : clock.at >= length - 0.01 ? <RotateCcw className="size-3.5" /> : <Play className="size-3.5" />}
        </button>
        <input
          type="range"
          min={0}
          max={Math.max(length, 0.001)}
          step={0.01}
          value={clock.at}
          onChange={(e) => clock.seek(Number(e.target.value))}
          className="h-1 flex-1 cursor-pointer accent-[var(--primary)]"
          aria-label={t("replay.title")}
        />
        <span className="w-14 shrink-0 text-right font-mono text-[11px] tabular-nums text-muted-foreground">
          {clock.at.toFixed(2)}s
        </span>
      </div>

      {bodyBlind && <p className="text-[11px] leading-snug text-faint">{t("replay.bodyUnknown")}</p>}
    </div>
  );
}

/** A pedal, as a bar. Nought to one. */
function Bar({ label, v, colour }: { label: string; v: number; colour: string }) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-14 shrink-0 text-[10px] uppercase tracking-wide text-muted-foreground">{label}</span>
      <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-black/30">
        <span
          className="block h-full rounded-full"
          style={{ width: `${Math.max(0, Math.min(1, v)) * 100}%`, background: colour }}
        />
      </span>
    </div>
  );
}
