import { useCallback, useRef, useState } from "react";
import { open as pickPaths } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { planDrop } from "../../api/mods";
import { useDropReview } from "../../Context/DropReview";
import { useT } from "../../i18n/context";
import type { TKey } from "../../i18n/core";

/** What the drop pipeline recognises by name. Anything else still stages — the classifier
 *  carries an unknown file through and asks where it goes — so the picker offers "all files"
 *  alongside this, rather than hiding a download the user knows is a mod. */
const MOD_EXTENSIONS = ["pkz", "zip", "rar", "7z", "pnt"];

/**
 * Turning filesystem paths into a staged plan, for review.
 *
 * Dragging is the fast way to produce paths, but it is not always available: a webview that
 * never receives the OS drop event — remote desktop, some shells, a policy that eats it —
 * leaves that user with no way to install a downloaded file at all. A picker produces the
 * same list of paths, so both feed this one function and a picked file is staged, classified,
 * reviewed and committed exactly like a dropped one.
 */
export function useImport() {
  const { reviewPlan } = useDropReview();
  const [staging, setStaging] = useState(false);

  // Kept in refs so `stagePaths` is stable: `DropZone` re-subscribes to the OS drop event
  // whenever its handler changes, and a fresh identity per render would churn the listener.
  const t = useT();
  const tRef = useRef(t);
  tRef.current = t;
  const reviewRef = useRef(reviewPlan);
  reviewRef.current = reviewPlan;

  /** Stage and classify these paths, then put the plan up for review. Reads only.
   *  `failureKey` because the two entrances have to own their own failure: someone who
   *  clicked Import never dropped anything. */
  const stagePaths = useCallback(
    async (paths: string[], failureKey: TKey = "drop.scanFailed") => {
      if (paths.length === 0) return;
      setStaging(true);
      try {
        reviewRef.current(await planDrop(paths));
      } catch (e) {
        toast.error(tRef.current(failureKey), { description: String(e) });
      } finally {
        setStaging(false);
      }
    },
    [],
  );

  /**
   * Ask for paths with a native picker, then stage them.
   *
   * Files and folders are separate modes because the OS dialog is one or the other: an
   * unpacked track is a folder, a download is a file, and no single dialog offers both.
   */
  const pickAndImport = useCallback(
    async (mode: "files" | "folder") => {
      const picked = await pickPaths(
        mode === "folder"
          ? { directory: true, multiple: true }
          : {
              multiple: true,
              filters: [
                {
                  name: tRef.current("import.modFiles"),
                  extensions: MOD_EXTENSIONS,
                },
                { name: tRef.current("import.allFiles"), extensions: ["*"] },
              ],
            },
      ).catch((e) => {
        toast.error(tRef.current("import.pickFailed"), {
          description: String(e),
        });
        return null;
      });

      // Cancelling the dialog is not a failure — it just yields nothing.
      if (picked === null) return;
      await stagePaths(
        Array.isArray(picked) ? picked : [picked],
        "import.readFailed",
      );
    },
    [stagePaths],
  );

  return { stagePaths, pickAndImport, staging };
}
