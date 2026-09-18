/**
 * The creator's-page window — the front-end side of `creator_page.rs`.
 *
 * Opening a mod, or installing one (including Install & join), shows that mod's real
 * mxb-mods.com page behind the app so the site keeps its ad revenue. Both calls check the
 * player's preference first and swallow every error: this exists to support the creator and
 * must never be able to break the thing the player actually asked for.
 */
import { invoke } from "@tauri-apps/api/core";
import { readAdSupport } from "../lib/adSupport";

/**
 * Show `url` (a mxb-mods.com / gpb-mods.com page) behind the app. No-ops when the player has
 * opted out, or when there's no URL to show. The Rust side refuses anything that isn't a
 * catalog page, so a bad URL is safe to pass.
 */
export async function openCreatorPage(url: string | null | undefined): Promise<void> {
  if (!url) return;
  const prefs = readAdSupport();
  if (!prefs.enabled) return;
  // Beside the app (the default) so the page is fully visible — a real, countable view —
  // unless the player chose to keep it behind.
  const placement = prefs.behindApp ? "behind" : "beside";
  try {
    await invoke<void>("open_creator_page", { url, placement });
  } catch {
    // Display-only; a window that won't build must not stop the mod from installing.
  }
}

/** Close the creator's page — called when the player leaves the mod. */
export async function closeCreatorPage(): Promise<void> {
  try {
    await invoke<void>("close_creator_page");
  } catch {
    // Nothing to close, or no Tauri host (tests): both are fine.
  }
}
