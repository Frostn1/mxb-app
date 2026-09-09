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
import type { BaseTKey, Locale, LocalePref, TFunc } from "./core";

export type {
  BaseTKey,
  BaseTranslation,
  Locale,
  LocalePref,
  TFunc,
  TVars,
} from "./core";

export interface I18nContextValue {
  /** The user's preference, `system` included. */
  locale: LocalePref;
  /** What's actually rendering right now. */
  resolved: Locale;
  setLocale: (pref: LocalePref) => void;
  /** Widened here on purpose: one provider serves both the shared components,
   *  which know only the base keys, and the app, which knows all of them. */
  t: TFunc<string>;
}

export const I18nContext = createContext<I18nContextValue | null>(null);

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}

/**
 * The common case — just the translate function.
 *
 * Defaults to the base keys, which is what a shared component may safely use.
 * Each app re-exports this narrowed to its own key union from `i18n/index.ts`,
 * so app code keeps the "a missing key can't reach a build" guarantee over the
 * full dictionary. Import it from there, not from here.
 */
export function useT<K extends string = BaseTKey>(): TFunc<K> {
  return useI18n().t as TFunc<K>;
}
