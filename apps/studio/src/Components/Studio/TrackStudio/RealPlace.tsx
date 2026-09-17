import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { ArrowLeft, Crosshair, Download, MapPin, Search, Trash2, Undo2 } from "lucide-react";

import { Button } from "@frost/shared/Components/ui/button";
import { Input } from "@frost/shared/Components/ui/input";
import { Switch } from "@frost/shared/Components/ui/switch";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import {
  fetchPlace,
  findPlace,
  forgetPlace,
  humanBytes,
  importPlaceDem,
  listPlaces,
  placeCoverage,
  placeLayers,
  placePaths,
  savePlaceTrace,
  traceLength,
  type CoverageReport,
  type Place,
  type PlaceHit,
  type PlaceLayers,
} from "@/api/trackplace";

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

  const refresh = useCallback(() => {
    void listPlaces()
      .then(setPlaces)
      .catch((e) => toast.error(String(e)));
  }, []);

  // Reading the places folder is a local directory listing, not a network call, so it is
  // fine on mount. Nothing here reaches the internet until a button says so.
  useEffect(() => refresh(), [refresh]);

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
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<PlaceHit[] | null>(null);
  const [picked, setPicked] = useState<PlaceHit | null>(null);
  const [cover, setCover] = useState<CoverageReport | null>(null);
  // A full lap is normally 2000 m or more, which does not fit a 470 m plot. The default
  // here is the one a real circuit actually needs, not the generator's out-of-the-box size.
  const [plot, setPlot] = useState(1200);
  const [busy, setBusy] = useState<string | null>(null);

  const onSearch = async () => {
    setBusy("search");
    setHits(null);
    setPicked(null);
    setCover(null);
    try {
      const found = await findPlace(query);
      setHits(found);
      if (found.length === 1) void onPick(found[0]);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const onPick = async (hit: PlaceHit) => {
    setPicked(hit);
    setCover(null);
    setBusy("coverage");
    try {
      setCover(await placeCoverage(hit.lat, hit.lon));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  };

  const onFetch = async () => {
    if (!picked || !cover?.best) return;
    setBusy("fetch");
    try {
      const place = await fetchPlace(
        picked.label.split(",")[0],
        picked.lat,
        picked.lon,
        plot,
        cover.best,
      );
      toast.success(t("place.fetched", { name: place.name }));
      onRefresh();
      onTrace(place.slug);
    } catch (e) {
      toast.error(t("place.fetchFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const onImport = async () => {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "GeoTIFF", extensions: ["tif", "tiff"] }],
    });
    if (typeof path !== "string") return;
    setBusy("import");
    try {
      const place = await importPlaceDem("", path, "", "");
      toast.success(t("place.imported", { name: place.name }));
      onRefresh();
      onTrace(place.slug);
    } catch (e) {
      toast.error(t("place.importFailed"), { description: String(e) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-none items-center gap-2 border-b border-border px-4 py-2.5">
        <Button variant="ghost" size="sm" onClick={onClose} disabled={busy !== null}>
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
              onKeyDown={(e) => e.key === "Enter" && query.trim() && void onSearch()}
              placeholder={t("place.searchPlaceholder")}
              className="h-10"
              disabled={busy !== null}
            />
            <Button
              className="h-10 flex-none"
              onClick={() => void onSearch()}
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
                    onClick={() => void onPick(h)}
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

          {/* ── What is actually here ─────────────────────────────────────── */}
          {picked && (
            <div className="mt-5 border border-border p-3">
              <div className="font-cond text-[10px] font-semibold uppercase tracking-[0.22em] text-faint">
                {t("place.coverageTitle")}
              </div>
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
                      onClick={() => void onFetch()}
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
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [drag, setDrag] = useState<number | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);

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

  const onCanvasClick = (ev: React.MouseEvent) => {
    if (drag !== null) return;
    const cell = eventToCell(ev);
    if (!cell) return;
    setPts((p) => [...p, toWorld(cell[0], cell[1])]);
  };

  const onPointDown = (ev: React.MouseEvent, i: number) => {
    ev.stopPropagation();
    // Alt-click removes, which keeps the right mouse button free for panning.
    if (ev.altKey) {
      setPts((p) => p.filter((_, j) => j !== i));
      setStartIndex((s) => (s >= i && s > 0 ? s - 1 : s));
      return;
    }
    setDrag(i);
  };

  const onCanvasMove = (ev: React.MouseEvent) => {
    if (drag === null) return;
    const cell = eventToCell(ev);
    if (!cell) return;
    const w = toWorld(cell[0], cell[1]);
    setPts((p) => p.map((q, j) => (j === drag ? [w[0], w[1], ...q.slice(2)] : q)));
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
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-none flex-wrap items-center gap-2 border-b border-border px-4 py-2.5">
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
            onClick={() => setPts((p) => p.slice(0, -1))}
            disabled={pts.length === 0}
          >
            <Undo2 className="size-3.5" />
            {t("place.undo")}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setPts([])} disabled={pts.length === 0}>
            {t("place.clear")}
          </Button>
          <Button size="sm" onClick={() => void onSave()} disabled={pts.length < 3 || saving}>
            {saving ? t("place.saving") : t("place.saveTrace")}
          </Button>
        </div>
      </div>

      <div
        ref={boxRef}
        className="relative min-h-0 flex-1 overflow-hidden bg-black"
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

      <div className="flex-none border-t border-border px-4 py-2">
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
