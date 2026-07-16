import { useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowLeft,
  ExternalLink,
  Check,
  Copy,
  Snowflake,
  FileDown,
} from "lucide-react";
import { Swiper, SwiperSlide } from "swiper/react";
import { Navigation, Pagination } from "swiper/modules";
import type { Swiper as SwiperClass } from "swiper";
import "swiper/css";
import "swiper/css/navigation";
import "swiper/css/pagination";
import { open } from "@tauri-apps/plugin-shell";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
import {
  buildDestinations,
  buildRiderDestinations,
  destStorageKey,
  getInstalledMods,
  getModDetail,
  isBlockedDownload,
  isLiveryContext,
  isSoundContext,
  normalizeModName,
  resolveInitialFolder,
  scanRiderTargets,
  sortMirrors,
  type DestOption,
  type ModType,
} from "../../api/mods";
import type {
  DownloadOption,
  InstalledMod,
  InstallStage,
  ModDetail as Detail,
} from "../../types";
import InstallDialog, { type InstallChoice } from "./InstallDialog";
import { useInstall } from "../../Context/Install";
import { fileFormat, formatDate } from "../../lib/mods";
import { Badge } from "@/Components/ui/badge";
import { Button } from "@/Components/ui/button";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "@/Components/ui/alert-dialog";
import { cn } from "@/lib/utils";

interface ModDetailProps {
  slug: string;
  modType: ModType;
  /** Browse category the mod was opened under — drives bike-livery routing. */
  categoryId: number;
  installedNames: Set<string>;
  onBack: () => void;
}

const CHAIN: { key: string; label: string }[] = [
  { key: "resolving", label: "Resolve" },
  { key: "downloading", label: "Download" },
  { key: "extracting", label: "Extract" },
  { key: "placing", label: "Place" },
  { key: "reload", label: "Reload" },
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
  installedNames,
  onBack,
}: ModDetailProps) {
  const livery = isLiveryContext(modType, categoryId);
  const sound = isSoundContext(modType, categoryId);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [installed, setInstalled] = useState<InstalledMod[]>([]);
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
  const [activeImg, setActiveImg] = useState(0);
  const swiperRef = useRef<SwiperClass | null>(null);

  const { active, startInstall, startImport } = useInstall();
  const myActive = active && active.slug === slug ? active : null;

  useEffect(() => {
    let cancelled = false;
    setDetail(null);
    setLoadError(null);
    setBlocked(null);
    setDestOptions([]);
    setGuess("");
    setSuggestions([]);
    setActiveImg(0);
    getModDetail(slug)
      .then(async (d) => {
        if (cancelled) return;
        setDetail(d);
        try {
          const inst = await getInstalledMods(modType.installSubpath);
          if (cancelled) return;
          setInstalled(inst);
          // Rider paints route into a model's/profile's folder; everything else
          // uses the generic (track/bike) destination logic.
          const dest =
            modType.id === "rider"
              ? buildRiderDestinations(await scanRiderTargets(), d.title)
              : buildDestinations(modType, d.title, inst, livery, sound);
          if (cancelled) return;
          setDestOptions(dest.options);
          setGuess(dest.guess);
          setSuggestions(dest.suggestions);
        } catch {
          setInstalled([]);
          setDestOptions([]);
        }
      })
      .catch((e) => !cancelled && setLoadError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [slug, modType, livery, sound]);

  const folderCounts = useMemo(() => {
    const m = new Map<string, number>();
    for (const it of installed) m.set(it.folder, (m.get(it.folder) ?? 0) + 1);
    return m;
  }, [installed]);

  // "Official" mirror + metadata for the collapsed install panel.
  const mirrors = useMemo(() => (detail ? sortMirrors(detail) : []), [detail]);

  const primary = mirrors[0] ?? null;
  const format = primary ? fileFormat(primary.url) : null;
  const mirrorNames = [...new Set(mirrors.map((m) => m.host))].join(" · ");

  const destKey = destStorageKey(modType);
  const initialFolder = useMemo(
    () => resolveInitialFolder(modType, destOptions, guess, livery, sound),
    [modType, destOptions, guess, livery, sound],
  );

  const isInstalled =
    detail !== null && installedNames.has(normalizeModName(detail.title));

  // Already have it? Confirm before overwriting; otherwise open the dialog.
  const openInstall = () => {
    if (isInstalled) setConfirmReinstall(true);
    else setDialogOpen(true);
  };

  const handleConfirm = ({ destFolder, mirror }: InstallChoice) => {
    localStorage.setItem(destKey, destFolder);
    setDialogOpen(false);
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
        url: mirror.url,
        host: mirror.host,
      });
    }
  };

  const chooseAndImport = async () => {
    const picked = await pickFile({
      multiple: false,
      filters: [{ name: "Mod files", extensions: ["pkz", "zip", "rar", "7z"] }],
    });
    if (typeof picked !== "string" || !detail) return;
    setBlocked(null);
    startImport({
      slug,
      title: detail.title,
      subpath: modType.installSubpath,
      destFolder: localStorage.getItem(destKey) ?? "",
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
        <div className="mt-6 rounded-xl border border-destructive/30 bg-destructive/[0.06] p-4 text-[13px] text-destructive">
          Couldn&apos;t load this mod: {loadError}
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

  return (
    <div className="flex h-full flex-col overflow-hidden px-7 py-5">
      <Breadcrumb
        modType={modType}
        title={detail.title}
        onBack={onBack}
        link={detail.link}
      />

      <div className="mt-4 flex min-h-0 flex-1 gap-6">
        {/* left: gallery + description */}
        <div className="flex min-w-0 flex-1 flex-col gap-3.5 overflow-y-auto pr-1">
          {detail.images.length > 0 ? (
            <>
              <Swiper
                className="frost-gallery aspect-video w-full flex-none"
                modules={[Navigation, Pagination]}
                navigation
                pagination={{ clickable: true }}
                onSwiper={(s) => (swiperRef.current = s)}
                onSlideChange={(s) => setActiveImg(s.activeIndex)}
              >
                {detail.images.map((src) => (
                  <SwiperSlide key={src}>
                    <img
                      src={src}
                      alt={detail.title}
                      className="size-full object-cover"
                    />
                  </SwiperSlide>
                ))}
              </Swiper>
              {detail.images.length > 1 && (
                <div className="flex flex-none gap-2 overflow-x-auto pb-1">
                  {detail.images.slice(0, 8).map((src, i) => (
                    <button
                      key={src}
                      onClick={() => swiperRef.current?.slideTo(i)}
                      className={cn(
                        "aspect-video w-24 flex-none overflow-hidden rounded-md border transition-opacity",
                        i === activeImg
                          ? "border-primary"
                          : "border-transparent opacity-60 hover:opacity-100",
                      )}
                    >
                      <img src={src} alt="" className="size-full object-cover" />
                    </button>
                  ))}
                </div>
              )}
            </>
          ) : (
            <div className="grid aspect-video w-full flex-none place-items-center rounded-xl border border-border bg-gradient-to-br from-[#3a3f45] to-[#20242a] text-foreground/20">
              No screenshots
            </div>
          )}

          <div className="flex flex-col gap-2 pt-1">
            <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
              About this {modType.id === "bikes" ? "bike" : modType.id === "rider" ? "rider gear" : "track"}
            </span>
            <div
              className="mod-description"
              // Authored HTML from mxb-mods.com's REST API.
              dangerouslySetInnerHTML={{ __html: detail.descriptionHtml }}
            />
          </div>
        </div>

        {/* right rail */}
        <div className="flex w-[340px] flex-none flex-col gap-3 overflow-y-auto">
          <div className="flex flex-col gap-1.5">
            <h1 className="text-[24px] font-bold leading-tight tracking-[-0.3px]">
              {detail.title}
            </h1>
            <div className="flex flex-wrap items-center gap-2 text-[12px] text-muted-foreground">
              <span>{formatDate(detail.date)}</span>
              {detail.version && (
                <>
                  <span className="text-faint">·</span>
                  <span className="rounded-[5px] bg-foreground/[0.07] px-1.5 py-px font-mono text-[11px]">
                    {detail.version}
                  </span>
                </>
              )}
              {isInstalled && (
                <Badge variant="success" className="ml-0.5">
                  <Check className="size-3" strokeWidth={3} /> In library
                </Badge>
              )}
            </div>
          </div>

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
                    Try again
                  </Button>
                  <Button size="sm" variant="outline" onClick={copyError}>
                    <Copy className="size-3.5" /> {copied ? "Copied" : "Copy"}
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
                <Button className="h-11 w-full text-[14px]" onClick={openInstall}>
                  {isInstalled ? "Reinstall" : "Add to Library"}
                </Button>
                <Row label="Host" value={primary.host} />
                <Row
                  label="Installs to"
                  value={`${modType.installSubpath.replace(/\//g, "\\")}\\`}
                  mono
                />
              </>
            ) : (
              <p className="text-[12.5px] text-muted-foreground">
                No download link was found on this page — open it on mxb-mods.com.
              </p>
            )}
          </div>

          {/* frostmod hint */}
          <div className="flex items-center gap-2.5 rounded-[10px] border border-success/25 bg-success/[0.06] px-3 py-2.5">
            <span className="size-[7px] flex-none rounded-full bg-success" />
            <span className="text-[12px] text-success/90">
              FrostMod will hot-reload the {modType.id === "rider" ? "rider" : modType.id === "bikes" ? "bike" : "track"} list
              when this finishes.
            </span>
          </div>

          {/* details */}
          <div className="flex flex-col gap-2.5 rounded-xl border border-white/[0.07] bg-card px-4 py-3.5">
            <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
              Details
            </span>
            {format && <Row label="Format" value={format} mono />}
            {mirrorNames && <Row label="Mirrors" value={mirrorNames} />}
            <Row label="Type" value={modType.label} />
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
            <AlertDialogTitle>Reinstall “{detail.title}”?</AlertDialogTitle>
            <AlertDialogDescription>
              This mod is already in your library. Reinstalling downloads it again
              and overwrites the installed files.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                setConfirmReinstall(false);
                setDialogOpen(true);
              }}
            >
              Reinstall
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
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
  return (
    <div className="flex items-center gap-2 text-[12.5px] text-muted-foreground">
      <button
        onClick={onBack}
        className="flex cursor-default items-center gap-1 font-semibold text-primary hover:brightness-110"
      >
        <ArrowLeft className="size-3.5" /> Browse
      </button>
      <span className="text-faint">/</span>
      <span>{modType.label}</span>
      <span className="text-faint">/</span>
      <span className="truncate text-foreground/85">{title}</span>
      {link && (
        <button
          onClick={() => open(link)}
          className="ml-auto flex cursor-default items-center gap-1 text-[12px] text-primary hover:brightness-110"
        >
          View on mxb-mods.com <ExternalLink className="size-3" />
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
  const mb = (n?: number) => (n ? Math.round(n / 1e6) : 0);
  const label =
    stage === "done"
      ? "Added to your library"
      : stage === "downloading"
        ? "Downloading…"
        : stage === "extracting"
          ? "Extracting…"
          : stage === "placing"
            ? "Adding to library…"
            : "Resolving download…";
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
              {s.label}
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
  return (
    <div className="flex flex-col gap-3.5">
      <div className="flex flex-col gap-1">
        <span className="text-[14px] font-bold">Finish in your browser</span>
        <span className="text-[12px] leading-relaxed text-muted-foreground">
          {host} only allows browser downloads. Download it, then point MXB App at
          the file to finish the install.
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
              Download from {host}
            </span>
            <Button size="sm" className="w-full" onClick={onOpen}>
              Open {host} <ExternalLink className="size-3.5" />
            </Button>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-[12.5px] text-muted-foreground">
              Then add the file
            </span>
            <button
              onClick={onChoose}
              className="flex cursor-default flex-col items-center gap-1 rounded-lg border border-dashed border-foreground/20 px-3 py-3 transition-colors hover:border-primary/50"
            >
              <FileDown className="size-4 text-muted-foreground" />
              <span className="text-[12px] font-semibold text-primary">
                Choose the downloaded file
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
