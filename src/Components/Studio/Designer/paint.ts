/**
 * The painting half of the designer: tools, one stroke, and the way back out of it.
 *
 * No React in here. A stroke is pixels going into a canvas, and it has to keep up with a
 * pointer — routing every sample through state would put a render between the hand and the
 * mark.
 *
 * Every tool runs through the same session. Press takes a snapshot of the layer and clears a
 * scratch canvas; move draws the stroke *into the scratch*, never into the layer; render puts
 * the layer back to the snapshot and lays the scratch over it once. So the preview during the
 * drag and the result after it are the same pixels by construction, and a soft brush can't
 * bruise into dark blobs where its own stamps overlap — the overlap happens inside the scratch,
 * and the scratch is composited once.
 *
 * Render is confined to the region the tool actually marked since the last one, which is what
 * keeps that "once" affordable at sheet size. Restoring and re-laying is per pixel — the pixels
 * outside the marked region have the same two inputs they had a sample ago, so recomputing them
 * cannot change them. What the region is *for* is the rest of the chain: it is handed out
 * through `dirty` so the composite and the texture readback behind it can be told the same
 * thing, and a brush stroke stops costing a whole sheet per pointer sample at every stage.
 */

import { clampRegion, unionRegion, type Region } from "./layers";

/**
 * What the pointer does on the sheet.
 *
 * `move` is in here rather than beside it because it is the same choice: one row of buttons,
 * one thing selected, and the editor opens on `move` so it behaves exactly as it always has
 * until you reach for a brush.
 */
export type PaintTool =
  | "move"
  | "brush"
  | "eraser"
  | "gradient"
  | "fill"
  | "rect"
  | "ellipse"
  | "line";

export const PAINT_TOOLS: PaintTool[] = [
  "move",
  "brush",
  "eraser",
  "gradient",
  "fill",
  "rect",
  "ellipse",
  "line",
];

/** Tools whose result is defined by a press-drag-release, rather than by the path dragged. */
const DRAG_TOOLS = new Set<PaintTool>(["gradient", "rect", "ellipse", "line"]);

/**
 * One key per tool, the way every editor of this kind does it.
 *
 * Lives here rather than in the panel because two things need to agree on it: the panel shows
 * it in the tooltip, and the Designer is what actually listens for it.
 */
export const TOOL_KEYS: Record<PaintTool, string> = {
  move: "v",
  brush: "b",
  eraser: "e",
  gradient: "g",
  fill: "f",
  rect: "r",
  ellipse: "o",
  line: "l",
};

/** Whether the tool draws between two points — so the stage knows to show the two points. */
export function isDragTool(tool: PaintTool): boolean {
  return DRAG_TOOLS.has(tool);
}

/** Whether the tool has a round tip, and so a cursor whose size means something. */
export function hasTip(tool: PaintTool): boolean {
  return tool === "brush" || tool === "eraser";
}

export type GradientMode = "linear" | "radial";
export type ShapeStyle = "fill" | "outline";

export interface PaintSettings {
  tool: PaintTool;
  /** What the brush, the fill and the shapes paint with, and where a gradient starts. */
  colorA: string;
  /** Where a gradient ends. */
  colorB: string;
  /** Brush and eraser diameter, in sheet pixels. */
  size: number;
  /** 0–1. 1 is a hard edge, 0 is all falloff. */
  hardness: number;
  /** 0–1, applied once to the whole stroke rather than to each stamp. */
  opacity: number;
  gradient: GradientMode;
  /** End the gradient at nothing instead of at colour B. */
  fade: boolean;
  shape: ShapeStyle;
  /** Outline and line width, in sheet pixels. */
  strokeWidth: number;
}

export const DEFAULT_PAINT: PaintSettings = {
  tool: "move",
  colorA: "#e11d48",
  colorB: "#0f172a",
  size: 64,
  hardness: 0.85,
  opacity: 1,
  gradient: "linear",
  fade: false,
  shape: "fill",
  strokeWidth: 12,
};

export interface Point {
  x: number;
  y: number;
}

/** `#rrggbb` → `rgba(r, g, b, a)`. */
function withAlpha(hex: string, alpha: number): string {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return hex;
  const n = parseInt(m[1], 16);
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`;
}

function sizeTo(canvas: HTMLCanvasElement, w: number, h: number) {
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
}

/** A canvas holding a copy of `src`. Used for snapshots, so it must not share pixels. */
export function cloneCanvas(src: HTMLCanvasElement): HTMLCanvasElement {
  const out = document.createElement("canvas");
  out.width = src.width;
  out.height = src.height;
  out.getContext("2d")?.drawImage(src, 0, 0);
  return out;
}

/* ── The brush tip ──────────────────────────────────────────────────────────────────────── */

/**
 * The tip, pre-rendered once and stamped many times.
 *
 * A radial gradient built per stamp would be built a few hundred times per stroke; building it
 * once and blitting is the difference between a brush that follows the pointer and one that
 * lags behind it.
 */
const tips = new Map<string, HTMLCanvasElement>();

function brushTip(size: number, hardness: number, color: string): HTMLCanvasElement {
  const d = Math.max(1, Math.round(size));
  const key = `${d}|${hardness.toFixed(2)}|${color}`;
  const cached = tips.get(key);
  if (cached) return cached;

  const tip = document.createElement("canvas");
  tip.width = d;
  tip.height = d;
  const ctx = tip.getContext("2d");
  if (ctx) {
    const r = d / 2;
    const g = ctx.createRadialGradient(r, r, 0, r, r, r);
    // Never quite 1: an edge that steps straight from opaque to nothing is a jagged circle,
    // and a hard brush should still be a smooth one.
    g.addColorStop(Math.min(hardness, 0.96), withAlpha(color, 1));
    g.addColorStop(1, withAlpha(color, 0));
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, d, d);
  }

  // The cache is keyed on values a slider sweeps through, so it would otherwise grow one entry
  // per pixel of drag. Oldest out first; a stroke only ever needs the tip it's using.
  if (tips.size > 24) tips.delete(tips.keys().next().value as string);
  tips.set(key, tip);
  return tip;
}

/**
 * Stamp the tip along a segment.
 *
 * Pointer samples arrive far apart when the hand moves fast, and one stamp per sample is a
 * dotted line. Spacing is a fraction of the diameter so a big brush doesn't stamp thousands of
 * times across the same distance a small one crosses in ten.
 */
function stampSegment(
  ctx: CanvasRenderingContext2D,
  tip: HTMLCanvasElement,
  from: Point,
  to: Point,
) {
  const d = tip.width;
  const step = Math.max(1, d * 0.15);
  const dist = Math.hypot(to.x - from.x, to.y - from.y);
  const steps = Math.max(1, Math.ceil(dist / step));
  for (let i = 1; i <= steps; i += 1) {
    const k = i / steps;
    const x = from.x + (to.x - from.x) * k;
    const y = from.y + (to.y - from.y) * k;
    ctx.drawImage(tip, x - d / 2, y - d / 2);
  }
}

/* ── Gradients and shapes ───────────────────────────────────────────────────────────────── */

/**
 * Fill the whole scratch with colour A carried into colour B along the drag.
 *
 * The drag is the axis, not the extent — canvas clamps a gradient to its end stops, so
 * everything before the press is solid A and everything past the release is solid B. That's
 * what makes it usable for a shroud that fades: you drag across the part of the sheet where
 * the transition should happen, not across the whole sheet.
 *
 * Fading ends at colour B with zero alpha rather than at `transparent`, which is transparent
 * *black* and would drag a grey bruise through the middle of any light gradient.
 */
function gradientFill(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  from: Point,
  to: Point,
  s: PaintSettings,
) {
  const span = Math.hypot(to.x - from.x, to.y - from.y);
  const g =
    s.gradient === "radial"
      ? ctx.createRadialGradient(from.x, from.y, 0, from.x, from.y, Math.max(span, 1))
      : ctx.createLinearGradient(from.x, from.y, to.x, to.y);
  g.addColorStop(0, withAlpha(s.colorA, 1));
  g.addColorStop(1, s.fade ? withAlpha(s.colorB, 0) : withAlpha(s.colorB, 1));
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, w, h);
}

function drawShape(
  ctx: CanvasRenderingContext2D,
  tool: PaintTool,
  from: Point,
  to: Point,
  s: PaintSettings,
) {
  ctx.strokeStyle = withAlpha(s.colorA, 1);
  ctx.fillStyle = withAlpha(s.colorA, 1);
  ctx.lineWidth = Math.max(1, s.strokeWidth);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";

  if (tool === "line") {
    ctx.beginPath();
    ctx.moveTo(from.x, from.y);
    ctx.lineTo(to.x, to.y);
    ctx.stroke();
    return;
  }

  const x = Math.min(from.x, to.x);
  const y = Math.min(from.y, to.y);
  const w = Math.abs(to.x - from.x);
  const h = Math.abs(to.y - from.y);

  ctx.beginPath();
  if (tool === "ellipse") ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
  else ctx.rect(x, y, w, h);
  if (s.shape === "fill") ctx.fill();
  else ctx.stroke();
}

/**
 * Where a shape drawn between two points lands, with room for the pen that draws it.
 *
 * Padded by half the stroke width because a line is centred on its path, and by a couple of
 * pixels beyond that for the round cap and for the antialiasing at its edge — a region that
 * stopped at the geometry would leave the outer fringe of a thick outline unrestored.
 */
function shapeRegion(from: Point, to: Point, s: PaintSettings): Region {
  const pad = Math.max(1, s.strokeWidth) / 2 + 2;
  return {
    x: Math.min(from.x, to.x) - pad,
    y: Math.min(from.y, to.y) - pad,
    w: Math.abs(to.x - from.x) + pad * 2,
    h: Math.abs(to.y - from.y) + pad * 2,
  };
}

/**
 * Hold Shift: squares, circles, and axes that snap to 45°.
 *
 * Exported because the stage draws the guide you aim with and this draws what lands, and the
 * two have to agree — a dashed box in one place and a square in another is worse than no guide.
 */
export function constrained(from: Point, to: Point, tool: PaintTool): Point {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  if (tool === "line" || tool === "gradient") {
    const step = Math.PI / 4;
    const angle = Math.round(Math.atan2(dy, dx) / step) * step;
    const len = Math.hypot(dx, dy);
    return { x: from.x + Math.cos(angle) * len, y: from.y + Math.sin(angle) * len };
  }
  // Not `Math.sign`: a drag that is exactly vertical has `dx` of 0, and a square with a zero
  // sign on one axis collapses to a point — the shape would vanish rather than square up.
  const side = Math.max(Math.abs(dx), Math.abs(dy));
  return { x: from.x + (dx < 0 ? -side : side), y: from.y + (dy < 0 ? -side : side) };
}

/* ── One stroke ─────────────────────────────────────────────────────────────────────────── */

/** Reused across strokes: it's the size of a sheet, and only one stroke happens at a time. */
let scratch: HTMLCanvasElement | null = null;
function scratchFor(w: number, h: number): HTMLCanvasElement {
  if (!scratch) scratch = document.createElement("canvas");
  sizeTo(scratch, w, h);
  return scratch;
}

export class Stroke {
  /** The layer as it was before this stroke — the preview's base, and the undo entry. */
  readonly before: HTMLCanvasElement;
  private readonly scratch: HTMLCanvasElement;
  private readonly start: Point;
  private last: Point;
  /** Whether anything has actually been put down, so a stray click isn't an undo step. */
  private painted = false;
  /** What the scratch has gained since the last render, and so what render has to lay down. */
  private pending: Region | null = null;
  /** Where the last drag-tool shape landed, since the next one has to wipe it off again. */
  private shape: Region | null = null;
  /** What the last render actually rewrote. Read through `dirty`. */
  private touched: Region | null = null;

  constructor(
    private readonly target: HTMLCanvasElement,
    private readonly settings: PaintSettings,
    at: Point,
  ) {
    this.before = cloneCanvas(target);
    this.scratch = scratchFor(target.width, target.height);
    this.scratch.getContext("2d")?.clearRect(0, 0, target.width, target.height);
    this.start = at;
    this.last = at;

    // A click is a mark for the tools that have one: a dab of the brush, a flooded layer.
    // The drag tools have nothing to show until the pointer moves.
    if (this.settings.tool === "fill") {
      const ctx = this.scratch.getContext("2d");
      if (ctx) {
        ctx.fillStyle = withAlpha(this.settings.colorA, 1);
        ctx.fillRect(0, 0, this.scratch.width, this.scratch.height);
      }
      this.mark(this.whole());
      this.painted = true;
    } else if (!DRAG_TOOLS.has(this.settings.tool)) {
      this.dab(at, at);
      this.painted = true;
    }
    this.render();
  }

  /**
   * The region the last render rewrote, or null if it had nothing to do.
   *
   * The only thing anyone outside needs from the bookkeeping above: it says which part of the
   * layer the sheet's composite — and the texture read back off it — has to catch up with.
   */
  get dirty(): Region | null {
    return this.touched;
  }

  private whole(): Region {
    return { x: 0, y: 0, w: this.target.width, h: this.target.height };
  }

  private mark(r: Region) {
    this.pending = unionRegion(this.pending, r);
  }

  private dab(from: Point, to: Point) {
    const ctx = this.scratch.getContext("2d");
    if (!ctx) return;
    const { size, hardness, colorA, tool } = this.settings;
    // The eraser's colour never reaches the sheet — the scratch is punched out of the layer,
    // so only its alpha matters. Black keeps a soft edge from tinting what it half-erases.
    const tip = brushTip(size, hardness, tool === "eraser" ? "#000000" : colorA);
    stampSegment(ctx, tip, from, to);
    // The segment's own box, grown by the tip that was walked along it — the stamps at each
    // end hang half a diameter past the points themselves.
    const r = tip.width / 2 + 1;
    this.mark({
      x: Math.min(from.x, to.x) - r,
      y: Math.min(from.y, to.y) - r,
      w: Math.abs(to.x - from.x) + r * 2,
      h: Math.abs(to.y - from.y) + r * 2,
    });
  }

  /**
   * Take the stroke to `points` — every sample since the last call, for the tools that trace a
   * path, and only the latest for the ones that just need a current corner.
   */
  move(points: Point[], constrain: boolean) {
    // Cleared up front so a sample that draws nothing reports nothing, rather than leaving the
    // previous sample's region standing for a second helping of the work it already caused.
    this.touched = null;
    if (!points.length) return;
    const { tool } = this.settings;
    if (tool === "fill") return;

    if (DRAG_TOOLS.has(tool)) {
      const raw = points[points.length - 1];
      const to = constrain ? constrained(this.start, raw, tool) : raw;
      if (to.x === this.start.x && to.y === this.start.y) return;
      const ctx = this.scratch.getContext("2d");
      if (!ctx) return;
      // Redrawn from scratch each time, not added to: dragging a gradient back and forth
      // should show the gradient it would leave, not every one it passed through.
      ctx.clearRect(0, 0, this.scratch.width, this.scratch.height);
      if (tool === "gradient") {
        gradientFill(ctx, this.scratch.width, this.scratch.height, this.start, to, this.settings);
      } else {
        drawShape(ctx, tool, this.start, to, this.settings);
      }
      // Both boxes, because the scratch was wiped: the new shape has to be laid down and the
      // old one has to be taken back off. A gradient covers the sheet, so for that one they
      // are the same box and it is the whole thing.
      const next = tool === "gradient" ? this.whole() : shapeRegion(this.start, to, this.settings);
      this.mark(next);
      if (this.shape) this.mark(this.shape);
      this.shape = next;
      this.painted = true;
    } else {
      for (const p of points) {
        this.dab(this.last, p);
        this.last = p;
      }
      this.painted = true;
    }
    this.render();
  }

  /**
   * Put the layer back to the snapshot and lay the whole stroke over it, once — across the part
   * of it that has moved since the last time.
   *
   * Both steps are per pixel and both read the same two canvases, so a pixel outside the region
   * would be recomputed from unchanged inputs to the value it already holds. What the region
   * changes is the bill, not the picture.
   */
  private render() {
    const rect = this.pending && clampRegion(this.pending, this.target.width, this.target.height);
    this.pending = null;
    this.touched = rect ?? null;
    if (!rect) return;
    const ctx = this.target.getContext("2d");
    if (!ctx) return;
    const { x, y, w, h } = rect;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.save();
    ctx.globalAlpha = 1;
    ctx.globalCompositeOperation = "source-over";
    ctx.clearRect(x, y, w, h);
    ctx.drawImage(this.before, x, y, w, h, x, y, w, h);
    ctx.globalAlpha = this.settings.opacity;
    ctx.globalCompositeOperation =
      this.settings.tool === "eraser" ? "destination-out" : "source-over";
    ctx.drawImage(this.scratch, x, y, w, h, x, y, w, h);
    ctx.restore();
  }

  /** True when the stroke left something behind, and so is worth an undo entry. */
  end(): boolean {
    return this.painted;
  }
}

/* ── Undo ───────────────────────────────────────────────────────────────────────────────── */

export interface Snapshot {
  sheetId: string;
  layerId: string;
  canvas: HTMLCanvasElement;
}

/**
 * How much painting to remember, counted in pixels rather than in steps.
 *
 * A snapshot costs `width × height × 4` bytes, so "keep the last ten" means 160MB on a 2048²
 * sheet and 10MB on a 512² one. A pixel budget spends the same memory either way: six steps on
 * a big sheet, two dozen on a small one.
 */
const MAX_PIXELS = 24_000_000;

/**
 * Painting history, for the paint layers only.
 *
 * Deliberately not the whole editor's undo: adding, moving and deleting layers are React state
 * and would need their own mechanism. Undoing a brush stroke you can see is the thing you reach
 * for while painting, and it's the thing that has no other way back.
 */
export class PaintHistory {
  private past: Snapshot[] = [];
  private future: Snapshot[] = [];

  get canUndo() {
    return this.past.length > 0;
  }

  get canRedo() {
    return this.future.length > 0;
  }

  push(sheetId: string, layerId: string, before: HTMLCanvasElement) {
    this.past.push({ sheetId, layerId, canvas: before });
    // A new stroke is a new branch of the drawing; whatever was undone away is gone.
    this.future = [];
    this.trim();
  }

  /**
   * Swap the top of `from` onto the live layer, pushing what was there onto `to`.
   *
   * Entries whose layer has since been deleted are dropped rather than skipped over — there is
   * nothing left to restore them onto, and stepping past them would take the undo to a state
   * the visible layers were never in.
   */
  private step(
    from: Snapshot[],
    to: Snapshot[],
    resolve: (layerId: string) => HTMLCanvasElement | null,
  ): Snapshot | null {
    while (from.length) {
      const entry = from[from.length - 1];
      const live = resolve(entry.layerId);
      if (!live) {
        from.pop();
        continue;
      }
      from.pop();
      to.push({ ...entry, canvas: cloneCanvas(live) });
      sizeTo(live, entry.canvas.width, entry.canvas.height);
      const ctx = live.getContext("2d");
      if (ctx) {
        ctx.setTransform(1, 0, 0, 1, 0, 0);
        ctx.globalAlpha = 1;
        ctx.globalCompositeOperation = "source-over";
        ctx.clearRect(0, 0, live.width, live.height);
        ctx.drawImage(entry.canvas, 0, 0);
      }
      return entry;
    }
    return null;
  }

  /** Restore the previous state onto its layer, and hand back which one moved. */
  undo(resolve: (layerId: string) => HTMLCanvasElement | null): Snapshot | null {
    return this.step(this.past, this.future, resolve);
  }

  redo(resolve: (layerId: string) => HTMLCanvasElement | null): Snapshot | null {
    return this.step(this.future, this.past, resolve);
  }

  /** Drop everything belonging to layers that are no longer there. True if any were. */
  keepOnly(alive: (sheetId: string, layerId: string) => boolean): boolean {
    const live = (s: Snapshot) => alive(s.sheetId, s.layerId);
    const before = this.past.length + this.future.length;
    this.past = this.past.filter(live);
    this.future = this.future.filter(live);
    return this.past.length + this.future.length !== before;
  }

  clear() {
    this.past = [];
    this.future = [];
  }

  private trim() {
    const cost = (s: Snapshot) => s.canvas.width * s.canvas.height;
    let total = [...this.past, ...this.future].reduce((n, s) => n + cost(s), 0);
    // Oldest first — the step you're least likely to reach for is the one furthest back.
    while (total > MAX_PIXELS && this.past.length > 1) {
      total -= cost(this.past.shift() as Snapshot);
    }
  }
}
