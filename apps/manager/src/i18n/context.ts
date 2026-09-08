/**
 * The i18n context and its hooks.
 *
 * Split from the provider for the same reason `Context/FrostmodContext.ts` is
 * split from `Context/Frostmod.tsx`: Vite's Fast Refresh only hot-swaps a module
 * whose exports are *all* React components. With the hooks living alongside
 * `I18nProvider`, editing any locale file invalidated that module and forced a
 * full page reload of the app — on the files a translator edits most.
 *
 * Import `useT` / `useI18n` from here; import `I18nProvider` / `Trans` from `..`.
 */
import { createContext, useContext } from "react";
import type { Locale, LocalePref, TFunc } from "./core";

export type {
  Locale,
  LocalePref,
  TFunc,
  TKey,
  TVars,
  Translation,
} from "./core";

export interface I18nContextValue {
  /** The user's preference, `system` included. */
  locale: LocalePref;
  /** What's actually rendering right now. */
  resolved: Locale;
  setLocale: (pref: LocalePref) => void;
  t: TFunc;
}

export const I18nContext = createContext<I18nContextValue | null>(null);

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}

/** The common case — just the translate function. */
export function useT(): TFunc {
  return useI18n().t;
}
