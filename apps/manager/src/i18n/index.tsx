/**
 * The i18n components.
 *
 * This module exports **only** React components, which is what makes it a valid
 * Fast Refresh boundary — so editing a locale file hot-updates instead of
 * reloading the whole app. Keep it that way:
 *
 *   - hooks and the context  → `./context`
 *   - dictionaries and pure functions → `./core`
 *
 * (The `export type` block below is erased at build time and doesn't count.)
 */
import { Fragment, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import {
  readStoredLocale,
  resolveSystemLocale,
  setActiveLocale,
  STORAGE_KEY,
  template,
  translate,
  type Locale,
  type LocalePref,
  type TFunc,
  type TKey,
} from "./core";
import { I18nContext, useI18n } from "./context";

export type {
  Locale,
  LocalePref,
  TFunc,
  TKey,
  TVars,
  Translation,
} from "./core";

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<LocalePref>(readStoredLocale);
  const [systemLocale, setSystemLocale] = useState<Locale>(() =>
    resolveSystemLocale(),
  );

  // The webview reports a language change without a reload on some platforms.
  useEffect(() => {
    const onChange = () => setSystemLocale(resolveSystemLocale());
    window.addEventListener("languagechange", onChange);
    return () => window.removeEventListener("languagechange", onChange);
  }, []);

  const resolved: Locale = locale === "system" ? systemLocale : locale;

  // Set during render, not in an effect: the pure formatters in `lib/mods` read
  // it while this same render's children are producing their output.
  setActiveLocale(resolved);

  // Screen readers and CSS hyphenation both key off this.
  useEffect(() => {
    document.documentElement.lang = resolved;
  }, [resolved]);

  const setLocale = useCallback((pref: LocalePref) => {
    localStorage.setItem(STORAGE_KEY, pref);
    setLocaleState(pref);
  }, []);

  const t = useCallback<TFunc>(
    (key, vars) => translate(resolved, key, vars),
    [resolved],
  );

  const value = useMemo(
    () => ({ locale, resolved, setLocale, t }),
    [locale, resolved, setLocale, t],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

/**
 * A translated string with React nodes spliced into its placeholders.
 *
 * Needed wherever a sentence wraps styled markup (a path in mono, a bold name):
 * splitting such a sentence into prefix/suffix keys would hard-code English word
 * order, and German and French don't share it.
 *
 *   <Trans k="setup.autoDetect" values={{ hint: <code>{hint}</code> }} />
 */
export function Trans({
  k,
  values,
  count,
}: {
  k: TKey;
  values: Record<string, ReactNode>;
  count?: number;
}) {
  const { resolved } = useI18n();
  const raw = template(resolved, k, count === undefined ? undefined : { count });
  const parts = raw.split(/(\{\{\w+\}\})/g);
  return (
    <>
      {parts.map((part, i) => {
        const match = /^\{\{(\w+)\}\}$/.exec(part);
        if (!match) return part;
        const name = match[1];
        return (
          <Fragment key={i}>
            {name in values
              ? values[name]
              : count !== undefined && name === "count"
                ? count
                : part}
          </Fragment>
        );
      })}
    </>
  );
}
