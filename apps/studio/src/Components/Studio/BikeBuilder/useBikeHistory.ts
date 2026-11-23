import { useCallback, useRef, useState } from "react";

/** One thing done that can be taken back: what it was, and how to undo and redo it. */
export interface Step {
  label: string;
  undo: () => Promise<void>;
  redo: () => Promise<void>;
}

/** The longest history kept; older steps fall off the bottom. */
export const HISTORY_LIMIT = 100;

/**
 * Undo and redo for the Bike tab's edits: a part placed or taken off, moved or turned. Plain
 * stacks — doing something new clears what could be redone, as in Blender. One step runs at a
 * time; a key press while one is still on its way is dropped rather than queued, so a held
 * Ctrl+Z can't run past what's on screen.
 */
export class History {
  private done: Step[] = [];
  private undone: Step[] = [];
  private running = false;

  constructor(private onChange: () => void = () => {}) {}

  get canUndo() {
    return this.done.length > 0;
  }
  get canRedo() {
    return this.undone.length > 0;
  }
  /** What Ctrl+Z / Ctrl+Shift+Z would take back or do again. */
  get nextUndo(): string | null {
    return this.done[this.done.length - 1]?.label ?? null;
  }
  get nextRedo(): string | null {
    return this.undone[this.undone.length - 1]?.label ?? null;
  }

  /** Record something that's just been done. */
  push(step: Step) {
    this.done.push(step);
    if (this.done.length > HISTORY_LIMIT) this.done.shift();
    this.undone = [];
    this.onChange();
  }

  /** Take back the last step. Resolves to its label, or null when there was nothing to do. */
  async undo(): Promise<string | null> {
    return this.move(this.done, this.undone, "undo");
  }

  async redo(): Promise<string | null> {
    return this.move(this.undone, this.done, "redo");
  }

  private async move(from: Step[], to: Step[], how: "undo" | "redo"): Promise<string | null> {
    if (this.running) return null;
    const step = from.pop();
    if (!step) return null;
    this.running = true;
    try {
      await step[how]();
      to.push(step);
      return step.label;
    } catch (e) {
      // It didn't go through: it's still where it was.
      from.push(step);
      throw e;
    } finally {
      this.running = false;
      this.onChange();
    }
  }
}

/** A `History` for a component, re-rendering it when what can be undone changes. */
export function useBikeHistory() {
  const [, bump] = useState(0);
  const onChange = useCallback(() => bump((n) => n + 1), []);
  const ref = useRef<History | null>(null);
  if (!ref.current) ref.current = new History(onChange);
  return ref.current;
}
