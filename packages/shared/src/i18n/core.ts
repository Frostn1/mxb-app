/**
 * The non-React half of the i18n layer: dictionaries, key types, lookup and
 * formatting.
 *
 * Kept separate from `index.tsx` because React Fast Refresh only hot-swaps a
 * module whose exports are all components or hooks. With `LOCALE_OPTIONS` and
 * friends living alongside `I18nProvider`, every edit to the i18n layer forced a
 * full page reload in dev instead of a hot update.
 */
import { en } from "./locales/en";
import { it } from "./locales/it";
import { es } from "./locales/es";
import { fr } from "./locales/fr";
import { de } from "./locales/de";
import { ptBR } from "./locales/pt-BR";

export type Locale = "en" | "it" | "es" | "fr" | "de" | "pt-BR";
/** What the user picked — `system` follows the OS. */
export type LocalePref = Locale | "system";

/**
 * Every locale is typed against `en`, so `tsc` rejects a translation that is
 * missing a key or invents one. That's the whole reason this is a local module
 * and not i18next: a missing key can't reach a build.
 */
export type Translation = Record<keyof typeof en, string>;

const DICTS: Record<Locale, Translation> = { en, it, es, fr, de, "pt-BR": ptBR };

/** Listed in their own language — someone who lands in a language they can't
 *  read still has to be able to find their way out. */
export const LOCALE_OPTIONS: { value: LocalePref; label: string }[] = [
  { value: "system", label: "System" },
  { value: "en", label: "English" },
  { value: "it", label: "Italiano" },
  { value: "es", label: "Español" },
  { value: "fr", label: "Français" },
  { value: "de", label: "Deutsch" },
  { value: "pt-BR", label: "Português (BR)" },
];

type Key = keyof typeof en;
/** `"library.uninstalled_other"` → `"library.uninstalled"`, so the plural family
 *  is addressable by its base name while staying type-checked. */
type PluralBase<K> = K extends `${infer B}_other` ? B : never;
export type TKey = Key | PluralBase<Key>;

export type TVars = Record<string, string | number>;
export type TFunc = (key: TKey, vars?: TVars) => string;

export const STORAGE_KEY = "frost-locale";

function isLocale(v: string): v is Locale {
  return v in DICTS;
}

/**
 * The active locale, mirrored outside React.
 *
 * `Intl` date formatting lives in pure helpers (`lib/mods`) that dozens of call
 * sites use without a hook in scope. Reading the locale from here keeps those
 * helpers signature-compatible while still following the in-app language rather
 * than the OS one — picking Italian on an English machine has to move the dates
 * too, or half the UI stays English.
 */
let activeLocale: Locale = "en";
export function getLocale(): Locale {
  return activeLocale;
}
export function setActiveLocale(locale: Locale) {
  activeLocale = locale;
}

export function readStoredLocale(): LocalePref {
  const v = localStorage.getItem(STORAGE_KEY);
  if (v === "system" || (v && isLocale(v))) return v;
  return "system";
}

/** Map an OS tag (`de-AT`, `pt`, `en-GB`) onto a locale we actually ship. */
export function resolveSystemLocale(tag = navigator.language): Locale {
  if (isLocale(tag)) return tag;
  const base = tag.split("-")[0]?.toLowerCase() ?? "";
  // Brazilian is the only Portuguese we ship — better than dropping `pt` to English.
  if (base === "pt") return "pt-BR";
  return isLocale(base) ? base : "en";
}

function lookup(
  dict: Translation,
  locale: Locale,
  key: string,
  vars?: TVars,
): string | undefined {
  const d = dict as Record<string, string | undefined>;
  if (typeof vars?.count === "number") {
    // Intl gets the per-language rules right where a `n === 1` check wouldn't —
    // French, for one, treats 0 as singular.
    const category = new Intl.PluralRules(locale).select(vars.count);
    return d[`${key}_${category}`] ?? d[`${key}_other`] ?? d[key];
  }
  return d[key];
}

/**
 * Variables available to every string without its call site passing them.
 *
 * `{{game}}` is the case this exists for. The app drives more than one title, so any
 * string naming "MX Bikes" is wrong half the time — but threading the active game
 * through forty-odd `t()` calls would mean a placeholder rendering literally the first
 * time someone adds a string and forgets. Mirrored outside React for the same reason the
 * active locale is: the strings are read from plain helpers as well as components.
 *
 * Explicit vars always win, so a call site can still override.
 */
let ambientVars: TVars = {};

export function setAmbientVars(vars: TVars): void {
  ambientVars = vars;
}

function interpolate(str: string, vars?: TVars): string {
  const all = vars ? { ...ambientVars, ...vars } : ambientVars;
  return str.replace(/\{\{(\w+)\}\}/g, (whole, name: string) =>
    name in all ? String(all[name]) : whole,
  );
}

/** The uninterpolated string for a key, falling back to English. */
export function template(locale: Locale, key: string, vars?: TVars): string {
  const raw =
    lookup(DICTS[locale], locale, key, vars) ?? lookup(en, "en", key, vars);
  // The types make this unreachable; showing English beats showing a raw key.
  return raw ?? key;
}

export function translate(locale: Locale, key: string, vars?: TVars): string {
  return interpolate(template(locale, key, vars), vars);
}

/** Render a label that is either a real folder name or a translated phrase. */
export function labelOf(
  opt: { label: string; labelKey?: TKey; labelVars?: Record<string, string> },
  t: TFunc,
): string {
  return opt.labelKey ? t(opt.labelKey, opt.labelVars) : opt.label;
}
