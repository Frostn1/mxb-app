import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * The in-game overlay, in either app. Both register commands with these names, and when
 * MXB App and MXB Coach both run, their overlays link (`crates/core/src/overlaylink.rs`).
 */

/** The other app, while the two are linked. */
export interface OverlayPeer {
  app: "manager" | "coach";
  pid: number;
  version: string;
  /** Tab ids its overlay offers. */
  tabs: string[];
}

/** The in-game overlay's settings, plus what the game is doing right now. */
export interface OverlayState {
  enabled: boolean;
  /** Tauri accelerator string, e.g. `"CommandOrControl+Shift+X"`. */
  hotkey: string;
  gameRunning: boolean;
  /** A DirectX app owns the screen exclusively — nothing can be drawn over it. */
  fullscreenBlocked: boolean;
  /** Why the shortcut isn't live (usually another app owns the combo), or null. */
  hotkeyError: string | null;
  peer: OverlayPeer | null;
  /** Coach only: why MXB App holds the key instead. */
  deferred: "linked" | "oldManager" | "updateManager" | "updateCoach" | null;
  /** The local link between the two apps never came up, so they can't share one key or show
   *  each other's tabs. Each holds its own shortcut instead. */
  linkDown: boolean;
}

export function getOverlayState(): Promise<OverlayState> {
  return invoke<OverlayState>("overlay_state");
}

/** Show or hide the overlay. The global hotkey does the same thing. */
export function overlayToggle(): Promise<void> {
  return invoke<void>("overlay_toggle");
}

/** Dismiss the overlay and hand keyboard focus back to MX Bikes. */
export function overlayHide(): Promise<void> {
  return invoke<void>("overlay_hide");
}

/** Close the overlay and bring this app's main window to the front. */
export function overlayOpenMain(): Promise<void> {
  return invoke<void>("overlay_open_main");
}

export function setOverlayEnabled(enabled: boolean): Promise<void> {
  return invoke<void>("set_overlay_enabled", { enabled });
}

/** Rebind the overlay hotkey. Rejects (leaving the old one live) if the combo is taken. */
export function setOverlayHotkey(hotkey: string): Promise<void> {
  return invoke<void>("set_overlay_hotkey", { hotkey });
}

/** Hide this overlay and have the other app show its own in the same place, on `tab`. */
export function overlayHandoff(tab: string): Promise<void> {
  return invoke<void>("overlay_handoff", { tab });
}

export function getOverlayPeer(): Promise<OverlayPeer | null> {
  return invoke<OverlayPeer | null>("overlay_peer");
}

/** Fires when the other app links or goes. */
export function onOverlayPeer(cb: (peer: OverlayPeer | null) => void): Promise<UnlistenFn> {
  return listen<OverlayPeer | null>("overlay-peer", (e) => cb(e.payload));
}

/** Fires when the overlay is asked to show a tab (the other app handed it over). */
export function onOverlayTab(cb: (tab: string) => void): Promise<UnlistenFn> {
  return listen<string>("overlay-tab", (e) => cb(e.payload));
}

/** The tab the overlay was opened on, when a handoff built its window. */
export function initialOverlayTab(): string | null {
  return new URLSearchParams(window.location.search).get("tab");
}

/** Fires when the overlay was summoned while the game held the screen exclusively. */
export function onOverlayFullscreenBlocked(cb: () => void): Promise<UnlistenFn> {
  return listen("overlay-fullscreen-blocked", () => cb());
}
