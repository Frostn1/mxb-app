import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Bike,
  Eye,
  EyeOff,
  FilePlus2,
  Grid3x3,
  Layers as LayersIcon,
  Loader2,
  PackageOpen,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Save,
  Trash2,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import * as THREE from "three";
import { cn } from "@/lib/utils";
import { Button } from "../../ui/button";
import { Input } from "../../ui/input";
import {
  paintStudioExtract,
  paintStudioPixels,
  paintStudioSave,
  paintStudioStage,
  paintStudioTarget,
  textureBytes,
} from "../../../api/mods";
import { useT } from "../../../i18n/context";
import { IMAGE_EXTS, PaintDestBar, isBikeKind, usePaintDest } from "../paintDest";
import { CanvasStage } from "./CanvasStage";
import { Row, Slider } from "./controls";
import { PreviewPanel } from "./PreviewPanel";
import { LayerInspector } from "./LayerInspector";
import { PaintTools } from "./PaintTools";
import { bitmapFromRgba, composite, hasInk, sheetTexture, toPng } from "./composite";
import { EMPTY_GHOST, ghostShows, type Ghost } from "./ghost";
import {
  islandAt,
  partAt,
  partBox,
  partPath,
  triangleAt,
  uvParts,
  uvWireframe,
  type UvPart,
} from "./uv";
import {
  blankSheet,
  imageLayer,
  isCompanionMap,
  layerExtent,
  newId,
  paintLayer,
  shapeLayer,
  textLayer,
  unionRegion,
  type Layer,
  type PaintLayer,
  type ShapeLayer,
  type Region,
  type Sheet,
} from "./layers";
import {
  DEFAULT_PAINT,
  PaintHistory,
  SHAPE_TOOLS,
  Stroke,
  TOOL_KEYS,
  constrained,
  type PaintSettings,
  type PaintTool,
  type Point,
} from "./paint";
import type { EdfNode, PaintTexture } from "../../../types";

/**
 * The paint designer: layers on a sheet, the sheet on the model, and a `.pnt` at the end.
 *
 * MX Bikes paints are drawn as flat texture sheets and worn on curved geometry, which is why
 * doing this in an image editor is guesswork — you move a logo, save, pack, launch, look, and
 * move it again. Everything here exists to close that loop: the sheet you're drawing *is* the
 * texture on the model beside it, and Save writes the packed file the game reads rather than
 * an export somebody else's tool has to convert.
 *
 * There's a brush too, and a gradient, and shapes — see `paint.ts`. They paint into a layer of
 * their own rather than into the sheet, so the template underneath survives every stroke and
 * the whole tool kit inherits opacity, blending and stacking from the layer system for free.
 *
 * What it deliberately still isn't: a general image editor. No selections, no filters, no
 * masks. It draws liveries, and it knows where they go.
 */

/** A blank sheet's edge. Powers of two only — the backend would resize anything else. */
const BLANK_SIZE = 2048;

interface DesignerProps {
  /**
   * Sheets handed over from Paint Studio, by path — drawn on rather than replaced.
   *
   * Consumed once and cleared by `onIncomingLoaded`, so coming back to this tab later doesn't
   * silently throw away whatever has been drawn since.
   */
  incoming?: string[] | null;
  onIncomingLoaded?: () => void;
}

export default function Designer({ incoming, onIncomingLoaded }: DesignerProps) {
  const t = useT();
  const [sheets, setSheets] = useState<Sheet[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  // The sheets/layers rail folds away, because once a paint is set up the thing worth the
  // width is the canvas and the model — not the list of what you already chose.
  const [railOpen, setRailOpen] = useState(true);
  // One bump per change to any sheet's pixels. The canvas stage and the 3D preview both
  // follow it rather than trying to work out for themselves what a "change" is.
  const [version, setVersion] = useState(0);
  const [paint, setPaint] = useState<PaintSettings>(DEFAULT_PAINT);
  // Painting history, and a counter to bring its undo/redo buttons back into a render — the
  // stack itself is a mutable object, so nothing about it would reach React on its own.
  const history = useRef(new PaintHistory<Sheet[]>());
  const [historyRev, setHistoryRev] = useState(0);
  // The stroke in progress. A ref because it changes on every pointer sample and no render
  // depends on it — the pixels it writes are what reach the screen.
  const stroke = useRef<Stroke | null>(null);
  // The pane stays mounted while another Studio tab is on screen, so the keyboard shortcuts
  // need a way to tell whether they're the ones being typed at.
  const rootRef = useRef<HTMLDivElement>(null);

  /**
   * Reference underlays, by sheet id — deliberately *beside* the sheets rather than inside them.
   *
   * A ghost is something to look at while drawing, not part of what is drawn, and keeping it
   * out of `Sheet` means two things at once: the save path has nothing to filter out, and
   * fading one in and out doesn't count as a change to the sheet, so it never triggers the
   * recomposite that every real edit does.
   */
  const [ghosts, setGhosts] = useState<Map<string, Ghost>>(new Map());
  // The mesh the preview is showing, reported back by it. Null until one loads, and null again
  // if it fails — a UV map drawn from a model that isn't on screen would be a confident lie.
  const [geometry, setGeometry] = useState<EdfNode[] | null>(null);
  // Whether that mesh was assembled about the bike's mirror plane. Without it a position is a
  // number in some part's own frame, and the sides and facings read off it would be invented.
  const [assembled, setAssembled] = useState(false);
  // That same model's own textures — the look it ships with. Empty for anything that can't
  // say which of its textures are its own, which is every model but a bike.
  const [stockTextures, setStockTextures] = useState<PaintTexture[]>([]);

  const destState = usePaintDest();
  const { dest, hints } = destState;

  // Composites, one per sheet, owned here and reused: they're the size of the sheet, and
  // reallocating a 4096² canvas on every pointer move is not a thing to do.
  const canvases = useRef(new Map<string, HTMLCanvasElement>());
  // The textures wrapping those canvases, handed to the viewer. Same lifetime, same owner.
  const textures = useRef(new Map<string, THREE.DataTexture>());
  // The sheet object each canvas was last drawn from, so an untouched sheet isn't redrawn.
  const drawn = useRef(new Map<string, Sheet>());
  /**
   * What has changed on each stale sheet since its canvas was last drawn.
   *
   * An entry of `null` means "somewhere, unspecified" and buys nothing — the recomposite falls
   * back to the whole sheet, which is what it always did. A region is a promise that nothing
   * outside it moved, and only a stroke is in a position to make that promise, because only a
   * stroke knows where its own pixels went. Everything else goes through `patchSheet`, which
   * says `null` on the way past.
   *
   * Cleared by the recomposite that consumes it: a region held over from a redraw that already
   * happened would describe the wrong sheet by the time the next one came round.
   */
  const dirty = useRef(new Map<string, Region | null>());
  // The live map the viewer reads, plus the only thing allowed to change its identity.
  const overridesRef = useRef(new Map<string, THREE.Texture>());
  const [overrideNames, setOverrideNames] = useState("");
  // What was last published as `overrideNames`. Held beside the state so the recomposite below
  // can tell whether it has anything to say *before* saying it — see the note there.
  const publishedNames = useRef("");

  // The sheets as they stand, for the undo stack to snapshot on its way past an edit. A mirror
  // rather than a read of `sheets`: the callbacks that mutate are memoised on their own
  // dependencies, and one holding a stale array would remember a state that had already moved on.
  const sheetsRef = useRef(sheets);
  sheetsRef.current = sheets;

  const active = sheets.find((s) => s.id === activeId) ?? null;
  const bump = useCallback(() => setVersion((v) => v + 1), []);

  /**
   * The sheets that exist, as a string.
   *
   * A stroke replaces the sheet it touches on every pointer sample, so `sheets` is a new array
   * a hundred times a second while the brush is down — and an effect that follows it re-runs
   * just as often. The housekeeping below only cares about *which sheets there are*, which a
   * stroke never changes, so it follows this instead.
   */
  const sheetIdKey = useMemo(() => sheets.map((s) => s.id).join(" "), [sheets]);

  const canvasFor = useCallback((sheet: Sheet) => {
    let canvas = canvases.current.get(sheet.id);
    if (!canvas) {
      canvas = document.createElement("canvas");
      canvases.current.set(sheet.id, canvas);
    }
    return canvas;
  }, []);

  /**
   * Recomposite what changed and republish the override map.
   *
   * A **layout** effect, not a passive one, and that isn't a detail: React runs a child's
   * effects before its parent's, so with `useEffect` the canvas stage would blit the composite
   * *before* this redrew it — every drag would render one frame behind the pointer. Layout
   * effects run parent-last but ahead of every passive effect, which puts the redraw back in
   * front of the blit.
   *
   * Sheets are compared by identity: an edit replaces exactly the sheet it touched, so this
   * redraws one 2048² canvas per pointer move rather than all of them. The texture behind it
   * follows the same test rather than being rebuilt every pass — reading a canvas back is the
   * most expensive thing in here, and re-reading the three sheets a stroke did not touch was
   * paying that price for pixels that were identical to the ones already uploaded.
   *
   * What did change is redrawn and read back across the region the stroke reported, which is
   * the difference between a stamp's worth of work per sample and a sheet's worth.
   */
  useLayoutEffect(() => {
    const next = new Map<string, THREE.Texture>();
    for (const sheet of sheets) {
      const canvas = canvasFor(sheet);
      let tex = textures.current.get(sheet.id) ?? null;
      if (drawn.current.get(sheet.id) !== sheet) {
        const area = composite(canvas, sheet, dirty.current.get(sheet.id) ?? null);
        drawn.current.set(sheet.id, sheet);
        // Built the same way the viewer builds an installed paint's texture, so the drawing
        // lands on the mesh exactly where the `.pnt` would have. `needsUpdate` inside carries
        // the new pixels without any React work. A sheet that turned out to have no pixels to
        // redraw has none to upload either.
        if (area) tex = sheetTexture(canvas, tex, area);
      }
      if (!tex) tex = sheetTexture(canvas, null);
      if (!tex) continue;
      textures.current.set(sheet.id, tex);
      if (sheet.name.trim()) next.set(sheet.name.trim().toLowerCase(), tex);
    }
    dirty.current.clear();
    // Identity changes only when the *names* do. The viewer memoises one material per submesh
    // on this map, so handing it a fresh one per pointer move rebuilt every material in the
    // model on every frame of a drag — which is exactly as slow as it sounds. The textures
    // inside are the same objects either way; `needsUpdate` above is what carries the pixels.
    overridesRef.current = next;
    // Compared against a ref, and only *then* set. An updater that returns its own argument
    // still counts as an update: React re-renders this component to find out that nothing
    // moved, and this effect runs on every frame of a stroke — so guarding inside the updater
    // bought a wasted render of the whole editor per frame of every drag.
    const names = [...next.keys()].sort().join(" ");
    if (publishedNames.current !== names) {
      publishedNames.current = names;
      setOverrideNames(names);
    }
  }, [sheets, version, canvasFor]);

  const overrides = useMemo(
    () => new Map(overridesRef.current),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [overrideNames],
  );

  // Drop the canvases and textures of sheets that are gone, and everything on unmount.
  useEffect(() => {
    const live = new Set(sheets.map((s) => s.id));
    for (const [id, tex] of textures.current) {
      if (!live.has(id)) {
        tex.dispose();
        textures.current.delete(id);
        canvases.current.delete(id);
        drawn.current.delete(id);
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetIdKey]);

  useEffect(() => {
    const held = textures.current;
    return () => {
      held.forEach((tex) => tex.dispose());
      held.clear();
    };
  }, []);

  /**
   * Record the document as it stands, so the edit about to happen can be taken back.
   *
   * Before the mutation, never after: what undo needs is the state that existed a moment ago,
   * and once `setSheets` has run there is nothing left holding it. `key` collapses a run of
   * edits that are one gesture — see `PaintHistory.pushDoc`.
   */
  const remember = useCallback((key: string | null = null) => {
    history.current.pushDoc(sheetsRef.current, key);
    setHistoryRev((v) => v + 1);
  }, []);

  const patchSheet = useCallback(
    /**
     * `undoKey` names the gesture for coalescing, or `false` for an edit the history must not
     * record — one whose other half lives outside the document, where an undo would restore the
     * sheet and leave that half where it was.
     */
    (id: string, fn: (s: Sheet) => Sheet, undoKey: string | null | false = null) => {
      // Every route into a sheet but a stroke comes through here, and none of them says where it
      // drew. Marking the sheet wholly dirty on the way past is what lets the recomposite treat a
      // region as a promise rather than a hint: if one is there, a stroke put it there.
      if (undoKey !== false) remember(undoKey);
      dirty.current.set(id, null);
      setSheets((prev) => prev.map((s) => (s.id === id ? fn(s) : s)));
    },
    [remember],
  );

  const patchLayer = useCallback(
    (layerId: string, fn: (l: Layer) => Layer, undoKey: string | null | false = null) => {
      if (!activeId) return;
      patchSheet(
        activeId,
        (s) => ({
          ...s,
          layers: s.layers.map((l) => (l.id === layerId ? fn(l) : l)),
        }),
        undoKey,
      );
    },
    [activeId, patchSheet],
  );

  /* ── Painting ──────────────────────────────────────────────────────────────────────────
     A stroke writes straight into its layer's canvas, which React cannot see. `touchPaint`
     is what makes the change exist as far as the editor is concerned: a fresh layer with a
     higher `rev` inside a fresh sheet, which is exactly the signal the recomposite above
     watches for. ─────────────────────────────────────────────────────────────────────── */

  /**
   * Record where a stroke drew, without telling React anything.
   *
   * Split from the notification below because the two want different rates. Pixels have to be
   * accounted for the instant they land — miss one sample's region and that part of the stroke
   * never reaches the composite — while the render they add up to is worth doing once a frame.
   */
  const markPaint = useCallback((sheetId: string, region: Region | null) => {
    const held = dirty.current.get(sheetId);
    // Once a sheet is wholly dirty it stays that way until the redraw: a region unioned onto
    // "everything" would narrow the redraw to less than is actually stale.
    if (region && held !== null) dirty.current.set(sheetId, unionRegion(held ?? null, region));
    else dirty.current.set(sheetId, null);
  }, []);

  /** Make the pixels a stroke has already written exist as far as React is concerned. */
  const notifyPaint = useCallback(
    (sheetId: string, layerId: string) => {
      setSheets((prev) =>
        prev.map((s) =>
          s.id === sheetId
            ? {
                ...s,
                layers: s.layers.map((l) =>
                  l.id === layerId && l.kind === "paint" ? { ...l, rev: l.rev + 1 } : l,
                ),
              }
            : s,
        ),
      );
      bump();
    },
    [bump],
  );

  const touchPaint = useCallback(
    (sheetId: string, layerId: string, region?: Region | null) => {
      markPaint(sheetId, region ?? null);
      notifyPaint(sheetId, layerId);
    },
    [markPaint, notifyPaint],
  );

  /**
   * One render per frame, however fast the pointer reports.
   *
   * Pointer samples are not paced by the display. A high-rate mouse on a webview that doesn't
   * align them to the frame delivers a dozen or more between two paints, and each one used to
   * drive a full React commit, a recomposite, a texture upload and a redraw of both views — a
   * dozen renders where the screen could show one. The stroke itself still takes every sample
   * the moment it arrives, because that is what the mark is made of; only the telling-everyone
   * waits, and it waits at most until the next frame, which is the soonest anyone could see it.
   */
  // The shape layer a drag is currently rewriting, and where the press landed. Null except
  // between the press and the release of a shape tool — the stroke ref's opposite number.
  const shaping = useRef<{ id: string; from: Point } | null>(null);

  const queued = useRef<{ sheetId: string; layerId: string } | null>(null);
  const frame = useRef(0);

  const flushPaint = useCallback(() => {
    if (frame.current) {
      cancelAnimationFrame(frame.current);
      frame.current = 0;
    }
    const q = queued.current;
    queued.current = null;
    if (q) notifyPaint(q.sheetId, q.layerId);
  }, [notifyPaint]);

  const schedulePaint = useCallback(
    (sheetId: string, layerId: string) => {
      queued.current = { sheetId, layerId };
      if (!frame.current) frame.current = requestAnimationFrame(flushPaint);
    },
    [flushPaint],
  );

  // A stroke abandoned by an unmount has already written its pixels; the frame that would have
  // announced them has nowhere left to land.
  useEffect(
    () => () => {
      if (frame.current) cancelAnimationFrame(frame.current);
    },
    [],
  );

  /** The paint layer a stroke would land on: the selected one, when it is one. */
  const target = useMemo<PaintLayer | null>(() => {
    const layer = active?.layers.find((l) => l.id === selectedId);
    return layer?.kind === "paint" ? layer : null;
  }, [active, selectedId]);

  const addPaintLayer = useCallback(() => {
    if (!active) return;
    const layer = paintLayer(t("designer.paintLayerName"), active);
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, layer] }));
    setSelectedId(layer.id);
    bump();
  }, [active, bump, patchSheet, t]);

  /**
   * Choose a tool, and make sure it has somewhere to paint.
   *
   * Picking up a brush with no paint layer selected and having nothing happen is
   * indistinguishable from a broken brush, so this finds the paint layer already on the sheet
   * or starts one. The template is never a candidate — it is what's being drawn on top of.
   */
  const pickTool = useCallback(
    (tool: PaintTool) => {
      setPaint((p) => ({ ...p, tool }));
      // A shape makes its own layer on the press, so it needs nothing selected to land on.
      if (tool === "move" || SHAPE_TOOLS.has(tool) || !active) return;
      const selected = active.layers.find((l) => l.id === selectedId);
      if (selected?.kind === "paint") return;
      const existing = [...active.layers].reverse().find((l) => l.kind === "paint");
      if (existing) setSelectedId(existing.id);
      else addPaintLayer();
    },
    [active, addPaintLayer, selectedId],
  );


  const movePaint = useCallback(
    (points: Point[], constrain: boolean) => {
      // A shape in progress is a layer, not a stroke: the drag rewrites its box and the
      // composite redraws it, so what is on screen mid-drag is the shape itself rather than a
      // preview of one. See `startPaint`.
      const drawing = shaping.current;
      if (drawing) {
        const raw = points[points.length - 1];
        if (!raw) return;
        const to = constrain ? constrained(drawing.from, raw, paint.tool) : raw;
        patchLayer(
          drawing.id,
          (l) =>
            l.kind === "shape"
              ? {
                  ...l,
                  x: (drawing.from.x + to.x) / 2,
                  y: (drawing.from.y + to.y) / 2,
                  w: to.x - drawing.from.x,
                  h: -(to.y - drawing.from.y),
                }
              : l,
          `shape:${drawing.id}`,
        );
        bump();
        return;
      }
      const live = stroke.current;
      if (!live || !target || !activeId) return;
      live.move(points, constrain);
      if (!live.dirty) return;
      markPaint(activeId, live.dirty);
      schedulePaint(activeId, target.id);
    },
    [activeId, bump, markPaint, paint.tool, patchLayer, schedulePaint, target],
  );

  const endPaint = useCallback(() => {
    const drawing = shaping.current;
    if (drawing) {
      shaping.current = null;
      // A click with a shape tool selected is not a shape. Left in, it would be an invisible
      // layer in the list with handles too small to grab and nothing to see.
      if (active) {
        const made = active.layers.find((l) => l.id === drawing.id);
        if (made?.kind === "shape" && Math.abs(made.w) < 2 && Math.abs(made.h) < 2) {
          patchSheet(active.id, (sh) => ({
            ...sh,
            layers: sh.layers.filter((l) => l.id !== drawing.id),
          }));
          setSelectedId(null);
          bump();
        }
      }
      return;
    }
    const done = stroke.current;
    stroke.current = null;
    // Whatever the last frame didn't get to, now — a stroke that ended between two frames would
    // otherwise leave its final samples drawn on the layer and missing from the composite.
    flushPaint();
    // A press that put nothing down isn't a step to undo — clicking to check the tool would
    // otherwise fill the history with states identical to the one before them.
    if (!done?.end() || !target || !activeId) return;
    history.current.push(activeId, target.id, done.before);
    setHistoryRev((v) => v + 1);
  }, [active, activeId, bump, flushPaint, patchSheet, target]);

  /** The live canvas behind a layer id, wherever it lives — history spans every sheet. */
  const paintCanvas = useCallback(
    (layerId: string) => {
      for (const sheet of sheets) {
        const layer = sheet.layers.find((l) => l.id === layerId);
        if (layer?.kind === "paint") return layer.canvas;
      }
      return null;
    },
    [sheets],
  );

  // The stack is a mutable object, so React has no way to notice it moved. `historyRev` is the
  // notice, and reading it here is what makes these two follow it.
  const { canUndo, canRedo } = useMemo(
    () => ({ canUndo: history.current.canUndo, canRedo: history.current.canRedo }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [historyRev],
  );

  /**
   * Reading and replacing the document for the history — plus putting the cursor somewhere real
   * afterwards.
   *
   * Undoing the sheet you were looking at leaves `activeId` naming something that no longer
   * exists, and the editor would come back empty rather than showing what it just restored. The
   * same for a selected layer. Neither is part of the document, so neither is restored by it.
   */
  const docAccess = useMemo(
    () => ({
      read: () => sheetsRef.current,
      write: (next: Sheet[]) => {
        setSheets(next);
        setActiveId((cur) => (cur && next.some((s) => s.id === cur) ? cur : next[0]?.id ?? null));
        setSelectedId((cur) =>
          cur && next.some((s) => s.layers.some((l) => l.id === cur)) ? cur : null,
        );
      },
    }),
    [],
  );

  // Not while the pointer is still down: the stroke redraws its layer from its own snapshot on
  // the next sample, so an undo mid-drag would be silently taken back a moment later.
  const undo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.undo(paintCanvas, docAccess);
    setHistoryRev((v) => v + 1);
    if (!entry) return;
    // A document step replaces sheet objects wholesale, and the recomposite already follows
    // sheet identity — so it needs no region, only to be told the drawing moved.
    if (entry.kind === "pixels") touchPaint(entry.sheetId, entry.layerId);
    else bump();
  }, [bump, docAccess, paintCanvas, touchPaint]);

  const redo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.redo(paintCanvas, docAccess);
    setHistoryRev((v) => v + 1);
    if (!entry) return;
    if (entry.kind === "pixels") touchPaint(entry.sheetId, entry.layerId);
    else bump();
  }, [bump, docAccess, paintCanvas, touchPaint]);

  // Which paint layers exist, as a string. `sheets` changes on every pointer sample of a
  // stroke; this changes only when one is added or deleted, which is the only time the
  // history below could be holding something that no longer has a canvas to go back onto.
  const paintLayerKey = useMemo(
    () =>
      sheets
        .map((s) => `${s.id}:${s.layers.filter((l) => l.kind === "paint").map((l) => l.id)}`)
        .join("|"),
    [sheets],
  );

  useEffect(() => {
    const alive = new Map(
      sheets.map((s) => [
        s.id,
        new Set(s.layers.filter((l) => l.kind === "paint").map((l) => l.id)),
      ]),
    );
    if (history.current.keepOnly((sid, lid) => alive.get(sid)?.has(lid) ?? false)) {
      setHistoryRev((v) => v + 1);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [paintLayerKey]);

  /**
   * Tool keys, and undo.
   *
   * On the window rather than on the canvas, because a brush should be one key away wherever
   * the focus happens to be — but the Studio keeps this pane mounted behind whichever tab is
   * open, so an invisible Designer would otherwise steal every `b` typed into Paint Studio.
   */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!rootRef.current?.offsetParent) return;
      const el = e.target as HTMLElement | null;
      if (el && (/^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName) || el.isContentEditable)) return;
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) redo();
        else undo();
        return;
      }
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const key = e.key.toLowerCase();
      const tool = (Object.keys(TOOL_KEYS) as PaintTool[]).find((k) => TOOL_KEYS[k] === key);
      if (tool) {
        e.preventDefault();
        pickTool(tool);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pickTool, redo, undo]);

  /**
   * Pixels for a file the user picked, at the sheet's own resolution.
   *
   * The rows arrive in the order the file holds them, which is the order the mesh samples
   * them — and that is upside down from the template a painter works in. Nothing is flipped
   * here: the sheet, the composite and the save all stay in the file's own row order, and the
   * 2D stage turns it the right way up for display alone (see `CanvasStage`). Flipping the
   * pixels instead would put the editor's opinion about orientation inside the saved paint.
   */
  const readImage = useCallback(async (path: string) => {
    const tex = await paintStudioPixels(path);
    const buf = await textureBytes(tex.token);
    return { name: tex.name, bitmap: await bitmapFromRgba(buf, tex.width, tex.height) };
  }, []);

  /**
   * Start from an installed paint.
   *
   * This is the template step, and it matters more than it looks: the sheets come back named
   * the way the model binds them, so a livery drawn on top lands on the right bodywork. A
   * blank sheet has to be named by hand, and a wrong name is a paint that loads and shows
   * nothing.
   */
  const loadSheets = useCallback(
    async (paths: string[], nameHint?: string) => {
      // Not a silent return: a paint that unpacks to nothing looks exactly like a button that
      // doesn't work, and the difference matters — one is a broken app, the other is a file
      // this can't read.
      if (!paths.length) {
        toast.error(t("designer.noSheetsFound"));
        return;
      }
      const loaded = await Promise.all(paths.map((f) => readImage(f)));
      const next: Sheet[] = loaded.map(({ name: sheetName, bitmap }) => ({
        id: newId("sheet"),
        name: sheetName,
        width: bitmap.width,
        height: bitmap.height,
        base: bitmap,
        layers: [],
      }));
      remember();
      setSheets(next);
      setActiveId(next[0]?.id ?? null);
      setSelectedId(null);
      if (nameHint) setName((n) => n || nameHint);
      bump();
      toast.success(t("designer.loadedSheets", { count: String(next.length) }));
    },
    [bump, readImage, remember, t],
  );

  const startFromPaint = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "MX Bikes paint", extensions: ["pnt"] }],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    // Busy from here, not from inside `loadSheets`: unpacking the `.pnt` is the slow half —
    // it reads the file, inflates every sheet and writes them out — and leaving it outside the
    // spinner is why picking a paint looked like nothing had happened.
    setBusy(true);
    try {
      const template = await paintStudioExtract(path);
      await loadSheets(
        template.files,
        (path.replace(/\\/g, "/").split("/").pop() ?? "").replace(/\.pnt$/i, ""),
      );
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [loadSheets]);

  // Sheets sent over from Paint Studio. Same path as unpacking a paint here, because it is the
  // same thing — that tab has already done the unpacking.
  useEffect(() => {
    if (!incoming?.length) return;
    setBusy(true);
    void loadSheets(incoming)
      .catch((e) => toast.error(String(e).replace(/^Error:\s*/, "")))
      .finally(() => {
        setBusy(false);
        onIncomingLoaded?.();
      });
  }, [incoming, loadSheets, onIncomingLoaded]);

  /**
   * Texture names this model wants that have no sheet yet — what the create button offers,
   * and the name a new blank sheet is given.
   *
   * Colour sheets only. The hint line lists the companion maps too, because knowing the bike
   * has a `plastics_n` is worth knowing, but an *empty* one is worse than none: a paint
   * replaces textures by name, so saving a blank normal map strips the bike's real one.
   */
  const missingHints = useMemo(() => {
    const taken = new Set(sheets.map((s) => s.name.trim().toLowerCase()));
    return hints.filter((h) => !taken.has(h.trim().toLowerCase()) && !isCompanionMap(h));
  }, [hints, sheets]);

  const addBlankSheet = useCallback(() => {
    // Name it after a texture the chosen model actually asks for, when we know one — that's
    // the difference between a paint that shows and a paint that doesn't.
    const suggested = missingHints[0] ?? "";
    const sheet = blankSheet(suggested, BLANK_SIZE);
    remember();
    setSheets((prev) => [...prev, sheet]);
    setActiveId(sheet.id);
    setSelectedId(null);
    bump();
  }, [bump, missingHints, remember]);

  /**
   * One sheet per colour texture the model asks for that isn't on the list yet.
   *
   * The names are the whole binding — a sheet called anything else paints nothing — and until
   * now the only way to get them right without an installed paint to start from was to read
   * them off the hint line and type each one back in.
   *
   * Only the missing ones, so pressing it twice doesn't leave two sheets fighting over a name.
   */
  const addHintSheets = useCallback(() => {
    const made = missingHints.map((h) => blankSheet(h, BLANK_SIZE));
    if (!made.length) return;
    remember();
    setSheets((prev) => [...prev, ...made]);
    setActiveId(made[0].id);
    setSelectedId(null);
    bump();
  }, [bump, missingHints, remember]);

  const addImage = useCallback(async () => {
    if (!active) return;
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Images", extensions: IMAGE_EXTS }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    setBusy(true);
    try {
      const loaded = await Promise.all(paths.map((p) => readImage(p)));
      const added = loaded.map(({ name: layerName, bitmap }) =>
        imageLayer(layerName, bitmap, active),
      );
      patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, ...added] }));
      setSelectedId(added[added.length - 1]?.id ?? null);
      bump();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [active, bump, patchSheet, readImage]);

  const addText = useCallback(() => {
    if (!active) return;
    const layer = textLayer(t("designer.newTextValue"), active);
    patchSheet(active.id, (s) => ({ ...s, layers: [...s.layers, layer] }));
    setSelectedId(layer.id);
    bump();
  }, [active, bump, patchSheet, t]);

  const removeLayer = useCallback(
    (id: string) => {
      if (!activeId) return;
      patchSheet(activeId, (s) => ({ ...s, layers: s.layers.filter((l) => l.id !== id) }));
      setSelectedId((cur) => (cur === id ? null : cur));
      bump();
    },
    [activeId, bump, patchSheet],
  );

  const moveLayer = useCallback(
    (id: string, dx: number, dy: number) => {
      patchLayer(id, (l) => ({ ...l, x: l.x + dx, y: l.y + dy }), `layer:${id}`);
      bump();
    },
    [bump, patchLayer],
  );

  /** A corner drag, as an absolute scale. Clamped by the stage to the inspector's range. */
  const scaleLayer = useCallback(
    (id: string, scale: number) => {
      patchLayer(id, (l) => ({ ...l, scale }), `layer:${id}`);
      bump();
    },
    [bump, patchLayer],
  );

  /* ── The reference underlay ────────────────────────────────────────────────────────────
     None of this touches a sheet, with one exception: turning tracing on *moves* the template
     out of `Sheet.base`, which is a real edit to what would be saved and is meant to be — it
     is the whole difference between drawing over a paint and drawing from one. ───────── */

  /**
   * The model's bodywork for the active sheet.
   *
   * Cheap — no rasterising, just a walk over the mesh — so it is derived rather than cached
   * behind a toggle the way the wireframe is. Clipping and fitting need it whether or not
   * anyone has asked to *see* the islands, and keying it on the name rather than the sheet
   * keeps a brush stroke, which replaces the sheet object, from recomputing it.
   */
  const activeName = active?.name ?? "";
  // The rest of what the UV map is built from, pulled out for the same reason: a stroke
  // replaces the sheet object without changing any of these.
  const activeWidth = active?.width ?? 0;
  const activeHeight = active?.height ?? 0;
  // Left and right are asked for on bikes only: a bike arrives assembled about its mirror
  // plane, where gear is a single piece whose up-axis the viewer has to work out per mod.
  // And only when it *did* arrive that way — a bike that loaded without its `.geom` is a heap
  // of parts in their own frames, and a side named from that is worse than no side at all.
  const bike = isBikeKind(destState.kind);
  // The island the pointer is over, for the 3D view to light up. Held here rather than in the
  // stage because it crosses from the 2D half of the editor to the 3D one, and this is the
  // only thing that owns both.
  const [hoverIsland, setHoverIsland] = useState<Int32Array | null>(null);
  const parts = useMemo<UvPart[]>(
    () => (geometry ? uvParts(geometry, activeName, { assembled: bike && assembled }) : []),
    [geometry, activeName, bike, assembled],
  );

  const startPaint = useCallback(
    (at: Point, whole: boolean) => {
      // A rectangle, ellipse or line becomes a layer, not pixels. Made on the press and
      // rewritten as the drag goes, so the thing being dragged out *is* the finished object —
      // there is no separate preview to disagree with the result, and on release it is
      // already selected with handles on it.
      if (SHAPE_TOOLS.has(paint.tool) && active) {
        const shape = paint.tool as ShapeLayer["shape"];
        const layer = shapeLayer(
          t(`designer.tool.${shape}`),
          shape,
          at,
          at,
          paint.shape,
          paint.colorA,
          paint.strokeWidth,
        );
        remember();
        patchSheet(active.id, (sh) => ({ ...sh, layers: [...sh.layers, layer] }));
        setSelectedId(layer.id);
        shaping.current = { id: layer.id, from: at };
        bump();
        return;
      }
      if (!target || !activeId) return;
      // The bucket fills the uv triangle under the press, not the sheet and not the whole mesh
      // group. Worked out here rather than inside `Stroke`, because the parts are the editor's
      // knowledge of the model and paint.ts is deliberately ignorant of it — it puts pixels
      // down, wherever it is told to.
      //
      // The group is only the first cut: `shroud` is both flanks and often several islands, so
      // stopping there floods panels the press never pointed at. Left button takes the one
      // triangle under the pointer; right button takes the island it belongs to.
      let fillTo: { path: Path2D; box: Region } | null = null;
      if (paint.tool === "fill" && active && parts.length) {
        const u = at.x / active.width;
        const v = at.y / active.height;
        const under = partAt(parts, u, v);
        const pick = whole ? islandAt : triangleAt;
        const region = under ? (pick(under, u, v) ?? under) : null;
        if (region) {
          fillTo = {
            path: partPath(region, active.width, active.height),
            box: partBox(region, active.width, active.height),
          };
        }
      }
      const next = new Stroke(target.canvas, paint, at, fillTo);
      stroke.current = next;
      // Straight through rather than queued: the press is the one sample nobody would forgive a
      // frame's wait on, and a tool that puts nothing down on the press has nothing to show.
      if (next.dirty) touchPaint(activeId, target.id, next.dirty);
    },
    [active, activeId, bump, paint, parts, patchSheet, remember, t, target, touchPaint],
  );

  /** Pin the selected layer to a piece of bodywork, or let it cover the sheet again. */
  const clipLayer = useCallback(
    (label: string | null) => {
      if (!active || !selectedId) return;
      const part = label ? parts.find((p) => p.label === label) : null;
      patchLayer(selectedId, (l) => ({
        ...l,
        // Built here, at this sheet's size, so the composite never has to. Re-picking the
        // part is what rebuilds it, which is also the answer to a resized sheet.
        clip: part
          ? { label: part.label, path: partPath(part, active.width, active.height) }
          : null,
      }));
      bump();
    },
    [active, bump, parts, patchLayer, selectedId],
  );

  /**
   * Place and scale the selected layer to cover a part.
   *
   * Cover, not contain: a photo meant for a shroud should reach every edge of it, and the
   * clip is what trims the overspill. Contain would leave the sheet showing through at two
   * sides of anything whose shape didn't happen to match the panel's.
   */
  const fitLayer = useCallback(
    (label: string) => {
      const layer = active?.layers.find((l) => l.id === selectedId);
      const part = parts.find((p) => p.label === label);
      if (!active || !layer || !part || layer.kind === "paint") return;
      const bw = (part.maxU - part.minU) * active.width;
      const bh = (part.maxV - part.minV) * active.height;
      const { w, h } = layerExtent(layer);
      if (!w || !h || !bw || !bh) return;
      const scale = Math.min(4, Math.max(0.05, Math.max(bw / w, bh / h)));
      patchLayer(layer.id, (l) => ({
        ...l,
        x: (part.minU + part.maxU) * 0.5 * active.width,
        y: (part.minV + part.maxV) * 0.5 * active.height,
        scale,
      }));
      bump();
    },
    [active, bump, parts, patchLayer, selectedId],
  );

  const ghostOf = useCallback(
    (id: string | null | undefined) => (id && ghosts.get(id)) || EMPTY_GHOST,
    [ghosts],
  );

  const patchGhost = useCallback((id: string, fn: (g: Ghost) => Ghost) => {
    setGhosts((prev) => {
      const next = new Map(prev);
      next.set(id, fn(prev.get(id) ?? EMPTY_GHOST));
      return next;
    });
  }, []);

  /**
   * Take the model the preview loaded, and drop every wireframe built from the last one.
   *
   * Switching bikes keeps the sheet names, so without this a `livery` map rasterised from the
   * previous model would look perfectly valid over the new one while describing bodywork that
   * isn't there — the worst kind of wrong for a guide.
   */
  const geometryRef = useRef<EdfNode[] | null>(null);
  const onGeometry = useCallback((nodes: EdfNode[] | null, assembled: boolean) => {
    // Compared against a ref rather than inside a `setState` updater: an updater has to be
    // pure, and this has to invalidate the wires as well as record the mesh.
    if (geometryRef.current === nodes) return;
    geometryRef.current = nodes;
    setGeometry(nodes);
    setAssembled(assembled);
    setGhosts((gs) =>
      gs.size ? new Map([...gs].map(([id, g]) => [id, { ...g, wire: null, wireFor: null }])) : gs,
    );
  }, []);

  /** The same, for the model's own textures: a new bike, a new set of stock sheets. */
  const stockRef = useRef<PaintTexture[]>([]);
  const onStock = useCallback((textures: PaintTexture[]) => {
    if (stockRef.current === textures) return;
    stockRef.current = textures;
    setStockTextures(textures);
    setGhosts((gs) =>
      gs.size ? new Map([...gs].map(([id, g]) => [id, { ...g, stock: null, stockFor: null }])) : gs,
    );
  }, []);

  /**
   * Move the template between the sheet and the ghost.
   *
   * Moved, never copied. A template that stayed as `base` while also showing as a ghost would
   * be saved into the paint, which is the thing somebody asking to trace is trying to avoid;
   * and keeping the bitmap on the other side is what lets this be undone by pressing it again.
   */
  const toggleTrace = useCallback(
    (sheetId: string) => {
      const sheet = sheets.find((s) => s.id === sheetId);
      if (!sheet) return;
      const ghost = ghostOf(sheetId);
      // Not recorded by the history, either way: the bitmap moves between the sheet and the
      // ghost, and the ghost isn't part of the document — an undo would put the template back
      // on the sheet while the ghost still held it, which is the copy this is careful not to
      // make. Pressing the button again is the way back, as it always was.
      if (sheet.base) {
        const template = sheet.base;
        patchSheet(sheetId, (s) => ({ ...s, base: null }), false);
        patchGhost(sheetId, (g) => ({ ...g, template, showTemplate: true }));
      } else if (ghost.template) {
        const base = ghost.template;
        patchSheet(sheetId, (s) => ({ ...s, base }), false);
        patchGhost(sheetId, (g) => ({ ...g, template: null }));
      }
      bump();
    },
    [bump, ghostOf, patchGhost, patchSheet, sheets],
  );

  /**
   * Build the active sheet's UV map, once the user has asked for one.
   *
   * Lazily, and keyed on the sheet's *name*, because the name is the entire binding — rename a
   * sheet from `livery` to `plate` and it describes different triangles. Rasterising eagerly
   * would spend the work on sheets nobody looks at, and rasterising on every render would spend
   * it again on every brush stroke, since a stroke replaces the sheet object.
   *
   * Which is also why this takes the sheet apart rather than depending on it: every field it
   * reads survives a stroke untouched, so following the sheet itself would re-run this — and
   * re-check a guard that has already been satisfied — once per pointer sample of every drag.
   */
  useEffect(() => {
    if (!activeId || !geometry) return;
    // A half-typed name is not a name yet. Without this, every keystroke of "livery" would be
    // asked of the mesh and answered "nothing binds that", which is true and useless.
    if (!activeName.trim()) return;
    const ghost = ghosts.get(activeId) ?? EMPTY_GHOST;
    if (!ghost.showWire || ghost.wireFor === activeName) return;
    // `wireFor` records the attempt whether or not it found anything, so a name that matches
    // nothing is asked once rather than on every render. The panel reads the pair to say so —
    // out loud, because an empty overlay is indistinguishable from one still being built.
    const wire = uvWireframe(parts, activeWidth, activeHeight);
    patchGhost(activeId, (g) => ({ ...g, wire, wireFor: activeName }));
  }, [activeId, activeName, activeWidth, activeHeight, geometry, ghosts, parts, patchGhost]);

  /**
   * Fetch the model's own texture for the active sheet, the same way and for the same reasons.
   *
   * Lazy and keyed on the name again — the name is what picks the texture out of the model,
   * exactly as it picks the triangles — but this one crosses to the backend for pixels, so a
   * 2048² sheet is a few megabytes over IPC. Only the sheet on screen pays for it, and only
   * once: a paint with twenty sheets fetches the one being looked at.
   */
  const stockFetch = useRef<string | null>(null);
  useEffect(() => {
    if (!activeId || !activeName.trim() || !stockTextures.length) return;
    const ghost = ghosts.get(activeId) ?? EMPTY_GHOST;
    if (!ghost.showStock || ghost.stockFor === activeName) return;
    // One request in the air per sheet-and-name. This effect re-runs on every change to any
    // ghost — the opacity slider alone is dozens — and without this each of those would put
    // another copy of the same megabyte read on the wire while the first was still coming.
    const key = `${activeId}:${activeName}`;
    if (stockFetch.current === key) return;
    stockFetch.current = key;
    const want = activeName.trim().toLowerCase();
    const tex = stockTextures.find((t) => t.name.trim().toLowerCase() === want);
    void (async () => {
      // Null covers both "the model carries no such texture" and "the pixels are gone" — the
      // store evicts, and a rejected read is the same nothing to draw as a name that missed.
      const stock = tex
        ? await textureBytes(tex.token)
            .then((buf) => bitmapFromRgba(buf, tex.width, tex.height))
            .catch(() => null)
        : null;
      // Written against the sheet it was asked *for*, not whichever is active now, and both
      // halves land together. So the worst a slow answer can do is describe a name the sheet
      // no longer has — which the guard above then notices, and asks again.
      patchGhost(activeId, (g) => ({ ...g, stock, stockFor: activeName }));
      if (stockFetch.current === key) stockFetch.current = null;
    })();
  }, [activeId, activeName, ghosts, patchGhost, stockTextures]);

  // Ghosts of sheets that are gone. Each holds a decoded bitmap and a raster the size of the
  // sheet, so leaving them behind would keep a closed paint's pixels alive for the session.
  useEffect(() => {
    setGhosts((prev) => {
      if (!prev.size) return prev;
      const live = new Set(sheets.map((s) => s.id));
      if ([...prev.keys()].every((id) => live.has(id))) return prev;
      return new Map([...prev].filter(([id]) => live.has(id)));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sheetIdKey]);

  /** Reorder within the stack. `delta` of -1 is one step down (further back). */
  const reorder = useCallback(
    (id: string, delta: number) => {
      if (!activeId) return;
      patchSheet(activeId, (s) => {
        const at = s.layers.findIndex((l) => l.id === id);
        const to = at + delta;
        if (at < 0 || to < 0 || to >= s.layers.length) return s;
        const layers = [...s.layers];
        const [moved] = layers.splice(at, 1);
        layers.splice(to, 0, moved);
        return { ...s, layers };
      });
      bump();
    },
    [activeId, bump, patchSheet],
  );

  /**
   * Move a sheet within the list.
   *
   * Not cosmetic: `write` packs the sheets in this order, so it is the order they end up in
   * the `.pnt`. The mesh binds by name either way, but a paint whose sheets are ordered the
   * way its author expects is easier to diff and to hand to somebody else.
   */
  const reorderSheet = useCallback(
    (id: string, delta: number) => {
      // Keyed, so walking a sheet three places up is one step back rather than three.
      remember(`sheet-order:${id}`);
      setSheets((prev) => {
        const at = prev.findIndex((s) => s.id === id);
        const to = at + delta;
        if (at < 0 || to < 0 || to >= prev.length) return prev;
        const next = [...prev];
        const [moved] = next.splice(at, 1);
        next.splice(to, 0, moved);
        return next;
      });
    },
    [remember],
  );

  const removeSheet = useCallback(
    (id: string) => {
      remember();
      setSheets((prev) => {
        const next = prev.filter((s) => s.id !== id);
        setActiveId((cur) => (cur === id ? next[0]?.id ?? null : cur));
        return next;
      });
      bump();
    },
    [bump, remember],
  );

  /** Why saving isn't possible yet, as a translated message — or null when it is, and on an
   *  empty canvas, where there is nothing to fix yet and Save is simply off. */
  const blocked = useMemo<string | null>(() => {
    if (!sheets.length) return null;
    if (!name.trim()) return t("paints.needName");
    if (sheets.some((s) => !s.name.trim())) return t("paints.needTextureNames");
    const seen = new Set<string>();
    for (const s of sheets) {
      const key = s.name.trim().toLowerCase();
      if (seen.has(key)) return t("paints.duplicateName", { name: s.name.trim() });
      seen.add(key);
    }
    if (!dest) return t("paints.needTarget");
    return null;
  }, [sheets, name, dest, t]);

  const canSave = sheets.length > 0 && !blocked;

  const write = useCallback(
    async (overwrite: boolean) => {
      if (!dest) return;
      setBusy(true);
      try {
        // Composite every sheet first: they only exist as canvases until now, and doing it on
        // the way out is what stops a save shipping a frame older than the screen.
        //
        // Then drop the ones with nothing on them. A `.pnt` replaces the model's textures by
        // name, so an empty sheet doesn't add a blank — it *removes* whatever the bike had
        // there. Offering to create every missing sheet at once made that easy to do by
        // accident: create twenty, paint two, and the other eighteen would have wiped the
        // bike's normal and roughness maps.
        const inked = sheets.filter((sheet) => {
          const canvas = canvasFor(sheet);
          composite(canvas, sheet);
          return hasInk(canvas);
        });
        const blank = sheets.length - inked.length;
        if (!inked.length) {
          toast.error(t("designer.nothingToSave"));
          return;
        }
        const staged = await Promise.all(
          inked.map(async (sheet) => {
            const path = await paintStudioStage(sheet.name.trim(), await toPng(canvasFor(sheet)));
            return { path, name: sheet.name.trim() };
          }),
        );
        const outcome = await paintStudioSave({
          name: name.trim(),
          fileName: name.trim(),
          textures: staged,
          dest,
          overwrite,
        });
        // Said out loud, not silently: a sheet that doesn't get written is a sheet that
        // won't be in the file, and finding that out later is worse than reading it now.
        toast.success(t("paints.saved", { path: outcome.path }), {
          description: blank ? t("designer.blankSheetsSkipped", { count: blank }) : undefined,
        });
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(false);
      }
    },
    [canvasFor, dest, name, sheets, t],
  );

  const save = useCallback(async () => {
    if (!canSave || !dest) {
      if (blocked) toast.error(blocked);
      return;
    }
    try {
      const target = await paintStudioTarget(name.trim(), dest);
      // Overwriting is the normal case here — you save, look, adjust, save again — so this
      // asks with a toast action rather than a modal that would interrupt that rhythm.
      if (target.exists) {
        toast.warning(t("paints.replaceTitle"), {
          description: t("paints.replaceBody", { path: target.path }),
          action: { label: t("paints.replace"), onClick: () => void write(true) },
        });
        return;
      }
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
      return;
    }
    await write(false);
  }, [blocked, canSave, dest, name, t, write]);

  const selected = active?.layers.find((l) => l.id === selectedId) ?? null;

  return (
    <div
      ref={rootRef}
      className="flex min-h-0 flex-1 flex-col gap-3 overflow-hidden px-7 pb-6"
    >
      {/* The decisions made once — where it goes, what it's called, save — on one row, so
          the two things looked at continuously get the rest of the window. */}
      <div className="flex flex-none flex-wrap items-center gap-2">
        <Button
          variant="ghost"
          size="icon"
          className="size-8 flex-none"
          title={t(railOpen ? "designer.hideRail" : "designer.showRail")}
          aria-label={t(railOpen ? "designer.hideRail" : "designer.showRail")}
          onClick={() => setRailOpen((o) => !o)}
        >
          {railOpen ? (
            <PanelLeftClose className="size-4" />
          ) : (
            <PanelLeftOpen className="size-4" />
          )}
        </Button>
        <PaintDestBar state={destState} className="w-[290px]" />
        <Input
          value={name}
          placeholder={t("paints.namePlaceholder")}
          className="h-8 w-[168px]"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && void save()}
        />
        <Button
          size="sm"
          disabled={busy || !canSave}
          title={blocked ?? undefined}
          onClick={() => void save()}
        >
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
          {t("paints.save")}
        </Button>
        {blocked && (
          <span className="ml-auto max-w-[40%] truncate text-[11px] text-faint" title={blocked}>
            {blocked}
          </span>
        )}
      </div>

      <div
        className={cn(
          "grid min-h-0 flex-1 gap-3",
          railOpen
            ? "xl:grid-cols-[224px_minmax(0,1fr)_minmax(0,1fr)]"
            : "xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]",
        )}
      >
        {/* ── Sheets, layers, and the selected layer ───────────────────────────── */}
        <section className={cn("min-h-0 flex-col gap-3 overflow-y-auto", railOpen ? "flex" : "hidden")}>
          <SheetList
            sheets={sheets}
            activeId={activeId}
            hints={hints}
            onPick={(id) => {
              setActiveId(id);
              setSelectedId(null);
            }}
            onRename={(id, value) => {
              patchSheet(id, (s) => ({ ...s, name: value }));
              bump();
            }}
            onRemove={removeSheet}
            onReorder={reorderSheet}
            missingHints={missingHints}
            onAddBlank={addBlankSheet}
            onAddHintSheets={addHintSheets}
            onStartFromPaint={() => void startFromPaint()}
            busy={busy}
          />

          {active && (
            <GhostPanel
              ghost={ghostOf(active.id)}
              sheetName={active.name}
              hasBase={!!active.base}
              hasGeometry={!!geometry}
              hasStock={!!stockTextures.length}
              onTrace={() => toggleTrace(active.id)}
              onChange={(fn) => patchGhost(active.id, fn)}
            />
          )}

          {active && (
            <PaintTools
              settings={paint}
              onTool={pickTool}
              onChange={(patch) => setPaint((p) => ({ ...p, ...patch }))}
              canUndo={canUndo}
              canRedo={canRedo}
              onUndo={undo}
              onRedo={redo}
              onAddImage={() => void addImage()}
              onAddText={addText}
              busy={busy}
            />
          )}

          {active && (
            <LayerList
              layers={active.layers}
              selectedId={selectedId}
              onSelect={setSelectedId}
              onToggle={(id, visible) => {
                patchLayer(id, (l) => ({ ...l, visible }));
                bump();
              }}
              onRemove={removeLayer}
              onReorder={reorder}
              onAdd={addPaintLayer}
            />
          )}
          {selected && (
            <LayerInspector
              layer={selected}
              parts={parts}
              onClip={clipLayer}
              onFit={fitLayer}
              onChange={(fn) => {
                patchLayer(selected.id, fn, `layer:${selected.id}`);
                bump();
              }}
            />
          )}
        </section>

        {/* ── The sheet ────────────────────────────────────────────────────────── */}
        <section className="flex min-h-0 flex-col">
          {active ? (
            <CanvasStage
              className="flex-1"
              sheet={active}
              source={canvases.current.get(active.id) ?? null}
              version={version}
              ghost={ghostOf(active.id)}
              parts={parts}
              onHoverSpot={setHoverIsland}
              selectedId={selectedId}
              onSelect={setSelectedId}
              onMove={moveLayer}
              onScale={scaleLayer}
              tool={paint.tool}
              brushSize={paint.size}
              canPaint={!!target || SHAPE_TOOLS.has(paint.tool)}
              onPaintStart={startPaint}
              onPaintMove={movePaint}
              onPaintEnd={endPaint}
            />
          ) : (
            <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-lg border border-dashed border-border text-center">
              <p className="max-w-sm text-[12.5px] leading-relaxed text-muted-foreground">
                {t("designer.empty")}
              </p>
              <div className="flex gap-2">
                <Button size="sm" disabled={busy} onClick={() => void startFromPaint()}>
                  <PackageOpen className="size-3.5" /> {t("designer.startFromPaint")}
                </Button>
                <Button variant="outline" size="sm" onClick={addBlankSheet}>
                  <FilePlus2 className="size-3.5" /> {t("designer.blankSheet")}
                </Button>
            </div>
          </div>
        )}
      </section>

      {/* ── The model ────────────────────────────────────────────────────────── */}
      <section className="flex min-h-0 flex-col">
        <PreviewPanel
          state={destState}
          overrides={overrides}
          frameToken={version}
          onGeometry={onGeometry}
          onStock={onStock}
          highlight={hoverIsland}
          className="flex-1"
        />
      </section>
      </div>
    </div>
  );
}

function SheetList({
  sheets,
  activeId,
  hints,
  missingHints,
  onPick,
  onRename,
  onRemove,
  onReorder,
  onAddBlank,
  onAddHintSheets,
  onStartFromPaint,
  busy,
}: {
  sheets: Sheet[];
  activeId: string | null;
  /** Every texture name the model's paints use — shown in full, companion maps included. */
  hints: string[];
  /** The colour sheets among them that don't exist yet: what the create button would make. */
  missingHints: string[];
  onPick: (id: string) => void;
  onRename: (id: string, value: string) => void;
  onRemove: (id: string) => void;
  onReorder: (id: string, delta: number) => void;
  onAddBlank: () => void;
  onAddHintSheets: () => void;
  onStartFromPaint: () => void;
  busy: boolean;
}) {
  const t = useT();
  return (
    <div className="rounded-lg border border-border bg-card/40 p-3.5">
      <div className="mb-2.5 flex items-center gap-2">
        <h2 className="text-[13px] font-semibold">{t("designer.sheets")}</h2>
        <button
          type="button"
          className="ml-auto text-muted-foreground transition-colors hover:text-foreground"
          onClick={onAddBlank}
          title={t("designer.addSheet")}
        >
          <Plus className="size-4" />
        </button>
      </div>
      {/* Scrolls rather than growing: a bike's paint runs to two dozen sheets, and a list that
          long pushed the hint line and every button below the fold of the rail. */}
      <div className="flex max-h-[40vh] flex-col gap-1.5 overflow-y-auto pr-0.5">
        {sheets.map((sheet, i) => (
          <div
            key={sheet.id}
            className={cn(
              "flex items-center gap-1.5 rounded-md border px-1.5 py-1 transition-colors",
              sheet.id === activeId ? "border-primary bg-primary/10" : "border-border",
            )}
          >
            <button
              type="button"
              className="flex-none text-[11px] text-muted-foreground hover:text-foreground"
              onClick={() => onPick(sheet.id)}
              title={t("designer.editSheet")}
            >
              {sheet.width}²
            </button>
            <Input
              value={sheet.name}
              placeholder={t("designer.sheetName")}
              className="h-6 min-w-0 flex-1 border-0 bg-transparent px-1 text-[11.5px] shadow-none focus-visible:ring-0"
              onFocus={() => onPick(sheet.id)}
              onChange={(e) => onRename(sheet.id, e.target.value)}
            />
            <button
              type="button"
              className="flex-none px-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
              disabled={i === 0}
              onClick={() => onReorder(sheet.id, -1)}
              title={t("designer.moveUp")}
            >
              ↑
            </button>
            <button
              type="button"
              className="flex-none px-0.5 text-muted-foreground hover:text-foreground disabled:opacity-30"
              disabled={i === sheets.length - 1}
              onClick={() => onReorder(sheet.id, 1)}
              title={t("designer.moveDown")}
            >
              ↓
            </button>
            <button
              type="button"
              className="flex-none text-muted-foreground hover:text-destructive"
              onClick={() => onRemove(sheet.id)}
              title={t("common.remove")}
            >
              <Trash2 className="size-3.5" />
            </button>
          </div>
        ))}
      </div>

      {/* The names this model binds — what its mesh draws, plus whatever the paints already
          installed replace. A sheet named anything else binds to nothing, and this is the only
          place the right answer is visible — so it also offers to make them, rather than
          leaving the list to be copied out by hand. */}
      {!!hints.length && (
        <div className="mt-2 flex flex-col items-start gap-1.5">
          <p className="text-[11px] leading-snug text-faint">
            {t("paints.expected")} {hints.join(", ")}
          </p>
          {/* Full width and clipping, not sized to its label: the rail is 224px, a bike can
              want two dozen sheets, and `Button` is `whitespace-nowrap` — so a count in the
              label, or a longer word for it in another language, ran straight out of the rail. */}
          {!!missingHints.length && (
            <Button
              variant="outline"
              size="sm"
              className="w-full min-w-0 justify-start"
              onClick={onAddHintSheets}
              title={missingHints.join(", ")}
            >
              <FilePlus2 className="size-3.5" />
              <span className="truncate">
                {t("designer.createExpected", { count: missingHints.length })}
              </span>
            </Button>
          )}
        </div>
      )}

      {/* Only while there is nothing to draw on. Starting from a paint *replaces* every sheet
          — that's what makes it a template step — so offering it beside work in progress is
          offering to throw that work away. Adding another sheet is the ＋ above. */}
      {!sheets.length && (
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          <Button variant="outline" size="sm" disabled={busy} onClick={onStartFromPaint}>
            {busy ? (
              <Loader2 className="size-3.5 animate-spin" />
            ) : (
              <PackageOpen className="size-3.5" />
            )}
            {t("designer.startFromPaint")}
          </Button>
          <Button variant="outline" size="sm" onClick={onAddBlank}>
            <FilePlus2 className="size-3.5" /> {t("designer.blankSheet")}
          </Button>
        </div>
      )}
    </div>
  );
}

/**
 * The reference underlay's controls.
 *
 * Sits under the sheet list rather than in the layer panel, because a ghost belongs to the
 * sheet and not to the stack — it can't be reordered, selected, painted on or saved, and
 * putting it among things that can would be four wrong promises at once.
 */
function GhostPanel({
  ghost,
  sheetName,
  hasBase,
  hasGeometry,
  hasStock,
  onTrace,
  onChange,
}: {
  ghost: Ghost;
  sheetName: string;
  /** Whether the sheet still holds a template that tracing could lift out of it. */
  hasBase: boolean;
  hasGeometry: boolean;
  /** Whether the model on screen can say which of its textures are its own — bikes can. */
  hasStock: boolean;
  onTrace: () => void;
  onChange: (fn: (g: Ghost) => Ghost) => void;
}) {
  const t = useT();
  const tracing = !!ghost.template;
  // A map was built for this name and came back with nothing on it. Distinct from "not built
  // yet" (`wireFor` still null), which is why both halves are checked.
  const noMatch = ghost.showWire && ghost.wireFor === sheetName && !ghost.wire;
  // The same pair, asked of the model's textures rather than of its triangles.
  const noStock = hasStock && ghost.showStock && ghost.stockFor === sheetName && !ghost.stock;
  // Nothing to trace: a blank sheet never had a template, and one that did has already had it
  // lifted. The UV map is the guide that still applies, so the button says so rather than
  // sitting there dead with no explanation.
  const canTrace = hasBase || tracing;
  const showing = ghostShows(ghost);
  // Something to show, and an opaque template still in the sheet sitting on top of it. The
  // reference draws underneath, so this is showing nothing until the template is lifted out.
  const buried = showing && hasBase;

  return (
    <div className="rounded-lg border border-border bg-card/40 p-3.5">
      <div className="mb-2.5 flex items-center gap-2">
        <h2 className="text-[13px] font-semibold">{t("designer.reference")}</h2>
        <button
          type="button"
          className="ml-auto text-muted-foreground transition-colors hover:text-foreground disabled:opacity-30"
          disabled={!ghost.template && !ghost.stock && !ghost.wire}
          onClick={() =>
            onChange((g) => {
              // One eye over both, and it turns them off together rather than remembering
              // which was on — coming back to a "reference" that shows half of what it did
              // is the kind of state nobody is keeping track of.
              const off = ghostShows(g);
              return {
                ...g,
                showTemplate: !off,
                // Not gated on already having one, unlike the wire below: the stock texture is
                // fetched *because* this is on, so requiring it first would be a switch that
                // could never be turned back on.
                showStock: !off,
                showWire: !off && !!g.wire,
                // Faded all the way out counts as hidden, so switching back on has to undo
                // that too. Otherwise the eye says "showing" over a reference at zero.
                opacity: !off && g.opacity <= 0 ? EMPTY_GHOST.opacity : g.opacity,
              };
            })
          }
          title={t(showing ? "designer.hide" : "designer.show")}
        >
          {showing ? <Eye className="size-3.5" /> : <EyeOff className="size-3.5" />}
        </button>
      </div>

      <div className="mb-2 flex flex-wrap gap-1.5">
        <GhostToggle
          icon={<LayersIcon className="size-3.5" />}
          label={t("designer.traceTemplate")}
          title={t(canTrace ? "designer.traceHint" : "designer.noTemplate")}
          on={tracing && ghost.showTemplate}
          disabled={!canTrace}
          onClick={() => {
            // Already lifted and visible — this press is asking to see it in the paint again,
            // so put it back. Otherwise lift it, or just show what has already been lifted.
            if (!tracing || ghost.showTemplate) onTrace();
            else onChange((g) => ({ ...g, showTemplate: true }));
          }}
        />
        <GhostToggle
          icon={<Bike className="size-3.5" />}
          label={t("designer.stockTexture")}
          title={t(hasStock ? "designer.stockHint" : "designer.noStock")}
          on={ghost.showStock}
          disabled={!hasStock}
          onClick={() => onChange((g) => ({ ...g, showStock: !g.showStock }))}
        />
        <GhostToggle
          icon={<Grid3x3 className="size-3.5" />}
          label={t("designer.uvMap")}
          title={t(hasGeometry ? "designer.uvHint" : "designer.noGeometry")}
          on={ghost.showWire}
          disabled={!hasGeometry}
          onClick={() => onChange((g) => ({ ...g, showWire: !g.showWire }))}
        />
      </div>

      <Row label={t("designer.opacity")}>
        <Slider
          value={ghost.opacity}
          min={0}
          max={1}
          step={0.01}
          onChange={(v) => onChange((g) => ({ ...g, opacity: v }))}
          format={(v) => `${Math.round(v * 100)}%`}
        />
      </Row>

      {/* The reference is underneath, so an opaque sheet hides it completely. Saying so is
          the difference between a feature that looks broken and one that tells you the next
          move — which is the button directly above this line. */}
      {buried && (
        <p className="mt-1.5 text-[11px] leading-snug text-amber-500/90">
          {t("designer.ghostBuried")}
        </p>
      )}

      {/* The name binds the sheet to the mesh, so a name nothing asks for is worth saying
          plainly — it is the same mistake that makes a paint load and show nothing. */}
      {noMatch && (
        <p className="mt-1.5 text-[11px] leading-snug text-destructive">
          {t("designer.uvNoMatch", { name: sheetName.trim() })}
        </p>
      )}

      {/* Not the same miss as the one above, and worth saying separately: a model can draw a
          texture without shipping one of its own, so a sheet every paint replaces has islands
          to show and no stock artwork behind them. Said without claiming which of the two it
          is — with the UV map off there is nothing here that knows. */}
      {noStock && !noMatch && (
        <p className="mt-1.5 text-[11px] leading-snug text-faint">
          {t("designer.stockNoMatch", { name: sheetName.trim() })}
        </p>
      )}
    </div>
  );
}

function GhostToggle({
  icon,
  label,
  title,
  on,
  disabled,
  onClick,
}: {
  icon: React.ReactNode;
  label: string;
  title: string;
  on: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={title}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] font-medium transition-colors disabled:opacity-35",
        on ? "border-primary/60 bg-primary/10 text-foreground" : "border-border text-faint",
      )}
    >
      {icon}
      {label}
    </button>
  );
}

function LayerList({
  layers,
  selectedId,
  onSelect,
  onToggle,
  onRemove,
  onReorder,
  onAdd,
}: {
  layers: Layer[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onToggle: (id: string, visible: boolean) => void;
  onRemove: (id: string) => void;
  onReorder: (id: string, delta: number) => void;
  onAdd: () => void;
}) {
  const t = useT();
  return (
    <div className="rounded-lg border border-border bg-card/40 p-3.5">
      <div className="mb-2.5 flex items-center gap-2">
        <h2 className="text-[13px] font-semibold">{t("designer.layers")}</h2>
        <button
          type="button"
          className="ml-auto text-muted-foreground transition-colors hover:text-foreground"
          onClick={onAdd}
          title={t("designer.addPaint")}
        >
          <Plus className="size-4" />
        </button>
      </div>
      {!layers.length ? (
        <p className="text-[11px] leading-snug text-faint">{t("designer.noLayers")}</p>
      ) : (
        <div className="flex flex-col gap-1">
          {/* Top of the list is the top of the stack, which is how a layer panel reads —
              the array itself is bottom-first because that's the order it's drawn in. */}
          {[...layers].reverse().map((layer) => (
            <div
              key={layer.id}
              className={cn(
                "flex items-center gap-1.5 rounded-md border px-1.5 py-1 text-[11.5px] transition-colors",
                layer.id === selectedId ? "border-primary bg-primary/10" : "border-border",
              )}
            >
              <button
                type="button"
                className="flex-none text-muted-foreground hover:text-foreground"
                onClick={() => onToggle(layer.id, !layer.visible)}
                title={t(layer.visible ? "designer.hide" : "designer.show")}
              >
                {layer.visible ? <Eye className="size-3.5" /> : <EyeOff className="size-3.5" />}
              </button>
              <button
                type="button"
                className="min-w-0 flex-1 truncate text-left"
                onClick={() => onSelect(layer.id)}
              >
                {layer.kind === "text" ? layer.text || layer.name : layer.name}
              </button>
              <button
                type="button"
                className="flex-none px-0.5 text-muted-foreground hover:text-foreground"
                onClick={() => onReorder(layer.id, 1)}
                title={t("designer.raise")}
              >
                ↑
              </button>
              <button
                type="button"
                className="flex-none px-0.5 text-muted-foreground hover:text-foreground"
                onClick={() => onReorder(layer.id, -1)}
                title={t("designer.lower")}
              >
                ↓
              </button>
              <button
                type="button"
                className="flex-none text-muted-foreground hover:text-destructive"
                onClick={() => onRemove(layer.id)}
                title={t("common.remove")}
              >
                <Trash2 className="size-3.5" />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
