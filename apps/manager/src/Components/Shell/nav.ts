import type { GameCaps } from "@frost/shared/types";
import type { TKey } from "@/i18n";

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
  | "manage"
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
}

/** One tab in the context bar under the rail. */
export interface RailTab extends Gated {
  view: DashboardView;
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
  tabs?: RailTab[];
  /**
   * Views this item owns *without* offering them as tabs.
   *
   * Mods is one screen whose source is a filter inside it, so `shop` and `hub` still have to
   * resolve — an old deep link, the Downloads row that jumps to the store it came from — and
   * still have to light the right rail item. Putting them in `tabs` would put a second tab
   * row back under the rail, which is the thing this screen exists to remove.
   */
  owns?: DashboardView[];
}

/** The views the Mods screen answers for. `browse` is where the rail lands. */
export const MODS_VIEWS = ["browse", "hub", "shop"] as const;

export type ModsView = (typeof MODS_VIEWS)[number];

export function isModsView(view: DashboardView): view is ModsView {
  return (MODS_VIEWS as readonly string[]).includes(view);
}

/**
 * The rail, left to right.
 *
 * Four items, because four errands is all the app has: race, find a mod, see what you own,
 * set up the bike. Browse, Shop and MXB Hub were three rail items for one errand — three
 * catalogues of the same thing — so they are one MODS screen now, with the store picked by a
 * filter inside it rather than by the rail. Locker, Presets and Race mode share GARAGE for
 * the same reason: they all decide what you take onto the track.
 */
export const RAIL: RailItem[] = [
  {
    id: "servers",
    // The rail says Online: it is where riding with other people lives, and the screen
    // under it is a server browser plus Ranked, not servers alone.
    label: "nav.servers",
    view: "servers",
    tabs: [
      { view: "servers", label: "nav.serverBrowser" },
      { view: "ranked", label: "nav.ranked" },
    ],
  },
  // Never capability-gated: mxb-mods works for every title, and the two stores are what the
  // screen's own source filter hides when `caps.shop` is off.
  { id: "mods", label: "nav.mods", view: "browse", owns: ["shop", "hub"] },
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
      { view: "manage", label: "nav.manage", cap: "manage" },
    ],
  },
];

/** Which rail item owns a view, so the right item lights up and the right tabs show. */
export function railItemFor(view: DashboardView, items: RailItem[]): RailItem | undefined {
  return (
    items.find((it) => it.tabs?.some((t) => t.view === view)) ??
    items.find((it) => it.owns?.includes(view)) ??
    items.find((it) => it.view === view)
  );
}
