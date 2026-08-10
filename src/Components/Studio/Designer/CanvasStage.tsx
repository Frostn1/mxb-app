import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { useT } from "../../../i18n/context";
import { layerCorners } from "./composite";
import { hitTest, type Layer, type Sheet } from "./layers";
import { constrained, hasTip, isDragTool, type PaintTool, type Point } from "./paint";

interface CanvasStageProps {
  sheet: Sheet;
  /** The sheet's composite, already drawn. Blitted here rather than redrawn. */
  source: HTMLCanvasElement | null;
  /** Bumped by the editor whenever the composite changes, to force a repaint. */
  version: number;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  /** Drag, in sheet pixels. */
  onMove: (id: string, dx: number, dy: number) => void;
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
 */
export function CanvasStage({
  sheet,
  source,
  version,
  selectedId,
  onSelect,
  onMove,
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
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  // Where the pointer went down, and what it was doing — a drag of a layer, or of the view.
  const drag = useRef<{ id: string | null; x: number; y: number } | null>(null);
  const painting = useRef(false);
  // The press and current point of a gradient or shape drag, so it can be shown while it's
  // being aimed. Only ever set between press and release.
  const [guide, setGuide] = useState<{ from: Point; to: Point } | null>(null);

  const paints = tool !== "move" && canPaint;

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
    const cell = 8;
    ctx.save();
    ctx.beginPath();
    ctx.rect(left, top, w, h);
    ctx.clip();
    ctx.fillStyle = "#2a2c33";
    ctx.fillRect(left, top, w, h);
    ctx.fillStyle = "#33363e";
    for (let y = 0; y < h; y += cell) {
      for (let x = ((y / cell) % 2) * cell; x < w; x += cell * 2) {
        ctx.fillRect(left + x, top + y, cell, cell);
      }
    }
    ctx.restore();

    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(source, left, top, w, h);

    // Sheet edge, so you can see where the texture stops.
    ctx.strokeStyle = "rgba(255,255,255,0.18)";
    ctx.lineWidth = 1;
    ctx.strokeRect(left + 0.5, top + 0.5, w - 1, h - 1);

    const selected = sheet.layers.find((l) => l.id === selectedId);
    // A paint layer's box is the sheet's own edge, which is already drawn — outlining it again
    // would just put a blue border round everything for as long as a brush is selected.
    if (selected && selected.kind !== "paint") {
      const pts = layerCorners(selected).map(([sx, sy]) => toView({ x: sx, y: sy }));
      ctx.beginPath();
      ctx.moveTo(pts[0][0], pts[0][1]);
      for (const [px, py] of pts.slice(1)) ctx.lineTo(px, py);
      ctx.closePath();
      ctx.strokeStyle = "#3b82f6";
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.fillStyle = "#3b82f6";
      for (const [px, py] of pts) ctx.fillRect(px - 2.5, py - 2.5, 5, 5);
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
  }, [box, scale, originX, originY, sheet, source, selectedId, version, guide, tool, toView]);

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

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const at = toSheet(e.clientX, e.clientY);
      if (!at) return;
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
      const hit = hitTest(sheet.layers, at.x, at.y);
      onSelect(hit?.id ?? null);
      drag.current = { id: hit?.id ?? null, x: e.clientX, y: e.clientY };
    },
    [onPaintStart, onSelect, paints, sheet.layers, toSheet, tool],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      moveCursor(e.clientX, e.clientY);

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
      if (!d || !scale) return;
      const dx = e.clientX - d.x;
      const dy = e.clientY - d.y;
      if (!dx && !dy) return;
      d.x = e.clientX;
      d.y = e.clientY;
      if (d.id) onMove(d.id, dx / scale, dy / scale);
      else setPan((p) => ({ x: p.x + dx, y: p.y + dy }));
    },
    [moveCursor, onMove, onPaintMove, scale, toSheet, tool],
  );

  const endDrag = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      if (painting.current) {
        painting.current = false;
        setGuide(null);
        onPaintEnd();
      }
      drag.current = null;
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
          cursor: ringed ? "none" : paints ? "crosshair" : "default",
        }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onPointerLeave={() => {
          if (cursorRef.current) cursorRef.current.style.opacity = "0";
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
      <div className="pointer-events-none absolute bottom-2 left-2 flex items-center gap-2 rounded-md bg-white/[0.06] px-2 py-1 text-[11px] leading-none text-white/45">
        <span>
          {sheet.width}×{sheet.height}
        </span>
        <span>·</span>
        <span>{Math.round(zoom * 100)}%</span>
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
