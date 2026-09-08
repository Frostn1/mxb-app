/**
 * This app's i18n entry point. Import `useT`, `Trans` and `TKey` from here — not
 * from `@frost/shared/i18n`, which knows only the base keys.
 *
 * The shared package carries the machinery and the ~155 strings its own components
 * render. This module hands it the full dictionaries and re-narrows the key types to
 * them, which is what keeps "a missing key can't reach a build" true for app code:
 * `TKey` is computed from *this* app's `en`, so `tsc` still rejects a typo.
 */
import type { ReactNode } from "react";
import { I18nProvider, Trans as TransBase } from "@frost/shared/i18n";
import { useT as useTBase } from "@frost/shared/i18n/context";
import { registerDicts, type PluralBase, type TFunc } from "@frost/shared/i18n/core";
import { en } from "./locales/en";
import { it } from "./locales/it";
import { es } from "./locales/es";
import { fr } from "./locales/fr";
import { de } from "./locales/de";
import { ptBR } from "./locales/pt-BR";

// A side effect on import, deliberately: every module that needs a translated string
// reaches this one first, so the dictionaries are in place before anything renders.
registerDicts({ en, it, es, fr, de, "pt-BR": ptBR });

/** Every locale is typed against `en`, so a missing or invented key fails the build. */
export type Translation = Record<keyof typeof en, string>;

type Key = keyof typeof en;
export type TKey = Key | PluralBase<Key>;

/** The app's translate hook — the base one, narrowed to the full key union. */
export function useT(): TFunc<TKey> {
  return useTBase<TKey>();
}

/**
 * `Trans`, narrowed the same way.
 *
 * The shared component takes `k: string` because it cannot know an app's keys; this
 * cast is the single place it is re-tightened, and it checks every call site rather
 * than each one asserting for itself.
 */
export const Trans = TransBase as (props: {
  k: TKey;
  values: Record<string, ReactNode>;
  count?: number;
}) => ReturnType<typeof TransBase>;

export { I18nProvider };
export {
  APP_NAME,
  getLocale,
  labelOf,
  LOCALE_OPTIONS,
  LOCALES,
  setActiveLocale,
  setAmbientVars,
  template,
  translate,
  type Locale,
  type LocalePref,
  type TFunc,
  type TVars,
} from "@frost/shared/i18n/core";
export { useI18n } from "@frost/shared/i18n/context";
