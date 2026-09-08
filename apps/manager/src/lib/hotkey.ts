/**
 * Turning Tauri's accelerator syntax into something a player reads.
 *
 * Shared because two screens show the *same* live combo — the overlay settings and the
 * release showcase that points at them — and a combo written two ways reads as two
 * different shortcuts.
 */

/** `"CommandOrControl+Shift+X"` → `"Ctrl + Shift + X"` (`"Cmd + …"` on macOS). */
export function prettyHotkey(accelerator: string, isMac: boolean): string {
  return accelerator
    .split("+")
    .map((part) =>
      part === "CommandOrControl" ? (isMac ? "Cmd" : "Ctrl") : part,
    )
    .join(" + ");
}
