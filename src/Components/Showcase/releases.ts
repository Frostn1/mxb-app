/**
 * What each release is shown off with.
 *
 * One entry per version worth interrupting someone for — a headline feature and a
 * handful of one-liners, not the changelog. Everything the entry leaves out is what the
 * "release notes" link is for; a modal that reprints the full list is one nobody reads.
 *
 * Adding a release means adding an entry here and its strings to every locale. Nothing
 * else knows about versions.
 */
import {
  Apple,
  Blend,
  Brush,
  Crop,
  Grid3x3,
  Layers,
  Maximize2,
  Monitor,
  Languages,
  ListOrdered,
  Play,
  Bike,
  Gamepad2,
  Store,
  FolderInput,
  Wand2,
  Shield,
  Gauge,
  Palette,
  Sparkles,
  PersonStanding,
  Wrench,
  RefreshCw,
  Package,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { TKey } from "../../i18n/context";

export interface ReleaseHighlight {
  icon: LucideIcon;
  text: TKey;
}

export interface Release {
  /** Dotted version this showcase belongs to, matching the tag minus its `v`. */
  version: string;
  hero: {
    icon: LucideIcon;
    title: TKey;
    body: TKey;
    /** Show the player's live overlay combo under the hero copy. Only the overlay
     *  release has a shortcut to show, so it's opt-in rather than assumed. */
    showsHotkey?: boolean;
    /** Deep-link the hero's button to a Settings section. */
    settingsAction?: { section: "overlay"; label: TKey };
  };
  highlights: ReleaseHighlight[];
}

export const RELEASES: Release[] = [
  {
    version: "0.9.1",
    hero: {
      icon: Brush,
      title: "showcase.v091.hero.title",
      body: "showcase.v091.hero.body",
    },
    highlights: [
      { icon: Blend, text: "showcase.v091.gradient" },
      { icon: Layers, text: "showcase.v091.paintLayer" },
      { icon: Grid3x3, text: "showcase.v091.ghost" },
      { icon: Crop, text: "showcase.v091.parts" },
      { icon: Maximize2, text: "showcase.v091.resize" },
      { icon: Apple, text: "showcase.v091.macos" },
      { icon: Monitor, text: "showcase.v091.steamos" },
    ],
  },
  {
    version: "0.9.0",
    hero: {
      icon: Palette,
      title: "showcase.v090.hero.title",
      body: "showcase.v090.hero.body",
    },
    highlights: [
      { icon: Package, text: "showcase.v090.bundles" },
      { icon: Store, text: "showcase.v090.purchases" },
      { icon: Sparkles, text: "showcase.v090.reshade" },
      { icon: PersonStanding, text: "showcase.v090.ridingStyles" },
      { icon: Wrench, text: "showcase.v090.frostmod" },
      { icon: RefreshCw, text: "showcase.v090.updates" },
    ],
  },
  {
    version: "0.8.0",
    hero: {
      icon: Gamepad2,
      title: "showcase.v080.hero.title",
      body: "showcase.v080.hero.body",
    },
    highlights: [
      { icon: Store, text: "showcase.v080.shop" },
      { icon: FolderInput, text: "showcase.v080.dropzone" },
      { icon: Wand2, text: "showcase.v080.destinations" },
      { icon: Shield, text: "showcase.v080.protection" },
      { icon: Gauge, text: "showcase.v080.faster" },
    ],
  },
  {
    version: "0.7.0",
    hero: {
      icon: Monitor,
      title: "showcase.v070.hero.title",
      body: "showcase.v070.hero.body",
      showsHotkey: true,
      settingsAction: { section: "overlay", label: "showcase.v070.hero.action" },
    },
    highlights: [
      { icon: Languages, text: "showcase.v070.languages" },
      { icon: ListOrdered, text: "showcase.v070.browse" },
      { icon: Play, text: "showcase.v070.play" },
      { icon: Bike, text: "showcase.v070.paint" },
    ],
  },
];

/**
 * Compare dotted versions numerically: `"0.10.0"` is newer than `"0.9.0"`, which a
 * string comparison gets backwards. Unknown or blank input sorts oldest, which is what
 * makes a never-stamped config read as "hasn't seen anything yet".
 */
export function compareVersions(a: string, b: string): number {
  const parts = (v: string) =>
    v
      .trim()
      // A pre-release suffix (`0.7.0-beta.1`) compares as its release: someone on a
      // beta has already had the features, so it must not re-announce them.
      .replace(/[-+].*$/, "")
      .split(".")
      .map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const diff = (x[i] ?? 0) - (y[i] ?? 0);
    if (diff !== 0) return diff;
  }
  return 0;
}

/**
 * The showcase due to someone running `appVersion` who last saw `seenVersion`, or null.
 *
 * Deliberately not "the entry matching the running version": the showcase can ship a
 * patch release after the features it describes, and an upgrade that skips versions
 * (0.6.3 → 0.7.1) still deserves the newest thing it hasn't been told about.
 */
export function releaseToShow(
  appVersion: string,
  seenVersion: string | undefined,
): Release | null {
  if (!appVersion) return null;
  const seen = seenVersion ?? "";
  return (
    [...RELEASES]
      .sort((a, b) => compareVersions(b.version, a.version))
      .find(
        (r) =>
          compareVersions(r.version, appVersion) <= 0 &&
          (seen === "" || compareVersions(r.version, seen) > 0),
      ) ?? null
  );
}
