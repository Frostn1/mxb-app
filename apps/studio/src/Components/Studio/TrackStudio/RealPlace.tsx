import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import {
  ArrowLeft,
  Crosshair,
  Download,
  Map as MapIcon,
  MapPin,
  Search,
  Trash2,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";

import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import { buildTrack } from "@/api/trackgen";
import {
  forgetPlace,
  humanBytes,
  listPlaces,
  MAP_SPANS_M,
  placeLayers,
  placePaths,
  placeProgram,
  savePlaceTrace,
  traceLength,
  type FetchStage,
  type Place,
  type PlaceLayers,
  type PlaceMap,
} from "@/api/trackplace";
import {
  closeMap,
  fetchGround,
  importDem,
  mapZoom,
  pick,
  search,
  setPlot,
  setQuery,
  showMap,
  snapshot,
  subscribe,
  takeFetched,
  type FetchRun,
} from "./placeFinder";

/**
 * Building a track from a real place: find it, fetch its ground, trace the lap on it.
 *
 * Tracing lives here rather than as a separate tool for one reason. The thing a rider is
 * choosing between is *ribbons of dirt in a photograph*, and the only way to choose well is
 * to see the photograph and the ground's shape at the same time, at whatever zoom the
 * question needs. That is a UI problem, and Studio is where the UI is. A command-line step
 * would have to hand the job back to a human anyway, just with worse tools.
 *
 * Nothing here fetches on its own. Every request is a button.
 */

type Mode = { at: "browse" } | { at: "trace"; slug: string };

/** Which pictures are showing under the lap. */
type LayerView = "imagery" | "hillshade" | "both";

export default function RealPlace({ onClose }: { onClose: () => void }) {
  const [mode, setMode] = useState<Mode>({ at: "browse" });
  const [places, setPlaces] = useState<Place[]>([]);
  const finder = useSyncExternalStore(subscribe, snapshot);

  const refresh = useCallback(() => {
    void listPlaces()
      .then(setPlaces)
      .catch((e) => toast.error(String(e)));
  }, []);

  // Reading the places folder is a local directory listing, not a network call, so it is
  // fine on mount. Nothing here reaches the internet until a button says so.
  useEffect(() => refresh(), [refresh]);

  // A fetch that finished leaves the place behind rather than opening it itself, because it
  // may well have finished while this panel was not on screen. Whoever is here picks it up.
  useEffect(() => {
    if (!finder.fetched) return;
    const slug = takeFetched();
    if (!slug) return;
    refresh();
    setMode({ at: "trace", slug });
  }, [finder.fetched, refresh]);

  if (mode.at === "trace") {
    return (
      <TracePanel
        slug={mode.slug}
        onBack={() => {
          setMode({ at: "browse" });
          refresh();
        }}
      />
    );
  }
  return (
    <BrowsePanel
      places={places}
      onRefresh={refresh}
      onTrace={(slug) => setMode({ at: "trace", slug })}
      onClose={onClose}
    />
  );
}

// ── Finding and fetching ─────────────────────────────────────────────────────

function BrowsePanel({
  places,
  onRefresh,
  onTrace,
  onClose,
}: {
  places: Place[];
  onRefresh: () => void;
  onTrace: (slug: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  // Held outside this component on purpose — see `placeFinder`. A fetch takes minutes and the
  // rider is free to go and look at a lap they traced last week while it runs.
  const { query, hits, picked, cover, plot, busy, run, map, mapError } = useSyncExternalStore(
    subscribe,
    snapshot,
  );

  const onImport = async () => {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "GeoTIFF", extensions: ["tif", "tiff"] }],
    });
    if (typeof path !== "string") return;
    await importDem(path);
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-none items-center gap-2 border-b border-border px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={onClose} disabled={busy === "fetch"}>
          <ArrowLeft className="size-4" />
          {t("place.back")}
        </Button>
        <h2 className="flex-1 text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("place.title")}
        </h2>
        <Button variant="ghost" size="sm" onClick={() => void onImport()} disabled={busy !== null}>
          <Download className="size-4" />
          {busy === "import" ? t("place.importing") : t("place.importDem")}
        </Button>
      </div>

      {/* A fetch is a minute or two of somebody else's server cutting a plot out of a national
          survey. Left silent it reads as a hang, so it says which of the three things it is
          doing and creeps across each of them. */}
      {run && <FetchBar run={run} />}

      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
        <div className="mx-auto w-full max-w-[720px]">
          {/* ── Find it ───────────────────────────────────────────────────── */}
          <label className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("place.searchLabel")}
          </label>
          <div className="mt-2 flex items-center gap-2">
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && query.trim() && void search()}
              placeholder={t("place.searchPlaceholder")}
              className="h-10"
              disabled={busy !== null}
            />
            <Button
              className="h-10 flex-none"
              onClick={() => void search()}
              disabled={!query.trim() || busy !== null}
            >
              <Search className="size-4" />
              {busy === "search" ? t("place.searching") : t("place.search")}
            </Button>
          </div>
          <p className="mt-2 text-[12px] leading-relaxed text-muted-foreground">
            {t("place.searchHint")}
          </p>

          {hits?.length === 0 && (
            <p className="mt-3 text-[12.5px] text-muted-foreground">{t("place.noHits")}</p>
          )}
          {hits && hits.length > 1 && (
            <ol className="mt-3 border border-border">
              {hits.map((h, i) => (
                <li key={i}>
                  <button
                    type="button"
                    onClick={() => void pick(h)}
                    className={cn(
                      "flex w-full cursor-default items-start gap-2 px-3 py-2 text-left transition-colors hover:bg-muted/50",
                      picked === h && "bg-muted/60",
                    )}
                  >
                    <MapPin className="mt-0.5 size-3.5 flex-none text-faint" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[12.5px]">{h.label}</span>
                      <span className="block font-mono text-[10.5px] tabular-figures text-faint">
                        {h.lat.toFixed(5)}, {h.lon.toFixed(5)} · {h.kind}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
            </ol>
          )}

          {/* ── Look at it ────────────────────────────────────────────────── */}
          {picked && (map || mapError) && (
            <MapPicker
              map={map}
              error={mapError}
              busy={busy === "map"}
              onPick={(lat, lon) => void pick({ ...picked, lat, lon })}
            />
          )}

          {/* ── What is actually here ─────────────────────────────────────── */}
          {picked && (
            <div className="mt-5 border border-border p-3">
              <div className="flex items-center gap-2">
                <div className="flex-1 font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                  {t("place.coverageTitle")}
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() =>
                    map ? closeMap() : void showMap(picked.lat, picked.lon, MAP_SPANS_M[2])
                  }
                  disabled={busy !== null}
                >
                  <MapIcon className="size-3.5" />
                  {map ? t("place.mapClose") : t("place.mapOpen")}
                </Button>
              </div>
              <p className="mt-1 font-mono text-[10.5px] tabular-figures text-faint">
                {picked.lat.toFixed(5)}, {picked.lon.toFixed(5)}
              </p>
              {busy === "coverage" && (
                <p className="mt-2 text-[12.5px] text-muted-foreground">{t("place.checking")}</p>
              )}
              {cover && (
                <>
                  <ul className="mt-2 space-y-1.5">
                    {cover.sources.map((s) => (
                      <li key={s.id} className="flex items-start gap-2 text-[12.5px]">
                        <span
                          className={cn(
                            "mt-[3px] size-2 flex-none",
                            !s.covered
                              ? "bg-faint/40"
                              : s.id === cover.best
                                ? "bg-primary"
                                : "bg-muted-foreground/50",
                          )}
                        />
                        <span className="min-w-0 flex-1">
                          <span className="block">
                            {s.label}
                            {s.covered && (
                              <span className="ml-1.5 tabular-figures text-faint">
                                {t("place.cells", { cell: s.cellM.toFixed(s.cellM < 1 ? 1 : 0) })}
                              </span>
                            )}
                          </span>
                          {s.covered && s.dataset && (
                            <span className="block font-mono text-[10.5px] text-faint">
                              {s.dataset}
                              {s.collected ? ` · ${s.collected}` : ""}
                            </span>
                          )}
                          {s.note && (
                            <span
                              className={cn(
                                "block text-[11.5px] leading-snug",
                                s.covered ? "text-amber-500" : "text-faint",
                              )}
                            >
                              {s.note}
                            </span>
                          )}
                          <span className="block text-[10.5px] text-faint">{s.licence}</span>
                        </span>
                      </li>
                    ))}
                  </ul>

                  {/* ── How big a plot ──────────────────────────────────── */}
                  <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-border pt-3">
                    <label className="text-[12px] text-muted-foreground">{t("place.plot")}</label>
                    <Input
                      type="number"
                      min={100}
                      max={2000}
                      step={10}
                      value={plot}
                      onChange={(e) => setPlot(Number(e.target.value) || 1200)}
                      className="h-8 w-[90px] tabular-figures"
                      disabled={busy !== null}
                    />
                    <span className="text-[12px] text-faint">m</span>
                    <Button
                      className="ml-auto h-8"
                      onClick={() => void fetchGround()}
                      disabled={!cover.best || busy !== null}
                    >
                      {busy === "fetch" ? t("place.fetching") : t("place.fetch")}
                    </Button>
                  </div>
                  <p className="mt-2 text-[11.5px] leading-relaxed text-muted-foreground">
                    {t("place.plotHint")}
                  </p>
                </>
              )}
            </div>
          )}

          {/* ── What is already here ──────────────────────────────────────── */}
          <div className="mt-7">
            <div className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
              {t("place.places")}
            </div>
            {places.length === 0 ? (
              <p className="mt-2 text-[12.5px] text-muted-foreground">{t("place.noPlaces")}</p>
            ) : (
              <ol className="mt-2 border border-border">
                {places.map((p) => (
                  <PlaceRow key={p.slug} place={p} onTrace={onTrace} onRefresh={onRefresh} />
                ))}
              </ol>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

/** What each stage of a fetch is called on screen. */
const STAGE_KEY: Record<FetchStage, "place.stageElevation" | "place.stageHillshade" | "place.stageImagery"> = {
  elevation: "place.stageElevation",
  hillshade: "place.stageHillshade",
  imagery: "place.stageImagery",
};

/** The bar across the top of the panel while a fetch is running. */
function FetchBar({ run }: { run: FetchRun }) {
  const t = useT();
  return (
    <div className="flex-none border-b border-border px-4 py-2">
      <div className="mx-auto w-full max-w-[720px]">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-[12px]">{t("place.fetchingName", { name: run.name })}</span>
          <span className="ml-auto truncate text-[11.5px] text-muted-foreground">
            {t(STAGE_KEY[run.stage])}
            {run.detail ? ` · ${run.detail}` : ""}
          </span>
          <span className="tabular-figures font-cond text-[11px] text-faint">
            {Math.round(run.progress * 100)}%
          </span>
        </div>
        <div className="mt-1.5 h-1 w-full bg-muted">
          <div
            className="h-full bg-primary transition-[width] duration-200 ease-linear"
            style={{ width: `${Math.round(run.progress * 100)}%` }}
          />
        </div>
      </div>
    </div>
  );
}

// ── Picking a circuit off a photograph ───────────────────────────────────────

/**
 * An aerial photograph you can point at, instead of a name you have to spell.
 *
 * Searching "Ironman" gives nine places in five countries with the circuit seventh, and
 * searching "Saint-Jean" gives the town while the circuit is kilometres out of it — both cost
 * a fetch and a build before anybody finds out. A rider knows a circuit the moment they see it
 * from above, so this shows them that.
 *
 * There is no map of the world behind it, because there is no map of the world we are allowed
 * to use: every worldwide aerial basemap sharp enough to pick a circuit out of is licensed for
 * viewing inside its owner's own product. So this shows the openly licensed surveys and says
 * so plainly where there are none, rather than filling the space with grey.
 *
 * Every picture is one request a rider asked for. Moving, zooming and clicking each spend
 * exactly one; nothing is fetched ahead, and nothing is fetched at all until the map is opened.
 */
function MapPicker({
  map,
  error,
  busy,
  onPick,
}: {
  map: PlaceMap | null;
  error: string | null;
  busy: boolean;
  onPick: (lat: number, lon: number) => void;
}) {
  const t = useT();
  const [drag, setDrag] = useState<{ x: number; y: number; dx: number; dy: number } | null>(null);

  if (!map) {
    return (
      <div className="mt-5 border border-border p-3">
        <p className="text-[12.5px] leading-relaxed text-amber-500">{error}</p>
        <Button variant="ghost" size="sm" className="mt-2" onClick={closeMap}>
          {t("place.mapClose")}
        </Button>
      </div>
    );
  }

  /** A point in the picture, turned back into a place on the earth. */
  const toLatLon = (el: HTMLElement, clientX: number, clientY: number) => {
    const box = el.getBoundingClientRect();
    const perPx = map.spanM / box.width;
    const east = (clientX - box.left - box.width / 2) * perPx;
    const north = -(clientY - box.top - box.height / 2) * perPx;
    const dLat = 1 / 111_320;
    const dLon = 1 / (111_320 * Math.max(Math.cos((map.lat * Math.PI) / 180), 0.05));
    return [map.lat + north * dLat, map.lon + east * dLon] as const;
  };

  return (
    <div className="mt-5 border border-border">
      <div className="flex flex-wrap items-center gap-2 border-b border-border px-3 py-2">
        <div className="flex-1 font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
          {t("place.mapTitle")}
        </div>
        <span className="tabular-figures font-cond text-[11px] text-faint">
          {t("place.mapSpan", { span: String(map.spanM) })}
        </span>
        <Button variant="ghost" size="sm" onClick={() => mapZoom(-1)} disabled={busy}>
          <ZoomIn className="size-3.5" />
          {t("place.mapIn")}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => mapZoom(1)} disabled={busy}>
          <ZoomOut className="size-3.5" />
          {t("place.mapOut")}
        </Button>
        <Button variant="ghost" size="sm" onClick={closeMap} disabled={busy}>
          {t("place.mapClose")}
        </Button>
      </div>

      <div className="relative select-none overflow-hidden bg-black">
        <img
          src={map.image}
          alt=""
          draggable={false}
          className={cn("block w-full cursor-crosshair", busy && "opacity-50")}
          style={drag ? { transform: `translate(${drag.dx}px, ${drag.dy}px)` } : undefined}
          onMouseDown={(ev) => {
            ev.preventDefault();
            setDrag({ x: ev.clientX, y: ev.clientY, dx: 0, dy: 0 });
          }}
          onMouseMove={(ev) =>
            setDrag((d) => (d ? { ...d, dx: ev.clientX - d.x, dy: ev.clientY - d.y } : d))
          }
          onMouseLeave={() => setDrag(null)}
          onMouseUp={(ev) => {
            const started = drag;
            setDrag(null);
            if (busy) return;
            const el = ev.currentTarget;
            const moved = started ? Math.hypot(started.dx, started.dy) : 0;
            // A drag moves the photograph under the middle; a click moves the middle to
            // where you clicked. Either way it is one request, and only on letting go.
            if (moved > 4 && started) {
              const box = el.getBoundingClientRect();
              const [lat, lon] = toLatLon(
                el,
                box.left + box.width / 2 - started.dx,
                box.top + box.height / 2 - started.dy,
              );
              void showMap(lat, lon, map.spanM);
              return;
            }
            const [lat, lon] = toLatLon(el, ev.clientX, ev.clientY);
            void showMap(lat, lon, map.spanM);
          }}
        />
        {/* Where the plot would be centred, which is the middle of the picture. */}
        <div className="pointer-events-none absolute left-1/2 top-1/2 size-5 -translate-x-1/2 -translate-y-1/2">
          <div className="absolute left-1/2 top-0 h-full w-px -translate-x-1/2 bg-amber-400" />
          <div className="absolute left-0 top-1/2 h-px w-full -translate-y-1/2 bg-amber-400" />
        </div>
        {busy && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <span className="bg-black/70 px-2 py-1 text-[11.5px] text-white">
              {t("place.mapLoading")}
            </span>
          </div>
        )}
      </div>

      <div className="px-3 py-2">
        <div className="flex flex-wrap items-center gap-2">
          <p className="min-w-0 flex-1 text-[11.5px] leading-relaxed text-muted-foreground">
            {t("place.mapHint")}
          </p>
          <Button size="sm" onClick={() => onPick(map.lat, map.lon)} disabled={busy}>
            {t("place.mapUse")}
          </Button>
        </div>
        <p className="mt-1 text-[10.5px] leading-relaxed text-faint">
          {t("place.attribution")}: {map.attribution}
        </p>
      </div>
    </div>
  );
}

function PlaceRow({
  place,
  onTrace,
  onRefresh,
}: {
  place: Place;
  onTrace: (slug: string) => void;
  onRefresh: () => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);

  const onForget = async () => {
    setBusy(true);
    try {
      await forgetPlace(place.slug);
      onRefresh();
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  };

  const onCopy = async () => {
    try {
      const paths = await placePaths(place.slug);
      await navigator.clipboard.writeText(`${paths.dem}\n${paths.trace}`);
      toast.success(t("place.copied"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <li className="flex items-center gap-3 border-b border-border px-3 py-2 last:border-b-0">
      <div className="min-w-0 flex-1">
        <div className="truncate text-[12.5px]">
          {place.name}
          {place.hasTrace && (
            <span className="ml-2 font-cond text-[10px] uppercase tracking-[0.12em] text-primary">
              {t("place.hasLap")}
            </span>
          )}
        </div>
        <div className="truncate font-mono text-[10.5px] tabular-figures text-faint">
          {t("place.cells", { cell: place.dem.cellM.toFixed(place.dem.cellM < 1 ? 1 : 0) })} ·{" "}
          {t("place.relief", { relief: (place.dem.maxZ - place.dem.minZ).toFixed(1) })} ·{" "}
          {humanBytes(place.bytes)}
        </div>
        <div className="truncate text-[10.5px] text-faint">{place.dem.source}</div>
      </div>
      <Button variant="outline" size="sm" onClick={() => onTrace(place.slug)} disabled={busy}>
        <Crosshair className="size-3.5" />
        {t("place.trace")}
      </Button>
      <Button variant="ghost" size="sm" onClick={() => void onCopy()} disabled={busy}>
        {t("place.copyPaths")}
      </Button>
      <Button variant="ghost" size="sm" onClick={() => void onForget()} disabled={busy}>
        <Trash2 className="size-3.5" />
        {busy ? t("place.forgetting") : t("place.forget")}
      </Button>
    </li>
  );
}

// ── Tracing ──────────────────────────────────────────────────────────────────

function TracePanel({ slug, onBack }: { slug: string; onBack: () => void }) {
  const t = useT();
  const [layers, setLayers] = useState<PlaceLayers | null>(null);
  const [view, setView] = useState<LayerView>("imagery");
  const [pts, setPts] = useState<number[][]>([]);
  const [closed, setClosed] = useState(true);
  const [width, setWidth] = useState(6);
  const [startIndex, setStartIndex] = useState(0);
  const [saving, setSaving] = useState(false);
  const [building, setBuilding] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [drag, setDrag] = useState<number | null>(null);
  // Every edit the rider can make, so the keyboard can walk back through them. The button
  // used to drop the last point, which is not undo: it could not take back a move or a
  // removal, which are the edits most likely to be a slip.
  const [past, setPast] = useState<number[][][]>([]);
  const [future, setFuture] = useState<number[][][]>([]);
  const boxRef = useRef<HTMLDivElement>(null);
  // A drag ends with a mouse-up, and the browser turns that into a click on the canvas —
  // which used to drop a new point under the one just moved. Set while dragging and cleared
  // by the click it causes.
  const draggedRef = useRef(false);

  useEffect(() => {
    void placeLayers(slug)
      .then((l) => {
        setLayers(l);
        if (!l.imagery) setView("hillshade");
        if (l.trace) {
          setPts(l.trace.points);
          setClosed(l.trace.closed);
          setWidth(l.trace.defaultWidthM);
          setStartIndex(l.trace.startIndex ?? 0);
        }
      })
      .catch((e) => toast.error(String(e)));
  }, [slug]);

  const dem = layers?.dem;

  /** A click in the picture, turned back into a place on the earth. */
  const toWorld = useCallback(
    (col: number, row: number): [number, number] => {
      if (!dem) return [0, 0];
      return [dem.originE + col * dem.cellM, dem.originN - row * dem.cellM];
    },
    [dem],
  );

  /** And back the other way, to draw the lap where it belongs. */
  const toPixel = useCallback(
    (e: number, n: number): [number, number] => {
      if (!dem) return [0, 0];
      return [(e - dem.originE) / dem.cellM, (dem.originN - n) / dem.cellM];
    },
    [dem],
  );

  const length = useMemo(() => traceLength(pts, closed), [pts, closed]);

  const eventToCell = (ev: React.MouseEvent): [number, number] | null => {
    const svg = ev.currentTarget as SVGSVGElement;
    const rect = svg.getBoundingClientRect();
    if (!dem || rect.width === 0) return null;
    const col = ((ev.clientX - rect.left) / rect.width) * dem.width;
    const row = ((ev.clientY - rect.top) / rect.height) * dem.height;
    return [col, row];
  };

  /** Change the lap and remember what it was, so the change can be taken back. */
  const edit = useCallback((next: (p: number[][]) => number[][]) => {
    setPts((p) => {
      const after = next(p);
      if (after === p) return p;
      setPast((h) => [...h.slice(-199), p]);
      setFuture([]);
      return after;
    });
  }, []);

  const undo = useCallback(() => {
    setPast((h) => {
      if (h.length === 0) return h;
      const prev = h[h.length - 1];
      setPts((cur) => {
        setFuture((f) => [cur, ...f.slice(0, 199)]);
        return prev;
      });
      return h.slice(0, -1);
    });
  }, []);

  const redo = useCallback(() => {
    setFuture((f) => {
      if (f.length === 0) return f;
      const nextPts = f[0];
      setPts((cur) => {
        setPast((h) => [...h.slice(-199), cur]);
        return nextPts;
      });
      return f.slice(1);
    });
  }, []);

  // Cmd-Z on a Mac, Ctrl-Z elsewhere, with Shift for redo — what every drawing tool does.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== "z") return;
      const el = document.activeElement;
      // Never steal undo from a field the rider is typing in.
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement) return;
      e.preventDefault();
      if (e.shiftKey) redo();
      else undo();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  const onCanvasClick = (ev: React.MouseEvent) => {
    if (drag !== null) return;
    if (draggedRef.current) {
      draggedRef.current = false;
      return;
    }
    const cell = eventToCell(ev);
    if (!cell) return;
    edit((p) => [...p, toWorld(cell[0], cell[1])]);
  };

  const onPointDown = (ev: React.MouseEvent, i: number) => {
    ev.stopPropagation();
    // Alt-click removes, which keeps the right mouse button free for panning.
    if (ev.altKey) {
      edit((p) => p.filter((_, j) => j !== i));
      setStartIndex((s) => (s >= i && s > 0 ? s - 1 : s));
      return;
    }
    // One history step per drag, taken now — a drag fires on every mouse move, and a step
    // per pixel would make undo useless.
    setPast((h) => [...h.slice(-199), pts]);
    setFuture([]);
    draggedRef.current = true;
    setDrag(i);
  };

  const onCanvasMove = (ev: React.MouseEvent) => {
    if (drag === null) return;
    const cell = eventToCell(ev);
    if (!cell) return;
    const w = toWorld(cell[0], cell[1]);
    setPts((p) => p.map((q, j) => (j === drag ? [w[0], w[1], ...q.slice(2)] : q)));
  };

  /**
   * Save the lap, turn the place into a programme, and build it — the whole way from a traced
   * picture to a track the game lists, without leaving this panel.
   */
  const onBuild = async () => {
    setBuilding(true);
    try {
      await savePlaceTrace(slug, pts, closed, width, startIndex);
      const program = await placeProgram(slug, false);
      const built = await buildTrack(program, null, true);
      toast.success(t("place.built", { name: program.name }), {
        description: built.installed ?? undefined,
      });
    } catch (e) {
      toast.error(t("place.buildFailed"), { description: String(e) });
    } finally {
      setBuilding(false);
    }
  };

  const onSave = async () => {
    setSaving(true);
    try {
      const path = await savePlaceTrace(slug, pts, closed, width, startIndex);
      toast.success(t("place.saved"), { description: path });
    } catch (e) {
      toast.error(t("place.saveFailed"), { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const polyline = useMemo(() => {
    if (!dem || pts.length === 0) return "";
    const px = pts.map((p) => toPixel(p[0], p[1]));
    const d = px.map(([c, r]) => `${c.toFixed(2)},${r.toFixed(2)}`).join(" ");
    return closed && px.length > 2 ? `${d} ${px[0][0].toFixed(2)},${px[0][1].toFixed(2)}` : d;
  }, [pts, closed, dem, toPixel]);

  if (!layers || !dem) {
    return (
      <div className="flex h-full items-center justify-center text-[12.5px] text-muted-foreground">
        {t("place.opening")}
      </div>
    );
  }

  const views: { value: LayerView; label: string }[] = [
    ...(layers.imagery ? [{ value: "imagery" as const, label: t("place.layerImagery") }] : []),
    { value: "hillshade", label: t("place.layerHillshade") },
    ...(layers.imagery ? [{ value: "both" as const, label: t("place.layerBoth") }] : []),
  ];

  return (
    /* Three rows that cannot squeeze each other out: the tools, the picture, and the notes.
       The toolbar used to wrap — on a modest window it took four rows, and between it and the
       credits there was more chrome than panel, so the help text and the attribution fell off
       the bottom of the window where nobody could read them. It is one row now and scrolls
       sideways if it has to; the notes are capped and scroll on their own. The picture, which
       is the only thing here with an appetite, gives up whatever is left. */
    <div className="grid h-full min-h-0 grid-rows-[auto_minmax(0,1fr)_auto] overflow-hidden">
      <div className="flex flex-nowrap items-center gap-2 overflow-x-auto border-b border-border px-4 py-2.5 [&>*]:shrink-0">
        <Button variant="ghost" size="sm" onClick={onBack} disabled={saving}>
          <ArrowLeft className="size-4" />
          {t("place.back")}
        </Button>
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {layers.name}
        </h2>
        <Segmented size="sm" options={views} value={view} onChange={setView} />
        <div className="flex items-center gap-1.5">
          <Switch checked={closed} onCheckedChange={setClosed} />
          <span className="text-[12px] text-muted-foreground">{t("place.closed")}</span>
        </div>
        <div className="flex items-center gap-1.5">
          <span className="text-[12px] text-muted-foreground">{t("place.width")}</span>
          <Input
            type="number"
            min={1}
            max={30}
            step={0.5}
            value={width}
            onChange={(e) => setWidth(Number(e.target.value) || 6)}
            className="h-8 w-[70px] tabular-figures"
          />
        </div>
        <span className="tabular-figures font-cond text-[11px] text-faint">
          {t("place.points", { count: pts.length })} · {t("place.length", { length: length.toFixed(0) })}
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="sm"
            onClick={undo}
            disabled={past.length === 0}
          >
            <Undo2 className="size-3.5" />
            {t("place.undo")}
          </Button>
          <Button variant="ghost" size="sm" onClick={redo} disabled={future.length === 0}>
            {t("place.redo")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => edit(() => [])} disabled={pts.length === 0}>
            {t("place.clear")}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => void onSave()}
            disabled={pts.length < 3 || saving || building}
          >
            {saving ? t("place.saving") : t("place.saveTrace")}
          </Button>
          <Button size="sm" onClick={() => void onBuild()} disabled={pts.length < 3 || building}>
            {building ? t("place.building") : t("place.build")}
          </Button>
        </div>
      </div>

      <div
        ref={boxRef}
        className="relative min-h-0 overflow-hidden bg-black"
        onWheel={(ev) => {
          const next = Math.min(12, Math.max(1, zoom * (ev.deltaY < 0 ? 1.15 : 1 / 1.15)));
          setZoom(next);
        }}
        onMouseDown={(ev) => {
          // The right button pans, which leaves the left one free to draw.
          if (ev.button !== 2) return;
          ev.preventDefault();
          const from = { x: ev.clientX, y: ev.clientY };
          const was = { ...pan };
          const move = (m: MouseEvent) =>
            setPan({ x: was.x + (m.clientX - from.x), y: was.y + (m.clientY - from.y) });
          const up = () => {
            window.removeEventListener("mousemove", move);
            window.removeEventListener("mouseup", up);
          };
          window.addEventListener("mousemove", move);
          window.addEventListener("mouseup", up);
        }}
        onContextMenu={(ev) => ev.preventDefault()}
      >
        <div
          className="absolute left-1/2 top-1/2 origin-center"
          style={{
            transform: `translate(-50%, -50%) translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
            width: "min(100%, 100vh)",
            aspectRatio: `${dem.width} / ${dem.height}`,
          }}
        >
          {layers.imagery && view !== "hillshade" && (
            <img
              src={layers.imagery}
              alt=""
              className="absolute inset-0 size-full object-fill"
              draggable={false}
            />
          )}
          {(view === "hillshade" || view === "both") && (
            <img
              src={layers.hillshade}
              alt=""
              className={cn(
                "absolute inset-0 size-full object-fill",
                view === "both" && "opacity-55 mix-blend-luminosity",
              )}
              draggable={false}
            />
          )}
          <svg
            viewBox={`0 0 ${dem.width} ${dem.height}`}
            className="absolute inset-0 size-full cursor-crosshair"
            onClick={onCanvasClick}
            onMouseMove={onCanvasMove}
            onMouseUp={() => setDrag(null)}
          >
            {pts.length > 1 && (
              <polyline
                points={polyline}
                fill="none"
                stroke="#22d3ee"
                strokeWidth={Math.max(dem.width / 400, 0.8)}
                strokeLinejoin="round"
                vectorEffect="non-scaling-stroke"
                style={{ strokeWidth: 2.5 }}
              />
            )}
            {pts.map((p, i) => {
              const [c, r] = toPixel(p[0], p[1]);
              return (
                <circle
                  key={i}
                  cx={c}
                  cy={r}
                  r={dem.width / 180 / Math.sqrt(zoom)}
                  fill={i === startIndex ? "#f59e0b" : "#22d3ee"}
                  stroke="#0b0b0b"
                  strokeWidth={dem.width / 900}
                  className="cursor-grab"
                  onMouseDown={(ev) => onPointDown(ev, i)}
                  onDoubleClick={(ev) => {
                    ev.stopPropagation();
                    setStartIndex(i);
                  }}
                />
              );
            })}
          </svg>
        </div>
      </div>

      <div className="max-h-[38%] overflow-y-auto border-t border-border px-4 py-2">
        <p className="text-[11.5px] leading-relaxed text-muted-foreground">
          {t("place.traceHelp")}
        </p>
        {!layers.imagery && (
          <p className="mt-1 text-[11.5px] leading-relaxed text-amber-500">
            {t("place.noImagery")}
          </p>
        )}
        {layers.attributions.length > 0 && (
          <p className="mt-1 text-[10.5px] leading-relaxed text-faint">
            {t("place.attribution")}: {layers.attributions.join(" · ")}
          </p>
        )}
      </div>
    </div>
  );
}
