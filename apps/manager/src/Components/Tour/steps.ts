import {
  Snowflake,
  Compass,
  Search,
  LayoutGrid,
  Download,
  ListChecks,
  Library as LibraryIcon,
  Bike,
  Shirt,
  RefreshCw,
  Settings as SettingsIcon,
  Check,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { TKey } from "@/i18n";
import type { GameCaps } from "@frost/shared/types";
import type { DashboardView } from "../Shell/nav";

export interface Step {
  /** View to switch to before highlighting, so the real screen sits behind the spotlight. */
  view?: DashboardView;
  /** CSS selector of the element to spotlight. Omit, or match nothing, for a centred bubble. */
  selector?: string;
  icon: LucideIcon;
  title: TKey;
  body: TKey;
  /** Capability the active game must have for this step to apply. A step that
   *  spotlights a nav item the game doesn't show would highlight nothing. */
  cap?: keyof GameCaps;
}

export const STEPS: Step[] = [
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
  // The install walk-through: search, pick a card, add it, watch it download.
  {
    view: "browse",
    selector: '[data-tour="browse-search"]',
    icon: Search,
    title: "tour.search.title",
    body: "tour.search.body",
  },
  {
    view: "browse",
    selector: '[data-tour="mod-card"]',
    icon: LayoutGrid,
    title: "tour.card.title",
    body: "tour.card.body",
  },
  {
    // Spotlights the button when a mod page is open; otherwise a centred bubble explains it.
    view: "browse",
    selector: '[data-tour="mod-install"]',
    icon: Download,
    title: "tour.install.title",
    body: "tour.install.body",
  },
  {
    // The queue chip only exists while something is downloading.
    view: "browse",
    selector: '[data-tour="download-queue"]',
    icon: ListChecks,
    title: "tour.queue.title",
    body: "tour.queue.body",
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
    view: "locker",
    cap: "viewer",
    selector: '[data-tour="locker-bikes"]',
    icon: Bike,
    title: "tour.lockerUse.title",
    body: "tour.lockerUse.body",
  },
  {
    view: "presets",
    selector: '[data-tour="presets"]',
    icon: Shirt,
    title: "tour.presets.title",
    body: "tour.presets.body",
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

/** The steps this game actually has: a step gated on a capability the game lacks is dropped,
 *  because its spotlight would dim the screen and point at nothing. */
export function stepsFor(caps: Partial<GameCaps>, all: Step[] = STEPS): Step[] {
  return all.filter((s) => !s.cap || !!caps[s.cap]);
}
