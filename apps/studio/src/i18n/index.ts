/**
 * The studio's i18n entry point.
 *
 * Only the base dictionary for now — the shared components' ~155 strings. The studio's own
 * ~480 arrive with the tools they belong to, at which point this grows the same
 * `registerDicts` + narrowed `TKey` shape the manager has.
 */
export { APP_NAME, getLocale, LOCALE_OPTIONS, type Locale, type LocalePref } from "@frost/shared/i18n/core";
export { I18nProvider } from "@frost/shared/i18n";
export { useI18n, useT } from "@frost/shared/i18n/context";
