import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let inFlight = false;

/** localStorage key remembering the last update version the user dismissed. */
const DISMISSED_UPDATE_KEY = "mxb-dismissed-update";

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
    // Don't nag: on a silent (launch) check, stay quiet if the user already
    // dismissed this exact version.
    if (silent && localStorage.getItem(DISMISSED_UPDATE_KEY) === update.version) {
      return;
    }
    toast(`Update available — v${update.version}`, {
      description:
        update.body?.trim().slice(0, 140) ||
        "A new version of MXB App is ready to install.",
      duration: Infinity,
      // A close button lets the user clear it; remember the version so a launch
      // check won't surface it again until there's a newer one.
      closeButton: true,
      onDismiss: () => localStorage.setItem(DISMISSED_UPDATE_KEY, update.version),
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
