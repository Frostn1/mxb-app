import { useEffect, useMemo, useState } from "react";
import { Loader2 } from "lucide-react";
import type { BikePose } from "@frost/shared/Components/Viewer/ModelViewer";
import { TrackViewer, type ViewerActor, type ViewerCamera, type ViewerLine } from "@frost/shared/Components/Viewer/TrackViewer";
import { useTrackScene } from "@frost/shared/Components/Viewer/useTrackScene";
import { coachRiderBody, previewModelSwap } from "@frost/shared/api/mods";
import { riderPoseFrom } from "@frost/shared/lib/riderMotion";
import { cn } from "@frost/shared/lib/utils";
import type { BikeModel, RiderPart } from "@frost/shared/types";
import { rideGround, type Ground, type Ride } from "@/api/cinema";
import { useT } from "@/i18n";
import type { Moment } from "@/lib/ride";

/** How much of the bike's own lean and pitch to draw — Coach's replay figure. */
const LEAN_SHOW = 0.85;
/** Half the measured travel on screen, as Coach draws it: the whole stroke puts the wheel through the guard. */
const SHOW = 0.5;
/** Ride drawn either side of the playhead, seconds: enough to read where the bike is going. */
const CONTEXT_S = 25;

const PATH = "rgba(245, 245, 247, 0.55)";
const PICK = "#ff9f0a";

/** The bike and body, fetched once per ride and kept while the rider scrubs. */
const bikes = new Map<string, Promise<BikeModel>>();
function bikeFor(id: string): Promise<BikeModel> {
  let p = bikes.get(id);
  if (!p) {
    p = previewModelSwap(id, "", undefined);
    bikes.set(id, p);
  }
  return p;
}

/**
 * The ride on its own track, light: the track MXB App draws, the line the bike took, and the
 * bike itself at the playhead, posed from the recording the way Coach replays a corner. The
 * picked range is drawn over the line, so a jump reads as the stretch about to be exported.
 */
export default function Preview({
  ride,
  now,
  range,
  cameraProbe,
  className,
}: {
  ride: Ride;
  /** Filled with a reader for the view's camera: the shot is taken from where the user left it. */
  cameraProbe?: { current: (() => ViewerCamera | null) | null };
  now: Moment | null;
  range: [number, number] | null;
  className?: string;
}) {
  const t = useT();
  const [ground, setGround] = useState<Ground | null>(null);
  const [why, setWhy] = useState<string | null>(null);
  const [model, setModel] = useState<BikeModel | null>(null);
  const [body, setBody] = useState<RiderPart[]>([]);
  const path = ride.summary.path;

  useEffect(() => {
    let live = true;
    setGround(null);
    setWhy(null);
    rideGround(path)
      .then((g) => {
        if (!live) return;
        setGround(g.ground);
        setWhy(g.why);
      })
      .catch(() => live && setWhy("unreadable"));
    return () => {
      live = false;
    };
  }, [path]);

  useEffect(() => {
    let live = true;
    bikeFor(ride.summary.bikeId)
      .then((m) => live && setModel(m))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [ride.summary.bikeId]);

  useEffect(() => {
    let live = true;
    coachRiderBody(ride.summary.rider)
      .then((b) => live && setBody(b))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [ride.summary.rider]);

  const scene = useTrackScene(ground?.path ?? null, { prefix: ground?.prefix ?? null });
  const lift = ground?.lift ?? 0;

  // The line, near the playhead, and the picked range over it.
  const at = now?.t ?? 0;
  const bucket = Math.floor(at / 5);
  const lines = useMemo<ViewerLine[]>(() => {
    const pts = (a: number, b: number): [number, number, number][] => {
      const out: [number, number, number][] = [];
      const step = Math.max(1, Math.floor(ride.count / 40000));
      for (let k = 0; k < ride.count; k += step) {
        const tt = ride.t[k];
        if (tt < a || tt > b) continue;
        out.push([ride.x[k], ride.y[k] - lift + 0.3, ride.z[k]]);
      }
      return out;
    };
    const c = bucket * 5;
    const out: ViewerLine[] = [{ points: pts(c - CONTEXT_S, c + CONTEXT_S + 5), colour: PATH, width: 1.4 }];
    if (range && range[1] - range[0] > 0.05) out.push({ points: pts(range[0], range[1]), colour: PICK, width: 4 });
    return out;
  }, [ride, lift, bucket, range]);

  const actor = useMemo<ViewerActor | null>(() => {
    if (!now || !model) return null;
    const travel = ride.suspMaxTravel;
    const pose: Partial<BikePose> = {
      steer: now.steer,
      spin: now.spin,
      forkUp: now.used[0] * travel[0] * 1000 * SHOW,
      rearDrop: -now.used[1] * travel[1] * 1000 * SHOW,
    };
    const known = (v: number | null, ok: boolean) => (ok && v != null ? v : NaN);
    const riderPose = riderPoseFrom({
      leanLR: known(now.lean[0], ride.leanKnown[0]),
      leanFB: known(now.lean[1], ride.leanKnown[1]),
      stance: !ride.stanceKnown || now.stance === 0 ? "unknown" : now.stance === 1 ? "stand" : "sit",
      bikeRoll: -now.roll,
    });
    return {
      nodes: model.nodes,
      rig: model.rig,
      textures: model.base,
      at: [now.x, now.y - lift, now.z],
      yaw: now.yaw,
      roll: -now.roll * LEAN_SHOW,
      pitch: now.pitch * LEAN_SHOW,
      pose,
      riderParts: body.length > 0 ? body : null,
      riderPose,
    };
  }, [now, model, body, ride, lift]);

  // Aimed a metre over the bike's feet, so a bike in the air stays in the frame.
  const follow = useMemo(() => (actor ? { x: actor.at[0], z: actor.at[2], y: actor.at[1] + 1 } : null), [actor]);
  const real = ground != null && scene.terrain != null;
  const waiting = ground != null && !scene.terrain;

  return (
    <div className={cn("relative overflow-hidden rounded-[var(--radius)] border border-border bg-black", className)}>
      {real && scene.terrain ? (
        <TrackViewer
          terrain={scene.terrain}
          overview={scene.overview}
          scenery={scene.scenery}
          surfaces={scene.surfaces}
          backdrop={scene.backdrop}
          ground={scene.ground}
          groundLayers={scene.groundLayers}
          placements={scene.placements}
          gameView={scene.groundLayers.length > 0}
          lines={lines}
          actor={actor}
          follow={follow}
          cameraProbe={cameraProbe}
          frameActor
          followDistance={5.5}
          className="absolute inset-0 h-full w-full"
        />
      ) : null}
      {(waiting || (!ground && !why)) && (
        <div className="absolute inset-0 flex flex-col items-center justify-center gap-3">
          <Loader2 className="size-6 animate-spin text-white/80" />
          <span className="text-[13px] text-white/85">{t("preview.loadingTrack")}</span>
        </div>
      )}
      {!ground && why && (
        <div className="absolute inset-0 flex items-center justify-center p-6 text-center">
          <div>
            <div className="text-[13px] font-semibold text-warning">{t("preview.noTrack")}</div>
            <div className="mt-1 text-[12px] text-white/70">{t(`preview.why.${why}` as "preview.why.notInstalled")}</div>
          </div>
        </div>
      )}
      {now && (
        <div className="pointer-events-none absolute left-3 top-3 flex items-start gap-4">
          <div>
            <div className="font-mono text-[22px] font-bold leading-none tabular-nums text-primary">
              {Math.round(now.speed * 3.6)}
              <span className="ml-1 text-[11px] font-normal text-muted-foreground">km/h</span>
            </div>
            <div className="mt-0.5 text-[11px] text-muted-foreground">
              {t("preview.gear")} {now.gear > 0 ? now.gear : "N"}
            </div>
          </div>
          {now.air && (
            <span className="rounded-full bg-[#ff9f0a]/90 px-2.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-black">
              {t("preview.airborne")}
            </span>
          )}
          {now.crashed && (
            <span className="rounded-full bg-[#ff6961]/90 px-2.5 py-0.5 text-[11px] font-semibold uppercase tracking-wide text-black">
              {t("preview.crashed")}
            </span>
          )}
        </div>
      )}
    </div>
  );
}
