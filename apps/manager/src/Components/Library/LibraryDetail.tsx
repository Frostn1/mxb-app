import { useEffect, useMemo, useState } from "react";
import {
  ArrowLeft,
  FolderOpen,
  Trash2,
  FolderInput,
  Lock,
  ShieldCheck,
  Box,
  Check,
  Maximize2,
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
import { readTrackInfo } from "@frost/shared/api/tracks";
import type { LibraryEntry, PkzMeta } from "@frost/shared/types";
import { ViewerDialog } from "@frost/shared/Components/Viewer/ViewerDialog";
import { ModelViewer } from "@frost/shared/Components/Viewer/ModelViewer";
import { useBikeModel } from "@frost/shared/Components/Viewer/useBikeModel";
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
import { ActionBar, StateChip } from "../ModPage/ActionBar";
import MediaPanel, { type Figure } from "../ModPage/Media";
import { Facts, Note, Panel, WhatsInside, type InsideGroup } from "../ModPage/Panels";

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

/** A track's layouts, named by the heightfields the archive carries. */
function layoutName(entryName: string): string {
  const base = entryName.split("/").pop() ?? entryName;
  return base.replace(/\.[^.]+$/, "");
}

/**
 * One installed mod, on the one mod page.
 *
 * Same bar, same picture, same column of cards as Browse and the stores. The state card here
 * is the one about the file on disk — where it is, and the three things you can do to it.
 */
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
  const [layouts, setLayouts] = useState<string[]>([]);
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

  // What the archive actually holds, for a track. Cheap: the backend answers it from the
  // archive's index without inflating anything, and a locked file simply refuses — in which
  // case the block below disappears rather than guessing.
  useEffect(() => {
    let alive = true;
    setLayouts([]);
    if (!isTrack || entry.locked) return;
    readTrackInfo(entry.path, entry.prefix)
      .then((info) => {
        if (!alive) return;
        const names = info.files
          .filter((f) => f.role === "heightfield")
          .map((f) => layoutName(f.name));
        setLayouts([...new Set(names)]);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [entry.path, entry.prefix, entry.locked, isTrack]);

  const title = meta?.name?.trim() || displayName(entry.name);
  const Icon: LucideIcon = categoryIcon(entry.category);
  const image = preview || meta?.thumbnail || null;
  const categoryName = CATEGORY_LABEL[entry.category]
    ? t(CATEGORY_LABEL[entry.category])
    : t("libraryDetail.mod");

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

  const { bikePreview, config } = useConfig();
  const view = useMemo(
    () => entryViewerProps(entry, entries, bikePreview),
    [entry, entries, bikePreview],
  );

  // A bike's "picture" is its paint sheet blown up to 16:9 — a flat grid of panels that
  // tells you almost nothing about the bike. Where the model can be drawn, it is drawn here
  // instead, on the same loader the viewer dialog uses.
  const bike = useBikeModel({
    enabled: view?.mode === "bike" && !!view.modelSource,
    modelSource: view?.modelSource,
    tyres: config.previewTyres ?? undefined,
  });
  const bikeNodes = bike.model?.nodes?.length ? bike.model.nodes : null;
  // Whichever paint the model leads with — the dialog is where you go to try the others.
  const bikeTextures = bike.model?.paints[0]?.textures ?? [];
  const standIn =
    bikeTextures.find((tex) => ["livery", "bike_parts"].includes(tex.name.toLowerCase())) ??
    null;
  // Nothing loaded and nothing still coming: fall back to the picture rather than leave an
  // empty frame where a bike should be.
  const showStage = view?.mode === "bike" && !!view.modelSource && (bike.loading || !!bikeNodes);

  // A track's layouts, and everything a bike or a gear model carries under it — the model
  // swaps, the liveries, the sounds. Each group is what a real scan found, so a mod with
  // none of them shows no block at all.
  const inside: InsideGroup[] = [
    ...(layouts.length > 0
      ? [
          {
            key: "layouts",
            label: "Layouts",
            items: layouts.map((l) => ({ key: l, label: l, icon: Mountain })),
          },
        ]
      : []),
    ...related.map(([category, items]) => ({
      key: category,
      label: CATEGORY_LABEL[category] ? t(CATEGORY_LABEL[category]) : category,
      items: items.map((it) => ({
        key: it.path,
        label: displayName(it.name),
        hint: it.size ? formatBytes(it.size) : undefined,
        icon: CATEGORY_ICON[it.category] ?? Icon,
        onClick: () => onOpenEntry(it),
      })),
    })),
  ];

  // A stock track has no file of its own — it is one folder inside the install's shared
  // `tracks.pkz` — so everything that acts on a file is off, and the archive is all we reveal.
  const isStock = !!entry.stock;
  const canMove = entry.kind === "pkz" && !isStock;

  const figures: Figure[] = [
    ...(meta?.length ? [{ label: t("libraryDetail.length"), value: formatLength(meta.length) }] : []),
    ...(meta?.altitude != null
      ? [{ label: t("libraryDetail.altitude"), value: `${meta.altitude} m` }]
      : []),
    ...(meta?.location ? [{ label: t("libraryDetail.location"), value: meta.location }] : []),
  ];

  return (
    <div className="flex h-full flex-col overflow-hidden">
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

      <ActionBar
        image={image}
        fallbackIcon={Icon}
        title={title}
        meta={[categoryName, meta?.author, entry.parent]}
      >
        <StateChip icon={Check} tone="success">
          In library
        </StateChip>
        {view && (
          <Button variant="secondary" onClick={() => setView3d(true)}>
            <Box className="size-3.5" /> View in 3D
          </Button>
        )}
        {isTrack && (
          <Button variant="secondary" onClick={() => setViewTrack(true)}>
            <Mountain className="size-3.5" /> {t("trackViewer.open")}
          </Button>
        )}
        <Button variant="outline" onClick={() => onReveal(entry)}>
          <FolderOpen className="size-3.5" /> Show in Explorer
        </Button>
      </ActionBar>

      <div className="flex min-h-0 flex-1 gap-6 px-7 pb-5 pt-4">
        <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
          {showStage ? (
            <div className="relative aspect-video w-full overflow-hidden rounded-xl border border-border bg-black/40">
              <ModelViewer
                mode="bike"
                texture={standIn}
                textures={bikeTextures}
                nodes={bikeNodes}
                rig={bike.model?.rig ?? null}
                loading={bike.loading}
                noStandIn
                className="absolute inset-0"
              />
              {/* The one control the stage needs: the dialog is where the paints, the tyres
                  and a canvas worth turning the bike on live. */}
              <button
                onClick={() => setView3d(true)}
                title={t("library.quick3d")}
                aria-label={t("library.quick3d")}
                className="absolute right-3 top-3 grid size-7 cursor-default place-items-center rounded-lg border border-white/25 bg-black/45 text-white/85 transition-colors hover:text-white"
              >
                <Maximize2 className="size-3.5" />
              </button>
            </div>
          ) : (
          <MediaPanel
            images={image ? [image] : []}
            title={title}
            figures={figures}
            /* Where it stands with you belongs on the thing itself. A locked mod says both:
               it is yours, and it is sealed. */
            badge={
              <>
                <StateChip icon={Check} tone="success" overlay>
                  {t("modDetail.inLibrary")}
                </StateChip>
                {meta?.locked && (
                  <StateChip icon={Lock} overlay>
                    {t("libraryDetail.lockedWord")}
                  </StateChip>
                )}
              </>
            }
          />
          )}

          {securedLocked && (
            <Note icon={ShieldCheck} tone="primary">
              <div className="flex flex-col items-start gap-2.5">
                <span>{t("libraryDetail.securedLockedNote")}</span>
                <Button size="sm" disabled={unlocking} onClick={() => void handleUnlock()}>
                  <ShieldCheck className="size-3.5" /> {t("settings.mxbsecureUnlockBtn")}
                </Button>
              </div>
            </Note>
          )}

          {meta?.locked && !securedLocked && (
            <Note icon={Lock}>
              {meta?.name?.trim() ? (
                <Trans
                  k="libraryDetail.lockedWithMeta"
                  values={{
                    locked: (
                      <b className="text-foreground/80">{t("libraryDetail.lockedWord")}</b>
                    ),
                  }}
                />
              ) : (
                <Trans
                  k="libraryDetail.lockedNoMeta"
                  values={{
                    locked: (
                      <b className="text-foreground/80">{t("libraryDetail.lockedWord")}</b>
                    ),
                  }}
                />
              )}
            </Note>
          )}

          {inside.length === 0 &&
            modType.id !== "rider" &&
            !meta?.locked &&
            !meta?.author &&
            !meta?.length && (
              <p className="text-[12.5px] text-muted-foreground">
                {t("libraryDetail.noEmbedded")}
              </p>
            )}
        </div>

        <div className="flex w-[320px] flex-none flex-col gap-3 overflow-y-auto pb-1">
          {/* The state card: the file itself, and the three things you can do to it. */}
          <Panel label="On disk">
            <Facts
              rows={[
                {
                  label: t("libraryDetail.format"),
                  value:
                    entry.kind === "folder"
                      ? t("libraryDetail.extractedFolder")
                      : entry.kind === "loose"
                        ? t("libraryDetail.paintFile")
                        : t("libraryDetail.packagedPkz"),
                },
                // A size the app has actually measured on disk — unlike a store listing,
                // which never states one.
                { label: t("libraryDetail.size"), value: entry.size ? formatBytes(entry.size) : null },
                { label: t("libraryDetail.folder"), value: folderLabel(entry.folder) },
                { label: t("libraryDetail.belongsTo"), value: entry.parent },
                { label: "Path", value: entry.path, mono: true },
              ]}
            />
            {!isStock && (
              <div className="flex flex-wrap gap-2 pt-0.5">
                <Button variant="outline" size="sm" onClick={() => onShare(entry)}>
                  <Share2 className="size-3.5" /> {t("share.share")}
                </Button>
                {canMove && (
                  <Button variant="outline" size="sm" onClick={() => onMove(entry)}>
                    <FolderInput className="size-3.5" /> Move
                  </Button>
                )}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => onUninstall(entry)}
                  className="text-destructive hover:text-destructive"
                >
                  <Trash2 className="size-3.5" /> Uninstall
                </Button>
              </div>
            )}
          </Panel>

          <WhatsInside groups={inside} />
        </div>
      </div>

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
