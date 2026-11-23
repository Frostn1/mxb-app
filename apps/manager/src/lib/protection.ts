import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

/**
 * What FrostMod's crash guards caught on this PC, and the table of known MX Bikes crashes.
 *
 * FrostMod writes one line per kind of crash a guard stopped in a session
 * (`frostmod-protection.jsonl`); the backend reads it and bundles the table
 * (`src-tauri/src/data/crash_sites.json`, built from the crash reports FrostMod sends).
 */

export interface Caught {
  /** Unix seconds. */
  t: number;
  /** FrostMod's kind id; matches a `KnownCrash.id`. */
  kind: string;
  site: string;
  track: string;
  server: string;
  v: string;
}

export type CrashStatus = "fixed" | "partly" | "open" | "outside";

export interface KnownCrash {
  id: string;
  name: string;
  what: string;
  sites: string[];
  status: CrashStatus;
  fixedIn: string | null;
  writeup: string | null;
  reports: number;
  riders: number;
  share: number;
}

export interface CrashTable {
  build: string;
  from: string;
  to: string;
  reports: number;
  riders: number;
  coverage: number;
  crashes: KnownCrash[];
}

export interface ProtectionReport {
  caught: Caught[];
  table: CrashTable | null;
}

const WEEK_S = 7 * 24 * 3600;

export function nowS(): number {
  return Math.floor(Date.now() / 1000);
}

/** Catches newer than `since` (unix seconds), newest first. */
export function caughtSince(caught: Caught[], since: number): Caught[] {
  return caught.filter((c) => c.t >= since);
}

export function caughtThisWeek(caught: Caught[], now = nowS()): Caught[] {
  return caughtSince(caught, now - WEEK_S);
}

/** The plain name for a kind, from the table; the raw id if the table doesn't know it. */
export function crashName(table: CrashTable | null, kind: string): string {
  return table?.crashes.find((c) => c.id === kind)?.name ?? kind;
}

/** Distinct crash names in `caught`, most frequent first. */
export function namesOf(table: CrashTable | null, caught: Caught[]): string[] {
  const n = new Map<string, number>();
  for (const c of caught) n.set(c.kind, (n.get(c.kind) ?? 0) + 1);
  return [...n.entries()].sort((a, b) => b[1] - a[1]).map(([k]) => crashName(table, k));
}

/** Known crashes FrostMod stops (fully). */
export function fixedCount(table: CrashTable | null): number {
  return table?.crashes.filter((c) => c.status === "fixed").length ?? 0;
}

export async function fetchProtection(): Promise<ProtectionReport> {
  try {
    return await invoke<ProtectionReport>("protection_report");
  } catch {
    return { caught: [], table: null };
  }
}

/** The report, read on mount and every minute; `refresh` re-reads now. */
export function useProtection(): ProtectionReport & { refresh: () => void } {
  const [report, setReport] = useState<ProtectionReport>({ caught: [], table: null });
  const refresh = useCallback(() => {
    void fetchProtection().then(setReport);
  }, []);
  useEffect(() => {
    refresh();
    const id = window.setInterval(refresh, 60_000);
    return () => window.clearInterval(id);
  }, [refresh]);
  return { ...report, refresh };
}
