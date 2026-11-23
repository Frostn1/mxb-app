import { useEffect, useRef, useState } from "react";
import { ModelViewer, type BikePose } from "@frost/shared/Components/Viewer/ModelViewer";
import { previewModelSwap } from "@frost/shared/api/mods";
import type { BikeModel } from "@frost/shared/types";
import { useT } from "@/i18n";

/** What a click was about, and when — the time is what lets the same thing move twice. */
export type Poke = { axis: "fork" | "shock" | "steer" | "shake" | "revs" | "bog"; at: number };

/** One stroke in and back out, and how many of them a pick runs. */
const STROKE_MS = 1150;
const STROKES = 2;
/**
 * How much of the measured travel to show.
 *
 * A rider who used 305 of 310 mm has all but bottomed the fork, and drawing that puts the front
 * wheel through the mudguard on screen — true, and unreadable. Half of it reads as a stroke
 * while keeping the bike a bike.
 */
const SHOW = 0.5;
/**
 * Bars, degrees either way, and how far the wheels roll.
 *
 * Every roll figure is a whole number of turns, so the wheel is back where it started when the
 * movement ends and the reset to neutral cannot be seen.
 */
const STEER_DEG = 13;
const SPIN_DEG = 360;
/** A headshake: quick, small, and the BARS go both ways. The wheels never do. */
const SHAKE_DEG = 9;
const SHAKE_HZ = 5;
/** The driven wheel: revving out against bogging down, seven turns against one. */
const REVS_DEG = 2520;
const BOG_DEG = 360;

/**
 * How far a wheel has rolled, 0 to 1, for a movement that speeds up and slows down.
 *
 * The distance travelled, not the speed: it only ever grows. Easing the angle itself would
 * bring it back down again, and a wheel whose angle falls is a wheel rolling backwards, which
 * no bike on this page is ever doing.
 */
const rolled = (at: number) => (1 - Math.cos(at * Math.PI)) / 2;

/** The bike, fetched once per id and shared, so the step it appears on is never the first ask. */
const loading = new Map<string, Promise<BikeModel>>();

export function preloadBike(bikeId: string): Promise<BikeModel> | null {
  if (!bikeId) return null;
  const going = loading.get(bikeId);
  if (going) return going;
  // Kept even when it rejects: a bike that isn't installed shouldn't be asked for again on
  // every step change, and the component tells the rider either way.
  const p = previewModelSwap(bikeId, "", undefined);
  loading.set(bikeId, p);
  return p;
}

/**
 * The rider's own bike, beside the feel chips, and it moves when they say what it does.
 *
 * The shared renderer only — not `ViewerPanel`, which wraps it in a card, a "3D Preview"
 * header, an expand button and a rider/bike/both switch. None of that belongs on a step whose
 * whole subject is the machine, and the header sat on top of the travel figure.
 *
 * Picking a feel moves the part it is about: the fork strokes, the shock squats, "slow to turn
 * in" turns the bars. The wheels roll through all of it. The distances are the lap's own, so a
 * rider who barely used the fork sees barely any movement — the point is to answer "which end
 * is this about" with movement rather than a word, and to be true while doing it.
 */
export default function BikeRender({
  bikeId,
  travel,
  maxTravel,
  poke,
}: {
  /** The bike the session was ridden on, as the recorder wrote it. */
  bikeId: string;
  /** Share of each end's travel in use, 0 extended to 1 bottomed. Front then rear. */
  travel?: [number, number] | null;
  /** Each end's full stroke in metres. Without it a share has no distance to become. */
  maxTravel?: [number, number] | null;
  /** What to move, and when it was asked for. */
  poke?: Poke | null;
}) {
  const t = useT();
  const [model, setModel] = useState<BikeModel | null>(null);
  const [failed, setFailed] = useState(false);
  const [offset, setOffset] = useState<Partial<BikePose> | null>(null);
  const frame = useRef<number | null>(null);
  /**
   * The measurements the movement reads, held rather than watched.
   *
   * A pick changes what the sheet asks the backend for, which lands as a fresh plan, which is
   * fresh arrays — so watching them ran the movement again on every answer, a deselect
   * included. A movement happens when the rider says a thing, and only then.
   */
  const measured = useRef<{ travel?: [number, number] | null; max?: [number, number] | null }>({});
  measured.current = { travel, max: maxTravel };

  useEffect(() => {
    setFailed(false);
    const p = preloadBike(bikeId);
    if (!p) return;
    let live = true;
    p.then((m) => live && setModel(m)).catch(() => live && setFailed(true));
    return () => {
      live = false;
    };
  }, [bikeId]);

  // Stepped by hand rather than with a spring library: the viewer draws on demand, so each
  // step of the movement has to be a React commit or the frame never lands.
  useEffect(() => {
    if (!poke) return;
    const { travel: used, max } = measured.current;
    const end = poke.axis === "shock" ? 1 : 0;
    const mm = used && max ? Math.max(0, used[end]) * max[end] * 1000 * SHOW : 0;
    const needsTravel = poke.axis === "fork" || poke.axis === "shock";
    if (needsTravel && mm <= 0) return;
    const started = performance.now();
    const total = STROKE_MS * STROKES;
    const step = (now: number) => {
      const gone = now - started;
      if (gone >= total) {
        setOffset(null);
        frame.current = null;
        return;
      }
      const phase = (gone / STROKE_MS) * Math.PI;
      // Half a sine per stroke: in and back out, hardest through the middle, still at each end.
      const how = Math.sin(phase) ** 2;
      // The wheels roll through the whole thing, away and settling, so the bike reads as
      // moving rather than as a part being dragged.
      const roll = gone / total;
      const spin = SPIN_DEG * rolled(roll);
      // How hard the movement is going right now, for the things that ramp rather than stroke.
      const swell = Math.sin(roll * Math.PI);
      setOffset(
        poke.axis === "fork"
          ? { forkUp: mm * how, spin }
          : poke.axis === "shock"
            ? { rearDrop: -mm * how, spin }
            : poke.axis === "steer"
              ? // Bars one way and back, not squared, so they sweep through centre rather than
                // bouncing off it.
                { steer: STEER_DEG * Math.sin(phase * 2), spin }
              : poke.axis === "shake"
                ? // A headshake is the bars fighting both ways at speed: quick, small, swelling
                  // and settling. The front wheel swings with them because it hangs off them,
                  // and it keeps rolling forward while it does.
                  {
                    steer: SHAKE_DEG * swell * Math.sin(roll * Math.PI * 2 * SHAKE_HZ * STROKES),
                    spin,
                  }
                : // Gearing: the driven wheel, and only it. Revving out spins away under the
                  // engine; bogging barely turns for the same throttle.
                  { spinRear: (poke.axis === "revs" ? REVS_DEG : BOG_DEG) * roll * roll * (3 - 2 * roll) },
      );
      frame.current = requestAnimationFrame(step);
    };
    frame.current = requestAnimationFrame(step);
    return () => {
      if (frame.current != null) cancelAnimationFrame(frame.current);
      frame.current = null;
      setOffset(null);
    };
  }, [poke]);

  // A bike that came apart has nothing honest to show: its parts are each in their own frame,
  // so drawing them puts a pile on screen and calls it the rider's bike.
  if (failed || (model && !model.assembled)) {
    return <p className="text-[12px] text-faint">{t("bike.noModel")}</p>;
  }

  return (
    <div className="relative h-[240px] overflow-hidden rounded-[var(--radius)] border border-border">
      <ModelViewer
        mode="bike"
        nodes={model?.nodes ?? null}
        rig={model?.rig ?? null}
        textures={model?.base}
        bikePoseOffset={offset}
        loading={!model}
        hideHints
        className="absolute inset-0"
      />
      {travel && maxTravel && (
        <div className="pointer-events-none absolute right-3 top-3 text-right">
          <div className="font-mono text-[18px] font-bold tabular-nums text-primary">
            {Math.round(travel[0] * maxTravel[0] * 1000)}
            <span className="text-[11px] text-muted-foreground"> / {Math.round(maxTravel[0] * 1000)} mm</span>
          </div>
          <div className="text-[11px] text-muted-foreground">{t("bike.forkUsed")}</div>
        </div>
      )}
    </div>
  );
}
