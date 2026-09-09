import { useCallback, useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";
import {
  AlertTriangle,
  ChevronLeft,
  ArrowLeft,
  ExternalLink,
  Check,
  Copy,
  Snowflake,
  FileDown,
  Maximize2,
  ChevronRight,
  X,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-shell";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { useT, type TKey } from "@frost/shared/i18n/context";
import {
  buildDestinations,
  buildRiderDestinations,
  defaultMirrorIndex,
  destStorageKey,
  getInstalledMods,
  getModDetail,
  isBlockedDownload,
  isLiveryContext,
  isServerOnly,
  isSoundContext,
  riderTarget,
  resolveInitialFolder,
  scanBikeTargets,
  scanRiderTargets,
  sortMirrors,
  type DestOption,
  type ModType,
} from "@frost/shared/api/mods";
import type {
  DownloadOption,
  InstalledMod,
  InstallStage,
  ModDetail as Detail,
} from "@frost/shared/types";
import { ContextBarLeft } from "../Shell/ContextBar";
import CachedImg from "@frost/shared/Components/ui/cached-img";
import RichDescription from "./RichDescription";
import InstallDialog, { type InstallChoice } from "./InstallDialog";
import { useInstall } from "../../Context/Install";
import type { InstalledIndex } from "../../lib/installedMatch";
import { fileFormat, formatDate } from "@frost/shared/lib/mods";
import { Button } from "@frost/shared/Components/ui/button";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@frost/shared/Components/ui/alert-dialog";
import { cn } from "@frost/shared/lib/utils";
import { useConfig } from "@frost/shared/Context/Config";

interface ModDetailProps {
  slug: string;
  modType: ModType;
  /** Browse category the mod was opened under — drives bike-livery routing. */
  categoryId: number;
  installed: InstalledIndex;
  onBack: () => void;
}

const CHAIN: { key: string; label: TKey }[] = [
  { key: "resolving", label: "modDetail.stageResolve" },
  { key: "downloading", label: "modDetail.stageDownload" },
  { key: "extracting", label: "modDetail.stageExtract" },
  { key: "placing", label: "modDetail.stagePlace" },
  { key: "reload", label: "modDetail.stageReload" },
];

function stageIndex(stage: InstallStage): number {
  switch (stage) {
    case "resolving":
      return 0;
    case "downloading":
      return 1;
    case "extracting":
      return 2;
    case "placing":
      return 3;
    // The bytes are down and classified; what is left is the user's decision, not ours.
    case "review":
      return 3;
    case "done":
      return 4;
    default:
      return -1;
  }
}

export default function ModDetail({
  slug,
  modType,
  categoryId,
  installed,
  onBack,
}: ModDetailProps) {
  const t = useT();
  const { game } = useConfig();
  const livery = isLiveryContext(modType, categoryId);
  const sound = isSoundContext(modType, categoryId);
  // Which rider folder this category installs into — a gear model's paints, something worn
  // on the rider model, or a model of its own. `null` when the category doesn't say.
  const rider = useMemo(
    () => riderTarget(game, modType, categoryId),
    [game, modType, categoryId],
  );
  const [derivedDest, setDerivedDest] = useState(false);
  /** Which screenshot the hero is showing. Declared with the other hooks: it used to
   *  sit below the loading and error returns, which is a rules-of-hooks violation. */
  const [heroIdx, setHeroIdx] = useState(0);
  /** Whether the screenshot is open full-window. */
  const [zoom, setZoom] = useState(false);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // The raw file list — only for destination folders and their counts. The badge uses
  // the `installed` index prop, which also sees folders and paints.
  const [installedFiles, setInstalledFiles] = useState<InstalledMod[]>([]);
  const [destOptions, setDestOptions] = useState<DestOption[]>([]);
  const [guess, setGuess] = useState("");
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [blocked, setBlocked] = useState<{
    mirror: DownloadOption;
    step1: boolean;
  } | null>(null);
  const [copied, setCopied] = useState(false);
  const [confirmReinstall, setConfirmReinstall] = useState(false);
  // Bumped by the Retry button below. The load otherwise only re-runs when the slug changes,
  // so a user the catalog refused once had no way back short of leaving the page.
  const [reloadKey, setReloadKey] = useState(0);

  const { activeFor, startInstall, startImport } = useInstall();
  const myActive = activeFor(slug);

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setLoadError(null);
    setBlocked(null);
    setDestOptions([]);
    setGuess("");
    setSuggestions([]);
    setDerivedDest(false);
    getModDetail(slug)
      .then(async (d) => {
        if (cancelled) return;
        setDetail(d);
        try {
          const inst = await getInstalledMods(modType.installSubpath);
          if (cancelled) return;
          setInstalledFiles(inst);
          // OEM bikes own no file until they're painted, so the scan of `mods/bikes` can't
          // see them — the backend reads their ids out of the profile as well.
          const bikeTargets =
            modType.id === "bikes" ? await scanBikeTargets().catch(() => []) : [];
          if (cancelled) return;
          // Rider content routes into a gear model's, or the rider model's, own folder;
          // everything else uses the generic (track/bike) destination logic.
          const dest =
            modType.id === "rider"
              ? buildRiderDestinations(
                  game,
                  await scanRiderTargets(),
                  d.title,
                  d.categories,
                  rider,
                )
              : buildDestinations(
                  modType,
                  d.title,
                  inst,
                  livery,
                  sound,
                  d.categories,
                  bikeTargets,
                );
          if (cancelled) return;
          setDestOptions(dest.options);
          setGuess(dest.guess);
          setSuggestions(dest.suggestions);
          setDerivedDest("derived" in dest && dest.derived === true);
        } catch {
          setInstalledFiles([]);
          setDestOptions([]);
        }
      })
      .catch((e) => !cancelled && setLoadError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [slug, modType, livery, sound, game, rider, reloadKey]);

  const folderCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const it of installedFiles) m.set(it.folder, (m.get(it.folder) ?? 0) + 1);
    return m;
  }, [installedFiles]);

  // "Official" mirror + metadata for the collapsed install panel.
  const mirrors = useMemo(() => (detail ? sortMirrors(detail) : []), [detail]);

  // What the dialog would start on — the best playable file, so the panel below the button
  // describes the download that's actually about to run.
  const primary = mirrors[defaultMirrorIndex(mirrors)] ?? null;
  const format = primary ? fileFormat(primary.url) : null;
  // Server builds aren't mirrors of the playable file, so they don't belong in this count.
  const mirrorNames = [
    ...new Set(mirrors.filter((m) => !m.isServer).map((m) => m.host)),
  ].join(" · ");
  const serverOnly = isServerOnly(mirrors);

  const destKey = destStorageKey(game, modType);
  const initialFolder = useMemo(
    () =>
      resolveInitialFolder(game, modType, destOptions, guess, livery, sound, {
        target: rider,
        derived: derivedDest,
      }),
    [game, modType, destOptions, guess, livery, sound, rider, derivedDest],
  );

  const isInstalled = detail !== null && installed.has(detail.title);

  // Already have it? Confirm before overwriting; otherwise open the dialog.
  const openInstall = () => {
    if (isInstalled) setConfirmReinstall(true);
    else setDialogOpen(true);
  };

  const handleConfirm = ({ destFolder, mirror }: InstallChoice) => {
    localStorage.setItem(destKey, destFolder);
    setDialogOpen(false);
    // A mod always has mirrors, so the dialog always returns one here; the field is optional
    // only because a shop purchase, which has a single file, uses the same dialog.
    if (!mirror) return;
    if (isBlockedDownload(mirror)) {
      setBlocked({ mirror, step1: false });
      // pre-remember the chosen folder for the import step
      localStorage.setItem(destKey, destFolder);
    } else if (detail) {
      startInstall({
        slug,
        title: detail.title,
        subpath: modType.installSubpath,
        destFolder,
        categoryId,
        url: mirror.url,
        host: mirror.host,
      });
    }
  };

  const chooseAndImport = async () => {
    const picked = await pickFile({
      multiple: false,
      filters: [
        { name: t("modDetail.modFiles"), extensions: ["pkz", "zip", "rar", "7z"] },
      ],
    });
    if (typeof picked !== "string" || !detail) return;
    setBlocked(null);
    startImport({
      slug,
      title: detail.title,
      subpath: modType.installSubpath,
      destFolder: localStorage.getItem(destKey) ?? "",
      categoryId,
      path: picked,
    });
  };

  const copyError = () => {
    if (!myActive?.message) return;
    navigator.clipboard.writeText(myActive.message);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  if (loadError) {
    return (
      <div className="flex h-full flex-col px-7 py-5">
        <Breadcrumb modType={modType} title="—" onBack={onBack} link={null} />
        <div className="mt-6 flex flex-col items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/[0.06] p-4">
          <p className="text-[13px] font-semibold text-destructive">
            {t("modDetail.loadFailed")}
          </p>
          {/* Selectable: a block explains itself in a sentence, and carries the Cloudflare
              ray that identifies it — which is the whole of what a bug report needs. */}
          <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
            {loadError.replace(/^Error:\s*/, "")}
          </p>
          <Button variant="outline" size="sm" onClick={() => setReloadKey((n) => n + 1)}>
            {t("common.retry")}
          </Button>
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-full flex-col px-7 py-5">
        <Breadcrumb modType={modType} title="…" onBack={onBack} link={null} />
        <div className="grid flex-1 place-items-center text-muted-foreground">
          <Snowflake className="size-7 animate-spin [animation-duration:2.5s]" />
        </div>
      </div>
    );
  }

  const pct =
    myActive?.total && myActive.received
      ? Math.round((myActive.received / myActive.total) * 100)
      : undefined;
  const idx = myActive ? stageIndex(myActive.stage) : -1;

  // Clamped once: opening a mod with fewer screenshots than the last one leaves the index
  // past the end, and the thumbnail strip and the viewer have to agree with the hero on
  // which picture that is.
  const shotIdx = Math.min(heroIdx, Math.max(0, detail.images.length - 1));
  const shot = detail.images[shotIdx];

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Browse's type tabs go with Browse, so the bar above would otherwise be an empty
          44px band. It carries where you are instead. */}
      <ContextBarLeft>
        <span className="flex items-center gap-2 font-cond text-[12.5px] font-semibold uppercase tracking-[0.16em]">
          <button
            onClick={onBack}
            className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
          >
            {t(modType.label)}
          </button>
          <span className="text-faint">/</span>
          <span className="max-w-[420px] truncate text-foreground">{detail.title}</span>
        </span>
      </ContextBarLeft>

      {/* The artwork carries the name. A breadcrumb over a text column was the same page
          every catalog has; this is the one the mockup drew. */}
      <div className="relative h-[330px] flex-none overflow-hidden bg-card">
        {shot && (
          <>
            {/* A blurred copy fills the band; the screenshot itself is shown whole beside it.
                `object-cover` here cropped a 16:9 shot into a 4:1 slot — most of the picture
                was off-screen, and cycling the thumbnails just swapped one sliver for another.
                The 42% column is exactly the width a 16:9 image fills at this height, so the
                two gradients below stop where the picture starts and never wash over it. */}
            <CachedImg
              src={shot}
              width={640}
              alt=""
              aria-hidden
              className="absolute inset-0 size-full scale-125 object-cover opacity-60 blur-[22px]"
            />
            <button
              onClick={() => setZoom(true)}
              aria-label={detail.title}
              className="group absolute inset-y-0 right-0 w-[42%] cursor-default"
            >
              <CachedImg
                src={shot}
                width={1280}
                alt={detail.title}
                // `drop-shadow`, not `shadow`: with `object-contain` the element is the whole
                // 42% column, so a box shadow would draw an edge where the picture isn't. A
                // filter follows the pixels, which is what has to lift off the blur behind it.
                className="size-full object-contain object-right drop-shadow-[-16px_0_26px_rgba(0,0,0,0.45)]"
              />
              <span className="absolute right-3 top-3 grid size-7 place-items-center border border-white/25 bg-black/45 text-white/85 opacity-0 transition-opacity group-hover:opacity-100">
                <Maximize2 className="size-3.5" />
              </span>
            </button>
          </>
        )}
        {/* Both scrims stop at 58% — the picture's edge — so the title stays readable over a
            bright screenshot without any of it washing across the picture itself. */}
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-[rgba(6,6,7,0.92)] via-[rgba(6,6,7,0.6)] via-35% to-transparent to-58%" />
        <div className="pointer-events-none absolute bottom-0 left-0 h-[200px] w-[58%] bg-gradient-to-t from-[rgba(6,6,7,0.9)] via-[rgba(6,6,7,0.45)] via-50% to-transparent" />

        <button
          onClick={onBack}
          className="u-skew absolute left-7 top-5 flex h-8 cursor-default items-center border border-white/25 bg-black/40 px-3 text-white/85 transition-colors hover:text-white"
        >
          <span className="u-unskew flex items-center gap-1.5">
            <ChevronLeft className="size-3.5" />
            <span className="font-cond text-[12px] font-semibold uppercase tracking-[0.14em]">
              {t(modType.label)}
            </span>
          </span>
        </button>

        <div className="absolute inset-x-0 bottom-0 max-w-[58%] px-7 pb-5">
          <h1 className="font-cond text-[42px] font-bold uppercase leading-[0.94] tracking-[0.005em] text-white">
            {detail.title}
          </h1>
          <div className="mt-2.5 flex flex-wrap items-center gap-2.5 text-[12.5px] text-white/65">
            {detail.author && <span className="text-white/85">{detail.author}</span>}
            {detail.author && <span className="text-white/30">/</span>}
            <span className="tabular-figures">{formatDate(detail.date)}</span>
            {detail.version && (
              <>
                <span className="text-white/30">/</span>
                <span className="font-mono text-[11.5px]">{detail.version}</span>
              </>
            )}
            {isInstalled && (
              <>
                <span className="text-white/30">/</span>
                <span className="flex items-center gap-1 text-success">
                  <Check className="size-3" strokeWidth={3} /> In library
                </span>
              </>
            )}
          </div>
        </div>
      </div>

      <div className="flex min-h-0 flex-1 gap-6 px-7 pb-5 pt-4">
        {/* left: gallery + description */}
        <div className="flex min-w-0 flex-1 flex-col gap-3.5 overflow-y-auto pr-1">
          {detail.images.length > 1 && (
            <div className="flex flex-none gap-2 overflow-x-auto pb-1">
              {detail.images.map((img, i) => (
                <button
                  key={img}
                  onClick={() => setHeroIdx(i)}
                  className={cn(
                    "u-notch relative h-[62px] w-[104px] flex-none overflow-hidden bg-card transition-opacity",
                    i === shotIdx ? "outline outline-2 -outline-offset-2 outline-primary" : "opacity-60 hover:opacity-100",
                  )}
                >
                  <CachedImg src={img} width={240} alt="" className="size-full object-cover" />
                </button>
              ))}
            </div>
          )}

          <div className="flex flex-col gap-2 pt-1">
            <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
              About this {modType.id === "bikes" ? "bike" : modType.id === "rider" ? "rider gear" : "track"}
            </span>
            {/* Authored HTML from mxb-mods.com's REST API. */}
            <RichDescription html={detail.descriptionHtml} />
          </div>
        </div>

        {/* right rail */}
        <div className="flex w-[340px] flex-none flex-col gap-3 overflow-y-auto">
          {/* install panel */}
          <div className="flex flex-col gap-3 rounded-xl border border-input bg-card p-4">
            {myActive && idx >= 0 ? (
              <InstallProgress
                stage={myActive.stage}
                idx={idx}
                pct={pct}
                received={myActive.received}
                total={myActive.total}
              />
            ) : myActive?.stage === "error" ? (
              <div className="flex flex-col gap-2">
                <div className="rounded-lg border border-destructive/40 bg-destructive/[0.08] p-3 text-[12px] text-destructive">
                  <span className="select-text font-mono">{myActive.message}</span>
                </div>
                <div className="flex gap-2">
                  <Button
                    size="sm"
                    className="flex-1"
                    onClick={() => setDialogOpen(true)}
                  >
                    {t("common.tryAgain")}
                  </Button>
                  <Button size="sm" variant="outline" onClick={copyError}>
                    <Copy className="size-3.5" /> {copied ? t("modDetail.copied") : t("modDetail.copy")}
                  </Button>
                </div>
              </div>
            ) : blocked ? (
              <BlockedHost
                host={blocked.mirror.host}
                step1={blocked.step1}
                onOpen={() => {
                  open(blocked.mirror.url);
                  setBlocked((b) => (b ? { ...b, step1: true } : b));
                }}
                onChoose={chooseAndImport}
              />
            ) : primary ? (
              <>
                {/* Every file this page offers is a dedicated-server build. Said before the
                    button, not after the install: it lands in the library either way and
                    then does nothing in-game, which reads as a broken mod. */}
                {serverOnly && (
                  <div className="flex items-start gap-2.5 border border-warning/30 bg-warning/[0.07] px-3 py-2.5">
                    <AlertTriangle className="mt-px size-3.5 flex-none text-warning" />
                    <span className="text-[12px] text-warning/90">
                      {t("modDetail.serverOnlyNotice")}
                    </span>
                  </div>
                )}
                <Button className="h-11 w-full text-[14px]" onClick={openInstall}>
                  {isInstalled ? t("browse.reinstall") : t("modDetail.addToLibrary")}
                </Button>
                <Row label={t("modDetail.host")} value={primary.host} />
                <Row
                  label={t("modDetail.installsTo")}
                  value={`${modType.installSubpath.replace(/\//g, "\\")}\\`}
                  mono
                />
              </>
            ) : (
              <p className="text-[12.5px] text-muted-foreground">
                {t("modDetail.noDownloadLink", { site: game.catalogDomain })}
              </p>
            )}
          </div>

          {/* What happens once the install finishes. FrostMod hot-reloads the game, but
              it's an MX Bikes plugin — promising a reload for a title that has none is
              worse than saying nothing, so that case gets the honest instruction. */}
          <div className="flex items-center gap-2.5 border border-success/25 bg-success/[0.06] px-3 py-2.5">
            <span className="size-[7px] flex-none rounded-full bg-success" />
            <span className="text-[12px] text-success/90">
              {t(game.caps.frostmod ? "modDetail.frostmodHint" : "modDetail.restartHint", {
                game: game.display,
                kind:
                  modType.id === "rider"
                    ? t("modDetail.kindRider")
                    : modType.id === "bikes"
                      ? t("modDetail.kindBike")
                      : t("modDetail.kindTrack"),
              })}
            </span>
          </div>

          {/* details */}
          <div className="flex flex-col gap-2.5 rounded-xl border border-white/[0.07] bg-card px-4 py-3.5">
            <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
              {t("modDetail.details")}
            </span>
            {format && <Row label={t("modDetail.format")} value={format} mono />}
            {mirrorNames && <Row label={t("modDetail.mirrors")} value={mirrorNames} />}
            <Row label={t("modDetail.type")} value={t(modType.label)} />
          </div>
        </div>
      </div>

      {detail && (
        <InstallDialog
          open={dialogOpen}
          onOpenChange={setDialogOpen}
          detail={detail}
          modType={modType}
          destOptions={destOptions}
          suggestions={suggestions}
          folderCounts={folderCounts}
          initialFolder={initialFolder}
          sound={sound}
          onConfirm={handleConfirm}
        />
      )}

      <AlertDialog open={confirmReinstall} onOpenChange={setConfirmReinstall}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("browse.reinstallOne", { title: detail.title })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("browse.reinstallOneBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setConfirmReinstall(false);
                setDialogOpen(true);
              }}
            >
              {t("browse.reinstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {zoom && shot && (
        <Lightbox
          images={detail.images}
          index={shotIdx}
          onIndex={setHeroIdx}
          onClose={() => setZoom(false)}
          title={detail.title}
        />
      )}
    </div>
  );
}

/**
 * One screenshot at the size the window allows.
 *
 * The hero band shows the whole picture but it is still a strip across the top of the page;
 * this is the look-at-it-properly view. Arrow keys walk the set, Escape and a click anywhere
 * off the picture leave.
 */
function Lightbox({
  images,
  index,
  onIndex,
  onClose,
  title,
}: {
  images: string[];
  index: number;
  onIndex: (i: number) => void;
  onClose: () => void;
  title: string;
}) {
  const t = useT();
  const step = useCallback(
    (d: number) => onIndex((index + d + images.length) % images.length),
    [index, images.length, onIndex],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowRight") step(1);
      else if (e.key === "ArrowLeft") step(-1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, step]);

  // Portalled: the detail page sits inside a clipped column, and a viewer that covers the
  // window has to be a child of the window.
  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/92 px-16 py-12"
      onClick={onClose}
    >
      <CachedImg
        src={images[index]}
        alt={title}
        className="max-h-full max-w-full object-contain"
        onClick={(e) => e.stopPropagation()}
      />

      <button
        onClick={onClose}
        aria-label={t("common.close")}
        title={t("common.close")}
        className="absolute right-5 top-5 grid size-9 cursor-default place-items-center border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
      >
        <X className="size-4" />
      </button>

      {images.length > 1 && (
        <>
          <button
            onClick={(e) => {
              e.stopPropagation();
              step(-1);
            }}
            aria-label={t("common.back")}
            className="absolute left-4 top-1/2 grid size-10 -translate-y-1/2 cursor-default place-items-center border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
          >
            <ChevronLeft className="size-5" />
          </button>
          <button
            onClick={(e) => {
              e.stopPropagation();
              step(1);
            }}
            aria-label={t("common.next")}
            className="absolute right-4 top-1/2 grid size-10 -translate-y-1/2 cursor-default place-items-center border border-white/20 bg-black/50 text-white/75 transition-colors hover:text-white"
          >
            <ChevronRight className="size-5" />
          </button>
          <span className="absolute inset-x-0 bottom-5 text-center font-mono text-[12px] tabular-figures text-white/55">
            {index + 1} / {images.length}
          </span>
        </>
      )}
    </div>,
    document.body,
  );
}

function Breadcrumb({
  modType,
  title,
  onBack,
  link,
}: {
  modType: ModType;
  title: string;
  onBack: () => void;
  link: string | null;
}) {
  const t = useT();
  const { game } = useConfig();
  return (
    <div className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
      <button
        onClick={onBack}
        className="flex cursor-default items-center gap-1 font-semibold text-primary hover:brightness-110"
      >
        <ArrowLeft className="size-3.5" /> {t("nav.browse")}
      </button>
      <span className="text-faint">/</span>
      <span>{t(modType.label)}</span>
      <span className="text-faint">/</span>
      <span className="truncate text-foreground/85">{title}</span>
      {link && (
        <button
          onClick={() => open(link)}
          className="ml-auto flex cursor-default items-center gap-1 text-[12px] text-primary hover:brightness-110"
        >
          {t("modDetail.viewOnSite", { site: game.catalogDomain })}{" "}
          <ExternalLink className="size-3" />
        </button>
      )}
    </div>
  );
}

function Row({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="flex items-center justify-between gap-3 text-[12px]">
      <span className="text-muted-foreground">{label}</span>
      <span
        className={cn(
          "truncate text-foreground/85",
          mono && "font-mono text-[11px]",
        )}
      >
        {value}
      </span>
    </div>
  );
}

function InstallProgress({
  stage,
  idx,
  pct,
  received,
  total,
}: {
  stage: InstallStage;
  idx: number;
  pct?: number;
  received?: number;
  total?: number;
}) {
  const t = useT();
  const mb = (n?: number) => (n ? Math.round(n / 1e6) : 0);
  const label =
    stage === "done"
      ? t("modDetail.addedToLibrary")
      : stage === "downloading"
        ? t("update.downloading")
        : stage === "extracting"
          ? t("modDetail.extracting")
          : stage === "review"
            ? t("modDetail.chooseWhatToInstall")
            : stage === "placing"
              ? t("modDetail.addingToLibrary")
              : t("modDetail.resolving");
  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-baseline justify-between">
        <span className="text-[12px] font-semibold text-foreground/85">{label}</span>
        {stage === "downloading" && total ? (
          <span className="text-[11px] text-muted-foreground">
            {mb(received)} of {mb(total)} MB{pct !== undefined ? ` · ${pct}%` : ""}
          </span>
        ) : null}
      </div>
      <div className="h-1 overflow-hidden rounded-full bg-foreground/[0.08]">
        <div
          className={cn(
            "h-full rounded-full bg-primary transition-[width]",
            pct === undefined &&
              stage !== "done" &&
              "w-1/3 animate-[frost-indeterminate_1.2s_ease-in-out_infinite]",
          )}
          style={
            stage === "done"
              ? { width: "100%" }
              : pct !== undefined
                ? { width: `${pct}%` }
                : undefined
          }
        />
      </div>
      <div className="flex flex-wrap items-center gap-1.5 text-[10.5px] text-faint">
        {CHAIN.map((s, i) => (
          <span key={s.key} className="flex items-center gap-1.5">
            <span
              className={cn(
                i < idx && "text-success",
                i === idx && "font-semibold text-primary",
              )}
            >
              {i < idx && "✓ "}
              {t(s.label)}
            </span>
            {i < CHAIN.length - 1 && <span>→</span>}
          </span>
        ))}
      </div>
    </div>
  );
}

function BlockedHost({
  host,
  step1,
  onOpen,
  onChoose,
}: {
  host: string;
  step1: boolean;
  onOpen: () => void;
  onChoose: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex flex-col gap-1">
        <span className="text-[14px] font-bold">
          {t("modDetail.finishInBrowser")}
        </span>
        <span className="text-[12px] leading-relaxed text-muted-foreground">
          {/* Proton Drive isn't a browser-only *policy* — the file is encrypted with a
              key that never leaves the URL fragment, so say what's actually true. */}
          {/proton/i.test(host)
            ? `${t("modDetail.protonHint")} ${t("modDetail.thenAddFile")}`
            : `${host} only allows browser downloads. Download it, then point MXB App at the file to finish the install.`}
        </span>
      </div>
      <div className="flex items-start gap-3">
        <div className="flex flex-none flex-col items-center gap-1 pt-0.5">
          <Step n={1} done={step1} active={!step1} />
          <span className="h-8 w-px bg-foreground/15" />
          <Step n={2} done={false} active={step1} />
        </div>
        <div className="flex flex-1 flex-col gap-3.5">
          <div className="flex flex-col gap-2">
            <span className="text-[12.5px] text-foreground/85">
              {t("modDetail.downloadFromHost", { host })}
            </span>
            <Button size="sm" className="w-full" onClick={onOpen}>
              {t("modDetail.openHost", { host })} <ExternalLink className="size-3.5" />
            </Button>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-[12.5px] text-muted-foreground">
              {t("modDetail.thenAddFile")}
            </span>
            <button
              onClick={onChoose}
              className="flex cursor-default flex-col items-center gap-1 rounded-lg border border-dashed border-foreground/20 px-3 py-3 transition-colors hover:border-primary/50"
            >
              <FileDown className="size-4 text-muted-foreground" />
              <span className="text-[12px] font-semibold text-primary">
                {t("modDetail.chooseDownloaded")}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function Step({ n, done, active }: { n: number; done: boolean; active: boolean }) {
  return (
    <span
      className={cn(
        "grid size-[22px] place-items-center rounded-full text-[11px] font-bold",
        done || active
          ? "bg-primary text-primary-foreground"
          : "border border-foreground/20 text-muted-foreground",
      )}
    >
      {done ? <Check className="size-3" strokeWidth={3} /> : n}
    </span>
  );
}
