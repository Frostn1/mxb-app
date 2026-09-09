import type { GameCaps } from "@frost/shared/types";
import type { TKey } from "@frost/shared/i18n/context";
import type { StudioTab } from "../Studio/Studio";

/**
 * A page in the shell. The template literal is how a plugin gets a nav row: its panels are
 * addressed `plugin:<plugin id>/<panel id>`, so the shell can route to one without the
 * union having to name plugins it will never know about at build time.
 */
export type DashboardView =
  | `plugin:${string}`
  | "browse"
  | "servers"
  | "ranked"
  | "shop"
  | "hub"
  | "library"
  | "downloads"
  | "locker"
  | "presets"
  | "studio"
  | "manage"
  | "secure"
  | "settings";

/**
 * Gating shared by rail items and their tabs.
 *
 * `cap` names a capability the active game must have. Gating on a capability rather than on
 * the game id keeps "why is this hidden" answerable in one place — and turning a feature on
 * for another title is a single `true` in `game.rs`.
 */
interface Gated {
  cap?: keyof GameCaps;
  /** Hidden unless the optional local content-lock module is present. */
  needsLock?: boolean;
  /** Hidden unless the mxbsecure module is present AND the experimental flag is on. */
  needsSecure?: boolean;
}

/** One tab in the context bar under the rail. */
export interface RailTab extends Gated {
  view: DashboardView;
  /** Which Studio sub-view this tab opens, for the tabs that are all `view: "studio"`. */
  studio?: StudioTab;
  label: TKey;
  /** A label the app cannot translate, because a plugin wrote it. Wins over `label`. */
  rawLabel?: string;
}

/** One item in the top rail. Its `tabs` become the context bar when it is active. */
export interface RailItem extends Gated {
  id: string;
  label: TKey;
  /** A label the app cannot translate, because a plugin wrote it. Wins over `label`. */
  rawLabel?: string;
  /** Where clicking the rail item lands when it has no tabs, or its tabs are all hidden. */
  view: DashboardView;
  studio?: StudioTab;
  tabs?: RailTab[];
}

/**
 * The rail, left to right.
 *
 * Seven items is the ceiling before the row stops scanning, which is why Locker and Presets
 * share GARAGE: they act on the same two things — a bike, and the look on it. The slot that
 * freed is what carries Race mode, which decides what the game mounts at startup and was too
 * big to leave as an icon.
 */
export const RAIL: RailItem[] = [
  { id: "browse", label: "nav.browse", view: "browse" },
  { id: "shop", label: "nav.shop", view: "shop", cap: "shop" },
  { id: "hub", label: "nav.hub", view: "hub", cap: "shop" },
  {
    id: "library",
    label: "nav.library",
    view: "library",
    tabs: [
      { view: "library", label: "nav.library" },
      { view: "downloads", label: "nav.downloads" },
    ],
  },
  {
    id: "garage",
    label: "nav.garage",
    view: "locker",
    tabs: [
      { view: "locker", label: "nav.locker", cap: "viewer" },
      { view: "presets", label: "nav.presets" },
    ],
  },
  {
    id: "studio",
    label: "nav.studio",
    view: "studio",
    studio: "designer",
    tabs: [
      { view: "studio", studio: "designer", label: "nav.designer" },
      { view: "studio", studio: "paints", label: "nav.paints" },
      { view: "studio", studio: "rider", label: "nav.rider", cap: "viewer" },
      { view: "studio", studio: "pose", label: "nav.pose", cap: "viewer" },
      { view: "studio", studio: "track", label: "nav.track" },
      { view: "studio", studio: "protect", label: "nav.protect", needsLock: true },
      { view: "secure", label: "nav.secure", needsSecure: true },
    ],
  },
  { id: "manage", label: "nav.manage", view: "manage", cap: "manage" },
  {
    id: "servers",
    label: "nav.servers",
    view: "servers",
    tabs: [
      { view: "servers", label: "nav.serverBrowser" },
      { view: "ranked", label: "nav.ranked" },
    ],
  },
];

/** Which rail item owns a view, so the right item lights up and the right tabs show. */
export function railItemFor(view: DashboardView, items: RailItem[]): RailItem | undefined {
  return (
    items.find((it) => it.tabs?.some((t) => t.view === view)) ??
    items.find((it) => it.view === view)
  );
}
