import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  FolderOpen,
  Trash2,
  FolderInput,
  Lock,
  ShieldCheck,
  Maximize2,
  Box,
  Mountain,
  Share2,
  type LucideIcon,
} from "lucide-react";
import { toast } from "sonner";
import {
  getPkzMeta,
  getPkzPreview,
  mxbsecureUnlock,
  type ModType,
} from "@frost/shared/api/mods";
import type { LibraryEntry, PkzMeta } from "@frost/shared/types";
import { ViewerDialog } from "@frost/shared/Components/Viewer/ViewerDialog";
import { TrackViewerDialog } from "@frost/shared/Components/Viewer/TrackViewerDialog";
import { entryViewerProps } from "@frost/shared/Components/Viewer/entryViewer";
import { useConfig } from "@frost/shared/Context/Config";
import {
  displayName,
  folderLabel,
  formatBytes,
  formatLength,
} from "@frost/shared/lib/mods";
import { CATEGORY_ICON, CATEGORY_LABEL, categoryIcon } from "./categories";
import { Trans } from "@/i18n";
import { ContextBarLeft } from "../Shell/ContextBar";
import { useT } from "@/i18n";
import { Button } from "@frost/shared/Components/ui/button";
import { useSteamLink } from "@/lib/useSteamLink";

interface LibraryDetailProps {
  entry: LibraryEntry;
  entries: LibraryEntry[];
  modType: ModType;
  onClose: () => void;
  onReveal: (e: LibraryEntry) => void;
  onUninstall: (e: LibraryEntry) => void;
  onMove: (e: LibraryEntry) => void;
  onShare: (e: LibraryEntry) => void;
  onOpenEntry: (e: LibraryEntry) => void;
  /** Re-scan the library — called after a secured file is unlocked here, so it flips to open. */
  onChanged?: () => void;
}

function ownerKey(entry: LibraryEntry): string {
  return entry.kind === "folder" ? entry.name : displayName(entry.name);
}

export default function LibraryDetail({
  entry,
  entries,
  modType,
  onClose,
  onReveal,
  onUninstall,
  onMove,
  onShare,
  onOpenEntry,
  onChanged,
}: LibraryDetailProps) {
  const t = useT();
  const [meta, setMeta] = useState<PkzMeta | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const [lightbox, setLightbox] = useState(false);
  const [view3d, setView3d] = useState(false);
  const [viewTrack, setViewTrack] = useState(false);
  const [unlocking, setUnlocking] = useState(false);
  const { linkSteam } = useSteamLink({ onUnlocked: onChanged });
  // A track has its own viewer: it isn't a model with paints, it's a terrain grid.
  const isTrack = entry.category === "track";
  // A secured file with no key on this account yet — offer to unlock it right here.
  const securedLocked = !!entry.secured && !!entry.locked;

  const handleUnlock = async () => {
    setUnlocking(true);
    try {
      await mxbsecureUnlock(entry.path);
      toast.success(t("settings.mxbsecureUnlockOk"));
      onChanged?.();
      onClose();
    } catch (e) {
      const msg = String(e);
      if (/no Steam account linked/i.test(msg) || /Steam ID/i.test(msg)) {
        toast.error(t("settings.mxbsecureUnlockFail"), {
          description: t("settings.unlockNeedsSteam"),
          action: { label: t("settings.steamLinkBtn"), onClick: () => void linkSteam() },
        });
      } else if (/not entitled/i.test(msg)) {
        toast.error(t("settings.mxbsecureUnlockFail"), {
          description: t("settings.unlockNotOwned"),
        });
      } else {
        toast.error(t("settings.mxbsecureUnlockFail"), { description: msg });
      }
    } finally {
      setUnlocking(false);
    }
  };

  useEffect(() => {
    let alive = true;
    setMeta(null);
    setPreview(null);
    getPkzMeta(entry.path, entry.prefix)
      .then((m) => alive && setMeta(m))
      .catch(() => {});
    getPkzPreview(entry.path, entry.prefix)
      .then((p) => alive && setPreview(p))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [entry.path, entry.prefix]);

  const title = meta?.name?.trim() || displayName(entry.name);
  const Icon: LucideIcon = categoryIcon(entry.category);
  const image = preview || meta?.thumbnail || null;

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

  const { bikePreview } = useConfig();
  const view = useMemo(
    () => entryViewerProps(entry, entries, bikePreview),
    [entry, entries, bikePreview],
  );

  const rows: [string, string][] = [];
  if (meta?.author) rows.push([t("libraryDetail.author"), meta.author]);
  if (meta?.length) rows.push([t("libraryDetail.length"), formatLength(meta.length)]);
  if (meta?.altitude != null) rows.push([t("libraryDetail.altitude"), `${meta.altitude} m`]);
  if (meta?.location) rows.push([t("libraryDetail.location"), meta.location]);
  rows.push([
    t("libraryDetail.type"),
    CATEGORY_LABEL[entry.category] ? t(CATEGORY_LABEL[entry.category]) : t("libraryDetail.mod"),
  ]);
  if (entry.parent) rows.push([t("libraryDetail.belongsTo"), entry.parent]);
  rows.push([
    t("libraryDetail.format"),
    entry.kind === "folder"
      ? t("libraryDetail.extractedFolder")
      : entry.kind === "loose"
        ? t("libraryDetail.paintFile")
        : t("libraryDetail.packagedPkz"),
  ]);
  if (entry.size) rows.push([t("libraryDetail.size"), formatBytes(entry.size)]);
  rows.push([t("libraryDetail.folder"), folderLabel(entry.folder)]);

  // A stock track has no file of its own — it is one folder inside the install's shared
  // `tracks.pkz` — so everything that acts on a file is off, and Extract is on instead.
  const isStock = !!entry.stock;
  const canMove = entry.kind === "pkz" && !isStock;

  return (
    <div className="flex h-full flex-col">
      <ContextBarLeft>
        <span className="flex items-center gap-2 font-cond text-[12.5px] font-semibold tracking-[-0.02em]">
          <button
            onClick={onClose}
            className="flex cursor-default items-center gap-1.5 text-muted-foreground transition-colors hover:text-foreground"
          >
            <ArrowLeft className="size-3.5" />
            {t("nav.library")}
          </button>
          <span className="text-faint">/</span>
          <span className="max-w-[420px] truncate text-foreground">{title}</span>
        </span>
      </ContextBarLeft>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-8 pt-4">
        <div className="flex gap-6">
          {/* left: preview */}
          <div className="flex w-[420px] flex-none flex-col gap-3">
            {image ? (
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
                    <Lock className="size-3" /> Locked
                  </span>
                )}
              </button>
            ) : (
              <div className="relative grid aspect-video w-full place-items-center overflow-hidden rounded-xl border border-white/[0.07] bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/25">
                <Icon className="size-10" strokeWidth={1.25} />
                {meta?.locked && (
                  <span className="absolute bottom-2 left-2 flex items-center gap-1 rounded bg-black/60 px-1.5 py-0.5 text-[11px] text-white/80">
                    <Lock className="size-3" /> Locked
                  </span>
                )}
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              {view && (
                <Button variant="outline" size="sm" onClick={() => setView3d(true)}>
                  <Box className="size-3.5" /> View in 3D
                </Button>
              )}
              {isTrack && (
                <Button variant="outline" size="sm" onClick={() => setViewTrack(true)}>
                  <Mountain className="size-3.5" /> {t("trackViewer.open")}
                </Button>
              )}
              {canMove && (
                <Button variant="outline" size="sm" onClick={() => onMove(entry)}>
                  <FolderInput className="size-3.5" /> Move
                </Button>
              )}
              {/* Sharing a stock entry would share the install's whole 1.7 GB archive, and
                  there is nothing of it to uninstall. Reveal stays: it shows the archive. */}
              {!isStock && (
                <Button variant="outline" size="sm" onClick={() => onShare(entry)}>
                  <Share2 className="size-3.5" /> {t("share.share")}
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={() => onReveal(entry)}>
                <FolderOpen className="size-3.5" /> Show in Explorer
              </Button>
              {!isStock && (
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onUninstall(entry)}
                  className="text-destructive hover:text-destructive"
                >
                  <Trash2 className="size-3.5" /> Uninstall
                </Button>
              )}
            </div>
          </div>

          {/* right: info */}
          <div className="flex min-w-0 flex-1 flex-col gap-4">
            <div className="flex flex-col gap-1.5">
              <div className="flex items-center gap-2 text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                <Icon className="size-3.5" />{" "}
                {CATEGORY_LABEL[entry.category]
                  ? t(CATEGORY_LABEL[entry.category])
                  : t("libraryDetail.mod")}
              </div>
              <h1 className="font-cond text-[26px] font-bold leading-[1.05] tracking-[-0.045em]">
                {title}
              </h1>
            </div>

            {securedLocked && (
              <div className="flex flex-col gap-3 rounded-lg border border-primary/25 bg-primary/[0.06] px-3.5 py-3 text-[12px] leading-relaxed text-muted-foreground">
                <div className="flex items-start gap-2.5">
                  <ShieldCheck className="mt-0.5 size-3.5 flex-none text-primary" />
                  <span>{t("libraryDetail.securedLockedNote")}</span>
                </div>
                <div>
                  <Button size="sm" disabled={unlocking} onClick={() => void handleUnlock()}>
                    <ShieldCheck className="size-3.5" /> {t("settings.mxbsecureUnlockBtn")}
                  </Button>
                </div>
              </div>
            )}

            {meta?.locked && !securedLocked && (
              <div className="flex items-start gap-2.5 rounded-lg border border-white/[0.08] bg-foreground/[0.03] px-3.5 py-2.5 text-[12px] leading-relaxed text-muted-foreground">
                <Lock className="mt-0.5 size-3.5 flex-none text-faint" />
                {meta?.name?.trim() ? (
                  <span>
                    <Trans
                      k="libraryDetail.lockedWithMeta"
                      values={{
                        locked: (
                          <b className="text-foreground/80">
                            {t("libraryDetail.lockedWord")}
                          </b>
                        ),
                      }}
                    />
                  </span>
                ) : (
                  <span>
                    <Trans
                      k="libraryDetail.lockedNoMeta"
                      values={{
                        locked: (
                          <b className="text-foreground/80">
                            {t("libraryDetail.lockedWord")}
                          </b>
                        ),
                      }}
                    />
                  </span>
                )}
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
                      {CATEGORY_LABEL[category] ? t(CATEGORY_LABEL[category]) : category} · {items.length}
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
                  {t("libraryDetail.noEmbedded")}
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
        initialMode={view?.mode}
        paintPaths={view?.paintPaths ?? []}
        modelSource={view?.modelSource}
        gearSource={view?.gearSource}
        gearPart={view?.gearPart}
        stockGearPart={view?.stockGearPart}
        initialPaint={view?.initialPaint}
        initialGoggles={view?.initialGoggles}
      />

      {/* Mounted only once opened: the viewer reads the terrain as it mounts, and a track
          detail page shouldn't pay for that until someone asks to see it. */}
      {isTrack && viewTrack && (
        <TrackViewerDialog
          open={viewTrack}
          onOpenChange={setViewTrack}
          path={entry.path}
          prefix={entry.prefix}
          title={title}
        />
      )}
    </div>
  );
}
