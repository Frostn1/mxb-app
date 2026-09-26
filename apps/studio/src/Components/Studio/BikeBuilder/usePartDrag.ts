import { useCallback, useRef, useState } from "react";
import type { LibraryPart } from "../../../api/bikebuild";

/**
 * Dragging a part from the tray onto the 3D viewport — with pointer events, not HTML5
 * drag-and-drop. Tauri's window keeps `dragDropEnabled` on by default, which hands every
 * drag to the OS for its own file-drop handling before the webview ever sees a `dragstart`
 * (see the same note on this in `TrackStudio.tsx`, where lap reordering hit the same wall).
 * A plain pointer sequence has no such problem: it's not a drag as far as the OS is
 * concerned, just a held-down click that moves.
 *
 * The drop target is "was the pointer over the viewport when it came up", checked with
 * `elementFromPoint` against a `data-bike-viewport` marker — not a raycast onto a
 * particular mount, because a part's role already says exactly where it goes.
 */
export function usePartDrag(onDrop: (part: LibraryPart) => void) {
  const [dragging, setDragging] = useState<{ part: LibraryPart; x: number; y: number } | null>(null);
  const drop = useRef(onDrop);
  drop.current = onDrop;

  const startDrag = useCallback((part: LibraryPart, e: React.PointerEvent) => {
    e.preventDefault();
    setDragging({ part, x: e.clientX, y: e.clientY });

    const onMove = (ev: PointerEvent) => setDragging({ part, x: ev.clientX, y: ev.clientY });
    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setDragging(null);
      const target = document.elementFromPoint(ev.clientX, ev.clientY);
      if (target?.closest("[data-bike-viewport]")) drop.current(part);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }, []);

  return { dragging, startDrag };
}
