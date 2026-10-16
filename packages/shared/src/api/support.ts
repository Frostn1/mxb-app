import { invoke } from "@tauri-apps/api/core";

/**
 * The "support the developer" card, in the manager and the studio.
 *
 * Both register the commands from `crates/core/src/support.rs`, which holds the schedule.
 * Nothing here decides whether to ask — this side reports a good moment and draws the card.
 */

/**
 * The Buy Me a Coffee page.
 *
 * The handle is the auto-generated one the account was created with, not a word — it's meant
 * to be opened, never typed, so don't "tidy" it. `supportUrl` in `supporters.json` overrides
 * it on the manager's Supporters page.
 */
export const SUPPORT_URL = "https://buymeacoffee.com/22ypnh5yfme";

/** The window event a finished install or export fires. See {@link supportMoment}. */
export const SUPPORT_MOMENT = "frost:support-moment";

/**
 * Something just went right: an install landed, an export was written.
 *
 * Call it only once the work is actually over — never with more of it still queued — since
 * the card may follow a few seconds later. Whether it does is up to the schedule.
 */
export function supportMoment(): void {
  window.dispatchEvent(new Event(SUPPORT_MOMENT));
}

/** Is now a good time? `success` after a {@link supportMoment}, `launch` a while after start. */
export function supportDue(trigger: "success" | "launch"): Promise<boolean> {
  return invoke<boolean>("support_due", { trigger });
}

/** The card is on screen. */
export function supportShown(): Promise<void> {
  return invoke("support_shown");
}

/** "Remind me later": a week. */
export function supportLater(): Promise<void> {
  return invoke("support_later");
}

/** "Support" was pressed and the page opened. */
export function supportOpened(): Promise<void> {
  return invoke("support_opened");
}

/** "Don't show again". */
export function supportNever(): Promise<void> {
  return invoke("support_never");
}
