import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  FolderOpen,
  Trash2,
  FolderInput,
  Lock,
  Maximize2,
  Box,
  type LucideIcon,
} from "lucide-react";
import { getPkzMeta, getPkzPreview, type ModType } from "../../api/mods";
import type { LibraryEntry, LibraryCategory, PkzMeta } from "../../types";
import { ViewerDialog } from "../Viewer/ViewerDialog";
import {
  displayName,
  folderLabel,
  formatBytes,
  formatLength,
} from "../../lib/mods";
import { CATEGORY_ICON, CATEGORY_LABEL, categoryIcon } from "./categories";
import { Button } from "@/Components/ui/button";

interface LibraryDetailProps {
  entry: LibraryEntry;
  /** All entries of the current type, for listing a model's paints / swaps. */
  entries: LibraryEntry[];
  modType: ModType;
  onClose: () => void;
  onReveal: (e: LibraryEntry) => void;
  onUninstall: (e: LibraryEntry) => void;
  onMove: (e: LibraryEntry) => void;
  onOpenEntry: (e: LibraryEntry) => void;
}

/** The owner key a model's paints/swaps point back to (their `parent`). */
function ownerKey(entry: LibraryEntry): string {
  return entry.kind === "folder" ? entry.name : displayName(entry.name);
}

/**
 * Full detail view for one installed item: large preview (click to enlarge),
 * parsed metadata, on-disk location, and — for a bike/gear model — the paints,
 * goggles and model-swaps that belong to it.
 */
export default function LibraryDetail({
  entry,
  entries,
  modType,
  onClose,
  onReveal,
  onUninstall,
  onMove,
  onOpenEntry,
}: LibraryDetailProps) {
  const [meta, setMeta] = useState<PkzMeta | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [lightbox, setLightbox] = useState(false);
  const [view3d, setView3d] = useState(false);

  useEffect(() => {
    let alive = true;
    setMeta(null);
    setPreview(null);
    getPkzMeta(entry.path)
      .then((m) => alive && setMeta(m))
      .catch(() => {});
    getPkzPreview(entry.path)
      .then((p) => alive && setPreview(p))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [entry.path]);

  const title = meta?.name?.trim() || displayName(entry.name);
  const Icon: LucideIcon = categoryIcon(entry.category);
  const image = preview || meta?.thumbnail || null;

  // A model's contents: paints / goggles / model-swaps pointing back to it.
  const related = useMemo(() => {
    const owner = ownerKey(entry);
    const kids = entries.filter((e) => e.parent && e.parent === owner && e.path !== entry.path);
    const byCat = new Map<string, LibraryEntry[]>();
    for (const k of kids) {
      const list = byCat.get(k.category) ?? [];
      list.push(k);
      byCat.set(k.category, list);
    }
    return [...byCat.entries()];
  }, [entries, entry]);

  // Rider-side gear categories (the rest default to the bike model).
  const RIDER_CATS = new Set<LibraryCategory>([
    "helmet",
    "helmetPaint",
    "goggles",
    "boots",
    "bootPaint",
    "protection",
    "protectionPaint",
    "gloves",
    "outfit",
  ]);
  // Whether this item can be shown in the 3D viewer, which paints to offer, and
  // which model to start on. A paint entry previews itself; a model gathers the
  // paints that belong to it.
  const isPaint = entry.kind === "loose" && /\.pnt$/i.test(entry.name);
  const viewable =
    entry.category === "bike" || RIDER_CATS.has(entry.category) || isPaint;
  const viewerMode: "bike" | "rider" = RIDER_CATS.has(entry.category)
    ? "rider"
    : "bike";
  // A gear *model* entry (not a paint) previews on the rider in its own slot.
  const gearPart: "helmet" | "boots" | "protection" | undefined = isPaint
    ? undefined
    : entry.category === "helmet"
      ? "helmet"
      : entry.category === "boots"
        ? "boots"
        : entry.category === "protection"
          ? "protection"
          : undefined;
  // A loose gear *paint* (its model not necessarily installed) previews on the
  // game's stock model for that slot — the gear analogue of a rider-outfit paint
  // rendering on the stock body. Outfit/glove paints keep the rider-body path.
  const stockGearPart: "helmet" | "boots" | "protection" | undefined =
    entry.category === "helmetPaint"
      ? "helmet"
      : entry.category === "bootPaint"
        ? "boots"
        : entry.category === "protectionPaint"
          ? "protection"
          : undefined;
  const paintPaths = useMemo(() => {
    if (isPaint) return [entry.path];
    const owner = ownerKey(entry);
    return entries
      .filter(
        (e) =>
          e.parent === owner &&
          e.kind === "loose" &&
          /\.pnt$/i.test(e.name),
      )
      .map((e) => e.path);
  }, [entries, entry, isPaint]);

  const rows: [string, string][] = [];
  if (meta?.author) rows.push(["Author", meta.author]);
  if (meta?.length) rows.push(["Length", formatLength(meta.length)]);
  if (meta?.altitude != null) rows.push(["Altitude", `${meta.altitude} m`]);
  if (meta?.location) rows.push(["Location", meta.location]);
  rows.push(["Type", CATEGORY_LABEL[entry.category] ?? "Mod"]);
  if (entry.parent) rows.push(["Belongs to", entry.parent]);
  rows.push(["Format", entry.kind === "folder" ? "Extracted folder" : entry.kind === "loose" ? "Paint file" : "Packaged .pkz"]);
  if (entry.size) rows.push(["Size", formatBytes(entry.size)]);
  rows.push(["Folder", folderLabel(entry.folder)]);

  const canMove = entry.kind === "pkz";

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3 px-7 pb-3.5 pt-5">
        <button
          onClick={onClose}
          className="flex cursor-default items-center gap-1 text-[12.5px] font-semibold text-primary hover:brightness-110"
        >
          <ArrowLeft className="size-3.5" /> Library
        </button>
        <span className="text-faint">/</span>
        <span className="truncate text-[12.5px] text-muted-foreground">{title}</span>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-8">
        <div className="flex gap-6">
          {/* left: preview */}
          <div className="flex w-[420px] flex-none flex-col gap-3">
            {image ? (
              // Fill the column at the image's own aspect ratio — as large as
              // possible without cropping (only tall images are capped).
              <button
                onClick={() => setLightbox(true)}
                className="group relative block w-full cursor-pointer overflow-hidden rounded-xl border border-white/[0.07] bg-black/25"
              >
                <img
                  src={image}
                  alt={title}
                  className="mx-auto block max-h-[440px] w-full object-contain"
                />
                <span className="absolute right-2 top-2 rounded-md bg-black/55 p-1.5 text-white/80 opacity-0 transition-opacity group-hover:opacity-100">
                  <Maximize2 className="size-3.5" />
                </span>
                {meta?.locked && (
                  <span className="absolute bottom-2 left-2 flex items-center gap-1 rounded bg-black/60 px-1.5 py-0.5 text-[11px] text-white/80">
                    <Lock className="size-3" /> non-plain
                  </span>
                )}
              </button>
            ) : (
              <div className="relative grid aspect-video w-full place-items-center overflow-hidden rounded-xl border border-white/[0.07] bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25">
                <Icon className="size-10" strokeWidth={1.25} />
                {meta?.locked && (
                  <span className="absolute bottom-2 left-2 flex items-center gap-1 rounded bg-black/60 px-1.5 py-0.5 text-[11px] text-white/80">
                    <Lock className="size-3" /> non-plain
                  </span>
                )}
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              {viewable && (
                <Button variant="outline" size="sm" onClick={() => setView3d(true)}>
                  <Box className="size-3.5" /> View in 3D
                </Button>
              )}
              {canMove && (
                <Button variant="outline" size="sm" onClick={() => onMove(entry)}>
                  <FolderInput className="size-3.5" /> Move
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={() => onReveal(entry)}>
                <FolderOpen className="size-3.5" /> Show in Explorer
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() => onUninstall(entry)}
                className="text-destructive hover:text-destructive"
              >
                <Trash2 className="size-3.5" /> Uninstall
              </Button>
            </div>
          </div>

          {/* right: info */}
          <div className="flex min-w-0 flex-1 flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <div className="flex items-center gap-2 text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                <Icon className="size-3.5" /> {CATEGORY_LABEL[entry.category] ?? "Mod"}
              </div>
              <h1 className="text-[22px] font-bold leading-tight tracking-[-0.3px]">
                {title}
              </h1>
            </div>

            {meta?.locked && (
              <div className="flex items-start gap-2.5 rounded-lg border border-white/[0.08] bg-foreground/[0.03] px-3.5 py-2.5 text-[12px] leading-relaxed text-muted-foreground">
                <Lock className="mt-0.5 size-3.5 flex-none text-faint" />
                <span>
                  This track is <b className="text-foreground/80">non-plain</b>{" "}
                  (encrypted copy-protection), so its name, length and preview
                  can’t be read from the file — only its filename and size.
                </span>
              </div>
            )}

            <div className="rounded-xl border border-white/[0.07] bg-card p-4">
              <dl className="grid grid-cols-2 gap-x-6 gap-y-3.5">
                {rows.map(([label, value]) => (
                  <div key={label} className="flex min-w-0 flex-col gap-1">
                    <dt className="text-[10px] font-semibold uppercase tracking-[0.9px] text-faint">
                      {label}
                    </dt>
                    <dd className="select-text break-words text-[13px] font-medium text-foreground/90">
                      {value}
                    </dd>
                  </div>
                ))}
              </dl>
              <div className="mt-4 flex flex-col gap-1 border-t border-white/[0.06] pt-3.5">
                <dt className="text-[10px] font-semibold uppercase tracking-[0.9px] text-faint">
                  Path
                </dt>
                <dd className="select-text break-all font-mono text-[11px] text-muted-foreground">
                  {entry.path}
                </dd>
              </div>
            </div>

            {related.length > 0 && (
              <div className="flex flex-col gap-3">
                {related.map(([category, items]) => (
                  <div key={category} className="flex flex-col gap-2">
                    <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                      {CATEGORY_LABEL[category] ?? category} · {items.length}
                    </span>
                    <div className="flex flex-col gap-1">
                      {items.map((it) => {
                        const RowIcon = CATEGORY_ICON[it.category] ?? Icon;
                        return (
                          <button
                            key={it.path}
                            onClick={() => onOpenEntry(it)}
                            className="flex cursor-default items-center gap-2.5 rounded-lg border border-white/[0.06] bg-card px-3 py-2 text-left transition-colors hover:border-white/15"
                          >
                            <RowIcon className="size-3.5 flex-none text-faint" />
                            <span className="min-w-0 flex-1 truncate text-[12.5px]">
                              {displayName(it.name)}
                            </span>
                            <span className="flex-none text-[11px] text-faint">
                              {formatBytes(it.size)}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                ))}
              </div>
            )}

            {related.length === 0 &&
              modType.id !== "rider" &&
              !meta?.locked &&
              !meta?.author &&
              !meta?.length && (
                <p className="text-[12.5px] text-muted-foreground">
                  No embedded details were found for this item.
                </p>
              )}
          </div>
        </div>
      </div>

      {lightbox && image && (
        <div
          onClick={() => setLightbox(false)}
          className="fixed inset-0 z-50 flex cursor-zoom-out items-center justify-center bg-black/80 p-8 backdrop-blur-sm"
        >
          <img
            src={image}
            alt={title}
            className="max-h-[85vh] max-w-[min(1000px,90vw)] rounded-lg object-contain shadow-2xl"
          />
        </div>
      )}

      <ViewerDialog
        open={view3d}
        onOpenChange={setView3d}
        title={title}
        initialMode={viewerMode}
        paintPaths={paintPaths}
        modelSource={entry.category === "bike" ? entry.path : undefined}
        gearSource={gearPart ? entry.path : undefined}
        gearPart={gearPart}
        stockGearPart={stockGearPart}
      />
    </div>
  );
}
