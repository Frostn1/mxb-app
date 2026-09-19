import { useCallback, useEffect, useState } from "react";
import { scanLibrary } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { useConfig } from "@frost/shared/Context/Config";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";

/**
 * The first-run checklist: what a brand new install is missing, and the one button each.
 *
 * The game arrives with no bikes and no tracks of its own, so the first thing a new player
 * meets is a manager with an empty library and no idea that the bikes are a download of
 * their own. Every step here already exists somewhere in the app — this is a route into
 * them, not a second way of doing them.
 *
 * It shows itself only to an install whose `mods/bikes` is empty. Anyone who already rides
 * is marked done on the spot and never sees it, which is also what keeps it from appearing
 * to an existing player the first time they update.
 */

/**
 * The stock bikes, on mxb-mods. Every other step opens a category; this one opens a post,
 * because there is exactly one pack a new rider wants and sending them to a page of fifty
 * bike mods to find it is the problem this screen exists to solve. GP Bikes has no
 * equivalent post, so it gets the category like the other steps.
 */
const OEM_PACK = { slug: "oem-bike-pack", categoryId: 45 };

/** One row. `subpath` is what decides whether it's already satisfied. */
interface Step {
  id: "bikes" | "tracks" | "rider";
  subpath: string;
  title: TKey;
  body: TKey;
  action: TKey;
}

const STEPS: Step[] = [
  {
    id: "bikes",
    subpath: "mods/bikes",
    title: "getStarted.bikes.title",
    body: "getStarted.bikes.body",
    action: "getStarted.bikes.action",
  },
  {
    id: "tracks",
    subpath: "mods/tracks",
    title: "getStarted.tracks.title",
    body: "getStarted.tracks.body",
    action: "getStarted.tracks.action",
  },
  {
    id: "rider",
    subpath: "mods/rider",
    title: "getStarted.rider.title",
    body: "getStarted.rider.body",
    action: "getStarted.rider.action",
  },
];

interface GetStartedProps {
  /** Skipped, or nothing left to do — persisted by the caller, so it doesn't come back. */
  onDone: () => void;
  /** Taken up on a step. Closes the checklist for this session without settling it: if the
   *  install didn't happen, the next launch is entitled to ask once more. */
  onClose: () => void;
  /** Open Browse on one of the mod types. */
  onBrowse: (id: Step["id"]) => void;
  /** Open one mod's page in Browse. */
  onOpenMod: (slug: string, categoryId: number) => void;
  /** Bumped by every install, so the ticks land without the player reopening anything. */
  refreshKey: number;
}

export default function GetStarted({
  onDone,
  onClose,
  onBrowse,
  onOpenMod,
  refreshKey,
}: GetStartedProps) {
  const t = useT();
  const { game } = useConfig();
  // `null` until the first scan comes back: an empty checklist drawn over a full mods
  // folder, even for one frame, reads as "the app lost my mods".
  const [counts, setCounts] = useState<Record<Step["id"], number> | null>(null);
  // Did this install already have bikes before the checklist opened? Answered by the first
  // scan and never revisited — installing the pack from here has to tick the row, not close
  // the window out from under the two steps below it.
  const [preexisting, setPreexisting] = useState<boolean | null>(null);

  const rescan = useCallback(async () => {
    const found = await Promise.all(
      STEPS.map((s) => scanLibrary(s.subpath).catch(() => [])),
    );
    const next = Object.fromEntries(
      STEPS.map((s, i) => [s.id, found[i].length]),
    ) as Record<Step["id"], number>;
    setCounts(next);
    setPreexisting((seen) => seen ?? next.bikes > 0);
  }, []);

  useEffect(() => {
    void rescan();
  }, [rescan, refreshKey]);

  // Someone who already had bikes is not a new player. Marked done rather than merely
  // hidden, so the scan doesn't run again on every launch.
  useEffect(() => {
    if (preexisting) onDone();
  }, [preexisting, onDone]);

  if (counts === null || preexisting !== false) return null;

  /** Where a step's button goes. The bikes step is the only one that names a mod. */
  const go = (step: Step) => {
    if (step.id === "bikes" && game.id === "mxb") {
      onOpenMod(OEM_PACK.slug, OEM_PACK.categoryId);
    } else {
      onBrowse(step.id);
    }
    onClose();
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/80 backdrop-blur-sm px-10">
      <div className="flex w-full max-w-[520px] flex-col gap-7 rounded-2xl border border-input bg-card p-9 shadow-2xl">
        <div className="flex flex-col gap-2">
          <h1 className="text-[22px] font-extrabold tracking-[-0.4px]">
            {t("getStarted.title")}
          </h1>
          <p className="text-[13.5px] leading-relaxed text-muted-foreground">
            {t("getStarted.body")}
          </p>
        </div>

        <div className="flex flex-col">
          {STEPS.map((step, i) => {
            const done = counts[step.id] > 0;
            return (
              <div
                key={step.id}
                className={cn(
                  "flex items-start gap-4 border-t border-input py-4",
                  i === STEPS.length - 1 && "border-b",
                  done && "text-muted-foreground",
                )}
              >
                <span className="mt-0.5 w-4 shrink-0 font-cond text-[15px] font-bold tabular-nums text-faint">
                  {i + 1}
                </span>
                <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                  <span className="text-[13.5px] font-semibold">{t(step.title)}</span>
                  <span className="text-[12.5px] leading-relaxed text-muted-foreground">
                    {done
                      ? t("getStarted.installed", { count: counts[step.id] })
                      : t(step.body)}
                  </span>
                </div>
                {!done && (
                  <Button
                    size="sm"
                    variant={step.id === "bikes" ? "default" : "outline"}
                    className="mt-0.5 shrink-0"
                    onClick={() => go(step)}
                  >
                    {t(step.action)}
                  </Button>
                )}
              </div>
            );
          })}
        </div>

        <div className="flex items-center justify-end">
          <Button variant="ghost" onClick={onDone}>
            {t("getStarted.later")}
          </Button>
        </div>
      </div>
    </div>
  );
}
