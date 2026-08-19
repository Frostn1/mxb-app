import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { useT } from "../../../i18n/context";
import { layerCorners } from "./composite";
import type { Ghost } from "./ghost";
import {
  faceAt,
  partAt,
  partPath,
  partsAt,
  sideAt,
  spotAt,
  type Face,
  type Side,
  type UvPart,
} from "./uv";
import { hitTest, type Layer, type Sheet } from "./layers";
import { constrained, hasTip, isDragTool, type PaintTool, type Point } from "./paint";

/** How far the pointer must travel across the sheet, in uv, before the 3D spot is redrawn. */
const SPOT_STEP = 0.004;

/** Half-edge of a drawn corner handle, and how far off one a press still counts, in view px. */
const HANDLE = 3.5;
const GRAB = 9;

/** Side of one checkerboard square, in view pixels. Fixed, so zoom doesn't resize the board. */
const CELL = 8;

/**
 * The transparency checkerboard, as a two-square tile the canvas repeats for us.
 *
 * Drawn a square at a time this is a few thousand `fillRect` calls to cover the sheet, repeated
 * on every repaint — which during a stroke is every frame, to redraw a pattern that never
 * changes. One tile handed to `createPattern` is the same picture in a single fill.
 */
let tile: HTMLCanvasElement | null = null;
function checkerTile(): HTMLCanvasElement {
  if (!tile) {
    tile = document.createElement("canvas");
    tile.width = CELL * 2;
    tile.height = CELL * 2;
    const ctx = tile.getContext("2d");
    if (ctx) {
      ctx.fillStyle = "#2a2c33";
      ctx.fillRect(0, 0, CELL * 2, CELL * 2);
      ctx.fillStyle = "#33363e";
      ctx.fillRect(0, 0, CELL, CELL);
      ctx.fillRect(CELL, CELL, CELL, CELL);
    }
  }
  return tile;
}

/** The range a corner drag can take a layer to. The same as the inspector's slider, so the
 *  two controls can't disagree about how big a logo is allowed to get. */
const MIN_SCALE = 0.05;
const MAX_SCALE = 4;

/** How long after a press a second one still counts as a double-click, and how far it may move.
 *  The platform's own thresholds aren't readable from a webview; these are the usual ones, and
 *  the slop matters more than the interval on a touchpad, where a "stationary" finger drifts. */
const DOUBLE_MS = 400;
const DOUBLE_SLOP = 6;

interface CanvasStageProps {
  sheet: Sheet;
  /** The sheet's composite, already drawn. Blitted here rather than redrawn. */
  source: HTMLCanvasElement | null;
  /** Bumped by the editor whenever the composite changes, to force a repaint. */
  version: number;
  /** The reference underlay, if any. Drawn here and nowhere else — see `ghost.ts`. */
  ghost: Ghost | null;
  /** The model's bodywork for this sheet, for naming what the pointer is over. */
  parts: UvPart[];
  /**
   * What the pointer is on, as triangle references into the model — null off the sheet.
   *
   * Reported so the 3D view can light the piece up. Which flank a region paints is the one
   * thing a word has never settled on its own: "left" means the bike's left, the preview
   * opens looking at that flank from the front, and the two readings of the sentence differ
   * by exactly the mistake this is here to end.
   */
  onHoverSpot?: (tris: Int32Array | null) => void;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** Drag, in sheet pixels. */
  onMove: (id: string, dx: number, dy: number) => void;
  /** Corner drag, as the layer's new absolute scale. */
  onScale: (id: string, scale: number) => void;
  /** What the pointer does here. `move` is the select-and-drag behaviour. */
  tool: PaintTool;
  /** Brush and eraser diameter in sheet pixels, for the cursor. */
  brushSize: number;
  /** True when there is a paint layer for a stroke to land on. */
  canPaint: boolean;
  onPaintStart: (at: Point) => void;
  onPaintMove: (points: Point[], constrain: boolean) => void;
  onPaintEnd: () => void;
  className?: string;
}

/**
 * The 2D half of the editor: the sheet on a checkerboard, with the selected layer outlined.
 *
 * Draws the composite rather than the layers — the sheet is composited once by the editor and
 * this only ever blits it, so what's on screen and what would be saved cannot drift apart.
 *
 * Zoom and pan are view state and live here; nothing in them reaches the sheet. Strokes are the
 * other way round: this turns pointer events into sheet coordinates and hands them up, because
 * the pixels they land on belong to the layer, not to the view they were drawn through.
 *
 * The ghost is drawn here for the same reason, read backwards: a guide that never enters the
 * composite cannot reach the file, whatever anyone later does to the save path.
 */
export function CanvasStage({
  sheet,
  source,
  version,
  ghost,
  parts,
  onHoverSpot,
  selectedId,
  onSelect,
  onMove,
  onScale,
  tool,
  brushSize,
  canPaint,
  onPaintStart,
  onPaintMove,
  onPaintEnd,
  className,
}: CanvasStageProps) {
  const t = useT();
  const wrapRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<HTMLCanvasElement>(null);
  // The brush cursor, moved by writing to its style rather than through state — a ring that
  // re-rendered the stage on every mouse move would redraw the whole sheet to move a circle.
  const cursorRef = useRef<HTMLDivElement>(null);
  // The checkerboard pattern, built once. Patterns belong to the context they were made from,
  // and this canvas keeps the same one for its whole life — resizing it doesn't replace it.
  const checker = useRef<CanvasPattern | null>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  // Where the pointer went down, and what it was doing — a drag of a layer, or of the view.
  const drag = useRef<{ id: string | null; x: number; y: number } | null>(null);
  // A corner drag: the layer's centre and the distance the press was from it, both in sheet
  // pixels, plus the scale it started at. Scale comes out as a ratio of distances, so the
  // grabbed corner stays under the pointer however the view is zoomed or panned mid-drag.
  const sizing = useRef<{ id: string; cx: number; cy: number; dist: number; scale: number } | null>(
    null,
  );
  const painting = useRef(false);
  // The last left press, for recognising a double-click ourselves. Null once one has been
  // recognised, so a run of fast clicks pairs up rather than firing on every press after the
  // second. See `onPointerDown` for why the DOM's own `dblclick` can't be used here.
  const lastPress = useRef<{ t: number; x: number; y: number } | null>(null);
  // Where the 3D highlight was last asked for, so it is only asked again once the pointer
  // has moved far enough for the answer to look different.
  const spotFrom = useRef<{ u: number; v: number } | null>(null);
  // Which corner the pointer is over, purely so the cursor can say the layer is resizable.
  const [overHandle, setOverHandle] = useState(-1);
  // The piece of bodywork under the pointer. The whole reason the UV map is worth having is
  // being able to answer "what am I painting on" — an outline you have to interpret answers it
  // less well than a name does.
  const [overPart, setOverPart] = useState<UvPart | null>(null);
  // The largest part sharing that point, when something else does. Two parts over one texel is
  // ordinary — and it is also the whole reason a small bracket used to answer for the panel it
  // sits on, so the overlap is named rather than resolved out of sight.
  const [overAlso, setOverAlso] = useState<string | null>(null);
  // And which flank of the bike that piece is, at the point being pointed at rather than over
  // the part as a whole — the two are different answers wherever the flanks share an island.
  const [overSide, setOverSide] = useState<Side | null>(null);
  // Which way the surface there faces. The answer that says a rear fender's island has its
  // underside in it, which is otherwise only discoverable by painting and looking.
  const [overFace, setOverFace] = useState<Face | null>(null);
  // Whether the sheet is currently washed left/right — the islands are up, they are visible,
  // and this model was assembled well enough to have sides at all.
  const washed =
    !!ghost?.showWire && !!ghost.wire && ghost.opacity > 0 && parts.some((part) => part.flanks);
  // The press and current point of a gradient or shape drag, so it can be shown while it's
  // being aimed. Only ever set between press and release.
  const [guide, setGuide] = useState<{ from: Point; to: Point } | null>(null);

  const paints = tool !== "move" && canPaint;

  /**
   * The hovered part's outline, built when the hover moves rather than when the view repaints.
   *
   * A part is a few thousand triangles and the repaint below runs on every frame of a stroke,
   * with the pointer parked on whatever it was over when the press happened — so rebuilding the
   * path there was paying for the same unchanged shape once per sample of the drawing.
   */
  const overPath = useMemo(
    () => (overPart ? partPath(overPart, sheet.width, sheet.height) : null),
    [overPart, sheet.width, sheet.height],
  );

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setBox({ w: width, h: height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Scale that fits the sheet in the box, then the user's zoom on top.
  const fit =
    box.w && box.h ? Math.min(box.w / sheet.width, box.h / sheet.height) * 0.92 : 0;
  const scale = fit * zoom;
  const originX = box.w / 2 + pan.x;
  const originY = box.h / 2 + pan.y;

  /** Client coordinates → sheet pixels. */
  const toSheet = useCallback(
    (clientX: number, clientY: number) => {
      const rect = viewRef.current?.getBoundingClientRect();
      if (!rect || !scale) return null;
      const vx = clientX - rect.left - originX;
      const vy = clientY - rect.top - originY;
      return { x: vx / scale + sheet.width / 2, y: vy / scale + sheet.height / 2 };
    },
    [originX, originY, scale, sheet.width, sheet.height],
  );

  /** Sheet pixels → the view, for drawing overlays over the blitted composite. */
  const toView = useCallback(
    (p: Point): [number, number] => [
      originX + (p.x - sheet.width / 2) * scale,
      originY + (p.y - sheet.height / 2) * scale,
    ],
    [originX, originY, scale, sheet.width, sheet.height],
  );

  /**
   * The selected layer's corners in view space — what's drawn as handles, and what's grabbed.
   *
   * Computed rather than remembered from the last paint, so a handle is always tested against
   * where the corner is now. A resize moves all four while the pointer is still down, and a
   * cached set would have the grab point drift away from the square under the cursor.
   */
  const handles = useCallback((): [number, number][] => {
    const selected = sheet.layers.find((l) => l.id === selectedId);
    // Paint layers are the sheet — see `layers.ts`. There is nothing to resize, and the
    // corners would sit on the sheet edge where they'd catch every click near the border.
    if (!selected || selected.kind === "paint") return [];
    return layerCorners(selected).map((c) => toView({ x: c[0], y: c[1] }));
  }, [sheet.layers, selectedId, toView]);

  /**
   * The corner under a client point, or -1.
   *
   * Corners are still *drawn* on a layer too small to have grabbable ones — the box is how you
   * see what's selected, and it has to survive being small. What's dropped here is the grab:
   * four 18px zones on a box 20px across leave no middle to take hold of, and a layer you can
   * resize but can no longer drag is a worse trade than one that needs a zoom first.
   */
  const handleAt = useCallback(
    (clientX: number, clientY: number) => {
      const rect = viewRef.current?.getBoundingClientRect();
      if (!rect) return -1;
      const pts = handles();
      if (!pts.length) return -1;
      const xs = pts.map((p) => p[0]);
      const ys = pts.map((p) => p[1]);
      const room = Math.min(Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys));
      if (room < GRAB * 3) return -1;

      const vx = clientX - rect.left;
      const vy = clientY - rect.top;
      for (let i = 0; i < pts.length; i += 1) {
        if (Math.abs(pts[i][0] - vx) <= GRAB && Math.abs(pts[i][1] - vy) <= GRAB) return i;
      }
      return -1;
    },
    [handles],
  );

  // Repaint whenever anything visible changes. Not a `useEffect` on the layers themselves:
  // `version` is the editor's single "the composite moved" signal, and following it keeps
  // this from having an opinion about what counts as a change.
  useEffect(() => {
    const canvas = viewRef.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx || !box.w || !box.h) return;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    if (canvas.width !== box.w * dpr || canvas.height !== box.h * dpr) {
      canvas.width = box.w * dpr;
      canvas.height = box.h * dpr;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, box.w, box.h);
    if (!source || !scale) return;

    const w = sheet.width * scale;
    const h = sheet.height * scale;
    const left = originX - w / 2;
    const top = originY - h / 2;

    // Checkerboard first, so transparent parts of the sheet read as transparent rather than
    // as black — which on a livery is a real colour and would be badly misleading.
    ctx.save();
    ctx.beginPath();
    ctx.rect(left, top, w, h);
    ctx.clip();
    // Translated rather than offset per square, so the tile still starts at the sheet's own
    // corner: the pattern is laid out in the space the current transform describes.
    ctx.translate(left, top);
    const board = checker.current ?? ctx.createPattern(checkerTile(), "repeat");
    checker.current = board;
    // Flat where a pattern couldn't be built. It still says "nothing is drawn here", which is
    // the whole job — a board is only the clearer way of saying it.
    ctx.fillStyle = board ?? "#2a2c33";
    ctx.fillRect(0, 0, w, h);
    ctx.restore();

    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";

    // The whole reference goes *under* the drawing — that is what makes it a ghost rather than
    // an overlay. Both halves show through wherever the sheet is still transparent, which is
    // exactly where there is nothing drawn yet and exactly where you need to know which piece
    // of bodywork you are about to paint on.
    //
    // The consequence is worth being clear about: a sheet that still has its template baked in
    // is opaque, and an underlay beneath it is invisible. That is what the trace toggle is for
    // — it lifts the template out and leaves the sheet transparent, and the two features are
    // meant to be used together.
    if (ghost && ghost.opacity > 0) {
      ctx.save();
      ctx.globalAlpha = ghost.opacity;
      // Artwork first, islands over it: the outlines have to stay readable against whatever
      // is being traced, and they are the thinnest of the marks. The model's own texture goes
      // under the template rather than over it — someone who lifted a paint out to trace it
      // asked for *that* paint, and the stock plastics are the fallback beneath it.
      if (ghost.showStock && ghost.stock) {
        ctx.drawImage(ghost.stock, left, top, w, h);
      }
      if (ghost.showTemplate && ghost.template) {
        ctx.drawImage(ghost.template, left, top, w, h);
      }
      if (ghost.showWire && ghost.wire) {
        ctx.drawImage(ghost.wire, left, top, w, h);
      }
      ctx.restore();
    }

    ctx.drawImage(source, left, top, w, h);

    // Sheet edge, so you can see where the texture stops.
    ctx.strokeStyle = "rgba(255,255,255,0.18)";
    ctx.lineWidth = 1;
    ctx.strokeRect(left + 0.5, top + 0.5, w - 1, h - 1);

    // A paint layer's box is the sheet's own edge, which is already drawn — outlining it again
    // would just put a blue border round everything for as long as a brush is selected. That
    // exclusion lives in `handles`, which is also what the corner grab tests against, so the
    // squares you can see and the squares you can grab are one list.
    const pts = handles();
    if (pts.length) {
      ctx.beginPath();
      ctx.moveTo(pts[0][0], pts[0][1]);
      for (const [px, py] of pts.slice(1)) ctx.lineTo(px, py);
      ctx.closePath();
      ctx.strokeStyle = "#3b82f6";
      ctx.lineWidth = 1.5;
      ctx.stroke();
      // Filled white with a blue edge rather than solid blue: a handle has to look like
      // something you take hold of, or a resizable layer reads as a decorated one.
      ctx.lineWidth = 1;
      for (const [px, py] of pts) {
        ctx.fillStyle = "#ffffff";
        ctx.fillRect(px - HANDLE, py - HANDLE, HANDLE * 2, HANDLE * 2);
        ctx.strokeRect(px - HANDLE, py - HANDLE, HANDLE * 2, HANDLE * 2);
      }
    }

    // The piece under the pointer, picked out of the map. Only while the map is on: a highlight
    // that appeared over a livery with no islands showing would be an outline from nowhere.
    if (overPart && overPath && ghost?.showWire && ghost.wire) {
      ctx.save();
      ctx.translate(left, top);
      ctx.scale(w / sheet.width, h / sheet.height);
      ctx.fillStyle = `hsla(${overPart.hue}, 85%, 65%, 0.18)`;
      ctx.fill(overPath, "nonzero");
      ctx.restore();
    }

    // Where a gradient runs, or what a shape will cover. The stroke itself is already visible
    // in the composite underneath; this is the part you aim with.
    if (guide) {
      const [ax, ay] = toView(guide.from);
      const [bx, by] = toView(guide.to);
      ctx.save();
      ctx.setLineDash([5, 4]);
      ctx.strokeStyle = "rgba(255,255,255,0.75)";
      ctx.lineWidth = 1;
      ctx.beginPath();
      if (tool === "gradient" || tool === "line") {
        ctx.moveTo(ax, ay);
        ctx.lineTo(bx, by);
      } else {
        ctx.rect(Math.min(ax, bx), Math.min(ay, by), Math.abs(bx - ax), Math.abs(by - ay));
      }
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.fillStyle = "rgba(255,255,255,0.75)";
      for (const [px, py] of [
        [ax, ay],
        [bx, by],
      ]) {
        ctx.fillRect(px - 2, py - 2, 4, 4);
      }
      ctx.restore();
    }
  }, [
    box,
    scale,
    originX,
    originY,
    sheet,
    source,
    ghost,
    overPart,
    overPath,
    version,
    guide,
    tool,
    toView,
    handles,
  ]);

  /** Put the brush ring where the pointer is, at the size it will actually paint. */
  const moveCursor = useCallback(
    (clientX: number, clientY: number) => {
      const el = cursorRef.current;
      const rect = viewRef.current?.getBoundingClientRect();
      if (!el || !rect) return;
      const d = Math.max(6, brushSize * scale);
      el.style.width = `${d}px`;
      el.style.height = `${d}px`;
      el.style.transform = `translate(${clientX - rect.left - d / 2}px, ${
        clientY - rect.top - d / 2
      }px)`;
    },
    [brushSize, scale],
  );

  /**
   * Fill the view with one piece of bodywork.
   *
   * A shroud is a tenth of a 2048² sheet, and reaching it by wheel-and-drag means aiming at a
   * shape whose edges are the thing you were trying to see. Double-click is the gesture because
   * it costs no mode and no modifier: a paint tool lays its first dab where you clicked, and
   * the view arrives at the part you were already pointing at.
   */
  const focusPart = useCallback(
    (part: UvPart) => {
      if (!fit || !box.w || !box.h) return;
      // A degenerate island — one point, or a sliver — would divide the view to infinity.
      const pw = Math.max(1, (part.maxU - part.minU) * sheet.width);
      const ph = Math.max(1, (part.maxV - part.minV) * sheet.height);
      // 0.8 leaves the part's surroundings in frame. A panel cut exactly to the edges gives
      // nothing to judge where a decal sits relative to the seam beside it.
      const z = Math.min(8, Math.max(0.25, (Math.min(box.w / pw, box.h / ph) * 0.8) / fit));
      const s = fit * z;
      const cx = ((part.minU + part.maxU) / 2) * sheet.width;
      const cy = ((part.minV + part.maxV) / 2) * sheet.height;
      setZoom(z);
      // Pan is measured from the sheet's centre, which is where the blit is anchored.
      setPan({ x: -(cx - sheet.width / 2) * s, y: -(cy - sheet.height / 2) * s });
    },
    [box.w, box.h, fit, sheet.width, sheet.height],
  );

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const at = toSheet(e.clientX, e.clientY);
      if (!at) return;
      // The second press of a double-click, timed here rather than taken from the DOM's own
      // `dblclick`. The canvas captures the pointer on every press so a stroke survives leaving
      // the element, and a captured pointer is exactly the case where WebKit stops delivering
      // `dblclick` — so the gesture has to be recognised from the presses we already get.
      if (e.button === 0) {
        const prev = lastPress.current;
        const near = prev && Math.hypot(e.clientX - prev.x, e.clientY - prev.y) < DOUBLE_SLOP;
        const quick = prev && e.timeStamp - prev.t < DOUBLE_MS;
        lastPress.current = { t: e.timeStamp, x: e.clientX, y: e.clientY };
        if (near && quick && parts.length) {
          const part = partAt(parts, at.x / sheet.width, at.y / sheet.height);
          if (part) {
            // Forget the pair, so a third press starts a new one rather than focusing again
            // on every press of a rapid series.
            lastPress.current = null;
            focusPart(part);
            return;
          }
        }
      }
      e.currentTarget.setPointerCapture(e.pointerId);
      // Middle and right drag pan, whatever the tool — panning while painting matters more
      // than it does while dragging a logo, because a stroke needs the part you can't see.
      // Not mid-stroke, though: the view moving under a brush that is still down would drag
      // the stroke sideways across the sheet.
      if (e.button === 1 || e.button === 2) {
        if (!painting.current) drag.current = { id: null, x: e.clientX, y: e.clientY };
        return;
      }
      if (paints) {
        painting.current = true;
        if (isDragTool(tool)) setGuide({ from: at, to: at });
        onPaintStart(at);
        return;
      }
      // Corners before contents. A handle sits on the layer's own edge, so hit-testing first
      // would answer "you clicked the layer" every time and there would be no way to resize
      // anything — and the corner of a small layer often overlaps a bigger one behind it.
      const corner = handleAt(e.clientX, e.clientY);
      const selected = sheet.layers.find((l) => l.id === selectedId);
      if (corner >= 0 && selected) {
        const dist = Math.hypot(at.x - selected.x, at.y - selected.y);
        // A press exactly on the centre has no distance to take a ratio of. Can only happen
        // on a layer scaled down to nothing, and dropping the drag beats dividing by zero.
        if (dist > 0.5) {
          sizing.current = {
            id: selected.id,
            cx: selected.x,
            cy: selected.y,
            dist,
            scale: selected.scale,
          };
          return;
        }
      }
      const hit = hitTest(sheet.layers, at.x, at.y);
      onSelect(hit?.id ?? null);
      drag.current = { id: hit?.id ?? null, x: e.clientX, y: e.clientY };
    },
    [
      focusPart,
      handleAt,
      onPaintStart,
      onSelect,
      paints,
      parts,
      selectedId,
      sheet.layers,
      sheet.width,
      sheet.height,
      toSheet,
      tool,
    ],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      moveCursor(e.clientX, e.clientY);

      const size = sizing.current;
      if (size) {
        const p = toSheet(e.clientX, e.clientY);
        if (!p) return;
        // Scale as the ratio of distances from the layer's centre, which is where it grows
        // from — the same fixed point the inspector's slider uses, so dragging a corner and
        // typing a number move the layer's pixels the same way.
        const next = (size.scale * Math.hypot(p.x - size.cx, p.y - size.cy)) / size.dist;
        onScale(size.id, Math.min(MAX_SCALE, Math.max(MIN_SCALE, next)));
        return;
      }

      if (painting.current) {
        // Every sample the browser had, not just the one it chose to deliver. A fast stroke
        // arrives as a handful of far-apart points otherwise, and the brush would corner.
        const raw = e.nativeEvent.getCoalescedEvents?.() ?? [];
        const samples = raw.length ? raw : [e.nativeEvent];
        const points: Point[] = [];
        for (const s of samples) {
          const p = toSheet(s.clientX, s.clientY);
          if (p) points.push(p);
        }
        if (!points.length) return;
        onPaintMove(points, e.shiftKey);
        if (isDragTool(tool)) {
          const raw = points[points.length - 1];
          // Through the same constraint the stroke uses, so the guide shows the square that is
          // actually being drawn rather than the rectangle the pointer traced.
          setGuide((g) =>
            g ? { from: g.from, to: e.shiftKey ? constrained(g.from, raw, tool) : raw } : g,
          );
        }
        return;
      }

      const d = drag.current;
      if (!d) {
        // Nothing is being dragged, so this is a hover: say whether a corner is under the
        // pointer, and which piece of bodywork it is over. `setState` with an unchanged value
        // doesn't re-render, so this only costs anything on the moves that cross a boundary.
        setOverHandle(paints ? -1 : handleAt(e.clientX, e.clientY));
        if (parts.length) {
          const at = toSheet(e.clientX, e.clientY);
          const u = at ? at.x / sheet.width : 0;
          const v = at ? at.y / sheet.height : 0;
          // Everything under the pointer, not just the winner: the side and the facing are
          // asked of all of it, so a bracket sharing the panel's island can't answer for the
          // panel. `hits[0]` stays the most specific part, which is what gets outlined.
          const hits = at ? partsAt(parts, u, v) : [];
          setOverPart(hits[0] ?? null);
          setOverAlso(hits.length > 1 ? hits[hits.length - 1].label : null);
          setOverSide(hits.length ? sideAt(parts, u, v) : null);
          setOverFace(hits.length ? faceAt(parts, u, v) : null);
          // Every move the pointer has actually travelled on, rather than every event: the
          // spot is meant to track the pointer, but each one re-renders the 3D view, and a
          // trackpad emits them far faster than the picture can say anything new.
          if (onHoverSpot) {
            const last = spotFrom.current;
            const far = !last || Math.hypot(u - last.u, v - last.v) > SPOT_STEP;
            if (far) {
              spotFrom.current = at ? { u, v } : null;
              onHoverSpot(at ? spotAt(parts, u, v) : null);
            }
          }
        }
        return;
      }
      if (!scale) return;
      const dx = e.clientX - d.x;
      const dy = e.clientY - d.y;
      if (!dx && !dy) return;
      d.x = e.clientX;
      d.y = e.clientY;
      if (d.id) onMove(d.id, dx / scale, dy / scale);
      else setPan((p) => ({ x: p.x + dx, y: p.y + dy }));
    },
    [
      handleAt,
      moveCursor,
      onHoverSpot,
      onMove,
      onPaintMove,
      onScale,
      paints,
      parts,
      scale,
      sheet.height,
      sheet.width,
      toSheet,
      tool,
    ],
  );

  const endDrag = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (painting.current) {
        painting.current = false;
        setGuide(null);
        onPaintEnd();
      }
      drag.current = null;
      sizing.current = null;
      if (e.currentTarget.hasPointerCapture(e.pointerId)) {
        e.currentTarget.releasePointerCapture(e.pointerId);
      }
    },
    [onPaintEnd],
  );

  const onWheel = useCallback((e: React.WheelEvent<HTMLCanvasElement>) => {
    setZoom((z) => Math.min(8, Math.max(0.25, z * (e.deltaY < 0 ? 1.12 : 1 / 1.12))));
  }, []);

  const reset = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);


  const ringed = paints && hasTip(tool);
  // Corners come out of `layerCorners` clockwise from the top left, so 0/2 are one diagonal
  // and 1/3 the other. A rotated layer makes this an approximation — the arrow can end up a
  // notch off the true diagonal — but the thing it has to say is "this corner resizes", and
  // it says that at every angle.
  const resizeCursor = overHandle < 0 ? null : overHandle % 2 === 0 ? "nwse-resize" : "nesw-resize";

  return (
    <div
      ref={wrapRef}
      className={cn("relative min-h-0 overflow-hidden rounded-lg border border-border bg-[#16171c]", className)}
    >
      <canvas
        ref={viewRef}
        className="absolute inset-0 h-full w-full touch-none"
        style={{
          width: box.w,
          height: box.h,
          // The ring is the cursor for a brush, so the arrow would be a second one. Everything
          // else that paints aims at a point, and a crosshair is the thing that says so.
          cursor: ringed ? "none" : paints ? "crosshair" : (resizeCursor ?? "default"),
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onPointerLeave={() => {
          if (cursorRef.current) cursorRef.current.style.opacity = "0";
          setOverHandle(-1);
          setOverPart(null);
          spotFrom.current = null;
          onHoverSpot?.(null);
        }}
        onPointerEnter={() => {
          if (cursorRef.current) cursorRef.current.style.opacity = "1";
        }}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
      />
      <div
        ref={cursorRef}
        aria-hidden
        className={cn(
          "pointer-events-none absolute left-0 top-0 rounded-full border border-white/80 shadow-[0_0_0_1px_rgba(0,0,0,0.55)]",
          ringed ? "block" : "hidden",
        )}
      />
      {/* What the wash over the islands means. Only while the islands are showing, and only on
          a model that can say — a legend for a colour nobody can see is furniture. It sits
          here rather than in the readout below because the wash is on screen whether or not
          the pointer is over anything, and that is exactly when it needs explaining. */}
      {washed && (
        <div
          className="pointer-events-auto absolute left-2 top-2 flex items-center gap-2 rounded-md bg-white/[0.06] px-2 py-1 text-[11px] leading-none text-white/45"
          title={t("designer.flankWashHint")}
        >
          <span className="flex items-center gap-1">
            <span className="size-2 rounded-[2px] bg-[hsl(28_95%_55%)]" />
            {t("designer.flank.left")}
          </span>
          <span className="flex items-center gap-1">
            <span className="size-2 rounded-[2px] bg-[hsl(205_95%_60%)]" />
            {t("designer.flank.right")}
          </span>
        </div>
      )}
      <div className="pointer-events-none absolute bottom-2 left-2 flex items-center gap-2 rounded-md bg-white/[0.06] px-2 py-1 text-[11px] leading-none text-white/45">
        <span>
          {sheet.width}×{sheet.height}
        </span>
        <span>·</span>
        <span>{Math.round(zoom * 100)}%</span>
        {/* The answer to "what am I painting on", in words. Worth the corner it takes: the
            islands say a panel is *there*, and only this says which panel it is. */}
        {overPart && (
          <>
            <span>·</span>
            {/* The node first, because a group's name is whatever its author typed and the node
                is the bike's own part — `chassis` still means the shrouds on a pack that calls
                them `Metal.027`. */}
            {overPart.owner && <span className="text-white/70">{overPart.owner}</span>}
            <span className="max-w-[190px] truncate" title={t("designer.focusHint")}>
              {overAlso ? t("designer.partOver", { part: overPart.label, over: overAlso }) : overPart.label}
            </span>
            {/* Which flank, when the model can say. `both` is the one that saves an
                afternoon: it means this island is worn by each side of the bike. */}
            {overSide && overSide !== "centre" && (
              <span
                // The same colours the sheet is washed with, so the word and the region under
                // the pointer are recognisably the same answer.
                className={cn(
                  overSide === "left" && "text-[hsl(28_95%_62%)]",
                  overSide === "right" && "text-[hsl(205_95%_66%)]",
                  overSide === "both" && "text-white/45",
                )}
                title={overSide === "both" ? t("designer.flankSharedHint") : undefined}
              >
                {t(`designer.flank.${overSide}` as "designer.flank.left")}
              </span>
            )}
            {/* Only the answers worth acting on. "Top" is where a livery goes anyway, so
                saying it every time would bury the one that means "nobody will see this". */}
            {(overFace === "under" || overFace === "both") && (
              <span
                className="text-amber-300/70"
                title={t(`designer.faceHint.${overFace}` as "designer.faceHint.under")}
              >
                {t(`designer.face.${overFace}` as "designer.face.under")}
              </span>
            )}
          </>
        )}
      </div>
      <button
        type="button"
        onClick={reset}
        className="absolute bottom-2 right-2 rounded-md bg-white/[0.06] px-2 py-1 text-[11px] leading-none text-white/45 transition-colors hover:text-white/80"
      >
        {t("designer.resetView")}
      </button>
    </div>
  );
}

export type { Layer };
