import { useCallback, useEffect, useState } from "react";
import { X } from "lucide-react";
import { scanLibrary } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { useConfig } from "@frost/shared/Context/Config";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { useInstall } from "../../Context/Install";

/**
 * The first run, for an install that has nothing.
 *
 * Two shapes, one component. It opens as a panel — the whole window, because a new player
 * has nowhere else to be — and every step sends them into Browse, so taking one up collapses
 * it to a bar across the top rather than closing it. A panel that vanished on the first
 * click would leave someone on a search page with the other two steps gone, which is what
 * the first build of this did.
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

/** One step. `subpath` is what decides whether it's already satisfied. */
interface Step {
  id: "bikes" | "tracks" | "rider";
  subpath: string;
  title: TKey;
  action: TKey;
}

const STEPS: Step[] = [
  {
    id: "bikes",
    subpath: "mods/bikes",
    title: "getStarted.bikes.title",
    action: "getStarted.bikes.action",
  },
  {
    id: "tracks",
    subpath: "mods/tracks",
    title: "getStarted.tracks.title",
    action: "getStarted.tracks.action",
  },
  {
    id: "rider",
    subpath: "mods/rider",
    title: "getStarted.rider.title",
    action: "getStarted.rider.action",
  },
];

interface GetStartedProps {
  /** Dismissed, or nothing left to do — persisted by the caller, so it doesn't come back. */
  onDone: () => void;
  /** Open Browse on one of the mod types. */
  onBrowse: (id: Step["id"]) => void;
  /** Open one mod's page in Browse. */
  onOpenMod: (slug: string, categoryId: number) => void;
  /** Bumped by every install, so the counts land without the player reopening anything. */
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
  // offering the button that starts it — the app can sit here while a four-gigabyte pack
  // comes down, and "nothing has happened yet" has to stop being what that looks like.
  const { active } = useInstall();
  // `null` until the first scan comes back: a panel saying the folder is empty, drawn over
  // a full one even for a frame, reads as "the app lost my mods".
  const [counts, setCounts] = useState<Record<Step["id"], number> | null>(null);
  // Did this install already have bikes when it opened? Answered by the first scan and never
  // revisited — installing the pack from here has to tick that step, not take the panel away
  // while the other two are still undone.
  const [preexisting, setPreexisting] = useState<boolean | null>(null);
  // Collapsed to the bar, because a step was taken up and Browse is behind this.
  const [collapsed, setCollapsed] = useState(false);

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
    setCollapsed(true);
  };

  /** A step's button, the same in both shapes. */
  const stepButton = (step: Step, primary: boolean) => {
    const done = counts[step.id] > 0;
    const busy = done ? undefined : running(step);
    return (
      <Button
        size="sm"
        variant={primary && !done && !busy ? "default" : "outline"}
        className={cn("shrink-0", (done || busy) && "text-muted-foreground")}
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
  };

  // Collapsed: the same steps as a bar, so they stay reachable while Browse is being used.
  if (collapsed) {
    return (
      <div className="flex items-center gap-3 border-b border-primary/25 bg-primary/10 px-4 py-2 text-sm text-foreground">
        <span className="shrink-0 font-semibold">{t("getStarted.title")}</span>
        <span className="min-w-0 truncate text-muted-foreground">
          {t("getStarted.body")}
        </span>
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          {STEPS.map((step) => (
            <span key={step.id}>{stepButton(step, step.id === "bikes")}</span>
          ))}
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

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-background/80 backdrop-blur-sm px-10">
      <div className="flex w-full max-w-[520px] flex-col gap-7 rounded-2xl border border-input bg-card p-9 shadow-2xl">
        <div className="flex flex-col gap-2">
          <h1 className="text-[22px] font-extrabold tracking-[-0.4px]">
            {t("getStarted.title")}
          </h1>
          <p className="text-[13.5px] text-muted-foreground">{t("getStarted.body")}</p>
        </div>

        <div className="flex flex-col">
          {STEPS.map((step, i) => (
            <div
              key={step.id}
              className={cn(
                "flex items-center gap-4 border-t border-input py-3.5",
                i === STEPS.length - 1 && "border-b",
              )}
            >
              <span className="w-4 shrink-0 font-cond text-[15px] font-bold tabular-nums text-faint">
                {i + 1}
              </span>
              <span className="min-w-0 flex-1 text-[13.5px] font-semibold">
                {t(step.title)}
              </span>
              {stepButton(step, step.id === "bikes")}
            </div>
          ))}
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
