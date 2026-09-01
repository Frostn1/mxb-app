import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronDown,
  ChevronRight,
  Plus,
  Check,
  X,
  Search,
} from "lucide-react";
import { Dialog, DialogContent } from "@/Components/ui/dialog";
import { Button } from "@/Components/ui/button";
import { useT } from "../../i18n/context";
import { labelOf } from "../../i18n/core";
import { Badge } from "@/Components/ui/badge";
import { cn } from "@/lib/utils";
import {
  bikeNamesFromDest,
  bikeOfDest,
  bikeVariants,
  defaultMirrorIndex,
  destForVariant,
  installsOutsideMods,
  isBlockedDownload,
  isServerOnly,
  pickDownloadForBike,
  playableMirrors,
  sortMirrors,
  variantForBike,
  type DestOption,
  type ModType,
} from "../../api/mods";
import type { DownloadOption, ModDetail as Detail } from "../../types";

export interface InstallChoice {
  destFolder: string;
  /** Absent when there was nothing to choose between — a shop purchase has one file. */
  mirror?: DownloadOption;
}

interface InstallDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Absent for a shop purchase: it has no mirrors, and its artwork comes from the catalog. */
  detail?: Detail;
  /** Header title/thumbnail when there is no `detail` to take them from. */
  title?: string;
  image?: string | null;
  modType: ModType;
  destOptions: DestOption[];
  /** Ranked "probable" destination values (best first) — e.g. the matched bike. */
  suggestions: string[];
  /** folder value → number of mods currently installed there. */
  folderCounts: Map<string, number>;
  /** Preselected destination (remembered per category). */
  initialFolder: string;
  /** Bike **sound** mod: downloads are per-bike (not mirrors), so the picked
   * bike drives which link we grab. */
  sound?: boolean;
  onConfirm: (choice: InstallChoice) => void;
}

/** Where a ReShade preset lands, shown verbatim. Mirrors `reshade::PRESET_DIR`, under the
 *  game's install folder rather than the mods tree. */
const RESHADE_DEST = "FrostMod ReShade";

/** Every download the page offers, playable ones first and server builds last. */
function useMirrors(detail: Detail | undefined): DownloadOption[] {
  return useMemo(() => (detail ? sortMirrors(detail) : []), [detail]);
}

/**
 * How many mirrors are listed before the rest collapse behind "N more".
 *
 * More than one, deliberately: a single row reads as the only file there is, which is how
 * someone ends up installing a mirror that turned out to be a server build without ever
 * seeing there was a choice.
 */
const MIRRORS_SHOWN = 3;

export default function InstallDialog({
  open,
  onOpenChange,
  detail,
  title,
  image,
  modType,
  destOptions,
  suggestions,
  folderCounts,
  initialFolder,
  sound = false,
  onConfirm,
}: InstallDialogProps) {
  const t = useT();
  const mirrors = useMirrors(detail);
  const outsideMods = installsOutsideMods(modType);
  const [folder, setFolder] = useState(initialFolder);
  const [folderOpen, setFolderOpen] = useState(false);
  const [folderSearch, setFolderSearch] = useState("");
  const [creating, setCreating] = useState(false);
  const [newFolder, setNewFolder] = useState("");
  const [mirrorIdx, setMirrorIdx] = useState(0);
  const [mirrorsOpen, setMirrorsOpen] = useState(false);
  const [serverOpen, setServerOpen] = useState(false);

  // Reset transient state each time the dialog opens.
  useEffect(() => {
    if (open) {
      setFolder(initialFolder);
      setFolderOpen(false);
      setFolderSearch("");
      setCreating(false);
      setNewFolder("");
    }
  }, [open, initialFolder]);

  // The picked download resets with the dialog, and again if the page's downloads land
  // after it opened. Never onto a server build while a playable file is on offer — that
  // preselection is what quietly installed the wrong build.
  useEffect(() => {
    if (open) {
      setMirrorIdx(defaultMirrorIndex(mirrors));
      setMirrorsOpen(false);
      setServerOpen(false);
    }
  }, [open, mirrors]);

  // The bikes this picker can install to, for reading the downloads against.
  const bikeNames = useMemo(
    () => (modType.id === "bikes" ? bikeNamesFromDest(destOptions) : []),
    [modType.id, destOptions],
  );
  // A page that offers one file per bike rather than mirrors of one — the author labels the
  // blocks `250f` and `125t` and the site flags both as the default.
  const variants = useMemo(() => bikeVariants(mirrors, bikeNames), [mirrors, bikeNames]);
  // Sound packs are per-bike by category; a livery has to be read off its own downloads.
  const perBike = sound || variants.perBike;

  // The chosen bike (the folder value, minus any `/paints`) decides which *download* to
  // grab, since the links are per-bike rather than mirrors of one file.
  const bikeName = perBike ? bikeOfDest(folder) : "";
  useEffect(() => {
    if (!perBike || !bikeName) return;
    const picked = sound
      ? pickDownloadForBike(mirrors, bikeName)
      : variantForBike(mirrors, variants, bikeName);
    const idx = picked ? mirrors.indexOf(picked) : -1;
    if (idx >= 0) setMirrorIdx(idx);
  }, [perBike, sound, bikeName, mirrors, variants]);

  const folderLabel = useMemo(() => {
    if (creating && newFolder.trim()) return newFolder.trim();
    const opt = destOptions.find((o) => o.value === folder);
    if (opt) return labelOf(opt, t);
    return folder || t("library.rootFolder");
  }, [creating, newFolder, destOptions, folder, t]);

  // Probable destinations (ranked) resolved to options, best first.
  const suggestedOptions = useMemo(() => {
    const byValue = new Map(destOptions.map((o) => [o.value, o]));
    return suggestions
      .map((v) => byValue.get(v))
      .filter((o): o is DestOption => Boolean(o));
  }, [suggestions, destOptions]);

  // Command-style filter over every destination.
  const filteredOptions = useMemo(() => {
    const q = folderSearch.trim().toLowerCase();
    if (!q) return destOptions;
    return destOptions.filter((o) => labelOf(o, t).toLowerCase().includes(q));
  }, [folderSearch, destOptions, t]);

  const suggestedValues = useMemo(
    () => new Set(suggestedOptions.map((o) => o.value)),
    [suggestedOptions],
  );

  const selectedMirror = mirrors[mirrorIdx];
  const thumb = image ?? detail?.images[0];
  const subtitleType =
    modType.id === "bikes"
      ? t("category.bike")
      : modType.id === "rider"
        ? t("modType.rider")
        : t("category.track");

  const commitNewFolder = () => {
    const v = newFolder.trim();
    if (!v) return;
    setFolder(v);
    setCreating(false);
    setFolderOpen(false);
  };

  const chooseFolder = (value: string) => {
    setFolder(value);
    setCreating(false);
    setFolderOpen(false);
    setFolderSearch("");
  };

  const renderRow = (o: DestOption) => {
    const on = !creating && o.value === folder;
    const count = folderCounts.get(o.value) ?? 0;
    return (
      <button
        key={o.value || "__root__"}
        onClick={() => chooseFolder(o.value)}
        className={cn(
          "flex w-full cursor-default items-center justify-between gap-2 rounded-md px-3 py-2 text-[12.5px] transition-colors",
          on
            ? "bg-accent font-semibold text-accent-foreground"
            : "text-foreground/90 hover:bg-foreground/[0.06]",
        )}
      >
        <span className="min-w-0 flex-1 truncate text-left">{labelOf(o, t)}</span>
        <span className="flex flex-none items-center gap-2 text-[11px] text-faint">
          <span>{count} mods</span>
          {on && <Check className="size-3.5 text-primary" />}
        </span>
      </button>
    );
  };

  const confirm = () => {
    // Only a mod with mirrors has one to insist on; a purchase has a single file.
    if (detail && !selectedMirror) return;
    const destFolder = creating && newFolder.trim() ? newFolder.trim() : folder;
    onConfirm({ destFolder, mirror: selectedMirror });
  };

  const serverBuilds = useMemo(() => mirrors.filter((m) => m.isServer), [mirrors]);
  const playable = useMemo(() => playableMirrors(mirrors), [mirrors]);
  // Nothing playable on the page means there is no "other" list to fold the server builds
  // away behind: they are the downloads, shown as such and clearly marked.
  const serverOnly = isServerOnly(mirrors);
  const mainList = serverOnly ? mirrors : playable;

  // Show every option when they're per-bike files (each is a different bike, not a mirror);
  // otherwise list the first few and fold the rest away.
  const shownMirrors = mirrorsOpen || perBike ? mainList : mainList.slice(0, MIRRORS_SHOWN);
  const hiddenCount = mainList.length - shownMirrors.length;

  /**
   * Picking a per-bike file moves the destination to the bike it's for — the mismatch this
   * whole path exists to prevent is just as easy to create from the link side.
   */
  const chooseMirror = (idx: number) => {
    setMirrorIdx(idx);
    if (!perBike || sound || creating) return;
    if (variants.bikes[idx]?.has(bikeName)) return;
    const dest = destForVariant(variants, idx, suggestions);
    if (dest) setFolder(dest);
  };

  const renderMirror = (m: DownloadOption) => {
    const idx = mirrors.indexOf(m);
    const on = idx === mirrorIdx;
    const blocked = isBlockedDownload(m);
    // The author's own name for the file, where it says more than the host does. Two
    // MediaFire rows are otherwise indistinguishable — and one of them may be the server
    // build the parser had to guess at.
    const fileLabel =
      m.label && m.label.toLowerCase() !== m.host.toLowerCase() ? m.label : "";
    // Whether this file is the one for the bike being installed to. A sound pack is judged
    // by what the matcher settled on; a per-bike page says outright which bike each file is
    // for, so its rows are judged by that rather than by which one happens to be selected.
    const forThisBike = sound ? on : !!variants.bikes[idx]?.has(bikeName);
    // With no bike picked yet — the destination is still the bikes root — there is nothing to
    // match against, and calling every file "different" would be answering a question nobody
    // asked. Those rows read as they always have until a bike is chosen.
    const saysWhichBike = perBike && (sound || !!bikeName);
    const note = blocked
      ? t("installDialog.opensInBrowser")
      : m.isServer
        ? t("installDialog.serverBuildNote")
        : saysWhichBike
          ? forThisBike
            ? t("installDialog.matchedBike")
            : t("installDialog.differentBike")
          : m.isDefault
            ? t("installDialog.directFastest")
            : t("installDialog.direct");
    return (
      <button
        key={`${m.url}-${idx}`}
        onClick={() => chooseMirror(idx)}
        className={cn(
          "flex cursor-default items-center gap-[11px] rounded-[9px] border bg-background px-3 py-2.5 text-left transition-colors",
          on ? "border-primary/50" : "border-input hover:border-white/20",
        )}
      >
        <span
          className={cn(
            "size-[15px] flex-none rounded-full",
            on ? "border-4 border-primary" : "border-[1.5px] border-foreground/25",
          )}
        />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-[12.5px] font-semibold">{m.host}</span>
          <span className="text-[11px] text-muted-foreground">{note}</span>
          {fileLabel && (
            <span className="truncate font-mono text-[10.5px] text-faint">{fileLabel}</span>
          )}
        </span>
        {m.isServer ? (
          <Badge variant="warning" className="flex-none">
            {t("installDialog.serverBadge")}
          </Badge>
        ) : blocked ? (
          <Badge variant="warning" className="flex-none">
            {t("installDialog.browserBadge")}
          </Badge>
        ) : saysWhichBike ? (
          // Every block on a per-bike page carries the site's "Default" flag, so the badge
          // that meant "this is the one" said it about both files. Which bike it's for is
          // the thing worth flagging instead.
          forThisBike ? (
            <Badge variant="success" className="flex-none border-primary/35 text-primary">
              {t("installDialog.matchedBadge")}
            </Badge>
          ) : null
        ) : m.isDefault ? (
          <Badge variant="success" className="flex-none border-primary/35 text-primary">
            {t("installDialog.recommendedBadge")}
          </Badge>
        ) : null}
      </button>
    );
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        showClose={false}
        className="flex max-h-[85vh] max-w-[460px] flex-col gap-0 overflow-hidden rounded-2xl p-0"
      >
        {/* header */}
        <div className="flex flex-none items-center gap-3 border-b border-white/[0.07] px-[18px] pb-3.5 pt-4">
          <div
            className="h-[34px] w-[52px] flex-none rounded-md bg-gradient-to-br from-[#3a3f45] to-[#20242a] bg-cover bg-center"
            style={thumb ? { backgroundImage: `url(${thumb})` } : undefined}
          />
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[14px] font-bold">
              {title ?? detail?.title}
            </span>
            <span className="text-[11.5px] text-muted-foreground">
              {subtitleType}
              {detail?.version ? ` · ${detail.version}` : ""}
            </span>
          </div>
          <button
            onClick={() => onOpenChange(false)}
            className="cursor-default text-faint transition-colors hover:text-foreground"
          >
            <X className="size-4" />
          </button>
        </div>

        {/* body */}
        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-[18px] py-4">
          {/* destination */}
          <section className="flex flex-col gap-2">
            <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
              {t("installDialog.installTo")}
            </span>
            {/* A ReShade preset doesn't live in the mods tree and has exactly one home, so
                it shows where it lands and offers no folder to change. */}
            {outsideMods ? (
              <div className="flex items-center gap-2.5 rounded-[9px] border border-input bg-background px-3 py-2.5">
                <ChevronRight className="size-3.5 flex-none text-primary" />
                <span className="flex-1 truncate text-left font-mono text-[12px] text-muted-foreground">
                  <b className="text-foreground">{RESHADE_DEST}</b>
                </span>
              </div>
            ) : (
              <button
                onClick={() => setFolderOpen((v) => !v)}
                className="flex cursor-default items-center gap-2.5 rounded-[9px] border border-input bg-background px-3 py-2.5"
              >
                <ChevronRight className="size-3.5 flex-none text-primary" />
                <span className="flex-1 truncate text-left font-mono text-[12px] text-muted-foreground">
                  {modType.installSubpath.replace(/\//g, "\\")}\
                  <b className="text-foreground">{folderLabel}</b>
                </span>
                <span className="flex flex-none items-center gap-1 text-[11px] text-muted-foreground">
                  {t("installDialog.change")} <ChevronDown className="size-3" />
                </span>
              </button>
            )}

            {folderOpen && !outsideMods && (
              <div className="flex flex-col overflow-hidden rounded-[10px] border border-input bg-popover shadow-[0_12px_32px_rgba(0,0,0,0.5)]">
                {/* command-style search */}
                <div className="flex items-center gap-2 border-b border-border px-3 py-2">
                  <Search className="size-3.5 flex-none text-faint" />
                  <input
                    autoFocus
                    value={folderSearch}
                    onChange={(e) => setFolderSearch(e.target.value)}
                    placeholder={
                      modType.id === "bikes"
                        ? t("installDialog.searchBikes")
                        : t("installDialog.searchFolders")
                    }
                    className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
                  />
                </div>

                {/* scrollable results */}
                <div className="max-h-[240px] overflow-y-auto p-1.5">
                  {!folderSearch && suggestedOptions.length > 0 && (
                    <>
                      <div className="px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-faint">
                        {t("installDialog.probably")}
                      </div>
                      {suggestedOptions.map(renderRow)}
                      <div className="mx-1.5 my-1 h-px bg-border" />
                      <div className="px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-faint">
                        {t("installDialog.allFolders")}
                      </div>
                    </>
                  )}
                  {(folderSearch
                    ? filteredOptions
                    : destOptions.filter((o) => !suggestedValues.has(o.value))
                  ).map(renderRow)}
                  {folderSearch && filteredOptions.length === 0 && (
                    <div className="px-3 py-4 text-center text-[12px] text-muted-foreground">
                      {t("installDialog.noFolderMatch")}
                    </div>
                  )}
                </div>

                {/* new folder, pinned */}
                <div className="border-t border-border p-1.5">
                  {creating ? (
                    <input
                      autoFocus
                      value={newFolder}
                      onChange={(e) => setNewFolder(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") commitNewFolder();
                        if (e.key === "Escape") setCreating(false);
                      }}
                      onBlur={commitNewFolder}
                      placeholder={
                        modType.id === "bikes"
                          ? "KTM450/paints"
                          : t("library.newFolderName")
                      }
                      className="w-full rounded-md bg-transparent px-3 py-2 text-[12.5px] text-foreground placeholder:text-faint focus:outline-none"
                    />
                  ) : (
                    <button
                      onClick={() => {
                        setCreating(true);
                        setNewFolder(folderSearch);
                      }}
                      className="flex w-full cursor-default items-center gap-1.5 rounded-md px-3 py-2 text-[12.5px] font-semibold text-primary hover:bg-foreground/[0.06]"
                    >
                      <Plus className="size-3.5" /> {t("library.newFolder")}
                    </button>
                  )}
                </div>
              </div>
            )}
            <span className="text-[11px] text-faint">
              {t("installDialog.rememberedFor", { type: t(modType.label) })}
            </span>
          </section>

          {/* mirrors */}
          {mirrors.length > 0 && (
            <section className="flex flex-col gap-2">
              <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                {perBike
                  ? t("installDialog.downloadPerBike")
                  : t("installDialog.downloadFrom")}
              </span>
              {/* A mod with nothing but server files is worth saying outright: it installs
                  fine and then does nothing in-game, which reads as a broken install. */}
              {serverOnly && (
                <div className="flex items-start gap-2.5 rounded-[10px] border border-warning/30 bg-warning/[0.07] px-3 py-2.5">
                  <AlertTriangle className="mt-px size-3.5 flex-none text-warning" />
                  <span className="text-[11.5px] text-warning/90">
                    {t("installDialog.serverOnlyNotice")}
                  </span>
                </div>
              )}
              <div className="flex flex-col gap-1.5">
                {shownMirrors.map(renderMirror)}
                {!sound && hiddenCount > 0 && (
                  <button
                    onClick={() => setMirrorsOpen(true)}
                    className="flex cursor-default items-center gap-1 self-start px-1 text-[11px] text-muted-foreground hover:text-foreground"
                  >
                    {t("installDialog.moreMirrors", { count: hiddenCount })}
                    <ChevronDown className="size-3" />
                  </button>
                )}
              </div>

              {/* Server builds are listed, not hidden — folded away by default, because
                  they're rarely what's wanted, but reachable for whoever runs a server. */}
              {!serverOnly && serverBuilds.length > 0 && (
                <div className="flex flex-col gap-1.5">
                  <button
                    onClick={() => setServerOpen((v) => !v)}
                    className="flex cursor-default items-center gap-1 self-start px-1 text-[11px] text-muted-foreground hover:text-foreground"
                  >
                    {t("installDialog.serverFiles", { count: serverBuilds.length })}
                    <ChevronDown
                      className={cn("size-3 transition-transform", serverOpen && "rotate-180")}
                    />
                  </button>
                  {serverOpen && serverBuilds.map(renderMirror)}
                </div>
              )}

              {mirrors.length > 1 && sound && (
                <span className="text-[11px] text-faint">
                  {t("installDialog.perBikeHint")}
                </span>
              )}
            </section>
          )}
        </div>

        {/* footer */}
        <div className="flex flex-none gap-2.5 border-t border-white/[0.07] px-[18px] py-3.5">
          <Button
            className="min-w-0 flex-1"
            onClick={confirm}
            disabled={!!detail && !selectedMirror}
          >
            <span className="truncate">
              {t("installDialog.installToFolder", { folder: folderLabel })}
            </span>
          </Button>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
