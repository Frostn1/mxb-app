/** Shared display helpers for mod names, dates, and folders. */

const ARCHIVE_EXT = /\.(pkz|zip|rar|7z)$/i;

/** Drop a `.pkz`/`.zip`/… extension for display. */
export function displayName(name: string): string {
  return name.replace(ARCHIVE_EXT, "");
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
