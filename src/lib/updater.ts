import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let inFlight = false;

/**
 * Check GitHub Releases for a newer signed build. On `silent` (launch) checks
 * we stay quiet unless there's an update; a manual check reports either way.
 */
export async function checkForUpdates({ silent = false } = {}): Promise<void> {
  if (!inTauri() || inFlight) return;
  inFlight = true;
  try {
    const update = await check();
    if (!update) {
      if (!silent) toast.success("You're on the latest version");
      return;
    }
    toast(`Update available — v${update.version}`, {
      description:
        update.body?.trim().slice(0, 140) ||
        "A new version of MXB App is ready to install.",
      duration: Infinity,
      action: {
        label: "Restart & update",
        onClick: () => void installUpdate(update),
      },
    });
  } catch (e) {
    if (!silent) toast.error("Couldn't check for updates", { description: String(e) });
  } finally {
    inFlight = false;
  }
}

async function installUpdate(update: Update): Promise<void> {
  const id = toast.loading(`Downloading v${update.version}…`);
  try {
    await update.downloadAndInstall();
    toast.dismiss(id);
    await relaunch();
  } catch (e) {
    toast.dismiss(id);
    toast.error("Update failed", { description: String(e) });
  }
}
