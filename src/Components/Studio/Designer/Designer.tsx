import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  Eye,
  EyeOff,
  FilePlus2,
  Grid3x3,
  ImagePlus,
  Layers as LayersIcon,
  Loader2,
  PackageOpen,
  PaintRoller,
  PanelLeftClose,
  PanelLeftOpen,
  Save,
  Trash2,
  Type as TypeIcon,
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
import { IMAGE_EXTS, PaintDestBar, usePaintDest } from "../paintDest";
import { CanvasStage } from "./CanvasStage";
import { Row, Slider } from "./controls";
import { PreviewPanel } from "./PreviewPanel";
import { LayerInspector } from "./LayerInspector";
import { PaintTools } from "./PaintTools";
import { bitmapFromRgba, composite, sheetTexture, toPng } from "./composite";
import { EMPTY_GHOST, ghostShows, type Ghost } from "./ghost";
import { partPath, uvParts, uvWireframe, type UvPart } from "./uv";
import {
  blankSheet,
  imageLayer,
  layerExtent,
  newId,
  paintLayer,
  textLayer,
  unionRegion,
  type Layer,
  type PaintLayer,
  type Region,
  type Sheet,
} from "./layers";
import {
  DEFAULT_PAINT,
  PaintHistory,
  Stroke,
  TOOL_KEYS,
  type PaintSettings,
  type PaintTool,
  type Point,
} from "./paint";
import type { EdfNode } from "../../../types";

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
  const history = useRef(new PaintHistory());
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

  const active = sheets.find((s) => s.id === activeId) ?? null;
  const bump = useCallback(() => setVersion((v) => v + 1), []);

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
    const names = [...next.keys()].sort().join(" ");
    setOverrideNames((prev) => (prev === names ? prev : names));
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
  }, [sheets]);

  useEffect(() => {
    const held = textures.current;
    return () => {
      held.forEach((tex) => tex.dispose());
      held.clear();
    };
  }, []);

  const patchSheet = useCallback((id: string, fn: (s: Sheet) => Sheet) => {
    // Every route into a sheet but a stroke comes through here, and none of them says where it
    // drew. Marking the sheet wholly dirty on the way past is what lets the recomposite treat a
    // region as a promise rather than a hint: if one is there, a stroke put it there.
    dirty.current.set(id, null);
    setSheets((prev) => prev.map((s) => (s.id === id ? fn(s) : s)));
  }, []);

  const patchLayer = useCallback(
    (layerId: string, fn: (l: Layer) => Layer) => {
      if (!activeId) return;
      patchSheet(activeId, (s) => ({
        ...s,
        layers: s.layers.map((l) => (l.id === layerId ? fn(l) : l)),
      }));
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
      if (tool === "move" || !active) return;
      const selected = active.layers.find((l) => l.id === selectedId);
      if (selected?.kind === "paint") return;
      const existing = [...active.layers].reverse().find((l) => l.kind === "paint");
      if (existing) setSelectedId(existing.id);
      else addPaintLayer();
    },
    [active, addPaintLayer, selectedId],
  );

  const startPaint = useCallback(
    (at: Point) => {
      if (!target || !activeId) return;
      const next = new Stroke(target.canvas, paint, at);
      stroke.current = next;
      // Straight through rather than queued: the press is the one sample nobody would forgive a
      // frame's wait on, and a tool that puts nothing down on the press has nothing to show.
      if (next.dirty) touchPaint(activeId, target.id, next.dirty);
    },
    [activeId, paint, target, touchPaint],
  );

  const movePaint = useCallback(
    (points: Point[], constrain: boolean) => {
      const live = stroke.current;
      if (!live || !target || !activeId) return;
      live.move(points, constrain);
      if (!live.dirty) return;
      markPaint(activeId, live.dirty);
      schedulePaint(activeId, target.id);
    },
    [activeId, markPaint, schedulePaint, target],
  );

  const endPaint = useCallback(() => {
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
  }, [activeId, flushPaint, target]);

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

  // Not while the pointer is still down: the stroke redraws its layer from its own snapshot on
  // the next sample, so an undo mid-drag would be silently taken back a moment later.
  const undo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.undo(paintCanvas);
    setHistoryRev((v) => v + 1);
    if (entry) touchPaint(entry.sheetId, entry.layerId);
  }, [paintCanvas, touchPaint]);

  const redo = useCallback(() => {
    if (stroke.current) return;
    const entry = history.current.redo(paintCanvas);
    setHistoryRev((v) => v + 1);
    if (entry) touchPaint(entry.sheetId, entry.layerId);
  }, [paintCanvas, touchPaint]);

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
   * No orientation is imposed. A `.pnt` doesn't record which way up its rows are and paints in
   * the wild are stored both ways, so the editor shows exactly what the file holds — the same
   * rows the viewer renders and the game reads. A sheet that looks upside down here is upside
   * down in the file, and guessing otherwise would flip every correctly-made paint.
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
      setSheets(next);
      setActiveId(next[0]?.id ?? null);
      setSelectedId(null);
      if (nameHint) setName((n) => n || nameHint);
      bump();
      toast.success(t("designer.loadedSheets", { count: String(next.length) }));
    },
    [bump, readImage, t],
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

  const addBlankSheet = useCallback(() => {
    // Name it after a texture the chosen model actually asks for, when we know one — that's
    // the difference between a paint that shows and a paint that doesn't.
    const taken = new Set(sheets.map((s) => s.name.toLowerCase()));
    const suggested = hints.find((h) => !taken.has(h.toLowerCase())) ?? "";
    const sheet = blankSheet(suggested, BLANK_SIZE);
    setSheets((prev) => [...prev, sheet]);
    setActiveId(sheet.id);
    setSelectedId(null);
    bump();
  }, [bump, hints, sheets]);

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
      patchLayer(id, (l) => ({ ...l, x: l.x + dx, y: l.y + dy }));
      bump();
    },
    [bump, patchLayer],
  );

  /** A corner drag, as an absolute scale. Clamped by the stage to the inspector's range. */
  const scaleLayer = useCallback(
    (id: string, scale: number) => {
      patchLayer(id, (l) => ({ ...l, scale }));
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
  const parts = useMemo<UvPart[]>(
    () => (geometry ? uvParts(geometry, activeName) : []),
    [geometry, activeName],
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
  const onGeometry = useCallback((nodes: EdfNode[] | null) => {
    // Compared against a ref rather than inside a `setState` updater: an updater has to be
    // pure, and this has to invalidate the wires as well as record the mesh.
    if (geometryRef.current === nodes) return;
    geometryRef.current = nodes;
    setGeometry(nodes);
    setGhosts((gs) =>
      gs.size ? new Map([...gs].map(([id, g]) => [id, { ...g, wire: null, wireFor: null }])) : gs,
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
      if (sheet.base) {
        const template = sheet.base;
        patchSheet(sheetId, (s) => ({ ...s, base: null }));
        patchGhost(sheetId, (g) => ({ ...g, template, showTemplate: true }));
      } else if (ghost.template) {
        const base = ghost.template;
        patchSheet(sheetId, (s) => ({ ...s, base }));
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
   */
  useEffect(() => {
    if (!active || !geometry) return;
    // A half-typed name is not a name yet. Without this, every keystroke of "livery" would be
    // asked of the mesh and answered "nothing binds that", which is true and useless.
    if (!active.name.trim()) return;
    const ghost = ghosts.get(active.id) ?? EMPTY_GHOST;
    if (!ghost.showWire || ghost.wireFor === active.name) return;
    // `wireFor` records the attempt whether or not it found anything, so a name that matches
    // nothing is asked once rather than on every render. The panel reads the pair to say so —
    // out loud, because an empty overlay is indistinguishable from one still being built.
    const wire = uvWireframe(parts, active.width, active.height);
    patchGhost(active.id, (g) => ({ ...g, wire, wireFor: active.name }));
  }, [active, geometry, ghosts, parts, patchGhost]);

  // Ghosts of sheets that are gone. Each holds a decoded bitmap and a raster the size of the
  // sheet, so leaving them behind would keep a closed paint's pixels alive for the session.
  useEffect(() => {
    setGhosts((prev) => {
      if (!prev.size) return prev;
      const live = new Set(sheets.map((s) => s.id));
      if ([...prev.keys()].every((id) => live.has(id))) return prev;
      return new Map([...prev].filter(([id]) => live.has(id)));
    });
  }, [sheets]);

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
  const reorderSheet = useCallback((id: string, delta: number) => {
    setSheets((prev) => {
      const at = prev.findIndex((s) => s.id === id);
      const to = at + delta;
      if (at < 0 || to < 0 || to >= prev.length) return prev;
      const next = [...prev];
      const [moved] = next.splice(at, 1);
      next.splice(to, 0, moved);
      return next;
    });
  }, []);

  const removeSheet = useCallback(
    (id: string) => {
      setSheets((prev) => {
        const next = prev.filter((s) => s.id !== id);
        setActiveId((cur) => (cur === id ? next[0]?.id ?? null : cur));
        return next;
      });
      bump();
    },
    [bump],
  );

  /** Why saving isn't possible yet, as a translated message — or null when it is. */
  const blocked = useMemo<string | null>(() => {
    if (!sheets.length) return t("designer.needSheets");
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

  const write = useCallback(
    async (overwrite: boolean) => {
      if (!dest) return;
      setBusy(true);
      try {
        // Stage every sheet first: `paint_studio_save` packs files, and these only exist as
        // canvases until now. Composited once more on the way out so a save can't ship a
        // frame older than the screen.
        const staged = await Promise.all(
          sheets.map(async (sheet) => {
            const canvas = canvasFor(sheet);
            composite(canvas, sheet);
            const path = await paintStudioStage(sheet.name.trim(), await toPng(canvas));
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
        toast.success(t("paints.saved", { path: outcome.path }));
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(false);
      }
    },
    [canvasFor, dest, name, sheets, t],
  );

  const save = useCallback(async () => {
    if (blocked || !dest) {
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
  }, [blocked, dest, name, t, write]);

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
          disabled={busy || !!blocked}
          title={blocked ?? undefined}
          onClick={() => void save()}
        >
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
          {t("paints.save")}
        </Button>
        <span className="h-5 w-px flex-none bg-border" />
        <Button variant="outline" size="sm" disabled={!active || busy} onClick={() => void addImage()}>
          <ImagePlus className="size-3.5" /> {t("designer.addImage")}
        </Button>
        <Button variant="outline" size="sm" disabled={!active} onClick={addText}>
          <TypeIcon className="size-3.5" /> {t("designer.addText")}
        </Button>
        <Button variant="outline" size="sm" disabled={!active} onClick={addPaintLayer}>
          <PaintRoller className="size-3.5" /> {t("designer.addPaint")}
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
            onAddBlank={addBlankSheet}
            onStartFromPaint={() => void startFromPaint()}
            busy={busy}
          />

          {active && (
            <GhostPanel
              ghost={ghostOf(active.id)}
              sheetName={active.name}
              hasBase={!!active.base}
              hasGeometry={!!geometry}
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
            />
          )}
          {selected && (
            <LayerInspector
              layer={selected}
              parts={parts}
              onClip={clipLayer}
              onFit={fitLayer}
              onChange={(fn) => {
                patchLayer(selected.id, fn);
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
              selectedId={selectedId}
              onSelect={setSelectedId}
              onMove={moveLayer}
              onScale={scaleLayer}
              tool={paint.tool}
              brushSize={paint.size}
              canPaint={!!target}
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
  onPick,
  onRename,
  onRemove,
  onReorder,
  onAddBlank,
  onStartFromPaint,
  busy,
}: {
  sheets: Sheet[];
  activeId: string | null;
  hints: string[];
  onPick: (id: string) => void;
  onRename: (id: string, value: string) => void;
  onRemove: (id: string) => void;
  onReorder: (id: string, delta: number) => void;
  onAddBlank: () => void;
  onStartFromPaint: () => void;
  busy: boolean;
}) {
  const t = useT();
  return (
    <div className="rounded-lg border border-border bg-card/40 p-3.5">
      <h2 className="mb-2.5 text-[13px] font-semibold">{t("designer.sheets")}</h2>
      <div className="flex flex-col gap-1.5">
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

      {/* The names the installed paints use. A sheet named anything else binds to nothing,
          and this is the only place the right answer is visible. */}
      {!!hints.length && (
        <p className="mt-2 text-[11px] leading-snug text-faint">
          {t("paints.expected")} {hints.join(", ")}
        </p>
      )}

      <div className="mt-2.5 flex flex-wrap gap-1.5">
        <Button variant="outline" size="sm" disabled={busy} onClick={onStartFromPaint}>
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <PackageOpen className="size-3.5" />}
          {t("designer.startFromPaint")}
        </Button>
        <Button variant="outline" size="sm" onClick={onAddBlank}>
          <FilePlus2 className="size-3.5" /> {t("designer.blankSheet")}
        </Button>
      </div>
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
  onTrace,
  onChange,
}: {
  ghost: Ghost;
  sheetName: string;
  /** Whether the sheet still holds a template that tracing could lift out of it. */
  hasBase: boolean;
  hasGeometry: boolean;
  onTrace: () => void;
  onChange: (fn: (g: Ghost) => Ghost) => void;
}) {
  const t = useT();
  const tracing = !!ghost.template;
  // A map was built for this name and came back with nothing on it. Distinct from "not built
  // yet" (`wireFor` still null), which is why both halves are checked.
  const noMatch = ghost.showWire && ghost.wireFor === sheetName && !ghost.wire;
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
          disabled={!ghost.template && !ghost.wire}
          onClick={() =>
            onChange((g) => {
              // One eye over both, and it turns them off together rather than remembering
              // which was on — coming back to a "reference" that shows half of what it did
              // is the kind of state nobody is keeping track of.
              const off = ghostShows(g);
              return {
                ...g,
                showTemplate: !off,
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

      <p className="mt-1.5 text-[11px] leading-snug text-faint">{t("designer.ghostNote")}</p>
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
}: {
  layers: Layer[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onToggle: (id: string, visible: boolean) => void;
  onRemove: (id: string) => void;
  onReorder: (id: string, delta: number) => void;
}) {
  const t = useT();
  return (
    <div className="rounded-lg border border-border bg-card/40 p-3.5">
      <h2 className="mb-2.5 text-[13px] font-semibold">{t("designer.layers")}</h2>
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
