import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { toast } from "sonner";
import { useT } from "@/i18n";
import { blenderStatus, setBlenderPath, type BlenderStatus } from "../../../api/bikebuild";

/**
 * Blender's status and the actions on it — split out of the status indicator itself so the
 * indicator can move (it now lives in the title bar's own right-hand slot, not a bar the Bike
 * tab draws for itself) without the tab losing track of whether it can run a Blender job yet.
 */
export function useBlenderStatus() {
  const t = useT();
  const [status, setStatus] = useState<BlenderStatus | null>(null);
  const [checking, setChecking] = useState(false);

  const check = useCallback(() => {
    setChecking(true);
    blenderStatus()
      .then(setStatus)
      .catch((e) => toast.error(t("bike.blenderCheckFailed"), { description: String(e) }))
      .finally(() => setChecking(false));
  }, [t]);
  useEffect(() => check(), [check]);

  async function onPickBlender() {
    const file = await openDialog({ multiple: false, filters: [{ name: "Blender", extensions: ["exe"] }] });
    if (typeof file !== "string") return;
    setChecking(true);
    try {
      const next = await setBlenderPath(file);
      setStatus(next);
      if (!next.found || next.found.path.toLowerCase() !== file.toLowerCase()) toast.error(t("bike.notBlender"));
    } catch (e) {
      toast.error(t("bike.notBlender"), { description: String(e) });
    } finally {
      setChecking(false);
    }
  }

  async function onForgetBlender() {
    setChecking(true);
    try {
      setStatus(await setBlenderPath(""));
    } finally {
      setChecking(false);
    }
  }

  const found = status?.found ?? null;
  const ready = !!found?.supported;
  const summary =
    status === null
      ? t("bike.looking")
      : ready
        ? t("bike.blenderFound", { version: found!.version })
        : found
          ? t("bike.blenderTooOld", { version: found.version, min: status.minVersion })
          : t("bike.blenderMissing", { min: status.minVersion });

  return { status, checking, ready, summary, check, onPickBlender, onForgetBlender };
}
