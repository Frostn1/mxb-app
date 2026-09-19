/**
 * The download queue in the top rail: a chip that says how many, and a popover that shows
 * them — what is transferring, how fast, how long it has left, and what is waiting behind.
 *
 * Installs have always been queued (`Context/Install` runs two at a time), but the rail only
 * ever carried a count. The panel is where the queue becomes something you can act on: stop
 * one, or push a waiting one to the front.
 *
 * The rows themselves live in `Components/Downloads/LiveQueue` because the Downloads screen
 * shows the same queue, and the two must not drift.
 */
import { Download } from "lucide-react";
import { useInstall } from "../../Context/Install";
import { useT } from "@/i18n";
import { displayName } from "@frost/shared/lib/mods";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import {
  IN_PROGRESS,
  QueuedRow,
  RunningRow,
  progressOf,
} from "../Downloads/LiveQueue";

export default function DownloadQueue() {
  const t = useT();
  const { active, queued, cancel, promote } = useInstall();

  const installing = active.filter((a) => IN_PROGRESS.has(a.stage));

  // Nothing moving and nothing waiting: no chip, exactly as before.
  if (installing.length === 0 && queued.length === 0) return null;

  // The header names one transfer, so it follows the one furthest along — the next thing to
  // finish is the useful thing to watch.
  const lead =
    installing.length > 0
      ? installing.reduce((a, b) => (progressOf(b) > progressOf(a) ? b : a))
      : null;
  const total = installing.length + queued.length;

  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          title={t("downloads.open")}
          aria-label={t("downloads.open")}
          className="relative grid size-[30px] cursor-default place-items-center transition-colors hover:text-foreground"
        >
          <Download className="size-4" />
          <span className="absolute right-0 top-0 min-w-[14px] rounded-full bg-primary px-1 font-cond text-[9px] font-bold leading-[14px] tabular-figures text-primary-foreground">
            {total}
          </span>
        </button>
      </PopoverTrigger>

      <PopoverContent side="bottom" align="end" className="w-[320px] p-0">
        <div className="flex items-baseline justify-between gap-2 border-b border-white/[0.07] px-3.5 py-2.5">
          <span className="truncate font-cond text-[12px] font-bold">
            {installing.length > 1
              ? t("sidebar.installingCount", { count: installing.length })
              : lead
                ? t("sidebar.installing", { name: displayName(lead.title) })
                : t("downloads.title")}
          </span>
          {queued.length > 0 && (
            <span className="flex-none font-cond text-[10.5px] tabular-figures text-muted-foreground">
              {t("sidebar.queued", { count: queued.length })}
            </span>
          )}
        </div>

        <div className="flex max-h-[360px] flex-col overflow-y-auto py-1.5">
          {installing.map((it) => (
            <RunningRow key={it.key} item={it} onCancel={() => cancel(it.key)} />
          ))}

          {installing.length > 0 && queued.length > 0 && (
            <div className="my-1 h-px bg-white/[0.07]" />
          )}

          {queued.map((q, i) => (
            <QueuedRow
              key={q.key}
              item={q}
              position={i + 1}
              onCancel={() => cancel(q.key)}
              onPromote={i > 0 ? () => promote(q.key) : undefined}
            />
          ))}
        </div>
      </PopoverContent>
    </Popover>
  );
}
