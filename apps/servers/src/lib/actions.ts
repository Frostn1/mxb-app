import { errorText } from "./api";

export type ActionResult<T> = { ok: true; value: T; ms: number } | { ok: false; error: string; ms: number };

/**
 * The one way MSM runs a user action: apply `optimistic` at once (it returns the undo), do the
 * work in the background, undo and report the error on failure, and log the time it took
 * (`[msm-timing] name total=…ms`). The caller never awaits a round trip to update the screen.
 */
export async function runAction<T>(opts: { name: string; run: () => Promise<T>; optimistic?: () => () => void }): Promise<ActionResult<T>> {
  const undo = opts.optimistic?.();
  const started = performance.now();
  try {
    const value = await opts.run();
    const ms = Math.round(performance.now() - started);
    console.info(`[msm-timing] ${opts.name} total=${ms}ms`);
    return { ok: true, value, ms };
  } catch (e) {
    undo?.();
    const ms = Math.round(performance.now() - started);
    console.info(`[msm-timing] ${opts.name} failed after ${ms}ms`);
    return { ok: false, error: errorText(e), ms };
  }
}