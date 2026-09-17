import { invoke } from "@tauri-apps/api/core";

/**
 * Count something the rider did.
 *
 * Fire-and-forget in both directions: it never awaits, never throws, and never changes what
 * the caller does next. The backend holds the switch, the buffer and the vocabulary
 * (`crates/core/src/usage.rs`), so a call from here is a name and nothing else — there is no
 * payload to accidentally put a session path or a rider name into, and a name Coach is not
 * expected to report is dropped there rather than travelling.
 *
 * Names are `area.thing`: `view.sessions`, `coach.review`.
 */
export function track(name: string): void {
  void invoke("track_event", { name }).catch(() => {});
}
