/**
 * What an extension may ask for, and what the install dialog says about each.
 *
 * Mirrors `PERMISSIONS` in `crates/core/src/extensions.rs`; a test keeps the two lists equal.
 * Looks (themes, fonts, buttons, backgrounds) and sandboxed views need no permission: a view
 * can draw whatever it likes inside its own frame, and nothing else.
 *
 * The rule the wording follows: **anything granted to an extension can leave this PC.** A view
 * without `network` can't fetch, but the browser has other ways out of a frame that no policy
 * closes completely, so the dialog never promises that granted data stays local.
 */

export const PERMISSIONS = [
  "mods.read",
  "servers.read",
  "friends.presence",
  "game.state",
  "presets.read",
  "coach.read",
  "studio.launch",
  "network",
  "openExternal",
  "mcp.tools",
] as const;

export type Permission = (typeof PERMISSIONS)[number];

export function isPermission(p: unknown): p is Permission {
  return (PERMISSIONS as readonly unknown[]).includes(p);
}

/** Permissions that read other people's data. Shown unticked-by-default in the dialog. */
export const SENSITIVE: readonly Permission[] = ["friends.presence"];

/** Which bridge methods each permission opens. Everything not listed here is free. */
export const METHOD_PERMISSION: Record<string, Permission> = {
  "mods.list": "mods.read",
  "servers.list": "servers.read",
  "friends.list": "friends.presence",
  "game.state": "game.state",
  "presets.list": "presets.read",
  "coach.sessions": "coach.read",
  "studio.open": "studio.launch",
  "ui.openExternal": "openExternal",
  "mcp.result": "mcp.tools",
};
