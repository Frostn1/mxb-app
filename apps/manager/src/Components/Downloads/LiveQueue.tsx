/**
 * The queue as it is happening: what is transferring, how fast, and how long it has left.
 *
 * One set of rows, rendered in two places — the top rail's popover and the Downloads screen,
 * above the history. They have to agree: a download reading "18.4 MB/s · 4s left" in the
 * popover and showing only a bar on the Downloads screen is two answers to one question.
 *
 * The speed and the time left are worked out in `Context/Install` from the arrival times of
 * the progress events, because nothing in the backend's event carries either. That also means
 * they can go stale — a line that stops delivering stops sending events, and the last figure
 * would otherwise sit there looking live. `useLiveTick` re-renders once a second so a rate
 * that has gone quiet can be dropped instead.
 */
import { useEffect, useState } from "react";
import { ArrowUpToLine, X } from "lucide-react";
import { useInstall, type ActiveInstall, type QueuedInstall } from "../../Context/Install";
import { useT, type TFunc, type TKey } from "@/i18n";
import type { InstallStage } from "@frost/shared/types";
import { displayName, formatBytes } from "@frost/shared/lib/mods";
import { cn } from "@frost/shared/lib/utils";
import { useViewActive } from "../Shell/RetainedView";

/** Stages that mean an install is still moving — anything else is done, failed, or idle.
 *  `review` counts: the bytes are down but nothing is installed, and dropping the card would
 *  leave a staged pack with nothing on screen pointing at it. */
export const IN_PROGRESS = new Set<InstallStage>([
  "resolving",
  "downloading",
  "extracting",
  "placing",
  "review",
]);

/** Only the transfer can be stopped. Once the bytes are down, extraction and placement are
 *  mid-flight file operations with nothing safe to interrupt — so the X goes away rather than
 *  becoming a button that quietly does nothing. */
const CANCELLABLE = new Set<InstallStage>(["resolving", "downloading"]);

/** An import has no transfer to stop — it's a local file being unpacked, and the backend
 *  registers no cancel flag for it. Offering the X would be offering a button that lies. */
function cancellable(job: ActiveInstall): boolean {
  return job.source.kind !== "import" && CANCELLABLE.has(job.stage) && !job.cancelling;
}

const STAGE_LABEL: Record<string, TKey> = {
  resolving: "downloads.stageResolving",
  downloading: "downloads.stageDownloading",
  extracting: "downloads.stageExtracting",
  placing: "downloads.stagePlacing",
  review: "downloads.stageReview",
};

/** How long a rate stays on screen after the last progress event. Chunks land every 512 KiB,
 *  so even a slow line reports often; nothing for this long is a stall, and the honest thing
 *  to show then is no number at all rather than the speed it *used* to be doing. */
const RATE_STALE_MS = 2500;

export function pctOf(it: ActiveInstall): number | undefined {
  return it.total && it.received ? Math.round((it.received / it.total) * 100) : undefined;
}

/** 0–1, for picking which transfer a summary follows. Unknown counts as nothing. */
export function progressOf(it: ActiveInstall): number {
  return it.total && it.received ? it.received / it.total : 0;
}

/** "18.4 MB/s". One decimal below 100 in a unit, which is where the digit still says
 *  something; above that it's noise on a number that moves every tick anyway. */
function formatRate(bytesPerSecond: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytesPerSecond;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const rounded =
    unit === 0 || value >= 100 ? Math.round(value) : Math.round(value * 10) / 10;
  return `${rounded} ${units[unit]}/s`;
}

/** "9s", "2m 10s", "1h 04m". */
function formatEta(seconds: number): string {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${String(m % 60).padStart(2, "0")}m`;
}

/**
 * Re-render once a second while something is transferring.
 *
 * Progress events already re-render these rows far more often than this — until they stop.
 * That is the case this exists for: without a tick, a stalled transfer keeps its last rate
 * on screen forever, because nothing tells React to look again.
 */
function useLiveTick(enabled: boolean): void {
  const [, bump] = useState(0);
  useEffect(() => {
    if (!enabled) return;
    const id = window.setInterval(() => bump((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [enabled]);
}

/** "18.4 MB/s · 4s left", or just the rate when there is no total to count down to.
 *  `null` once the figures are too old to stand behind. */
function rateLine(item: ActiveInstall, t: TFunc<TKey>): string | null {
  if (item.speed === undefined || item.sampledAt === undefined) return null;
  if (performance.now() - item.sampledAt > RATE_STALE_MS) return null;
  const rate = formatRate(item.speed);
  return item.eta === undefined
    ? rate
    : `${rate} · ${t("downloads.timeLeft", { time: formatEta(item.eta) })}`;
}

/** How much of the file is here: "12 MB / 48 MB", or just what has arrived when the server
 *  sent no Content-Length. The shop's WebView downloads never send one. */
function sizeLine(item: ActiveInstall): string | null {
  if (item.received === undefined) return null;
  return item.total
    ? `${formatBytes(item.received)} / ${formatBytes(item.total)}`
    : formatBytes(item.received);
}

/** A transfer in flight, with its stage, its figures and its bar.
 *
 * `boxed` is the Downloads screen's version — the same row inside the card chrome the
 * history rows use, so the live section and the list below it read as one column. */
export function RunningRow({
  item,
  onCancel,
  boxed,
}: {
  item: ActiveInstall;
  onCancel: () => void;
  boxed?: boolean;
}) {
  const t = useT();
  const viewActive = useViewActive();
  useLiveTick(viewActive && item.stage === "downloading");
  const pct = pctOf(item);
  const rate = rateLine(item, t);
  const size = sizeLine(item);

  return (
    <div
      className={cn(
        "flex flex-col gap-1.5",
        boxed ? "rounded-xl border border-primary/25 bg-card p-3" : "px-3.5 py-2",
      )}
    >
      <div className="flex items-start gap-2">
        <span
          className={cn(
            "min-w-0 flex-1 truncate font-semibold",
            boxed ? "text-[13px]" : "text-[12px]",
          )}
        >
          {displayName(item.title)}
        </span>
        {pct !== undefined && (
          <span className="flex-none font-cond text-[11px] tabular-figures text-muted-foreground">
            {pct}%
          </span>
        )}
        {cancellable(item) && <IconButton label={t("downloads.cancel")} onClick={onCancel} />}
      </div>

      <div className="flex items-baseline justify-between gap-2 text-[10.5px] text-muted-foreground">
        <span className="truncate">
          {item.cancelling
            ? t("downloads.cancelling")
            : t(STAGE_LABEL[item.stage] ?? "downloads.stageDownloading")}
          {size && <span className="font-cond tabular-figures"> · {size}</span>}
        </span>
        {rate && (
          <span className="flex-none font-cond tabular-figures text-foreground/70">{rate}</span>
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

/** One install waiting its turn. `onPromote` is absent for whatever is already at the head:
 *  a button that would move a row to where it already is says nothing. */
export function QueuedRow({
  item,
  position,
  onCancel,
  onPromote,
  boxed,
}: {
  item: QueuedInstall;
  /** Its place in the waiting line, 1-based — the answer to "when does mine start". */
  position: number;
  onCancel: () => void;
  onPromote?: () => void;
  boxed?: boolean;
}) {
  const t = useT();
  return (
    <div
      className={cn(
        "flex items-center gap-2",
        boxed ? "rounded-xl border border-white/[0.07] bg-card p-3" : "px-3.5 py-2",
      )}
    >
      <span className="w-5 flex-none font-cond text-[11px] tabular-figures text-faint">
        {position}
      </span>
      <div className="flex min-w-0 flex-1 flex-col">
        <span
          className={cn("truncate font-medium", boxed ? "text-[13px]" : "text-[12px]")}
        >
          {displayName(item.title)}
        </span>
        <span className="text-[10.5px] text-muted-foreground">
          {t(item.preparing ? "downloads.preparing" : "downloads.waiting")}
        </span>
      </div>
      {onPromote && (
        <IconButton
          label={t("downloads.moveToFront")}
          onClick={onPromote}
          icon="promote"
        />
      )}
      <IconButton label={t("downloads.remove")} onClick={onCancel} />
    </div>
  );
}

function IconButton({
  label,
  onClick,
  icon = "cancel",
}: {
  label: string;
  onClick: () => void;
  icon?: "cancel" | "promote";
}) {
  const Icon = icon === "promote" ? ArrowUpToLine : X;
  return (
    <button
      onClick={onClick}
      title={label}
      aria-label={label}
      className={cn(
        "flex-none cursor-default rounded-full p-1 text-muted-foreground transition-colors",
        icon === "promote"
          ? "hover:bg-primary/15 hover:text-primary"
          : "hover:bg-destructive/15 hover:text-destructive",
      )}
    >
      <Icon className="size-3.5" />
    </button>
  );
}

/** A section heading in the Downloads screen's own voice — the same one the day groups use. */
function SectionLabel({ label, count }: { label: string; count: number }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="font-cond text-[12px] font-bold uppercase tracking-[1.2px] text-faint">
        ▸ {label}
      </span>
      <span className="text-[11px] tabular-figures text-faint">{count}</span>
    </div>
  );
}

/** How many installs are running or waiting. The Downloads screen needs the number as well
 *  as the rows: "nothing downloaded yet" under a live transfer is the screen contradicting
 *  itself on someone's first mod. */
export function useLiveQueueCount(): number {
  const { active, queued } = useInstall();
  return active.filter((a) => IN_PROGRESS.has(a.stage)).length + queued.length;
}

/**
 * The live half of the Downloads screen: what is transferring and what is behind it, above
 * the history. Renders nothing at all when the queue is empty, so a quiet app looks exactly
 * as it did.
 */
export default function LiveQueue() {
  const t = useT();
  const { active, queued, cancel, promote } = useInstall();
  const installing = active.filter((a) => IN_PROGRESS.has(a.stage));

  if (installing.length === 0 && queued.length === 0) return null;

  return (
    <div className="flex flex-col gap-6 pb-6">
      {installing.length > 0 && (
        <section className="flex flex-col gap-2.5">
          <SectionLabel label={t("downloads.sectionActive")} count={installing.length} />
          <div className="flex flex-col gap-2">
            {installing.map((it) => (
              <RunningRow key={it.key} item={it} onCancel={() => cancel(it.key)} boxed />
            ))}
          </div>
        </section>
      )}

      {queued.length > 0 && (
        <section className="flex flex-col gap-2.5">
          <SectionLabel label={t("downloads.sectionQueued")} count={queued.length} />
          <div className="flex flex-col gap-2">
            {queued.map((q, i) => (
              <QueuedRow
                key={q.key}
                item={q}
                position={i + 1}
                onCancel={() => cancel(q.key)}
                onPromote={i > 0 ? () => promote(q.key) : undefined}
                boxed
              />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}
