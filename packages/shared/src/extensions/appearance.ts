/**
 * Turning an extension's looks into CSS — the one place author data meets a stylesheet.
 *
 * No CSS from a package is ever applied. An extension describes a look as data (token
 * colours, a radius, a button shape, a background, a font) and this file builds the CSS
 * from it, so the only things that reach the page are:
 *
 *   - colours that parse as a hex value or a plain colour function with numeric arguments
 *     (no `url()`, no `var()`, nothing nested);
 *   - lengths and numbers inside fixed ranges;
 *   - `data:` URLs this file builds itself from the package's own bytes.
 *
 * That closes the classic CSS tricks — loading a remote image to report what's on screen,
 * attribute selectors that leak an input's value, `@import` — because an extension never
 * writes a selector, a property name or a URL.
 *
 * Pure: no DOM, so the rules can be tested on their own. `ExtensionAppearance.tsx` applies it.
 */

import type { ExtBackground, ExtButtonStyle, ExtColorSet, ExtComponents, ExtPlay, ExtTheme } from "./manifest";

/** A built-in colorway is only the visible choice while no extension theme owns the palette. */
export function colorwayActive(colorway: string, current: string, pickedTheme: unknown): boolean {
  return !pickedTheme && colorway === current;
}

/** Theme tokens an extension may set, by the name it uses for them. */
export const TOKENS: Record<string, string> = {
  window: "--window",
  background: "--background",
  foreground: "--foreground",
  card: "--card",
  cardForeground: "--card-foreground",
  popover: "--popover",
  popoverForeground: "--popover-foreground",
  primary: "--primary",
  primaryForeground: "--primary-foreground",
  secondary: "--secondary",
  secondaryForeground: "--secondary-foreground",
  muted: "--muted",
  mutedForeground: "--muted-foreground",
  faint: "--faint",
  accent: "--accent",
  accentForeground: "--accent-foreground",
  destructive: "--destructive",
  destructiveForeground: "--destructive-foreground",
  success: "--success",
  warning: "--warning",
  border: "--border",
  input: "--input",
  ring: "--ring",
};

const HEX = /^#(?:[0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;
// Arguments are numbers, %, deg/turn/rad, `none`, separators. No parentheses, so nothing nests.
const FUNC = /^(?:rgba?|hsla?|oklch|oklab|lab|lch)\([0-9a-z.,%\s/+-]{1,64}\)$/i;
const ARG_WORDS = /[a-z]+/gi;
const ALLOWED_WORDS = new Set(["deg", "turn", "rad", "grad", "none", "e"]);

/** A colour value safe to put after `--token:`, or null. */
export function cssColor(v: unknown): string | null {
  if (typeof v !== "string") return null;
  const s = v.trim();
  if (HEX.test(s)) return s.toLowerCase();
  if (!FUNC.test(s)) return null;
  const args = s.slice(s.indexOf("(") + 1, -1);
  for (const w of args.match(ARG_WORDS) ?? []) {
    if (!ALLOWED_WORDS.has(w.toLowerCase())) return null;
  }
  return s;
}

/** `Npx` or `Nrem` inside a range, or null. */
export function cssLength(v: unknown, maxPx: number): string | null {
  if (typeof v === "number" && Number.isFinite(v) && v >= 0 && v <= maxPx) return `${v}px`;
  if (typeof v !== "string") return null;
  const m = /^(\d{1,4}(?:\.\d{1,3})?)(px|rem)$/.exec(v.trim());
  if (!m) return null;
  const n = Number(m[1]);
  const px = m[2] === "rem" ? n * 16 : n;
  return px <= maxPx ? `${n}${m[2]}` : null;
}

function num(v: unknown, min: number, max: number): number | null {
  return typeof v === "number" && Number.isFinite(v) && v >= min && v <= max ? v : null;
}

/** The declarations for one colour set: only known tokens, only valid colours. */
export function tokenDecls(colors: ExtColorSet | undefined): string[] {
  if (!colors || typeof colors !== "object") return [];
  const out: string[] = [];
  for (const [name, value] of Object.entries(colors)) {
    const prop = Object.prototype.hasOwnProperty.call(TOKENS, name) ? TOKENS[name] : undefined;
    const color = cssColor(value);
    if (prop && color) out.push(`${prop}: ${color};`);
  }
  return out;
}

/** `[accent, chrome]` for a theme's swatch, like the built-in colorways have. */
export function themeSwatch(t: ExtTheme): [string, string] {
  const set = t.dark ?? t.light ?? {};
  return [cssColor(set.primary) ?? "#0a84ff", cssColor(set.background) ?? cssColor(set.window) ?? "#1c1c1e"];
}

export function buttonDecls(b: ExtButtonStyle): string[] {
  const out: string[] = [];
  const radius = cssLength(b.radius, 999);
  if (radius) out.push(`border-radius: ${radius} !important;`);
  if (b.uppercase === true) out.push("text-transform: uppercase !important;");
  const ls = num(b.letterSpacing, -0.05, 0.3);
  if (ls !== null) out.push(`letter-spacing: ${ls}em !important;`);
  const w = num(b.weight, 100, 900);
  if (w !== null) out.push(`font-weight: ${Math.round(w / 100) * 100} !important;`);
  const bw = num(b.borderWidth, 0, 4);
  if (bw !== null && bw > 0) out.push(`border: ${bw}px solid color-mix(in oklab, currentColor 35%, transparent) !important;`);
  if (b.shadow === "soft") out.push("box-shadow: 0 6px 18px -8px rgba(0, 0, 0, 0.55) !important;");
  if (b.shadow === "glow") {
    out.push("box-shadow: 0 0 18px -4px color-mix(in srgb, var(--primary) 75%, transparent) !important;");
  }
  return out;
}

/** The Play button's shape and colours. Its label and icon are React's (see `useExtensionBrand`). */
export function playDecls(p: ExtPlay): string[] {
  const out: string[] = [];
  const radius = cssLength(p.radius, 999);
  if (radius) out.push(`border-radius: ${radius} !important;`);
  const bg = cssColor(p.background);
  if (bg) out.push(`background-color: ${bg} !important;`);
  const fg = cssColor(p.foreground);
  if (fg) out.push(`color: ${fg} !important;`);
  if (p.uppercase === true) out.push("text-transform: uppercase;");
  if (p.width === "wide") out.push("padding-left: 2.25rem !important; padding-right: 2.25rem !important;");
  return out;
}

/** Card, field and tab-marker shapes. */
export function componentRules(c: ExtComponents, root: string): string[] {
  const rules: string[] = [];
  const card = cssLength(c.cardRadius, 32);
  const input = cssLength(c.inputRadius, 32);
  const bw = num(c.borderWidth, 0, 3);
  const cardDecls = [card && `border-radius: ${card} !important;`, bw !== null && `border-width: ${bw}px !important;`].filter(Boolean);
  if (cardDecls.length) rules.push(`${root}[data-ext-components] :is(.bg-card, .mxb-card):not(.mxb-server-sort-row) { ${cardDecls.join(" ")} }`);
  const inputDecls = [input && `border-radius: ${input} !important;`, bw !== null && `border-width: ${bw}px !important;`].filter(Boolean);
  if (inputDecls.length) rules.push(`${root}[data-ext-components] :is(.mxb-input, select) { ${inputDecls.join(" ")} }`);
  if (c.tabMarker === "block") {
    rules.push(`${root}[data-ext-components] .mxb-tab-marker { top: 4px; bottom: 4px !important; height: auto !important; border-radius: 2px !important; opacity: 0.18; }`);
  }
  return rules;
}

/** A data: URL is only ever one we built: a fixed mime and base64. */
const DATA_URL = /^data:(?:image\/(?:png|jpeg|webp|gif|svg\+xml)|font\/(?:woff2?|ttf|otf));base64,[A-Za-z0-9+/=]+$/;

export function isSafeDataUrl(u: string): boolean {
  return DATA_URL.test(u);
}

/** The `::before` layer a background is drawn on. `image` is a data URL, already resolved. */
export function backgroundDecls(b: ExtBackground, image: string | null): string[] {
  const layers: string[] = [];
  if (image && isSafeDataUrl(image)) layers.push(`url("${image}")`);
  const g = b.gradient;
  if (g && Array.isArray(g.stops)) {
    const stops = g.stops.map(cssColor).filter((c): c is string => c !== null).slice(0, 6);
    if (stops.length >= 2) {
      const angle = num(g.angle, -360, 360) ?? 180;
      layers.push(`linear-gradient(${angle}deg, ${stops.join(", ")})`);
    }
  }
  if (layers.length === 0) return [];
  const pos = b.position === "top" ? "center top" : b.position === "bottom" ? "center bottom" : "center";
  const size = b.size === "contain" ? "contain" : "cover";
  const out = [
    `background-image: ${layers.join(", ")};`,
    `background-position: ${pos};`,
    `background-size: ${size};`,
    "background-repeat: no-repeat;",
    `opacity: ${num(b.opacity, 0, 1) ?? 0.35};`,
  ];
  const blur = num(b.blur, 0, 24);
  if (blur) out.push(`filter: blur(${blur}px);`);
  return out;
}

/** The CSS family a font is registered under: ours, so it can never shadow a real one. */
export function fontFamilyName(extId: string, fontId: string): string {
  const slug = `${extId}-${fontId}`.replace(/[^a-z0-9-]/gi, "-").toLowerCase();
  return `mxbext-${slug}`;
}

export interface AppearanceInput {
  theme?: { ext: string; theme: ExtTheme } | null;
  buttons?: ExtButtonStyle | null;
  background?: { def: ExtBackground; image: string | null } | null;
  /** Generated family names, already registered with `document.fonts`. */
  fonts?: { body?: string; display?: string };
  play?: ExtPlay | null;
  components?: ExtComponents | null;
}

const ROOT = "html";

/**
 * The stylesheet for the picks in play, and the `data-*` attributes on `<html>` that switch
 * its rules on. Rules are keyed on those attributes so an empty pick costs nothing and so
 * they outrank a colorway (`.dark[data-colorway=…]`) without `!important`.
 */
export function compileAppearance(input: AppearanceInput): { css: string; attrs: Record<string, string> } {
  const rules: string[] = [];
  const attrs: Record<string, string> = {};

  const t = input.theme?.theme;
  if (t) {
    attrs["data-ext-theme"] = "";
    const dark = tokenDecls(t.dark ?? t.light);
    const light = tokenDecls(t.light ?? t.dark);
    if (dark.length) rules.push(`${ROOT}.dark[data-ext-theme] { ${dark.join(" ")} }`);
    if (light.length) rules.push(`${ROOT}.light[data-ext-theme] { ${light.join(" ")} }`);
    const radius = cssLength(t.radius, 32);
    if (radius) rules.push(`${ROOT}[data-ext-theme] { --radius: ${radius}; }`);
  }

  if (input.buttons) {
    const decls = buttonDecls(input.buttons);
    if (decls.length) {
      attrs["data-ext-buttons"] = "";
      rules.push(`${ROOT}[data-ext-buttons] .mxb-btn { ${decls.join(" ")} }`);
    }
  }

  if (input.background) {
    const decls = backgroundDecls(input.background.def, input.background.image);
    if (decls.length) {
      attrs["data-ext-bg"] = "";
      const main = `${ROOT}[data-ext-bg]:not(.overlay-window) main.bg-background`;
      rules.push(`${main} { position: relative; isolation: isolate; }`);
      rules.push(`${main}::before { content: ""; position: absolute; inset: 0; z-index: -1; pointer-events: none; ${decls.join(" ")} }`);
    }
  }

  if (input.play) {
    const decls = playDecls(input.play);
    if (decls.length) {
      attrs["data-ext-play"] = "";
      // Only while it is the action: running and starting keep the app's own outline.
      rules.push(`${ROOT}[data-ext-play] .mxb-play:not(:disabled) { ${decls.join(" ")} }`);
    }
  }

  if (input.components) {
    const cr = componentRules(input.components, ROOT);
    if (cr.length) {
      attrs["data-ext-components"] = "";
      rules.push(...cr);
    }
  }

  const family = (f: string) => (/^mxbext-[a-z0-9-]{1,90}$/.test(f) ? `"${f}"` : null);
  const body = input.fonts?.body ? family(input.fonts.body) : null;
  const display = input.fonts?.display ? family(input.fonts.display) : null;
  // The app's theme is `@theme inline`, so `.font-cond` / `.font-sans` carry the family
  // itself rather than reading the variable: both the variable and the classes are set.
  if (body) {
    attrs["data-ext-font"] = "";
    const stack = `${body}, system-ui, -apple-system, "Segoe UI", sans-serif`;
    rules.push(`${ROOT}[data-ext-font] { --font-sans: ${stack}; }`);
    rules.push(`${ROOT}[data-ext-font] body, ${ROOT}[data-ext-font] .font-sans { font-family: ${stack}; }`);
  }
  if (display) {
    attrs["data-ext-font"] = "";
    const stack = `${display}, ui-monospace, Menlo, Consolas, monospace`;
    rules.push(`${ROOT}[data-ext-font] { --font-cond: ${stack}; }`);
    rules.push(`${ROOT}[data-ext-font] .font-cond { font-family: ${stack}; }`);
  }

  return { css: rules.join("\n"), attrs };
}

/** Every attribute `compileAppearance` may set, for clearing them. */
export const APPEARANCE_ATTRS = ["data-ext-theme", "data-ext-buttons", "data-ext-bg", "data-ext-font", "data-ext-play", "data-ext-components"];
