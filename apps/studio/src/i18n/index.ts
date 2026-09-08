/**
 * Frost's Studio's i18n entry point. Import `useT` and `TKey` from here.
 *
 * Same arrangement as the manager's: `@frost/shared` carries the machinery and the ~155
 * strings its own components render, this module hands it the full dictionaries and
 * re-narrows the key types to them, so `tsc` still rejects a typo in a studio string.
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

// A side effect on import, deliberately — see the manager's copy for why.
registerDicts({ en, it, es, fr, de, "pt-BR": ptBR });

export type Translation = Record<keyof typeof en, string>;
type Key = keyof typeof en;
export type TKey = Key | PluralBase<Key>;

export function useT(): TFunc<TKey> {
  return useTBase<TKey>();
}

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
