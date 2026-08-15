/**
 * Everything the app has downloaded, newest first.
 *
 * The library answers "what do I have" by scanning the mods folder, which can't say *when*
 * anything arrived — and says nothing at all about a download that failed, since a failure
 * leaves no files behind and its toast is gone the moment it's dismissed. So this reads the
 * recorded history instead (see `Context/Downloads`), which is the only place both facts live.
 *
 * A failed row keeps its error and retries in place through the same install queue as any
 * other install; nothing here downloads anything itself.
 */
import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Download,
  ExternalLink,
  FileArchive,
  Library as LibraryIcon,
  MoreHorizontal,
  RotateCw,
  Search,
  Store,
  Trash2,
  X,
} from "lucide-react";
import type { DownloadRecord, DownloadStatus } from "../../types";
import { useDownloads } from "../../Context/Downloads";
import { useInstall, type ModTarget } from "../../Context/Install";
import { useT, type TFunc } from "../../i18n/context";
import { dayStart, displayName, formatBytes, formatDay, formatTime } from "../../lib/mods";
import { Segmented } from "@/Components/ui/segmented";
import { Button } from "@/Components/ui/button";
import HelpHint from "@/Components/ui/help-hint";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@/Components/ui/dropdown-menu";
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

type Filter = "all" | DownloadStatus;

interface DownloadsProps {
  /** Open a mod's page on the site — where a site download came from. */
  onOpenMod: (target: ModTarget) => void;
  /** Show an installed mod in the library, on the right tab and searched for by name. */
  onShowInLibrary: (record: DownloadRecord) => void;
  /** Send a failed shop purchase back to the Shop, which is the only place it can be
   *  re-downloaded from — a purchase link is signed in, so there's nothing to retry here. */
  onOpenShop: () => void;
}

/** Records for one calendar day, newest day first. */
interface DayGroup {
  key: number;
  label: string;
  items: DownloadRecord[];
}

function groupByDay(records: DownloadRecord[]): DayGroup[] {
  const groups: DayGroup[] = [];
  for (const r of records) {
    const key = dayStart(r.at);
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.items.push(r);
    else groups.push({ key, label: formatDay(r.at), items: [r] });
  }
  return groups;
}

function sourceLabel(record: DownloadRecord, t: TFunc): string {
  if (record.source === "site") return record.host || t("downloads.sourceSite");
  return record.source === "shop" ? t("downloads.sourceShop") : t("downloads.sourceFile");
}

/** Where it landed, as `tracks/MX2` — the answer to "so where did that one go". */
function destLabel(record: DownloadRecord): string {
  return [record.subpath, record.destFolder]
    .filter((p) => p && p !== ".")
    .join("/");
}

export default function Downloads({
  onOpenMod,
  onShowInLibrary,
  onOpenShop,
}: DownloadsProps) {
  const t = useT();
  const { records, loading, forget, clear, markSeen } = useDownloads();
  const { startInstall } = useInstall();
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [clearOpen, setClearOpen] = useState(false);

  // Being here is what "seen" means — it retires the sidebar's failure badge.
  useEffect(markSeen, [markSeen, records.length]);

  const counts = useMemo(
    () => ({
      all: records.length,
      installed: records.filter((r) => r.status === "installed").length,
      failed: records.filter((r) => r.status === "failed").length,
    }),
    [records],
  );

  const shown = useMemo(() => {
    const q = search.trim().toLowerCase();
    return records.filter((r) => {
      if (filter !== "all" && r.status !== filter) return false;
      if (!q) return true;
      return (
        r.title.toLowerCase().includes(q) ||
        destLabel(r).toLowerCase().includes(q) ||
        (r.host ?? "").toLowerCase().includes(q)
      );
    });
  }, [records, search, filter]);

  const groups = useMemo(() => groupByDay(shown), [shown]);

  /** Straight back through the install queue, with the same destination as last time. */
  const retry = (r: DownloadRecord) => {
    if (r.source === "shop") return onOpenShop();
    if (!r.url) return;
    startInstall({
      slug: r.slug,
      title: r.title,
      subpath: r.subpath,
      destFolder: r.destFolder,
      categoryId: r.categoryId ?? undefined,
      url: r.url,
      host: r.host ?? "",
    });
  };

  const canRetry = (r: DownloadRecord) =>
    r.status === "failed" && (r.source === "shop" || !!r.url);

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-none items-center gap-3.5 px-7 pb-3.5 pt-5">
        <div className="flex items-center gap-1.5">
          <h1 className="text-[21px] font-bold tracking-[-0.2px]">
            {t("nav.downloads")}
          </h1>
          <HelpHint title={t("nav.downloads")} description={t("downloads.help")} />
        </div>
        <Segmented
          value={filter}
          onChange={(v) => setFilter(v as Filter)}
          options={(["all", "installed", "failed"] as const).map((v) => ({
            value: v,
            label: (
              <span className="flex items-center gap-1.5">
                {t(
                  v === "all"
                    ? "downloads.filterAll"
                    : v === "installed"
                      ? "common.installed"
                      : "downloads.filterFailed",
                )}
                <span
                  className={cn(
                    "text-muted-foreground",
                    v === "failed" && counts.failed > 0 && "text-destructive",
                  )}
                >
                  {counts[v]}
                </span>
              </span>
            ),
          }))}
        />
        <div className="ml-auto flex w-[240px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-2">
          <Search className="size-3.5 text-faint" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={t("downloads.searchPlaceholder")}
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => setClearOpen(true)}
          disabled={records.length === 0}
        >
          <Trash2 className="size-3.5" /> {t("downloads.clearAction")}
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {loading ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {t("common.loading")}
          </p>
        ) : groups.length === 0 ? (
          <p className="py-16 text-center text-[13px] text-muted-foreground">
            {records.length === 0 ? t("downloads.empty") : t("downloads.noMatches")}
          </p>
        ) : (
          <div className="flex flex-col gap-6">
            {groups.map((group) => (
              <section key={group.key} className="flex flex-col gap-2.5">
                <div className="flex items-baseline gap-2">
                  <span className="text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
                    ▸ {group.label}
                  </span>
                  <span className="text-[11px] text-faint">{group.items.length}</span>
                </div>
                <div className="flex flex-col gap-2">
                  {group.items.map((r) => (
                    <DownloadRow
                      key={r.id}
                      record={r}
                      onRetry={canRetry(r) ? () => retry(r) : undefined}
                      onOpenMod={
                        r.slug && r.source === "site"
                          ? () =>
                              onOpenMod({
                                slug: r.slug,
                                subpath: r.subpath,
                                categoryId: r.categoryId ?? undefined,
                              })
                          : undefined
                      }
                      onShowInLibrary={
                        r.status === "installed" ? () => onShowInLibrary(r) : undefined
                      }
                      onForget={() => forget(r.id)}
                    />
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </div>

      <AlertDialog open={clearOpen} onOpenChange={setClearOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("downloads.clearTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("downloads.clearBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                clear();
                setClearOpen(false);
              }}
            >
              {t("common.clear")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

function DownloadRow({
  record,
  onRetry,
  onOpenMod,
  onShowInLibrary,
  onForget,
}: {
  record: DownloadRecord;
  onRetry?: () => void;
  onOpenMod?: () => void;
  onShowInLibrary?: () => void;
  onForget: () => void;
}) {
  const t = useT();
  const failed = record.status === "failed";
  const SourceIcon =
    record.source === "shop" ? Store : record.source === "file" ? FileArchive : Download;
  const dest = destLabel(record);

  return (
    <div
      className={cn(
        "flex items-start gap-3 rounded-xl border bg-card p-3",
        failed ? "border-destructive/30" : "border-white/[0.07]",
      )}
    >
      {failed ? (
        <AlertTriangle className="mt-0.5 size-4 flex-none text-destructive" />
      ) : (
        <CheckCircle2 className="mt-0.5 size-4 flex-none text-primary" />
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <div className="flex items-baseline gap-2">
          <span className="truncate text-[13px] font-semibold">
            {displayName(record.title)}
          </span>
          <span className="flex-none text-[11px] text-faint">{formatTime(record.at)}</span>
        </div>
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[11.5px] text-muted-foreground">
          <span className="flex items-center gap-1">
            <SourceIcon className="size-3" />
            {sourceLabel(record, t)}
          </span>
          {dest && <span className="text-faint">·</span>}
          {dest && <span className="truncate">{dest}</span>}
          {!!record.bytes && <span className="text-faint">·</span>}
          {!!record.bytes && <span>{formatBytes(record.bytes)}</span>}
        </div>
        {failed && record.error && (
          <p className="select-text text-[11.5px] text-destructive/90">{record.error}</p>
        )}
      </div>
      {onRetry && (
        <Button variant="outline" size="sm" className="flex-none" onClick={onRetry}>
          <RotateCw className="size-3.5" /> {t("common.retry")}
        </Button>
      )}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            aria-label={t("downloads.rowActions")}
            className="mt-0.5 flex-none cursor-default rounded-md px-1 text-faint transition-colors hover:text-foreground"
          >
            <MoreHorizontal className="size-4" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {onShowInLibrary && (
            <DropdownMenuItem onSelect={onShowInLibrary}>
              <LibraryIcon className="size-4" /> {t("downloads.showInLibrary")}
            </DropdownMenuItem>
          )}
          {onOpenMod && (
            <DropdownMenuItem onSelect={onOpenMod}>
              <ExternalLink className="size-4" /> {t("downloads.openModPage")}
            </DropdownMenuItem>
          )}
          {(onShowInLibrary || onOpenMod) && <DropdownMenuSeparator />}
          <DropdownMenuItem variant="destructive" onSelect={onForget}>
            <X className="size-4" /> {t("downloads.forget")}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
