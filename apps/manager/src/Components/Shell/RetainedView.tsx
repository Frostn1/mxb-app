import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { ContextSlots, type ContextSlotTargets } from "./ContextBar";

const DISCONNECTED_SLOTS: ContextSlotTargets = { left: null, right: null };

/** Keep quick back-and-forth navigation hot, then release old DOM and decoded images. */
export const RETAIN_IDLE_MS = 5 * 60_000;

/** The two-state latch behind lazy retention, kept pure so its transition is testable. */
export function retainAfterVisit(wasVisited: boolean, active: boolean): boolean {
  return wasVisited || active;
}

/** Hidden screens must not be able to portal controls into the visible context bar. */
export function slotsWhileActive(
  active: boolean,
  slots: ContextSlotTargets,
): ContextSlotTargets {
  return active ? slots : DISCONNECTED_SLOTS;
}

/** Whether the retained screen containing a component is currently visible. */
const ViewActivity = createContext(true);

/**
 * Lets the few components with continuous work pause it while their screen is retained but
 * hidden. One-shot loads deliberately do not need this: retaining their result is the point.
 */
export function useViewActive(): boolean {
  return useContext(ViewActivity);
}

/**
 * Follow a change signal while visible; while hidden, collapse any number of signals into one
 * refresh on the next visit. Retaining screens must not turn one install into several hidden
 * full-library scans, but they also must not reopen with stale data.
 */
export function useRefreshWhileActive(
  refresh: () => void | Promise<void>,
  subscribe: (notify: () => void) => Promise<() => void>,
): void {
  const active = useViewActive();
  const stale = useRef(false);

  useEffect(() => {
    if (!active || !stale.current) return;
    stale.current = false;
    void refresh();
  }, [active, refresh]);

  useEffect(() => {
    const pending = subscribe(() => {
      if (active) void refresh();
      else stale.current = true;
    });
    return () => {
      void pending.then((off) => off()).catch(() => {});
    };
  }, [active, refresh, subscribe]);
}

function useRetainedMount(active: boolean): boolean {
  const [retained, setRetained] = useState(active);

  useEffect(() => {
    if (active) {
      setRetained(true);
      return;
    }
    if (!retained) return;
    const timer = window.setTimeout(() => setRetained(false), RETAIN_IDLE_MS);
    return () => window.clearTimeout(timer);
  }, [active, retained]);

  // Render synchronously on a visit; `retained` only owns the hidden idle lifetime.
  return active || retained;
}

/** Retain an inner tab without changing the context-bar targets owned by its parent screen. */
export function RetainedPane({ active, children }: { active: boolean; children: ReactNode }) {
  const parentActive = useViewActive();
  const mounted = useRetainedMount(active);
  if (!mounted) return null;

  const visible = parentActive && active;
  return (
    <ViewActivity.Provider value={visible}>
      <div
        hidden={!active}
        aria-hidden={active ? undefined : true}
        data-pane-active={active ? "true" : "false"}
        style={{ display: active ? "contents" : "none" }}
      >
        {children}
      </div>
    </ViewActivity.Provider>
  );
}

interface RetainedViewProps {
  active: boolean;
  slots: ContextSlotTargets;
  children: ReactNode;
}

/**
 * Lazily mount a screen on its first visit, then hide it instead of destroying it.
 *
 * Recreating a grid remounts all of its images. On Windows those images are served from the
 * app cache through WebView2, so even a cache hit becomes a burst of disk reads, decoding and
 * layout on every tab switch. Keeping the DOM alive turns the next visit into one visibility
 * change. Hidden screens get no context-bar portal targets, so their controls cannot escape
 * the hidden subtree and overlap the active screen's controls.
 */
export default function RetainedView({ active, slots, children }: RetainedViewProps) {
  const mounted = useRetainedMount(active);
  if (!mounted) return null;

  return (
    <ViewActivity.Provider value={active}>
      <ContextSlots.Provider value={slotsWhileActive(active, slots)}>
        <div
          hidden={!active}
          aria-hidden={active ? undefined : true}
          data-view-active={active ? "true" : "false"}
          style={{ display: active ? "block" : "none" }}
          className="h-full min-h-0"
        >
          {children}
        </div>
      </ContextSlots.Provider>
    </ViewActivity.Provider>
  );
}
