import { createContext, useContext } from "react";
import type { FrostmodStatus, ReloadOutcome, VcRuntime } from "../types";

export interface FrostmodContextValue {
  /** Whether FrostMod is currently running (polled). `null` until first probe. */
  running: boolean | null;
  /** Install/version snapshot (`null` until first fetched). */
  status: FrostmodStatus | null;
  /** True while an install/update download is in flight. */
  installing: boolean;
  /** True while a `refreshStatus` GitHub check is in flight. */
  checking: boolean;
  /** True when the last `refreshStatus` failed (offline / GitHub error). */
  statusError: boolean;
  /** Manually ask FrostMod to live-reload the game now. */
  reload: () => Promise<ReloadOutcome>;
  /** Re-probe running status immediately. */
  refresh: () => void;
  /** Re-fetch the full install/version status (hits GitHub). */
  refreshStatus: () => Promise<void>;
  /** Download the latest FrostMod, then start it. */
  install: () => Promise<void>;
  /** True while a Visual C++ runtime install is in flight. */
  installingRuntime: boolean;
  /**
   * Install a missing Visual C++ runtime. Prompts for admin via Windows; resolves either
   * way, and opens Microsoft's download page if the prompt was declined.
   */
  installRuntime: (runtime: VcRuntime) => Promise<void>;
  /** Hide the runtime banner for this session (the Settings panel keeps showing it). */
  dismissRuntimeWarning: () => void;
  /** What the banner should warn about — `null` once dismissed this session. */
  runtimeWarning: VcRuntime | null;
  /**
   * The runtime that's actually missing, dismissal or not. Settings uses this: hiding a
   * bar you didn't understand shouldn't also erase the panel that explains it.
   */
  missingRuntime: VcRuntime | null;
  /** Launch FrostMod now if it isn't already running. */
  start: () => Promise<void>;
  /** Stop FrostMod now, whether or not this app started it. */
  stop: () => Promise<void>;
}

// Kept in a component-free module (like Config.ts) so the context identity stays
// stable across Vite Fast Refresh. If this lived alongside FrostmodProvider, hot
// updates would re-run createContext() and transiently break useContext, throwing
// "useFrostmod must be used within FrostmodProvider" mid-dev.
export const FrostmodContext = createContext<FrostmodContextValue | null>(null);

export function useFrostmod() {
  const ctx = useContext(FrostmodContext);
  if (!ctx) throw new Error("useFrostmod must be used within FrostmodProvider");
  return ctx;
}
