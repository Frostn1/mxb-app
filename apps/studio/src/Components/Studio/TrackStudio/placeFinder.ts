import { toast } from "sonner";
import { getLocale, translate } from "@/i18n";
import type { TKey } from "@/i18n";
import {
  fetchPlace,
  findPlace,
  importPlaceDem,
  MAP_SPANS_M,
  onPlaceFetchProgress,
  placeCoverage,
  placeMap,
  type CoverageReport,
  type FetchStage,
  type Place,
  type PlaceHit,
  type PlaceMap,
} from "@/api/trackplace";

/**
 * Finding a place, and the fetch it turns into, held outside the panel that starts it.
 *
 * A fetch is one to two minutes of a public server cutting a plot out of a national survey,
 * and the panel that starts it is one of two the rider swaps between — so the moment they went
 * to look at a lap they had already traced, the search results, the coverage report and every
 * sign of the running fetch went with the unmounted component. The work carried on in the
 * backend and the window forgot about it, which is the same as it having stopped as far as
 * anyone watching is concerned.
 *
 * So it lives here, in a module, where nothing can unmount it. A panel subscribes on mount and
 * sees whatever is going on; it unsubscribes on unmount and the fetch carries on regardless.
 */

/** Where a running fetch has got to. */
export interface FetchRun {
  /** The place, as the rider named it. */
  name: string;
  /** Which fetch the events belong to. Latched from the first one that arrives. */
  slug: string | null;
  stage: FetchStage;
  /** Which layer is being asked for, when there is more than one candidate. */
  detail: string;
  /** 0–1. Only ever goes up: a bar that goes backwards reads as a fault. */
  progress: number;
}

export interface FinderState {
  query: string;
  hits: PlaceHit[] | null;
  picked: PlaceHit | null;
  cover: CoverageReport | null;
  /** A full lap is normally 2000 m or more, which does not fit a 470 m plot. */
  plot: number;
  busy: "search" | "coverage" | "fetch" | "import" | "map" | null;
  /** The fetch running now, or null. */
  run: FetchRun | null;
  /** The aerial picture the rider is picking a spot off, once they have opened one. */
  map: PlaceMap | null;
  /** Why there is no map here, when there isn't. */
  mapError: string | null;
  /** A place that finished fetching and has not been opened for tracing yet. */
  fetched: string | null;
}

const EMPTY: FinderState = {
  query: "",
  hits: null,
  picked: null,
  cover: null,
  plot: 1200,
  busy: null,
  run: null,
  map: null,
  mapError: null,
  fetched: null,
};

let state: FinderState = EMPTY;
const watchers = new Set<() => void>();

function set(next: Partial<FinderState>) {
  state = { ...state, ...next };
  for (const w of watchers) w();
}

export function subscribe(fn: () => void): () => void {
  watchers.add(fn);
  return () => watchers.delete(fn);
}

export function snapshot(): FinderState {
  return state;
}

/** Said from here rather than from a component, because a component may be gone by now. */
function say(key: TKey, vars?: Record<string, string>): string {
  return translate(getLocale(), key, vars);
}

export function setQuery(query: string) {
  set({ query });
}

export function setPlot(plot: number) {
  set({ plot });
}

/** The place a rider has settled on, whether they typed its name or pointed at it. */
export async function pick(hit: PlaceHit) {
  set({ picked: hit, cover: null, busy: "coverage" });
  try {
    set({ cover: await placeCoverage(hit.lat, hit.lon) });
  } catch (e) {
    toast.error(String(e));
  } finally {
    set({ busy: null });
  }
}

export async function search() {
  const query = state.query.trim();
  if (!query) return;
  set({ busy: "search", hits: null, picked: null, cover: null });
  try {
    const found = await findPlace(query);
    set({ hits: found, busy: null });
    // One answer is not a choice, so it is made.
    if (found.length === 1) await pick(found[0]);
  } catch (e) {
    toast.error(String(e));
    set({ busy: null });
  }
}

// ── The map ──────────────────────────────────────────────────────────────────

/** Open the map on a spot, or move it. One request, because a rider asked for one picture. */
export async function showMap(lat: number, lon: number, spanM: number) {
  set({ busy: "map", mapError: null });
  try {
    set({ map: await placeMap(lat, lon, spanM) });
  } catch (e) {
    set({ mapError: String(e) });
  } finally {
    set({ busy: null });
  }
}

export function closeMap() {
  set({ map: null, mapError: null });
}

/** One step closer in, or one step wider, from the sizes the backend offers. */
export function mapZoom(step: 1 | -1) {
  const m = state.map;
  if (!m) return;
  const at = MAP_SPANS_M.indexOf(m.spanM);
  const next = MAP_SPANS_M[Math.min(MAP_SPANS_M.length - 1, Math.max(0, (at < 0 ? 2 : at) + step))];
  if (next === m.spanM) return;
  void showMap(m.lat, m.lon, next);
}

// ── Fetching ─────────────────────────────────────────────────────────────────

/** How often the bar is redrawn inside a stage. */
const TICK_MS = 200;

/** Where the bar sits inside a stage, from how long the stage has been going.
 *
 *  The same curve as a track build's, and for the same reason: a stage here is one HTTP
 *  request to somebody else's server, there is nothing finer to report from inside it, and a
 *  bar that sat still for forty seconds would read as a hang. Asymptotic, so it never arrives
 *  early — the stage's end belongs to the next event. */
function easeInto(span: { from: number; to: number; expect: number }, elapsed: number): number {
  const done = 1 - Math.exp((-2 * elapsed) / Math.max(span.expect, 0.5));
  return span.from + (span.to - span.from) * done;
}

let span: { from: number; to: number; expect: number; since: number } | null = null;
let ticker: number | null = null;

function startTicking() {
  if (ticker !== null) return;
  ticker = window.setInterval(() => {
    if (!state.run || !span) return;
    const next = easeInto(span, (Date.now() - span.since) / 1000);
    const to = Math.max(state.run.progress, next);
    // Late in a stage the curve is flat, and a new object per tick would redraw the panel to
    // move the bar by nothing.
    if (to - state.run.progress < 0.0005) return;
    set({ run: { ...state.run, progress: to } });
  }, TICK_MS);
}

function stopTicking() {
  if (ticker !== null) window.clearInterval(ticker);
  ticker = null;
  span = null;
}

/**
 * Fetch the ground and the picture for the place that is picked.
 *
 * Runs to the end whatever the panel does. Nothing here touches a component: what it leaves
 * behind is `fetched`, which whichever panel is on screen picks up and opens for tracing.
 */
export async function fetchGround() {
  const { picked, cover, plot } = state;
  if (!picked || !cover?.best || state.busy !== null) return;
  const name = picked.label.split(",")[0];
  set({
    busy: "fetch",
    fetched: null,
    run: { name, slug: null, stage: "elevation", detail: "", progress: 0 },
  });
  startTicking();

  // Listening is in place before the fetch is asked for, not alongside it: the first stage is
  // reported from inside the call, and `listen` is itself a round trip to the backend.
  const off = await onPlaceFetchProgress((p) => {
    if (!state.run) return;
    if (state.run.slug && p.slug !== state.run.slug) return;
    span = { from: p.from, to: p.to, expect: p.expect, since: Date.now() };
    set({
      run: {
        ...state.run,
        slug: p.slug,
        stage: p.stage,
        detail: p.detail,
        progress: Math.max(state.run.progress, p.from),
      },
    });
  });

  try {
    const place = await fetchPlace(name, picked.lat, picked.lon, plot, cover.best);
    toast.success(say("place.fetched", { name: place.name }));
    set({ fetched: place.slug, map: null, mapError: null });
  } catch (e) {
    toast.error(say("place.fetchFailed"), { description: String(e) });
  } finally {
    off();
    stopTicking();
    set({ busy: null, run: null });
  }
}

/** Take a GeoTIFF the rider downloaded themselves. */
export async function importDem(path: string) {
  set({ busy: "import", fetched: null });
  try {
    const place = await importPlaceDem("", path, "", "");
    toast.success(say("place.imported", { name: place.name }));
    set({ fetched: place.slug });
  } catch (e) {
    toast.error(say("place.importFailed"), { description: String(e) });
  } finally {
    set({ busy: null });
  }
}

/** The place a finished fetch left behind, handed over once. */
export function takeFetched(): string | null {
  const slug = state.fetched;
  if (slug) set({ fetched: null });
  return slug;
}

/** What a place looks like as a search hit, for pointing at one on the map. */
export function hitAt(lat: number, lon: number, label: string, kind: string): PlaceHit {
  return { label, lat, lon, kind };
}

export type { Place };
