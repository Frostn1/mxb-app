/**
 * The non-React half of the i18n layer: key types, lookup and formatting.
 *
 * Kept separate from `index.tsx` because React Fast Refresh only hot-swaps a
 * module whose exports are all components or hooks. With `LOCALE_OPTIONS` and
 * friends living alongside `I18nProvider`, every edit to the i18n layer forced a
 * full page reload in dev instead of a hot update.
 *
 * This module ships only the BASE dictionary — the strings the shared components
 * themselves render. An app calls `registerDicts` at startup with its own, fuller
 * dictionaries; until it does, the base ones are what renders.
 */
import { en as baseEn } from "./base/en";
import { it as baseIt } from "./base/it";
import { es as baseEs } from "./base/es";
import { fr as baseFr } from "./base/fr";
import { de as baseDe } from "./base/de";
import { ptBR as basePtBR } from "./base/pt-BR";

/** Every locale we ship. Also what `isLocale` tests against, so it no longer
 *  depends on which dictionaries happen to be registered. */
export const LOCALES = ["en", "it", "es", "fr", "de", "pt-BR"] as const;
export type Locale = (typeof LOCALES)[number];
/** What the user picked — `system` follows the OS. */
export type LocalePref = Locale | "system";

/** A dictionary as this layer handles it: flat, and not necessarily the base one. */
export type Dict = Record<string, string>;

/**
 * Every base locale is typed against base `en`, so `tsc` rejects a translation
 * that is missing a key or invents one. That's the whole reason this is a local
 * module and not i18next: a missing key can't reach a build. Each app keeps the
 * same guarantee over its own, larger dictionary — see its `i18n/index.ts`.
 */
export type BaseTranslation = Record<keyof typeof baseEn, string>;

type BaseKey = keyof typeof baseEn;
/** `"library.uninstalled_other"` → `"library.uninstalled"`, so the plural family
 *  is addressable by its base name while staying type-checked. */
export type PluralBase<K> = K extends `${infer B}_other` ? B : never;
export type BaseTKey = BaseKey | PluralBase<BaseKey>;

export type TVars = Record<string, string | number>;
export type TFunc<K extends string = string> = (key: K, vars?: TVars) => string;

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

export const STORAGE_KEY = "frost-locale";

const BASE_DICTS: Record<Locale, Dict> = {
  en: baseEn,
  it: baseIt,
  es: baseEs,
  fr: baseFr,
  de: baseDe,
  "pt-BR": basePtBR,
};

let DICTS: Record<Locale, Dict> = BASE_DICTS;
let FALLBACK: Dict = baseEn;

/**
 * Hand the layer the app's dictionaries.
 *
 * Called for its side effect from the app's `i18n/index.ts`, which imports the
 * dictionaries anyway to derive its `Translation` type — so the registration
 * happens on first import, before any component renders.
 */
export function registerDicts(dicts: Record<Locale, Dict>): void {
  DICTS = dicts;
  FALLBACK = dicts.en;
}

function isLocale(v: string): v is Locale {
  return (LOCALES as readonly string[]).includes(v);
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
  dict: Dict,
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
 * `{{app}}` is seeded here rather than by a provider because it is a build-time
 * constant and because `setAmbientVars` has exactly one caller (`App.tsx`) — the
 * overlay renders its own React tree in a separate webview where that never runs, so
 * anything waiting on a provider renders its placeholder literally over there.
 *
 * Explicit vars always win, so a call site can still override.
 */
/** The product name, for the handful of rendered strings that are hardcoded English
 *  rather than dictionary entries. Same source as the `{{app}}` placeholder. */
export const APP_NAME: string = import.meta.env.VITE_APP_NAME;

let ambientVars: TVars = { app: APP_NAME };

/** Merge, don't replace: the build-time seeds above have to survive this. */
export function setAmbientVars(vars: TVars): void {
  ambientVars = { ...ambientVars, ...vars };
}

/** The ambient value for a placeholder name, if there is one. */
export function ambientValue(name: string): string | number | undefined {
  return name in ambientVars ? ambientVars[name] : undefined;
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
    lookup(DICTS[locale], locale, key, vars) ?? lookup(FALLBACK, "en", key, vars);
  // The types make this unreachable; showing English beats showing a raw key.
  return raw ?? key;
}

export function translate(locale: Locale, key: string, vars?: TVars): string {
  return interpolate(template(locale, key, vars), vars);
}

/** Render a label that is either a real folder name or a translated phrase. */
export function labelOf<K extends string>(
  opt: { label: string; labelKey?: K; labelVars?: Record<string, string> },
  t: TFunc<K>,
): string {
  return opt.labelKey ? t(opt.labelKey, opt.labelVars) : opt.label;
}
