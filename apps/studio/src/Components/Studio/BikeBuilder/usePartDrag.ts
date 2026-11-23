import { useCallback, useRef, useState } from "react";
import { ROLES, type LibraryPart, type Role } from "../../../api/bikebuild";

/**
 * Dragging a part from the outliner onto the 3D viewport — with pointer events, not HTML5
 * drag-and-drop. Tauri's window keeps `dragDropEnabled` on by default, which hands every
 * drag to the OS for its own file-drop handling before the webview ever sees a `dragstart`
 * (see the same note on this in `TrackStudio.tsx`, where lap reordering hit the same wall).
 * A plain pointer sequence has no such problem: it's not a drag as far as the OS is
 * concerned, just a held-down click that moves.
 *
 * The drop target is "was the pointer over the viewport when it came up", checked with
 * `elementFromPoint` against a `data-bike-viewport` marker — not a raycast onto a
 * particular mount, because a part's role already says exactly where it goes. Dropped on an
 * outliner slot (`data-bike-slot`) instead, it goes in that slot, whatever its role was.
 */
export function usePartDrag(onDrop: (part: LibraryPart, slot: Role | null) => void) {
  const [dragging, setDragging] = useState<{ part: LibraryPart; x: number; y: number } | null>(null);
  const drop = useRef(onDrop);
  drop.current = onDrop;

  const startDrag = useCallback((part: LibraryPart, e: React.PointerEvent) => {
    // A press on an outliner row is usually a click (select it), so nothing shows until the
    // pointer has actually moved a few pixels — then it's a drag.
    const from = { x: e.clientX, y: e.clientY };
    let moving = false;

    const onMove = (ev: PointerEvent) => {
      if (!moving && Math.hypot(ev.clientX - from.x, ev.clientY - from.y) < 5) return;
      moving = true;
      setDragging({ part, x: ev.clientX, y: ev.clientY });
    };
    const onUp = (ev: PointerEvent) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setDragging(null);
      if (!moving) return;
      const target = document.elementFromPoint(ev.clientX, ev.clientY);
      const slot = target?.closest("[data-bike-slot]")?.getAttribute("data-bike-slot");
      if (slot && (ROLES as readonly string[]).includes(slot)) drop.current(part, slot as Role);
      else if (target?.closest("[data-bike-viewport]")) drop.current(part, null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }, []);

  return { dragging, startDrag };
}
