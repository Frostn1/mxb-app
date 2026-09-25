import type { UninstallInfo } from "../api/uninstall";

/** The confirm button stays off until the app's name is typed, ignoring case and spacing. */
export function confirmMatches(typed: string, product: string): boolean {
  const norm = (s: string) => s.trim().replace(/\s+/g, " ").toLowerCase();
  return product.trim() !== "" && norm(typed) === norm(product);
}

/** Whether "also delete my data" can be offered, and why not when it can't. */
export function dataChoice(info: UninstallInfo): { offered: boolean; blockedBy: string[] } {
  if (info.sharedWith.length > 0) return { offered: false, blockedBy: info.sharedWith };
  return { offered: info.dataDirs.length > 0, blockedBy: [] };
}

/** The app can remove itself; otherwise the section shows instructions only. */
export function canSelfRemove(info: UninstallInfo): boolean {
  return info.method === "launch" || info.method === "trash" || info.method === "delete";
}
