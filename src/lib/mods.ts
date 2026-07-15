/** Shared display helpers for mod names, dates, and folders. */

const MOD_EXT = /\.(pkz|zip|rar|7z|pnt)$/i;

/** Drop a `.pkz`/`.zip`/`.pnt`/… extension for display. */
export function displayName(name: string): string {
  return name.replace(MOD_EXT, "");
}

/** Human folder label; the type root shows as "(root)". */
export function folderLabel(folder: string): string {
  return folder || "(root)";
}

/** Compact "Jul 8, 2026" date. */
export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

/** Even shorter "Jul 8" for dense cards. */
export function formatDateShort(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Best-guess file format (e.g. ".pkz") from a download URL. */
export function fileFormat(url: string): string | null {
  const m = url.match(/\.(pkz|zip|rar|7z)(?:[?#]|$)/i);
  return m ? `.${m[1].toLowerCase()}` : null;
}

/** Human-readable file size, e.g. "45 MB" or "1.2 GB". */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 0) return "";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded = value >= 10 || unit === 0 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${units[unit]}`;
}

/** Human-readable track length from metres, e.g. "1.24 km" or "820 m". */
export function formatLength(metres: number): string {
  if (!metres || metres <= 0) return "";
  return metres >= 1000 ? `${(metres / 1000).toFixed(2)} km` : `${Math.round(metres)} m`;
}
