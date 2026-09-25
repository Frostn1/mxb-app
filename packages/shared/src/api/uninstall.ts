import { invoke } from "@tauri-apps/api/core";

/** How this install is removed; see `crates/core/src/uninstall.rs`. */
export interface UninstallInfo {
  /** The app's product name, e.g. "MXB Coach". */
  product: string;
  /** `launch` (Windows uninstaller), `trash` (macOS bundle), `delete` (AppImage), `package`
   *  (a .deb/.rpm: show `command`), `none` (e.g. a dev build). */
  method: "launch" | "trash" | "delete" | "package" | "none";
  command: string | null;
  /** What "also delete my data" would remove. */
  dataDirs: string[];
  /** Installed apps that read this app's data folder; deleting it is then refused. */
  sharedWith: string[];
  /** The Windows uninstaller, which has a "delete app data" box of its own. */
  nsis: boolean;
}

export const uninstallInfo = () => invoke<UninstallInfo>("uninstall_info");

/** Starts the uninstall and exits the app. Resolves only if something went wrong first. */
export const uninstallApp = (deleteData: boolean) => invoke<void>("uninstall_app", { deleteData });
