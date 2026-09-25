import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Check,
  Copy,
  FileDown,
  ExternalLink,
  Link2,
  Loader2,
  Trash2,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-shell";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import { useT, type TKey, APP_NAME } from "@/i18n";
import {
  buildDestinations,
  buildRiderDestinations,
  defaultMirrorIndex,
  entrySubpath,
  destStorageKey,
  getInstalledMods,
  getModDetail,
  isBlockedDownload,
  isLiveryContext,
  isServerOnly,
  isSoundContext,
  modLink,
  resetModsVerification,
  riderTarget,
  routesByContent,
  scanSubpaths,
  resolveInitialFolder,
  uninstallMod,
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
import { LoadingMark } from "../Shell/LoadingMark";
import RichDescription from "./RichDescription";
import InstallDialog, { type InstallChoice } from "./InstallDialog";
import { useInstall } from "../../Context/Install";
import type { InstalledIndex } from "../../lib/installedMatch";
import { displayName, formatDate } from "@frost/shared/lib/mods";
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
import { copyText } from "../../lib/clipboard";
import { toast } from "sonner";
import { ActionBar, StateChip, WishButton } from "../ModPage/ActionBar";
import { useWishlist, wishId } from "../../lib/useWishlist";
import MediaPanel, { type Figure } from "../ModPage/Media";
import { Note, Panel } from "../ModPage/Panels";

interface ModDetailProps {
  slug: string;
  modType: ModType;
  /** Browse category the mod was opened under — drives bike-livery routing. */
  categoryId: number;
  installed: InstalledIndex;
  /** Bump the library scan — an uninstall here changes what the badges say. */
  onChanged: () => void;
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

/**
 * One mod from the catalog, on the one mod page.
 *
 * The page is the same shape here, in the library and in the stores: a 60px action bar that
 * never scrolls, the picture on the left with its figures on the foot, and a column of cards
 * on the right. Only the bar and one card change with the state — here that card is the
 * download: where it comes from, where it lands, and how far along it is.
 */
export default function ModDetail({
  slug,
  modType,
  categoryId,
  installed,
  onChanged,
  onBack,
}: ModDetailProps) {
  const t = useT();
  const { game } = useConfig();
  const wishlist = useWishlist();
  const wishKey = wishId("browse", slug);
  const livery = isLiveryContext(modType, categoryId);
  const sound = isSoundContext(modType, categoryId);
  // Which rider folder this category installs into — a gear model's paints, something worn
  // on the rider model, or a model of its own. `null` when the category doesn't say.
  const rider = useMemo(
    () => riderTarget(game, modType, categoryId),
    [game, modType, categoryId],
  );
  const [derivedDest, setDerivedDest] = useState(false);
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
  const [confirmUninstall, setConfirmUninstall] = useState(false);
  const [removing, setRemoving] = useState(false);
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
          const inst = (await Promise.all(scanSubpaths(modType).map((s) => getInstalledMods(s)))).flat();
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

  // "Official" mirror + metadata for the download card.
  const mirrors = useMemo(() => (detail ? sortMirrors(detail) : []), [detail]);

  // What the dialog would start on — the best playable file, so the card below the bar
  // describes the download that's actually about to run.
  const primary = mirrors[defaultMirrorIndex(mirrors)] ?? null;
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

  // The file on disk this page's mod matched, when it matched one — what Uninstall removes.
  // Same match as the "in library" badge, so the two can never disagree about which mod
  // is already installed.
  const installedEntry = detail ? installed.match(detail.title) : null;
  const isInstalled = installedEntry !== null;

  /** Remove the matched file without a trip to the Library. It goes to the Recycle Bin,
   *  same as the Library's own Uninstall, so a wrong guess costs nothing. */
  const doUninstall = async () => {
    if (!installedEntry) return;
    setConfirmUninstall(false);
    setRemoving(true);
    try {
      await uninstallMod(installedEntry.path, entrySubpath(installedEntry.path, modType));
      toast.success(
        t("library.uninstalledOne", { name: displayName(installedEntry.name) }),
        { description: t("library.movedToBin") },
      );
      onChanged();
    } catch (e) {
      toast.error(t("library.uninstallFailed"), { description: String(e) });
    } finally {
      setRemoving(false);
    }
  };

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

  /** The `mxb://` link that opens this page in someone else's copy of the app. */
  const copyLink = () => {
    void copyText(modLink({ game: game.id, modType: modType.id, slug, category: categoryId }))
      .then((ok) =>
        ok
          ? toast.success(t("modDetail.linkCopied"))
          : toast.error(t("modDetail.copyLinkFailed")),
      );
  };

  const copyFailedDownload = () => {
    if (myActive?.source.kind !== "download") return;
    void copyText(myActive.source.url).then((ok) => {
      if (!ok) {
        toast.error(t("modDetail.copyLinkFailed"));
        return;
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const crumb = (title: string) => (
    <ContextBarLeft>
      <span className="flex items-center gap-2 font-cond text-[12.5px] font-semibold tracking-[-0.02em]">
        <button
          onClick={onBack}
          className="cursor-default text-muted-foreground transition-colors hover:text-foreground"
        >
          {t(modType.label)}
        </button>
        <span className="text-faint">/</span>
        <span className="max-w-[420px] truncate text-foreground">{title}</span>
      </span>
    </ContextBarLeft>
  );

  if (loadError) {
    return (
      <div className="flex h-full flex-col px-7 py-5">
        {crumb("—")}
        <div className="mt-6 flex flex-col items-start gap-3 rounded-xl border border-destructive/30 bg-destructive/[0.06] p-4">
          <p className="text-[13px] font-semibold text-destructive">
            {t("modDetail.loadFailed")}
          </p>
          {/* Selectable: a block explains itself in a sentence, and carries the Cloudflare
              ray that identifies it — which is the whole of what a bug report needs. */}
          <p className="select-text text-[12.5px] leading-relaxed text-muted-foreground">
            {loadError.replace(/^Error:\s*/, "")}
          </p>
          <div className="flex flex-wrap gap-2">
            {/* Retry also re-arms the site's check, if this session stopped showing it. */}
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                void resetModsVerification()
                  .catch(() => {})
                  .finally(() => setReloadKey((n) => n + 1))
              }
            >
              {t("common.retry")}
            </Button>
            {/* The way out that always works: the same page in the user's own browser. */}
            <Button
              variant="outline"
              size="sm"
              onClick={() => void open(`https://${game.catalogDomain}/${slug}/`)}
            >
              {t("modDetail.openOnSite", { site: game.catalogDomain })}
            </Button>
          </div>
        </div>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="flex h-full flex-col px-7 py-5">
        {crumb("…")}
        <div className="grid flex-1 place-items-center text-muted-foreground">
          <LoadingMark label={t("common.loading")} />
        </div>
      </div>
    );
  }

  const pct =
    myActive?.total && myActive.received
      ? Math.round((myActive.received / myActive.total) * 100)
      : undefined;
  const idx = myActive ? stageIndex(myActive.stage) : -1;
  const busy = idx >= 0 && myActive?.stage !== "done";

  // What a rider decides on, laid over the picture. The file's plumbing — its format, its
  // mirrors — is not a reason to install anything, so it does not get a figure.
  const figures: Figure[] = [
    { label: t("shopCatalog.updated"), value: formatDate(detail.date) },
    ...(detail.version ? [{ label: "Version", value: detail.version }] : []),
    ...(detail.categories.length
      ? [{ label: t("modDetail.categoryLabel"), value: detail.categories.slice(0, 2).join(", ") }]
      : []),
  ];

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Browse's type tabs go with Browse, so the bar above would otherwise be an empty
          44px band. It carries where you are instead. */}
      {crumb(detail.title)}

      <ActionBar
        image={detail.images[0] ?? null}
        title={detail.title}
        meta={[t(modType.label), detail.author, detail.version]}
      >
        {/* Nothing to want about a mod you already have, so the wish is offered only while
            it isn't in the library. */}
        {!isInstalled && (
          <WishButton
            wished={wishlist.has(wishKey)}
            onToggle={() =>
              wishlist.toggle({
                id: wishKey,
                source: "browse",
                slug,
                title: detail.title,
                author: detail.author ?? undefined,
                image: detail.images[0],
              })
            }
          />
        )}
        {/* Sharing a mod used to mean pasting the catalog URL, which opens a browser and
            leaves the reader to find the mod again in here. This link opens the app on this
            page instead, for anyone who has it. */}
        <Button variant="outline" onClick={copyLink} title={t("modDetail.copyLinkHint")}>
          <Link2 className="size-3.5" />
          {t("modDetail.copyLink")}
        </Button>
        {primary && (
          <Button onClick={openInstall} disabled={busy}>
            {busy && <Loader2 className="size-4 animate-spin" />}
            {isInstalled ? t("browse.reinstall") : t("modDetail.addToLibrary")}
          </Button>
        )}
      </ActionBar>

      <div className="flex min-h-0 flex-1 gap-6 px-7 pb-5 pt-4">
        {/* left: the picture, then what the author wrote */}
        <div className="flex min-w-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
          <MediaPanel
            images={detail.images}
            title={detail.title}
            figures={figures}
            /* Where this mod stands with you belongs on the thing itself, not beside the
               button — you read the picture first. */
            badge={
              <StateChip icon={isInstalled ? Check : undefined} tone={isInstalled ? "success" : "muted"} overlay>
                {isInstalled ? t("modDetail.inLibrary") : t("modDetail.notInstalled")}
              </StateChip>
            }
            emptyLabel={t("shopCatalog.noScreenshots")}
          />

          <div className="flex flex-col gap-2">
            <span className="font-cond text-[10.5px] font-bold uppercase tracking-[0.14em] text-faint">
              About this{" "}
              {modType.id === "bikes"
                ? t("modDetail.kindBike")
                : modType.id === "rider"
                  ? t("modDetail.kindRider")
                  : t("modDetail.kindTrack")}
            </span>
            {/* Authored HTML from mxb-mods.com's REST API. */}
            <RichDescription html={detail.descriptionHtml} />
          </div>
        </div>

        {/* right: the state card, then what holds for every state */}
        <div className="flex w-[320px] flex-none flex-col gap-3 overflow-y-auto pb-1">
          <Panel label={t("modDetail.stageDownload")}>
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
                  {myActive.source.kind === "download" && (
                    <Button size="sm" variant="outline" onClick={copyFailedDownload}>
                      <Copy className="size-3.5" />{" "}
                      {copied ? t("modDetail.copied") : t("modDetail.copyLink")}
                    </Button>
                  )}
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
                  <Note icon={AlertTriangle} tone="warning">
                    {t("modDetail.serverOnlyNotice")}
                  </Note>
                )}
                <p className="text-[11.5px] leading-relaxed text-faint">
                  {t("modDetail.fromHost", { host: primary.host })}
                  {mirrorNames.includes(",") ? ` (${mirrorNames})` : ""}
                  {" · "}
                  {routesByContent(modType) ? (
                    <span>{t("modType.autoDest")}</span>
                  ) : (
                    <span className="font-mono">
                      {`${modType.installSubpath.replace(/\//g, "\\")}\\`}
                    </span>
                  )}
                </p>
                {/* Only once it's actually on disk. Riders asked for it here because a track
                    they just downloaded and didn't like meant a trip to the Library. */}
                {isInstalled && (
                  <Button
                    variant="outline"
                    className="h-9 w-full text-[13px] text-destructive hover:text-destructive"
                    disabled={removing}
                    onClick={() => setConfirmUninstall(true)}
                  >
                    <Trash2 className="size-3.5" /> {t("library.uninstall")}
                  </Button>
                )}
              </>
            ) : (
              <p className="text-[12.5px] text-muted-foreground">
                {t("modDetail.noDownloadLink", { site: game.catalogDomain })}
              </p>
            )}
          </Panel>

          {/* There is no "What's inside" here on purpose: mxb-mods states a mod's mirrors,
              not its contents, and the download options are copies of one file rather than
              parts of it. Inventing a parts list out of them would be worse than the gap. */}

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

      <AlertDialog open={confirmUninstall} onOpenChange={setConfirmUninstall}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("library.confirmUninstall", {
                name: installedEntry ? displayName(installedEntry.name) : "",
              })}
            </AlertDialogTitle>
            <AlertDialogDescription>
              {t("library.confirmUninstallBody")}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction variant="destructive" onClick={() => void doUninstall()}>
              {t("library.uninstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
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
        {/* The only place a size belongs: bytes we are actually moving. Neither catalog
            states a file's size before the transfer starts, so nothing above claims one. */}
        {stage === "downloading" && total ? (
          <span className="text-[11px] tabular-figures text-muted-foreground">
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
      <div className="flex flex-wrap items-center gap-1.5 font-cond text-[10.5px] text-faint">
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
        <span className="font-cond text-[14px] font-bold tracking-[-0.03em]">
          {t("modDetail.finishInBrowser")}
        </span>
        <span className="text-[12px] leading-relaxed text-muted-foreground">
          {/* Proton Drive isn't a browser-only *policy* — the file is encrypted with a
              key that never leaves the URL fragment, so say what's actually true. */}
          {/proton/i.test(host)
            ? `${t("modDetail.protonHint")} ${t("modDetail.thenAddFile")}`
            : `${host} only allows browser downloads. Download it, then point ${APP_NAME} at the file to finish the install.`}
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
        "grid size-[22px] place-items-center rounded-full font-cond text-[11px] font-bold",
        done || active
          ? "bg-primary text-primary-foreground"
          : "border border-foreground/20 text-muted-foreground",
      )}
    >
      {done ? <Check className="size-3" strokeWidth={3} /> : n}
    </span>
  );
}
