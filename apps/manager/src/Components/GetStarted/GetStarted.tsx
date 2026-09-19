import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";
import { scanLibrary } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { useConfig } from "@frost/shared/Context/Config";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { useInstall } from "../../Context/Install";

/**
 * The first-run bar: what a new install still has none of, and the one button each.
 *
 * A bar rather than a modal, and it does not close when a step is taken up: the steps send
 * you into Browse, and a panel that disappeared on the first click would leave a new player
 * on a search page with no way back to the other two.
 *
 * It shows itself only to an install whose `mods/bikes` is empty. Anyone who already rides
 * is marked done on the spot and never sees it, which is also what keeps it from appearing
 * to an existing player the first time they update.
 */

/**
 * The stock bikes, on mxb-mods. Every other step opens a category; this one opens a post,
 * because there is exactly one pack a new rider wants and sending them to a page of fifty
 * bike mods to find it is the problem this bar exists to solve. GP Bikes has no equivalent
 * post, so it gets the category like the other steps.
 */
const OEM_PACK = { slug: "oem-bike-pack", categoryId: 45 };

/** One step. `subpath` is what decides whether it's already satisfied. */
interface Step {
  id: "bikes" | "tracks" | "rider";
  subpath: string;
  action: TKey;
}

const STEPS: Step[] = [
  { id: "bikes", subpath: "mods/bikes", action: "getStarted.bikes.action" },
  { id: "tracks", subpath: "mods/tracks", action: "getStarted.tracks.action" },
  { id: "rider", subpath: "mods/rider", action: "getStarted.rider.action" },
];

interface GetStartedProps {
  /** Dismissed, or nothing left to do — persisted by the caller, so it doesn't come back. */
  onDone: () => void;
  /** Open Browse on one of the mod types. */
  onBrowse: (id: Step["id"]) => void;
  /** Open one mod's page in Browse. */
  onOpenMod: (slug: string, categoryId: number) => void;
  /** Bumped by every install, so the ticks land without the player reopening anything. */
  refreshKey: number;
}

export default function GetStarted({
  onDone,
  onBrowse,
  onOpenMod,
  refreshKey,
}: GetStartedProps) {
  const t = useT();
  const { game } = useConfig();
  // What is already on its way. A step whose content is mid-download must not still be
  // offering the button that starts it — and the app can be left running on this bar while
  // a four-gigabyte pack comes down, so "nothing has happened yet" has to be visibly wrong.
  const { active } = useInstall();
  // `null` until the first scan comes back: a bar saying the folder is empty, drawn over a
  // full one even for a frame, reads as "the app lost my mods".
  const [counts, setCounts] = useState<Record<Step["id"], number> | null>(null);
  // Did this install already have bikes when the bar opened? Answered by the first scan and
  // never revisited — installing the pack from here has to tick that step, not take the bar
  // away while the other two are still undone.
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

  /** The install landing in this step's folder, if one is running. */
  const running = (step: Step) =>
    active.find(
      (a) =>
        a.subpath === step.subpath && a.stage !== "done" && a.stage !== "error",
    );

  /** Where a step's button goes. The bikes step is the only one that names a mod. */
  const go = (step: Step) => {
    if (step.id === "bikes" && game.id === "mxb") {
      onOpenMod(OEM_PACK.slug, OEM_PACK.categoryId);
    } else {
      onBrowse(step.id);
    }
  };

  return (
    <div className="flex items-center gap-3 border-b border-primary/25 bg-primary/10 px-4 py-2 text-sm text-foreground">
      <span className="shrink-0 font-semibold">{t("getStarted.title")}</span>
      <span className="min-w-0 truncate text-muted-foreground">
        {t("getStarted.body")}
      </span>

      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        {STEPS.map((step) => {
          const done = counts[step.id] > 0;
          const busy = done ? undefined : running(step);
          return (
            <Button
              key={step.id}
              size="sm"
              variant={step.id === "bikes" && !done && !busy ? "default" : "outline"}
              className={cn((done || busy) && "text-muted-foreground")}
              disabled={done || !!busy}
              onClick={() => go(step)}
            >
              {done
                ? t("getStarted.installed", { count: counts[step.id] })
                : busy
                  ? t("getStarted.installing")
                  : t(step.action)}
            </Button>
          );
        })}
        <Button
          size="icon"
          variant="ghost"
          className="size-8"
          onClick={onDone}
          aria-label={t("getStarted.later")}
        >
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}
