import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@/lib/utils";
import { useT } from "../../../i18n/context";
import { layerCorners } from "./composite";
import { hitTest, type Layer, type Sheet } from "./layers";

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
  className?: string;
}

/**
 * The 2D half of the editor: the sheet on a checkerboard, with the selected layer outlined.
 *
 * Draws the composite rather than the layers — the sheet is composited once by the editor and
 * this only ever blits it, so what's on screen and what would be saved cannot drift apart.
 *
 * Zoom and pan are view state and live here; nothing in them reaches the sheet.
 */
export function CanvasStage({
  sheet,
  source,
  version,
  selectedId,
  onSelect,
  onMove,
  className,
}: CanvasStageProps) {
  const t = useT();
  const wrapRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<HTMLCanvasElement>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  // Where the pointer went down, and what it was doing — a drag of a layer, or of the view.
  const drag = useRef<{ id: string | null; x: number; y: number } | null>(null);

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
    if (selected) {
      const pts = layerCorners(selected).map(
        ([sx, sy]) =>
          [originX + (sx - sheet.width / 2) * scale, originY + (sy - sheet.height / 2) * scale] as [
            number,
            number,
          ],
      );
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
  }, [box, scale, originX, originY, sheet, source, selectedId, version]);

  const onPointerDown = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
      const at = toSheet(e.clientX, e.clientY);
      if (!at) return;
      // Middle button and space-free right-drag pan; left picks and drags a layer.
      if (e.button === 1 || e.button === 2) {
        drag.current = { id: null, x: e.clientX, y: e.clientY };
      } else {
        const hit = hitTest(sheet.layers, at.x, at.y);
        onSelect(hit?.id ?? null);
        drag.current = { id: hit?.id ?? null, x: e.clientX, y: e.clientY };
      }
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [onSelect, sheet.layers, toSheet],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent<HTMLCanvasElement>) => {
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
    [onMove, scale],
  );

  const endDrag = useCallback((e: React.PointerEvent<HTMLCanvasElement>) => {
    drag.current = null;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  }, []);

  const onWheel = useCallback((e: React.WheelEvent<HTMLCanvasElement>) => {
    setZoom((z) => Math.min(8, Math.max(0.25, z * (e.deltaY < 0 ? 1.12 : 1 / 1.12))));
  }, []);

  const reset = useCallback(() => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  }, []);

  return (
    <div
      ref={wrapRef}
      className={cn("relative min-h-0 overflow-hidden rounded-lg border border-border bg-[#16171c]", className)}
    >
      <canvas
        ref={viewRef}
        className="absolute inset-0 h-full w-full touch-none"
        style={{ width: box.w, height: box.h }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onWheel={onWheel}
        onContextMenu={(e) => e.preventDefault()}
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
