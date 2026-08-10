import { useCallback, useMemo, useState } from "react";
import {
  Box,
  Brush,
  FileImage,
  FolderOpen,
  ImagePlus,
  Loader2,
  PackageOpen,
  RefreshCw,
  Save,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { open as openPath } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "../ui/alert-dialog";
import { ViewerDialog } from "../Viewer/ViewerDialog";
import type { EntryViewerProps } from "../Viewer/entryViewer";
import { SheetThumb } from "./SheetThumb";
import {
  gearPartOf,
  IMAGE_EXTS,
  isBikeKind,
  PaintDestBar,
  usePaintDest,
} from "../Studio/paintDest";
import {
  paintStudioExtract,
  paintStudioLoad,
  paintStudioSave,
  paintStudioTarget,
} from "../../api/mods";
import type { SavedPaint, StudioImage } from "../../types";
import { useT } from "../../i18n/context";

/**
 * Paint Studio — build a `.pnt` out of ordinary image files.
 *
 * MX Bikes reads a paint as a packed container of compressed sheets, which no image editor
 * writes. So the loop a painter actually works in — draw in GIMP, look at it on the model,
 * fix it, look again — has always had a converter someone else wrote sitting in the middle
 * of it. This is that step, done in the app that already has the 3D preview: pick the
 * sheets, say where they go, save, and the paint is installed where the game (and the
 * viewer) will find it.
 *
 * The thing that decides whether a paint *works* is not the pixels but the **names**. A
 * `.pnt` supplies textures by name and the mesh binds whichever names it asked for, so a
 * sheet called `livery` lands on the bodywork and the same sheet called `my_livery` lands
 * nowhere. Two things here exist for that alone: the expected-names line, read off the
 * paints already installed for the chosen model, and the unpack button — which writes an
 * existing paint's sheets out as `.tga` already named the way the model wants them.
 */

/** How a saved paint is shown in 3D — the viewer's own props, minus the mode it derives. */
type PreviewProps = Omit<EntryViewerProps, "mode">;

function fileStem(path: string): string {
  return (path.replace(/\\/g, "/").split("/").pop() ?? path).replace(/\.[^.]+$/, "");
}

function parentDir(path: string): string {
  return path.replace(/\\/g, "/").split("/").slice(0, -1).join("/");
}

interface PaintStudioProps {
  /**
   * Hand the sheets on screen to the Designer, to draw on rather than replace.
   *
   * Unpacking a paint here already produces exactly what the Designer wants to start from —
   * the real sheets, named the way the mesh binds them — and making someone unpack twice, once
   * per tab, is asking them to do the app's filing for it.
   */
  onSendToDesigner?: (sheets: StudioImage[]) => void;
}

export default function PaintStudio({ onSendToDesigner }: PaintStudioProps) {
  const t = useT();
  const [sheets, setSheets] = useState<StudioImage[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<SavedPaint | null>(null);
  const [clash, setClash] = useState<string | null>(null);
  const [view3d, setView3d] = useState(false);

  // A saved paint describes the file that was written; any edit after it makes that
  // description stale, so the preview/reveal buttons go with it.
  const invalidate = useCallback(() => setSaved(null), []);
  const destState = usePaintDest(invalidate);
  const { kind, folder, dest, hints, bikePreview } = destState;

  const addImages = useCallback(async () => {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Images", extensions: IMAGE_EXTS }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    if (!paths.length) return;
    setBusy(true);
    try {
      const loaded = await paintStudioLoad(paths);
      setSheets((prev) => {
        // Re-picking a file replaces its row rather than adding a second one under the
        // same name — the packer refuses duplicate names, and this is how you refresh.
        const next = [...prev];
        for (const img of loaded) {
          const at = next.findIndex((s) => s.path === img.path);
          if (at >= 0) next[at] = { ...img, name: next[at].name };
          else next.push(img);
        }
        return next;
      });
      invalidate();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [invalidate]);

  /** Re-read every sheet from disk — the button to press after saving in GIMP. */
  const reload = useCallback(async () => {
    if (!sheets.length) return;
    setBusy(true);
    try {
      const loaded = await paintStudioLoad(sheets.map((s) => s.path));
      setSheets((prev) => loaded.map((img, i) => ({ ...img, name: prev[i]?.name ?? img.name })));
      invalidate();
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [sheets, invalidate]);

  const unpack = useCallback(async () => {
    const picked = await openDialog({
      multiple: false,
      filters: [{ name: "MX Bikes paint", extensions: ["pnt"] }],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    setBusy(true);
    try {
      const template = await paintStudioExtract(path);
      // Load them straight in: the sheets now carry the names the model binds, which is
      // the entire reason to start from an existing paint rather than a blank canvas.
      const loaded = await paintStudioLoad(template.files);
      setSheets(loaded);
      setName((n) => n || fileStem(path));
      invalidate();
      toast.success(t("paints.unpacked", { count: String(template.files.length) }), {
        action: {
          label: t("paints.openFolder"),
          onClick: () => void openPath(template.dir).catch(() => {}),
        },
      });
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
    } finally {
      setBusy(false);
    }
  }, [invalidate, t]);

  const rename = useCallback(
    (path: string, value: string) => {
      setSheets((prev) => prev.map((s) => (s.path === path ? { ...s, name: value } : s)));
      invalidate();
    },
    [invalidate],
  );

  const drop = useCallback(
    (path: string) => {
      setSheets((prev) => prev.filter((s) => s.path !== path));
      invalidate();
    },
    [invalidate],
  );

  /** Why saving isn't possible yet, as a translated message — or null when it is. */
  const blocked = useMemo<string | null>(() => {
    if (!sheets.length) return t("paints.needSheets");
    if (!name.trim()) return t("paints.needName");
    if (sheets.some((s) => !s.name.trim())) return t("paints.needTextureNames");
    const seen = new Set<string>();
    for (const s of sheets) {
      const key = s.name.trim().toLowerCase();
      if (seen.has(key)) return t("paints.duplicateName", { name: s.name.trim() });
      seen.add(key);
    }
    if (!dest) return t("paints.needTarget");
    return null;
  }, [sheets, name, dest, t]);

  const write = useCallback(
    async (overwrite: boolean) => {
      if (!dest) return;
      setBusy(true);
      try {
        const outcome = await paintStudioSave({
          name: name.trim(),
          fileName: name.trim(),
          textures: sheets.map((s) => ({ path: s.path, name: s.name.trim() })),
          dest,
          overwrite,
        });
        setSaved(outcome);
        toast.success(t("paints.saved", { path: outcome.path }));
      } catch (e) {
        toast.error(String(e).replace(/^Error:\s*/, ""));
      } finally {
        setBusy(false);
      }
    },
    [dest, name, sheets, t],
  );

  const save = useCallback(async () => {
    if (blocked || !dest) {
      if (blocked) toast.error(blocked);
      return;
    }
    try {
      const target = await paintStudioTarget(name.trim(), dest);
      if (target.exists) {
        setClash(target.path);
        return;
      }
    } catch (e) {
      toast.error(String(e).replace(/^Error:\s*/, ""));
      return;
    }
    await write(false);
  }, [blocked, dest, name, write]);

  /**
   * How to show what was just saved in 3D.
   *
   * A paint installed into the game is shown on the model it was installed *for* — the
   * folder two levels up is that model, whether it's an extracted folder or a `.pkz` of the
   * same name beside it. One saved to a folder of the player's own isn't installed
   * anywhere, so gear falls back to the game's stock mesh for its slot, and a bike livery
   * has no mesh to go on at all.
   */
  const viewerProps = useMemo<PreviewProps | null>(() => {
    if (!saved) return null;
    const path = saved.path.replace(/\\/g, "/");
    const stem = fileStem(path);
    const modelDir = parentDir(parentDir(path));
    const part = gearPartOf(kind);
    if (folder) {
      if (part) return { paintPaths: [path], stockGearPart: part };
      return isBikeKind(kind) ? null : { paintPaths: [path] };
    }
    if (isBikeKind(kind)) {
      return bikePreview ? { paintPaths: [], modelSource: modelDir, initialPaint: stem } : null;
    }
    if (part) {
      return {
        paintPaths: [],
        gearSource: modelDir,
        gearPart: part,
        ...(kind.sub === "goggles" ? { initialGoggles: stem } : { initialPaint: stem }),
      };
    }
    return { paintPaths: [path] };
  }, [saved, kind, folder, bikePreview]);


  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 px-7 pb-6">
      {/* Same one-row treatment as the Designer: where it goes and what it's called are
          answered once, and the sheets are what the screen is actually for. */}
      <div className="flex flex-none flex-wrap items-center gap-2">
        <PaintDestBar state={destState} className="w-[290px]" />
        <Input
          value={name}
          placeholder={t("paints.namePlaceholder")}
          className="h-8 w-[168px]"
          onChange={(e) => {
            setName(e.target.value);
            invalidate();
          }}
          onKeyDown={(e) => e.key === "Enter" && void save()}
        />
        <Button
          size="sm"
          disabled={busy || !!blocked}
          title={blocked ?? undefined}
          onClick={() => void save()}
        >
          {busy ? <Loader2 className="size-3.5 animate-spin" /> : <Save className="size-3.5" />}
          {t("paints.save")}
        </Button>
        <span className="h-5 w-px flex-none bg-border" />
        <Button variant="outline" size="sm" onClick={() => void unpack()} disabled={busy}>
          <PackageOpen className="size-3.5" /> {t("paints.unpack")}
        </Button>
        {blocked && (
          <span className="ml-auto max-w-[40%] truncate text-[11px] text-faint" title={blocked}>
            {blocked}
          </span>
        )}
      </div>

      {saved && (
        <div className="flex flex-none flex-wrap items-center gap-2 rounded-md border border-border bg-card/40 px-2.5 py-1.5">
          <span className="min-w-0 flex-1 truncate text-[11px] text-muted-foreground" title={saved.path}>
            {saved.path}
          </span>
          {viewerProps && (
            <Button variant="outline" size="sm" onClick={() => setView3d(true)}>
              <Box className="size-3.5" /> {t("paints.preview3d")}
            </Button>
          )}
          <Button
            variant="outline"
            size="sm"
            onClick={() => void openPath(parentDir(saved.path)).catch(() => {})}
          >
            <FolderOpen className="size-3.5" /> {t("paints.openFolder")}
          </Button>
        </div>
      )}

      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
        {/* ── The sheets ────────────────────────────────────────────────────── */}
        <section className="flex min-w-0 flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-[13px] font-semibold">{t("paints.sheetsTitle")}</h2>
            <div className="ml-auto flex gap-2">
              {!!sheets.length && onSendToDesigner && (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy}
                  onClick={() => onSendToDesigner(sheets)}
                >
                  <Brush className="size-3.5" /> {t("paints.toDesigner")}
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={() => void reload()} disabled={busy || !sheets.length}>
                <RefreshCw className={cn("size-3.5", busy && "animate-spin")} /> {t("paints.reload")}
              </Button>
              <Button size="sm" onClick={() => void addImages()} disabled={busy}>
                <ImagePlus className="size-3.5" /> {t("paints.addImages")}
              </Button>
            </div>
          </div>

          {hints.length > 0 && (
            <p className="text-[11.5px] leading-snug text-muted-foreground">
              {t("paints.expected")}{" "}
              <span className="font-medium text-foreground">{hints.join(", ")}</span>
            </p>
          )}

          {!sheets.length ? (
            <div className="flex flex-1 flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border p-10 text-center">
              <FileImage className="size-6 text-muted-foreground" />
              <p className="max-w-sm text-[12.5px] leading-relaxed text-muted-foreground">
                {t("paints.empty")}
              </p>
            </div>
          ) : (
            <ul className="flex flex-col gap-2">
              {sheets.map((s) => {
                const unknown =
                  hints.length > 0 &&
                  !!s.name.trim() &&
                  !hints.some((h) => h.toLowerCase() === s.name.trim().toLowerCase());
                return (
                  <li
                    key={s.path}
                    className="flex items-start gap-3 rounded-lg border border-border bg-card/40 p-3"
                  >
                    <SheetThumb
                      token={s.token}
                      width={s.previewWidth}
                      height={s.previewHeight}
                      size={72}
                      className="flex-none rounded border border-border bg-background object-cover"
                    />
                    <div className="flex min-w-0 flex-1 flex-col gap-1.5">
                      <div className="flex items-center gap-2">
                        <Input
                          value={s.name}
                          onChange={(e) => rename(s.path, e.target.value)}
                          className="h-7 max-w-[220px] text-[12.5px]"
                        />
                        <span className="text-[11px] text-muted-foreground">
                          {s.width}×{s.height}
                        </span>
                        <button
                          type="button"
                          aria-label={t("common.remove")}
                          className="ml-auto text-muted-foreground hover:text-destructive"
                          onClick={() => drop(s.path)}
                        >
                          <Trash2 className="size-3.5" />
                        </button>
                      </div>
                      <span className="break-all text-[11px] leading-snug text-faint">{s.path}</span>
                      {s.resized && (
                        <span className="flex items-center gap-1.5 text-[11px] leading-snug text-amber-500">
                          <TriangleAlert className="size-3 flex-none" />
                          {t("paints.resized", {
                            from: `${s.sourceWidth}×${s.sourceHeight}`,
                            to: `${s.width}×${s.height}`,
                          })}
                        </span>
                      )}
                      {unknown && (
                        <span className="flex items-center gap-1.5 text-[11px] leading-snug text-amber-500">
                          <TriangleAlert className="size-3 flex-none" />
                          {t("paints.unknownName")}
                        </span>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>

      <AlertDialog open={!!clash} onOpenChange={(o) => !o && setClash(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("paints.replaceTitle")}</AlertDialogTitle>
            <AlertDialogDescription className="break-all">
              {t("paints.replaceBody", { path: clash ?? "" })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setClash(null);
                void write(true);
              }}
            >
              {t("paints.replace")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      <ViewerDialog
        open={view3d}
        onOpenChange={setView3d}
        title={saved ? fileStem(saved.path) : undefined}
        paintPaths={viewerProps?.paintPaths ?? []}
        modelSource={viewerProps?.modelSource}
        gearSource={viewerProps?.gearSource}
        gearPart={viewerProps?.gearPart}
        stockGearPart={viewerProps?.stockGearPart}
        initialPaint={viewerProps?.initialPaint}
        initialGoggles={viewerProps?.initialGoggles}
      />
    </div>
  );
}
