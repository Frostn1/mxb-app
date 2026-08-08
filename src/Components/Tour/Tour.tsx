import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Snowflake,
  Compass,
  Library as LibraryIcon,
  Bike,
  Shirt,
  User,
  RefreshCw,
  Settings as SettingsIcon,
  Check,
  ArrowLeft,
  ArrowRight,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@/Components/ui/button";
import { cn } from "@/lib/utils";
import { useT, type TKey } from "../../i18n/context";
import { useConfig } from "../../Context/Config";
import type { GameCaps } from "../../types";
import type { DashboardView } from "../Shell/Sidebar";

/** Bumped when the tour changes enough to warrant showing it again. */
export const TOUR_DONE_KEY = "mxb:tourDone:v1";

interface TourContextValue {
  /** Restart the interactive tour from the beginning (e.g. the Settings button). */
  startTour: () => void;
}

export const TourContext = createContext<TourContextValue>({ startTour: () => {} });
export const useTour = () => useContext(TourContext);

interface Step {
  /** View to switch to before highlighting, so the real screen sits behind the spotlight. */
  view?: DashboardView;
  /** CSS selector of the element to spotlight. Omit for a centered, un-anchored step. */
  selector?: string;
  icon: LucideIcon;
  title: TKey;
  body: TKey;
  /** Capability the active game must have for this step to apply. A step that
   *  spotlights a nav item the game doesn't show would highlight nothing. */
  cap?: keyof GameCaps;
}

const STEPS: Step[] = [
  {
    icon: Snowflake,
    title: "tour.welcomeTour.title",
    body: "tour.welcomeTour.body",
  },
  {
    view: "browse",
    selector: '[data-tour="browse"]',
    icon: Compass,
    title: "tour.browse.title",
    body: "tour.browse.body",
  },
  {
    view: "library",
    selector: '[data-tour="library"]',
    icon: LibraryIcon,
    title: "tour.library.title",
    body: "tour.library.body",
  },
  {
    view: "locker",
    cap: "viewer",
    selector: '[data-tour="locker"]',
    icon: Bike,
    title: "tour.locker.title",
    body: "tour.locker.body",
  },
  {
    view: "presets",
    selector: '[data-tour="presets"]',
    icon: Shirt,
    title: "tour.presets.title",
    body: "tour.presets.body",
  },
  {
    view: "rider",
    selector: '[data-tour="rider"]',
    icon: User,
    title: "tour.rider.title",
    body: "tour.rider.body",
    cap: "viewer",
  },
  {
    selector: '[data-tour="frostmod"]',
    icon: RefreshCw,
    title: "tour.frostmod.title",
    body: "tour.frostmod.body",
    cap: "frostmod",
  },
  {
    view: "settings",
    selector: '[data-tour="settings"]',
    icon: SettingsIcon,
    title: "tour.settings.title",
    body: "tour.settings.body",
  },
  {
    icon: Check,
    title: "tour.done.title",
    body: "tour.done.body",
  },
];

interface Rect {
  top: number;
  left: number;
  width: number;
  height: number;
}

const BUBBLE_W = 340;
const PAD = 8; // spotlight padding around the target
const GAP = 16; // gap between spotlight and bubble

/** Where to place the bubble given the spotlight rect and the bubble's own height
 *  (centered when there's no rect). Vertically centres on the target, then clamps so
 *  the whole bubble stays inside the viewport — important for low targets. */
function bubbleStyle(rect: Rect | null, bubbleH: number): React.CSSProperties {
  if (typeof window === "undefined" || !rect) {
    return { left: "50%", top: "50%", transform: "translate(-50%, -50%)" };
  }
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  // Prefer the right of the target (the nav sits on the left); flip if it won't fit.
  let left = rect.left + rect.width + GAP;
  if (left + BUBBLE_W > vw - GAP) left = rect.left - BUBBLE_W - GAP;
  left = Math.max(GAP, Math.min(left, vw - BUBBLE_W - GAP));
  const h = bubbleH || 320;
  let top = rect.top + rect.height / 2 - h / 2;
  top = Math.max(GAP, Math.min(top, vh - h - GAP));
  return { left, top };
}

interface TourProps {
  navigate: (v: DashboardView) => void;
  onDone: () => void;
}

export default function Tour({ navigate, onDone }: TourProps) {
  const { game } = useConfig();
  // Only the steps whose target this game actually shows.
  const steps = useMemo(
    () => STEPS.filter((s) => !s.cap || game.caps[s.cap]),
    [game],
  );
  const t = useT();
  const [index, setIndex] = useState(0);
  const [rect, setRect] = useState<Rect | null>(null);
  const bubbleRef = useRef<HTMLDivElement>(null);
  const [bubbleH, setBubbleH] = useState(0);
  const step = steps[index];
  const Icon = step.icon;
  const isLast = index === steps.length - 1;

  // Switch to the step's view first, so the correct screen renders behind the spotlight.
  useLayoutEffect(() => {
    if (step.view) navigate(step.view);
  }, [index, step.view, navigate]);

  // Measure the target after the view switch has had a frame to lay out; keep it
  // in sync with window resizes. Un-anchored steps clear the rect (centered bubble).
  useEffect(() => {
    if (!step.selector) {
      setRect(null);
      return;
    }
    const selector = step.selector;
    const measure = () => {
      const el = document.querySelector(selector);
      if (!el) {
        setRect(null);
        return;
      }
      const r = el.getBoundingClientRect();
      setRect({ top: r.top, left: r.left, width: r.width, height: r.height });
    };
    const timer = window.setTimeout(measure, 70);
    window.addEventListener("resize", measure);
    return () => {
      window.clearTimeout(timer);
      window.removeEventListener("resize", measure);
    };
  }, [index, step.selector]);

  // Measure the bubble so it can be centred on its target and clamped to the viewport.
  useLayoutEffect(() => {
    if (bubbleRef.current) setBubbleH(bubbleRef.current.offsetHeight);
  }, [index, rect]);

  const next = () => (isLast ? onDone() : setIndex((i) => i + 1));
  const back = () => setIndex((i) => Math.max(0, i - 1));

  return (
    <div className="fixed inset-0 z-[60]">
      {/* Click-blocker: keeps the app behind the tour non-interactive. Also carries the
          dim for centered steps that have no spotlight cut-out. */}
      <div className={cn("absolute inset-0", !rect && "bg-black/70")} />

      {/* Spotlight: a padded rounded rect whose massive box-shadow dims everything else.
          pointer-events-none so it never intercepts clicks; it animates between targets. */}
      {rect && (
        <div
          className="pointer-events-none absolute rounded-xl outline outline-2 outline-white/70 transition-all duration-300 ease-out"
          style={{
            top: rect.top - PAD,
            left: rect.left - PAD,
            width: rect.width + PAD * 2,
            height: rect.height + PAD * 2,
            boxShadow: "0 0 0 9999px rgba(0,0,0,0.66)",
          }}
        />
      )}

      {/* Coach-mark bubble */}
      <div
        ref={bubbleRef}
        className="absolute flex w-[340px] flex-col gap-5 rounded-2xl border border-input bg-card p-6 shadow-2xl transition-all duration-300 ease-out"
        style={bubbleStyle(rect, bubbleH)}
      >
        <div className="flex flex-col gap-3">
          <div className="grid size-11 place-items-center rounded-[13px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
            <Icon className="size-[22px]" strokeWidth={2.5} />
          </div>
          <div className="flex flex-col gap-1.5">
            <h2 className="text-[17px] font-extrabold tracking-[-0.3px]">
              {t(step.title)}
            </h2>
            <p className="text-[13px] leading-relaxed text-muted-foreground">
              {t(step.body, { site: game.catalogDomain, game: game.display })}
            </p>
          </div>
        </div>

        <div className="flex items-center gap-1.5">
          {steps.map((_, i) => (
            <span
              key={i}
              className={cn(
                "h-1.5 rounded-full transition-all",
                i === index ? "w-4 bg-primary" : "w-1.5 bg-foreground/20",
              )}
            />
          ))}
        </div>

        <div className="flex items-center justify-between gap-3">
          {index > 0 ? (
            <Button variant="ghost" size="sm" onClick={back}>
              <ArrowLeft /> {t("common.back")}
            </Button>
          ) : (
            <Button variant="ghost" size="sm" onClick={onDone}>
              {t("common.skip")}
            </Button>
          )}
          <Button size="sm" onClick={next}>
            {isLast ? t("common.done") : t("common.next")}
            {!isLast && <ArrowRight />}
          </Button>
        </div>
      </div>
    </div>
  );
}
