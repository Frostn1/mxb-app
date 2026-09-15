import { useEffect, useState } from "react";
import {
  loadTrackOverview,
  loadTrackBackdrop,
  loadTrackGround,
  loadTrackGroundLayers,
  loadTrackScenery,
  loadTrackSurfaces,
  loadTrackTerrain,
  readTrackInfo,
  readTrackPlacements,
} from "../../api/tracks";
import type {
  TrackInfo,
  TrackOverview,
  TrackPlacement,
  TrackBackdrop,
  TrackGround,
  TrackGroundLayer,
  TrackScenery,
  TrackSceneryTexture,
  TrackTerrain,
} from "../../types";

/**
 * The two passes the terrain is loaded in.
 *
 * A coarse grid is a few tens of kilobytes and builds its mesh in a frame or two, so the
 * track is on screen almost immediately; the detailed one replaces it once it arrives. Both
 * come from the same cached master in the backend, so the second pass costs a resample and
 * a transfer rather than another read of the archive.
 */
const COARSE_DIM = 128;
const FINE_DIM = 2048;

/** The surface picture's longest edge. Matches the masks' own 2048, so the surface is drawn
 *  at the resolution the track painted it rather than at half of it, and matches the terrain
 *  grid it lies across. A track's own is rarely bigger, and past this the picture costs more
 *  to move than it adds. */
const OVERVIEW_DIM = 2048;

/**
 * The passes a track arrives in, in the order they settle.
 *
 * A track lands in pieces over several seconds — terrain, then sky, then ground, then the
 * scenery mesh, and its colours last because they are hundreds of megabytes of sheets. Until
 * now the only sign of that was one grey word in the header, so a half-painted track and a
 * finished one looked the same and there was no moment that said *done*.
 */
export const STEPS = ["terrain", "sky", "ground", "scenery", "colours"] as const;
export type Step = (typeof STEPS)[number];

/** `none` is a settled answer: a track that ships no scenery is finished, not still loading. */
export type StepState = "waiting" | "running" | "done" | "none" | "failed";

function freshSteps(): Record<Step, StepState> {
  return {
    terrain: "running",
    sky: "running",
    ground: "running",
    scenery: "running",
    // Nothing is asked for until the scenery mesh is up, so this one genuinely is waiting.
    colours: "waiting",
  };
}

/** Nothing asked for, so nothing to wait on. */
function idleSteps(): Record<Step, StepState> {
  return { terrain: "none", sky: "none", ground: "none", scenery: "none", colours: "none" };
}

export interface TrackScene {
  info: TrackInfo | null;
  terrain: TrackTerrain | null;
  overview: TrackOverview | null;
  scenery: TrackScenery | null;
  surfaces: TrackSceneryTexture[];
  backdrop: TrackBackdrop | null;
  ground: TrackGround | null;
  groundLayers: TrackGroundLayer[];
  placements: TrackPlacement[];
  /** Until the coarse terrain lands. */
  loading: boolean;
  refining: boolean;
  painting: boolean;
  steps: Record<Step, StepState>;
  /** Every pass has an answer. */
  settled: boolean;
  sceneryError: string | null;
  error: string | null;
}

/** `path` null loads nothing (and clears). `prefix` is the track's folder inside an archive
 *  holding several (a stock track in tracks.pkz): terrain, overview and info take it; the other
 *  loaders don't, so with a prefix they are skipped and their steps settle as "none".
 *  `generation` reloads when it changes. */
export function useTrackScene(
  path: string | null,
  opts?: { prefix?: string | null; generation?: number },
): TrackScene {
  const prefix = opts?.prefix ?? null;
  const generation = opts?.generation ?? 0;
  const [info, setInfo] = useState<TrackInfo | null>(null);
  const [terrain, setTerrain] = useState<TrackTerrain | null>(null);
  const [overview, setOverview] = useState<TrackOverview | null>(null);
  const [scenery, setScenery] = useState<TrackScenery | null>(null);
  const [surfaces, setSurfaces] = useState<TrackSceneryTexture[]>([]);
  const [backdrop, setBackdrop] = useState<TrackBackdrop | null>(null);
  const [ground, setGround] = useState<TrackGround | null>(null);
  const [groundLayers, setGroundLayers] = useState<TrackGroundLayer[]>([]);
  const [placements, setPlacements] = useState<TrackPlacement[]>([]);
  // True only until the *coarse* pass lands — the refine that follows happens under a
  // terrain that is already up, and covering it with a spinner would be a step backwards.
  const [loading, setLoading] = useState(false);
  const [refining, setRefining] = useState(false);
  // The surfaces are still inflating; the scenery is up but grey.
  const [painting, setPainting] = useState(false);
  // Where each pass has got to, so the sequence can be shown rather than guessed at.
  const [steps, setSteps] = useState<Record<Step, StepState>>(() =>
    path ? freshSteps() : idleSteps(),
  );
  // A scenery pass that fails has to say so. Swallowing it leaves a track looking as though
  // it simply has none, which is a different fact entirely.
  const [sceneryError, setSceneryError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setInfo(null);
    setTerrain(null);
    setOverview(null);
    setScenery(null);
    setSurfaces([]);
    setBackdrop(null);
    setGround(null);
    setGroundLayers([]);
    setSceneryError(null);
    setPlacements([]);
    setError(null);

    if (!path) {
      setLoading(false);
      setRefining(false);
      setPainting(false);
      setSteps(idleSteps());
      return;
    }

    let alive = true;
    setLoading(true);
    setSteps(freshSteps());

    const settle = (step: Step, state: StepState) => {
      if (alive) setSteps((s) => (s[step] === state ? s : { ...s, [step]: state }));
    };

    // The inventory doesn't inflate anything, so it lands while the terrain is still being
    // read and the panel can fill in around the empty canvas.
    readTrackInfo(path, prefix)
      .then((i) => alive && setInfo(i))
      .catch(() => {});

    // The loaders below read a whole archive as one track, so a track that is one folder of
    // several has nothing of its own for them to find.
    if (prefix) {
      settle("sky", "none");
      settle("ground", "none");
    } else {
      // Kilobytes of cfg, so these land while the terrain is still being read — the markers
      // are up before the scenery they stand among.
      readTrackPlacements(path)
        .then((p) => alive && setPlacements(p))
        .catch(() => {});

      // The sky is a few hundred triangles, so it lands with the first pass rather than after
      // the scenery — a track should never be on screen with nothing above it.
      loadTrackBackdrop(path)
        .then((b) => {
          if (!alive) return;
          setBackdrop(b);
          settle("sky", b ? "done" : "none");
        })
        .catch(() => settle("sky", "failed"));

      // One small sheet, so it lands early and the ground has grain from the first frame the
      // terrain is up.
      loadTrackGround(path)
        .then((g) => alive && setGround(g))
        .catch(() => {});

      // The ground the game draws. A few hundred kilobytes once reduced, and it replaces the
      // surface picture rather than adding to it, so it is worth having as early as possible.
      loadTrackGroundLayers(path)
        .then((l) => {
          if (!alive) return;
          setGroundLayers(l);
          settle("ground", l.length ? "done" : "none");
        })
        .catch(() => settle("ground", "failed"));
    }

    void (async () => {
      try {
        // Started before anything is awaited, so the archive read it needs overlaps the
        // terrain's rather than following it.
        const surface = loadTrackOverview(path, OVERVIEW_DIM, prefix).catch(() => null);
        // The heaviest read in the view — most of a track's bulk is its `.map` — so it is
        // started here and settled last, under a terrain that is already up.
        const objects = prefix
          ? Promise.resolve(null)
          : loadTrackScenery(path).catch((e) => {
              setSceneryError(e instanceof Error ? e.message : String(e));
              settle("scenery", "failed");
              settle("colours", "failed");
              return null;
            });

        const coarse = await loadTrackTerrain(path, COARSE_DIM, prefix);
        if (!alive) return;
        setTerrain(coarse);
        setLoading(false);

        setRefining(true);
        // Both together, and applied in one go: the mesh is built from the terrain *and*
        // whether there's a surface to lay on it, so settling them separately would build a
        // grid of a million-odd vertices twice and throw the first away.
        const [fine, map] = await Promise.all([
          loadTrackTerrain(path, FINE_DIM, prefix),
          surface,
        ]);
        if (!alive) return;
        setOverview(map);
        // Settled on its own: the scenery is a separate mesh, so it can arrive after the
        // terrain has refined without either waiting on the other. Its surfaces are asked
        // for only once the mesh is up, so the track is on screen — in outline — while the
        // hundreds of megabytes behind its colours are still inflating.
        void objects.then((s) => {
          if (!alive) return;
          setScenery(s);
          if (!s) {
            // A track with no scenery has no colours to wait for either — leaving that step
            // waiting would mean the sequence never finished on a bare track.
            settle("scenery", "none");
            settle("colours", "none");
            return;
          }
          settle("scenery", "done");
          settle("colours", "running");
          setPainting(true);
          loadTrackSurfaces(path)
            .then((tex) => {
              if (!alive) return;
              setSurfaces(tex);
              settle("colours", tex.length ? "done" : "none");
            })
            .catch((e) => {
              setSceneryError(e instanceof Error ? e.message : String(e));
              settle("colours", "failed");
            })
            .finally(() => alive && setPainting(false));
        });
        // Only an upgrade: a backend that capped the master below the fine size would
        // otherwise have us swap a grid for an identical one and rebuild the mesh for free.
        if (fine.width > coarse.width) setTerrain(fine);
        // Settled on the refine rather than the coarse pass: the track is on screen either
        // way, but it is not the terrain the track has until the detailed grid is up.
        settle("terrain", "done");
      } catch (e) {
        if (!alive) return;
        setError(e instanceof Error ? e.message : String(e));
        settle("terrain", "failed");
      } finally {
        if (alive) {
          setLoading(false);
          setRefining(false);
        }
      }
    })();

    return () => {
      alive = false;
    };
  }, [path, prefix, generation]);

  // Every pass has an answer — done, empty or failed. Nothing is still running.
  const settled = STEPS.every((s) => steps[s] !== "waiting" && steps[s] !== "running");

  return {
    info,
    terrain,
    overview,
    scenery,
    surfaces,
    backdrop,
    ground,
    groundLayers,
    placements,
    loading,
    refining,
    painting,
    steps,
    settled,
    sceneryError,
    error,
  };
}
