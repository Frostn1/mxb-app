import { invoke } from "@tauri-apps/api/core"

/**
 * Count something the creator did.
 *
 * Fire-and-forget in both directions: it never awaits, never throws, and never changes what
 * the caller does next. The backend holds the switch, the buffer and the rules about what a
 * name may be (`crates/core/src/usage.rs`), so a call from here is a name and nothing else —
 * there is no payload to accidentally put a file path or a track name into.
 *
 * Names are `area.thing`: `view.studio.designer`, `track.generate`.
 */
export function track(name: string): void {
  void invoke("track_event", { name }).catch(() => {})
}
