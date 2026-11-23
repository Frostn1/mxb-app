/**
 * MXB Cinema's i18n entry point. Import `useT` and `TKey` from here.
 *
 * Same arrangement as Coach's and the Studio's: `@frost/shared` carries the machinery, this
 * module hands it the dictionaries and narrows the key type to them, so `tsc` rejects a typo.
 * All seven of the lineup's languages.
 */
import { I18nProvider } from "@frost/shared/i18n";
import { useT as useTBase } from "@frost/shared/i18n/context";
import { registerDicts, type PluralBase, type TFunc } from "@frost/shared/i18n/core";
import { de } from "./locales/de";
import { en } from "./locales/en";
import { es } from "./locales/es";
import { fr } from "./locales/fr";
import { it } from "./locales/it";
import { ptBR } from "./locales/pt-BR";
import { sv } from "./locales/sv";

// A side effect on import, deliberately — see the manager's copy for why.
registerDicts({ en, it, es, fr, de, "pt-BR": ptBR, sv });

type Key = keyof typeof en;
export type TKey = Key | PluralBase<Key>;

export function useT(): TFunc<TKey> {
  return useTBase<TKey>();
}

export { I18nProvider };
export { setAmbientVars } from "@frost/shared/i18n/core";
