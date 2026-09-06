import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { toast } from "sonner";
import {
  buildTrack,
  onBuildProgress,
  type BuildPhase,
  type BuildResult,
  type BuildStep,
  type TrackProgram,
} from "../api/trackgen";
import { useT } from "../i18n/context";
import type { TKey } from "../i18n";

/**
 * The track build, held above the tab it was started from.
 *
 * Compiling a track is a minute or more of PiBoSo's compilers grinding over a two-thousand
 * square terrain, and it used to live entirely inside `TrackStudio` — which the Dashboard
 * unmounts the moment you look at anything else. The work carried on in the backend, but
 * every sign of it went: no bar, no phase, and the finished track announced itself only if
 * you happened to still be there. Held here, one level above the view switch, a build is
 * watchable from wherever you are and is still going when you come back.
 */

/** Where a build is: one of the phases, or over. */
export type BuildState = BuildPhase | "done" | "failed";

const RUNNING: BuildState[] = [
  "synthesising",
  "writing",
  "map",
  "trh",
  "centerline",
  "packaging",
  "installing",
];

export function isRunning(state: BuildState): boolean {
  return RUNNING.includes(state);
}

/**
 * What each state is called on screen.
 *
 * Named for what is being made rather than for the file or the tool doing it: "map" is the
 * argument TerrainEd takes, and nobody waiting on a track wants to be told about a `.map`.
 */
export const PHASE_KEY: Record<BuildState, TKey> = {
  synthesising: "track.phase.synthesising",
  writing: "track.phase.writing",
  map: "track.phase.map",
  trh: "track.phase.trh",
  centerline: "track.phase.centerline",
  packaging: "track.phase.packaging",
  installing: "track.phase.installing",
  done: "track.phase.done",
  failed: "track.phase.failed",
};

export interface TrackBuild {
  /** The track's name, which is what anything showing the build says. */
  name: string;
  state: BuildState;
  /** 0–1. Only ever goes up — a bar that goes backwards reads as a fault. */
  progress: number;
  /** Each compiler run and what it said, once there is one. */
  steps: BuildStep[];
  result: BuildResult | null;
  error: string | null;
  startedAt: number;
}

interface TrackBuildContextValue {
  /** The build running now, or the last one that finished. `null` until one is started. */
  build: TrackBuild | null;
  /** Compile and install a track. Ignored while one is already going. */
  start: (program: TrackProgram) => void;
  /** Clear a finished build's card. */
  dismiss: () => void;
}

const TrackBuildContext = createContext<TrackBuildContextValue | null>(null);

/** How often the bar is redrawn between phases. Fast enough to look alive, slow enough that
 *  a minute of building is three hundred renders rather than four thousand. */
const TICK_MS = 200;

/**
 * Where the bar sits inside a phase, from how long the phase has been going.
 *
 * Asymptotic, so it never arrives: the phase's end belongs to the next event, and a bar that
 * sat full for the last twenty seconds of the graphics pass would be a lie told slowly. At
 * the expected duration it is 86% of the way across, which is the honest shape — most builds
 * take about as long as the last one, and the ones that don't still creep.
 */
function easeInto(span: { from: number; to: number; expect: number }, elapsed: number): number {
  const done = 1 - Math.exp((-2 * elapsed) / Math.max(span.expect, 0.5));
  return span.from + (span.to - span.from) * done;
}

export function TrackBuildProvider({
  onInstalled,
  children,
}: {
  /** A built track lands in the mods tree, which is a library the rest of the app lists. */
  onInstalled?: () => void;
  children: ReactNode;
}) {
  const [build, setBuild] = useState<TrackBuild | null>(null);
  const t = useT();
  // Held in refs so the ticker and the event listener never rebuild mid-build — switching
  // language while a track compiles must not restart the bar.
  const tRef = useRef(t);
  tRef.current = t;
  const onInstalledRef = useRef(onInstalled);
  onInstalledRef.current = onInstalled;
  // The phase now running: where it sits on the bar, how long it is expected to take, and
  // when it started. Not state — only the ticker reads it, and it changes on a timer.
  const span = useRef<{ from: number; to: number; expect: number; since: number } | null>(null);
  // Which build the events belong to. A second window's build would report on the same
  // channel, and it is not this one's to draw.
  const slug = useRef<string | null>(null);
  const running = useRef(false);

  // Creep across the running phase. Keyed on whether anything is building rather than on the
  // build itself, or the tick that moves the bar would tear down the timer that produced it.
  const live = build !== null && isRunning(build.state);
  useEffect(() => {
    if (!live) return;
    const id = window.setInterval(() => {
      const at = span.current;
      if (!at) return;
      const next = easeInto(at, (Date.now() - at.since) / 1000);
      setBuild((cur) => {
        if (!cur || !isRunning(cur.state)) return cur;
        const to = Math.max(cur.progress, next);
        // Late in a phase the curve is flat, and a new object per tick would redraw the
        // whole app to move the bar by nothing.
        return to - cur.progress < 0.0005 ? cur : { ...cur, progress: to };
      });
    }, TICK_MS);
    return () => window.clearInterval(id);
  }, [live]);

  const start = useCallback((program: TrackProgram) => {
    if (running.current) return;
    running.current = true;
    span.current = null;
    slug.current = null;
    setBuild({
      name: program.name,
      state: "synthesising",
      progress: 0,
      steps: [],
      result: null,
      error: null,
      startedAt: Date.now(),
    });

    void (async () => {
      // Listening is in place before the build is asked for, not alongside it: the first
      // phase is reported from inside the call, and `listen` is itself a round trip to the
      // backend — started in parallel, the bar would miss the phase it opens on.
      const off = await onBuildProgress((p) => {
        slug.current ??= p.slug;
        if (p.slug !== slug.current) return;
        span.current = { from: p.from, to: p.to, expect: p.expect, since: Date.now() };
        setBuild((cur) =>
          cur ? { ...cur, state: p.phase, progress: Math.max(cur.progress, p.from) } : cur,
        );
      });
      try {
        const result = await buildTrack(program, null, true);
        const failed = result.steps.find((s) => !s.ok);
        setBuild((cur) =>
          cur
            ? {
                ...cur,
                state: failed ? "failed" : "done",
                progress: 1,
                steps: result.steps,
                result,
                error: failed ? failed.output : null,
              }
            : cur,
        );
        if (failed) {
          toast.error(tRef.current("track.buildStepFailed", { step: failed.name }), {
            description: failed.output.slice(0, 400),
          });
        } else {
          toast.success(tRef.current("track.rideIt"), {
            description: result.installed ?? result.pkz ?? result.dir,
          });
          onInstalledRef.current?.();
        }
      } catch (e) {
        setBuild((cur) =>
          cur ? { ...cur, state: "failed", progress: 1, error: String(e) } : cur,
        );
        toast.error(tRef.current("track.compileFailed"), { description: String(e) });
      } finally {
        running.current = false;
        span.current = null;
        off();
      }
    })();
  }, []);

  const dismiss = useCallback(
    () => setBuild((cur) => (cur && !isRunning(cur.state) ? null : cur)),
    [],
  );

  const value = useMemo(() => ({ build, start, dismiss }), [build, start, dismiss]);
  return <TrackBuildContext.Provider value={value}>{children}</TrackBuildContext.Provider>;
}

export function useTrackBuild() {
  const ctx = useContext(TrackBuildContext);
  if (!ctx) throw new Error("useTrackBuild must be used within TrackBuildProvider");
  return ctx;
}
