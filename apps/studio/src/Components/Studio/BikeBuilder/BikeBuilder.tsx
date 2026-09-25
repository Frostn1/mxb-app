import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Loader2, RefreshCw, Undo2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { blenderStatus, setBlenderPath, type BlenderStatus } from "../../../api/bikebuild";
import PartLibrary from "./PartLibrary";

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the
 * background, does the importing, placing and exporting (see `src-tauri/src/blender.rs`).
 * Here: find Blender, then the part library — parts added once, each given a role, and one
 * slot per role for the bike being built.
 */
export default function BikeBuilder() {
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
    const file = await openDialog({
      multiple: false,
      filters: [{ name: "Blender", extensions: ["exe"] }],
    });
    if (typeof file !== "string") return;
    setChecking(true);
    try {
      const next = await setBlenderPath(file);
      setStatus(next);
      if (!next.found || next.found.path.toLowerCase() !== file.toLowerCase())
        toast.error(t("bike.notBlender"));
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

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto p-6">
      <section className="flex flex-col gap-2 border border-border bg-card p-4">
        <div className="flex items-center gap-2">
          <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
            {t("bike.blender")}
          </h2>
          <Button size="sm" variant="ghost" className="ml-auto" onClick={check} disabled={checking}>
            {checking ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
          </Button>
        </div>
        {status === null ? (
          <p className="text-sm text-muted-foreground">{t("bike.looking")}</p>
        ) : found ? (
          <p className="text-sm">
            {found.supported
              ? t("bike.blenderFound", { version: found.version })
              : t("bike.blenderTooOld", { version: found.version, min: status.minVersion })}
            <span className="mt-0.5 block truncate font-mono text-[11px] text-muted-foreground">
              {found.path}
            </span>
          </p>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("bike.blenderMissing", { min: status.minVersion })}
          </p>
        )}
        <div className="flex gap-2">
          <Button size="sm" variant="outline" onClick={onPickBlender} disabled={checking}>
            <FolderOpen className="size-3.5" />
            {t("bike.chooseBlender")}
          </Button>
          {status?.saved && (
            <Button size="sm" variant="ghost" onClick={onForgetBlender} disabled={checking}>
              <Undo2 className="size-3.5" />
              {t("bike.findBlender")}
            </Button>
          )}
        </div>
      </section>

      <PartLibrary ready={ready} />
    </div>
  );
}
