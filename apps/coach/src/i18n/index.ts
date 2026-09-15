/**
 * MXB Coach's i18n entry point. Import `useT` and `TKey` from here.
 *
 * Same arrangement as the studio's: `@frost/shared` carries the machinery, this module hands
 * it the dictionary and narrows the key type to it, so `tsc` rejects a typo in a coach
 * string. English only for now, so every locale is served the English dictionary.
 */
import { I18nProvider } from "@frost/shared/i18n";
import { useT as useTBase } from "@frost/shared/i18n/context";
import { registerDicts, type PluralBase, type TFunc } from "@frost/shared/i18n/core";
import { en } from "./locales/en";

// A side effect on import, deliberately — see the manager's copy for why.
registerDicts({ en, it: en, es: en, fr: en, de: en, "pt-BR": en });

type Key = keyof typeof en;
export type TKey = Key | PluralBase<Key>;

export function useT(): TFunc<TKey> {
  return useTBase<TKey>();
}

export { I18nProvider };
export { setAmbientVars } from "@frost/shared/i18n/core";
