/**
 * The download queue: what's transferring now, what's waiting behind it, and an X on each.
 *
 * Installs have always been queued one at a time (`Context/Install`), but the only sign of it
 * was a "+N queued" line — you couldn't see what was in there, let alone take something out.
 * The sidebar card is now the trigger for a panel that shows the whole queue.
 */
import { Download, X } from "lucide-react";
import {
  useInstall,
  type ActiveInstall,
  type QueuedInstall,
} from "../../Context/Install";
import { useT } from "../../i18n/context";
import type { TKey } from "../../i18n";
import type { InstallStage } from "../../types";
import { displayName, formatBytes } from "../../lib/mods";
import { Popover, PopoverContent, PopoverTrigger } from "../ui/popover";
import { cn } from "@/lib/utils";

/** Stages that mean an install is still moving — anything else is done, failed, or idle. */
const IN_PROGRESS = new Set<InstallStage>([
  "resolving",
  "downloading",
  "extracting",
  "placing",
]);

/** Only the transfer can be stopped. Once the bytes are down, extraction and placement are
 *  mid-flight file operations with nothing safe to interrupt — so the X goes away rather than
 *  becoming a button that quietly does nothing. */
const CANCELLABLE = new Set<InstallStage>(["resolving", "downloading"]);

/** An import has no transfer to stop — it's a local file being unpacked, and the backend
 *  registers no cancel flag for it. Offering the X would be offering a button that lies. */
function cancellable(job: ActiveInstall): boolean {
  return (
    job.source.kind !== "import" && CANCELLABLE.has(job.stage) && !job.cancelling
  );
}

const STAGE_LABEL: Record<string, TKey> = {
  resolving: "downloads.stageResolving",
  downloading: "downloads.stageDownloading",
  extracting: "downloads.stageExtracting",
  placing: "downloads.stagePlacing",
};

export default function DownloadQueue({ collapsed }: { collapsed: boolean }) {
  const t = useT();
  const { active, queued, cancel } = useInstall();

  const installing = active.filter((a) => IN_PROGRESS.has(a.stage));
  // The collapsed card summarises: several transfers share one bar, so it follows the one
  // furthest along — the next thing to finish is the useful thing to watch.
  const lead =
    installing.length > 0
      ? installing.reduce((a, b) => (progressOf(b) > progressOf(a) ? b : a))
      : null;
  const pct = lead ? pctOf(lead) : undefined;

  // Nothing moving and nothing waiting: no card, exactly as before.
  if (installing.length === 0 && queued.length === 0) return null;

  const total = installing.length + queued.length;

  return (
    <Popover>
      <PopoverTrigger asChild>
        {collapsed ? (
          <button
            title={t("downloads.title")}
            aria-label={t("downloads.title")}
            className="relative flex cursor-default items-center justify-center rounded-lg py-2.5 text-muted-foreground transition-colors hover:bg-foreground/[0.05] hover:text-foreground"
          >
            <Download className="size-4" />
            <span className="absolute right-1.5 top-1 min-w-[14px] rounded-full bg-primary px-1 text-[9px] font-bold leading-[14px] text-primary-foreground">
              {total}
            </span>
          </button>
        ) : (
          <button
            title={t("downloads.open")}
            className="flex cursor-default flex-col gap-[7px] rounded-[10px] border border-white/[0.07] bg-[color-mix(in_srgb,var(--card)_60%,var(--window))] px-3 py-2.5 text-left transition-colors hover:border-white/[0.12]"
          >
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-[11.5px] font-semibold text-foreground/85">
                {installing.length > 1
                  ? t("sidebar.installingCount", { count: installing.length })
                  : lead
                    ? t("sidebar.installing", { name: displayName(lead.title) })
                    : t("downloads.title")}
              </span>
              {pct !== undefined && (
                <span className="flex-none text-[10.5px] text-muted-foreground">{pct}%</span>
              )}
            </div>
            {queued.length > 0 && (
              <span className="text-[10.5px] text-muted-foreground">
                {t("sidebar.queued", { count: queued.length })}
              </span>
            )}
            <div className="h-[3px] overflow-hidden rounded-full bg-foreground/[0.08]">
              <div
                className={cn(
                  "h-full rounded-full bg-primary transition-[width]",
                  pct === undefined &&
                    "w-1/3 animate-[frost-indeterminate_1.2s_ease-in-out_infinite]",
                )}
                style={pct !== undefined ? { width: `${pct}%` } : undefined}
              />
            </div>
          </button>
        )}
      </PopoverTrigger>

      <PopoverContent side="right" align="end" className="w-[300px] p-0">
        <div className="border-b border-white/[0.07] px-3.5 py-2.5">
          <span className="text-[12px] font-bold">{t("downloads.title")}</span>
        </div>
        <div className="flex max-h-[320px] flex-col overflow-y-auto py-1.5">
          {installing.map((it) => (
            <RunningRow key={it.key} item={it} onCancel={() => cancel(it.key)} />
          ))}

          {queued.map((q) => (
            <QueuedRow key={q.key} item={q} onCancel={() => cancel(q.key)} />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}

/** 0–1, for picking which transfer the collapsed card follows. Unknown counts as nothing. */
function progressOf(it: ActiveInstall): number {
  return it.total && it.received ? it.received / it.total : 0;
}

function pctOf(it: ActiveInstall): number | undefined {
  return it.total && it.received ? Math.round((it.received / it.total) * 100) : undefined;
}

function RunningRow({ item, onCancel }: { item: ActiveInstall; onCancel: () => void }) {
  const t = useT();
  const pct = pctOf(item);
  return (
    <div className="flex flex-col gap-1.5 px-3.5 py-2">
      <div className="flex items-start gap-2">
        <span className="min-w-0 flex-1 truncate text-[12px] font-semibold">
          {displayName(item.title)}
        </span>
        {cancellable(item) && (
          <CancelButton label={t("downloads.cancel")} onClick={onCancel} />
        )}
      </div>
      <div className="flex items-baseline justify-between gap-2 text-[10.5px] text-muted-foreground">
        <span className="truncate">
          {item.cancelling
            ? t("downloads.cancelling")
            : t(STAGE_LABEL[item.stage] ?? "downloads.stageDownloading")}
        </span>
        {item.received !== undefined && (
          <span className="flex-none tabular-nums">
            {item.total
              ? `${formatBytes(item.received)} / ${formatBytes(item.total)}`
              : formatBytes(item.received)}
          </span>
        )}
      </div>
      <div className="h-[3px] overflow-hidden rounded-full bg-foreground/[0.08]">
        <div
          className={cn(
            "h-full rounded-full bg-primary transition-[width]",
            pct === undefined &&
              "w-1/3 animate-[frost-indeterminate_1.2s_ease-in-out_infinite]",
          )}
          style={pct !== undefined ? { width: `${pct}%` } : undefined}
        />
      </div>
    </div>
  );
}

function QueuedRow({ item, onCancel }: { item: QueuedInstall; onCancel: () => void }) {
  const t = useT();
  return (
    <div className="group flex items-center gap-2 px-3.5 py-2">
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-[12px] font-medium">{displayName(item.title)}</span>
        <span className="text-[10.5px] text-muted-foreground">
          {t(item.preparing ? "downloads.preparing" : "downloads.waiting")}
        </span>
      </div>
      <CancelButton label={t("downloads.remove")} onClick={onCancel} />
    </div>
  );
}

function CancelButton({ label, onClick }: { label: string; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className="flex-none cursor-default rounded-full p-1 text-muted-foreground transition-colors hover:bg-destructive/15 hover:text-destructive"
    >
      <X className="size-3.5" />
    </button>
  );
}
