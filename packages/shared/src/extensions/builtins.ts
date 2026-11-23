/**
 * The looks the app ships, presented as extensions.
 *
 * The colorways and the supporter Style Pack predate extensions and keep their own machinery
 * (`Context/Theme.tsx`, `lib/accent.ts`, the rules in `theme.css`): the Style Pack is gated
 * on the supporter flag and Halloween on a date, which no third-party theme could express.
 * What changes is how they are listed: as two built-in extensions whose themes sit in the same
 * picker as everybody else's. Picking one of these sets the colorway; picking an extension's
 * theme draws over whichever colorway is set.
 */

import { COLORWAYS, COLORWAY_SWATCH, type AnyColorway } from "../Context/Theme";
import { PACK_COLORWAYS, PACK_SWATCH } from "../lib/accent";
import type { InstalledExtension } from "./manifest";

export interface BuiltinTheme {
  id: AnyColorway;
  label: string;
  swatch: [string, string];
  /** Needs the Style Pack (or is free right now — see `colorwayUnlocked`). */
  pack: boolean;
}

const LABELS: Record<string, string> = {
  frost: "Frost",
  ember: "Ember",
  moss: "Moss",
  violet: "Violet",
  rose: "Rose",
  slate: "Slate",
  retro: "Retro",
  night: "Night",
  works: "Works",
  lime: "Lime",
  halloween: "Halloween",
};

export const BUILTIN_THEMES: BuiltinTheme[] = [
  ...COLORWAYS.map((c) => ({ id: c, label: LABELS[c] ?? c, swatch: COLORWAY_SWATCH[c], pack: false })),
  ...PACK_COLORWAYS.map((c) => ({ id: c, label: LABELS[c] ?? c, swatch: PACK_SWATCH[c], pack: true })),
];

const builtin = (
  id: string,
  name: string,
  description: string,
  themes: BuiltinTheme[],
): InstalledExtension => ({
  id,
  manifest: {
    manifestVersion: 1,
    id,
    name,
    version: "1.0.0",
    author: "mxbsecure",
    description,
    permissions: [],
    contributes: {
      themes: themes.map((t) => ({ id: t.id, label: t.label })),
    },
  },
  version: "1.0.0",
  enabled: true,
  suspended: false,
  trust: "builtin",
  granted: [],
  autoUpdate: false,
  installedAt: 0,
  source: "builtin",
});

export const BUILTIN_EXTENSIONS: InstalledExtension[] = [
  builtin(
    "mxb.colorways",
    "Colorways",
    "The app's own palettes.",
    BUILTIN_THEMES.filter((t) => !t.pack),
  ),
  builtin(
    "mxb.style-pack",
    "Style Pack",
    "Supporter colorways and a custom accent. Unlocked by the Style Pack or as a supporter.",
    BUILTIN_THEMES.filter((t) => t.pack),
  ),
];
