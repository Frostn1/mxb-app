/**
 * `extension.json`, as the webview reads it.
 *
 * The backend (`crates/core/src/extensions.rs`) is the authority on whether a package is
 * well-formed: ids, versions, permissions, and that every file a contribution names is in
 * the package. What it hands over is still the author's JSON, so everything here that ends
 * up on screen is checked again by `appearance.ts` before it touches a stylesheet.
 */

import type { Permission } from "./permissions";

export interface ExtColorSet {
  [token: string]: string;
}

export interface ExtTheme {
  id: string;
  label: string;
  /** Token overrides for the dark look. */
  dark?: ExtColorSet;
  /** Token overrides for the light look. */
  light?: ExtColorSet;
  /** Corner radius for the whole window, e.g. `0.5rem` or `4px`. */
  radius?: string;
  /** A background from this extension's `backgrounds`, shown while the theme is on. */
  background?: string;
  /** A button style from this extension's `buttonStyles`. */
  buttonStyle?: string;
  /** Fonts from this extension's `fonts`. */
  fonts?: { body?: string; display?: string };
  /** The title bar's mark. The "by mxbsecure" byline and the window title never change. */
  brand?: ExtBrand;
  /** The Play button. */
  play?: ExtPlay;
  /** Shapes of the app's own components. */
  components?: ExtComponents;
}

export interface ExtBrand {
  /** png/webp/jpg/gif inside the package, at most 512 KB; drawn 20 px tall beside the name. */
  logo?: string;
  /** Replaces the product name in the title bar. Up to 24 characters. */
  wordmark?: string;
}

export const PLAY_ICONS = ["play", "flag", "bolt", "rocket", "gauge", "none"] as const;
export type PlayIcon = (typeof PLAY_ICONS)[number];

export interface ExtPlay {
  /** Up to 12 characters. "Running" and "Starting" stay the app's own words. */
  label?: string;
  icon?: PlayIcon;
  /** px or rem, up to 999px. */
  radius?: string;
  background?: string;
  foreground?: string;
  uppercase?: boolean;
  /** Extra horizontal padding: `wide` for a bigger target. */
  width?: "normal" | "wide";
}

export interface ExtComponents {
  /** Cards and panels. px or rem, up to 32px. */
  cardRadius?: string;
  /** Text fields and selects. px or rem, up to 32px. */
  inputRadius?: string;
  /** Card and field borders, 0 to 3 px. */
  borderWidth?: number;
  /** The active tab marker under the rail: a `line` (default) or a filled `block`. */
  tabMarker?: "line" | "block";
}

export interface ExtButtonStyle {
  id: string;
  label: string;
  /** `0px`–`999px`, or rem. */
  radius?: string;
  uppercase?: boolean;
  /** In em, -0.05 to 0.3. */
  letterSpacing?: number;
  /** 400–900. */
  weight?: number;
  /** 0–4 px. */
  borderWidth?: number;
  shadow?: "none" | "soft" | "glow";
}

export interface ExtGradient {
  /** Degrees. */
  angle?: number;
  /** 2–6 colours. */
  stops: string[];
}

export interface ExtBackground {
  id: string;
  label: string;
  /** A png/jpg/webp/gif inside the package. */
  image?: string;
  gradient?: ExtGradient;
  /** 0–1. */
  opacity?: number;
  /** px, 0–24. */
  blur?: number;
  position?: "center" | "top" | "bottom";
  size?: "cover" | "contain";
}

export interface ExtFont {
  id: string;
  /** Shown in pickers; the CSS family name is generated so it can't shadow a system font. */
  family: string;
  /** woff2/woff/ttf/otf inside the package. */
  src: string;
  weight?: string;
  style?: "normal" | "italic";
}

export interface ExtView {
  id: string;
  title: string;
  /** HTML inside the package. Scripts and stylesheets it links are inlined. */
  entry: string;
}

export interface ExtCommand {
  id: string;
  title: string;
}

export type ExtSettingType = "boolean" | "string" | "number" | "enum";

export interface ExtSetting {
  key: string;
  type: ExtSettingType;
  label: string;
  default?: boolean | string | number;
  /** For `enum`. */
  options?: string[];
}

export interface ExtMcpTool {
  name: string;
  description: string;
  /** JSON Schema for the arguments. */
  inputSchema?: Record<string, unknown>;
}

export interface ExtContributes {
  themes?: ExtTheme[];
  buttonStyles?: ExtButtonStyle[];
  backgrounds?: ExtBackground[];
  fonts?: ExtFont[];
  views?: ExtView[];
  /** An invisible page that runs while the extension is enabled: for commands and MCP tools. */
  main?: string;
  commands?: ExtCommand[];
  settings?: ExtSetting[];
  mcp?: { tools?: ExtMcpTool[] };
}

export interface ExtManifest {
  manifestVersion: 1;
  id: string;
  name: string;
  version: string;
  author: string;
  description?: string;
  homepage?: string;
  minAppVersion?: string;
  permissions?: Permission[];
  network?: string[];
  updateUrl?: string;
  contributes?: ExtContributes;
}

export type Trust = "builtin" | "verified" | "signed" | "unsigned";

export interface PendingUpdate {
  version: string;
  url: string;
  newPermissions: string[];
  newHosts: string[];
}

/** One installed extension, as `ext_list` returns it (plus the built-ins). */
export interface InstalledExtension {
  id: string;
  manifest: ExtManifest;
  version: string;
  enabled: boolean;
  suspended: boolean;
  trust: Trust;
  key?: string | null;
  publisher?: string | null;
  granted: string[];
  autoUpdate: boolean;
  pendingUpdate?: PendingUpdate | null;
  installedAt: number;
  source: string;
}

/** What `ext_inspect_*` returns: the install dialog's input. */
export interface InspectedPackage {
  token: string;
  manifest: ExtManifest;
  trust: Exclude<Trust, "builtin">;
  key: string | null;
  publisher: string | null;
  replaces: string | null;
  blocked: string | null;
  source: "file" | "url" | "folder" | "catalog";
  newPermissions: string[];
  newHosts: string[];
}

type ListKey = "themes" | "buttonStyles" | "backgrounds" | "fonts" | "views" | "commands" | "settings";

/** Arrays only, whatever the author wrote: a contribution list that isn't one is empty. */
export function list<K extends ListKey>(m: ExtManifest, key: K): NonNullable<ExtContributes[K]> {
  const v = m.contributes?.[key];
  return (Array.isArray(v) ? v : []) as NonNullable<ExtContributes[K]>;
}

/** Short ids only: letters, digits, dash and underscore. Used for every contribution id. */
export function validItemId(id: unknown): id is string {
  return typeof id === "string" && /^[A-Za-z0-9_-]{1,40}$/.test(id);
}

/** A label shown in the app: trimmed, one line, capped. */
export function cleanLabel(s: unknown, max = 40): string {
  if (typeof s !== "string") return "";
  // eslint-disable-next-line no-control-regex
  return s.replace(/[\u0000-\u001f\u007f‪-‮⁦-⁩]/g, "").trim().slice(0, max);
}
