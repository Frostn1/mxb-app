import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Box, FolderOpen, Loader2, RefreshCw, Undo2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import {
  PART_EXTENSIONS,
  blenderStatus,
  inspectPart,
  partSize,
  setBlenderPath,
  type BlenderStatus,
  type PartInspection,
} from "../../../api/bikebuild";

/**
 * Bike builder: put a bike together from parts, without having to be good at Blender.
 *
 * Studio chooses the parts and where they go; the rider's own Blender, run in the
 * background, does the importing, placing and exporting (see `src-tauri/src/blender.rs`).
 * This first cut is the part that has to work before anything else can: find Blender, and
 * bring one part through it, saying what came out.
 */
export default function BikeBuilder() {
  const t = useT();
  const [status, setStatus] = useState<BlenderStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [part, setPart] = useState<string | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [result, setResult] = useState<PartInspection | null>(null);

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

  async function onTryPart() {
    const file = await openDialog({
      multiple: false,
      filters: [{ name: t("bike.partFiles"), extensions: PART_EXTENSIONS }],
    });
    if (typeof file !== "string") return;
    setPart(file);
    setResult(null);
    setInspecting(true);
    try {
      setResult(await inspectPart(file));
    } catch (e) {
      toast.error(t("bike.inspectFailed"), { description: String(e) });
    } finally {
      setInspecting(false);
    }
  }

  const found = status?.found ?? null;
  const ready = !!found?.supported;
  const size = result ? partSize(result.bounds) : null;

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

      <section className="flex flex-col gap-2 border border-border bg-card p-4">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("bike.tryPart")}
        </h2>
        <p className="text-sm text-muted-foreground">{t("bike.tryPartHint")}</p>
        <div>
          <Button size="sm" onClick={onTryPart} disabled={!ready || inspecting}>
            {inspecting ? <Loader2 className="size-3.5 animate-spin" /> : <Box className="size-3.5" />}
            {t("bike.openPart")}
          </Button>
        </div>
        {part && (
          <p className="truncate font-mono text-[11px] text-muted-foreground">{part}</p>
        )}
        {result && (
          <div className="flex flex-col gap-1 text-sm">
            <p>
              {t("bike.partSummary", {
                objects: result.objects.length,
                tris: result.tris.toLocaleString(),
              })}
              {size &&
                ` · ${size.map((v) => v.toFixed(2)).join(" × ")} m`}
            </p>
            <ul className="max-h-64 overflow-y-auto font-mono text-[11px] text-muted-foreground">
              {result.objects.map((o) => (
                <li key={o.name} className="truncate">
                  {o.type === "MESH" ? "▪" : "·"} {o.name}
                  {o.parent ? ` ← ${o.parent}` : ""}
                  {o.tris !== undefined ? ` · ${o.tris.toLocaleString()} tris` : ""}
                  {o.type === "MESH" && o.uv === false ? ` · ${t("bike.noUv")}` : ""}
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>
    </div>
  );
}
