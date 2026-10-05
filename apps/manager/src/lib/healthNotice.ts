import type { HealthReport } from "@frost/shared/types";

/** Total online-only files, across every area the OneDrive check looks at. */
export function onlineOnlyTotal(report: HealthReport): number {
  const c = report.onedrive.onlineOnly;
  return c.bikes + c.tracks + c.paints + c.plugins;
}

/**
 * Which OneDrive notice applies, if any.
 *
 * `online` — files are online-only right now, the case that crashes; `piboso` — the PiBoSo
 * folder sits in OneDrive with everything still local; `game` — only the install folder does.
 * `canKeep` says whether "Keep on this device" has anything left to do.
 */
export function onedriveNotice(
  report: HealthReport,
): null | { kind: "online" | "piboso" | "game"; canKeep: boolean } {
  const o = report.onedrive;
  const online = onlineOnlyTotal(report);
  if (online === 0 && !o.pibosoInOnedrive && !o.gameInOnedrive) return null;
  const kind = online > 0 ? "online" : o.pibosoInOnedrive ? "piboso" : "game";
  // Pinning only helps what is still at risk of being evicted: an already-pinned folder with
  // nothing online-only has nothing left for the button to do.
  const canKeep = !!o.pibosoDir && (online > 0 || (o.pibosoInOnedrive && !o.pinned));
  return { kind, canKeep };
}
