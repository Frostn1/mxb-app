/**
 * Frost Converter's i18n entry point. Import `useT` from here.
 *
 * The Convert screen's strings are the shared base's; the converter adds only its update
 * bar's, worded as the Studio's.
 */
import { I18nProvider } from "@frost/shared/i18n";
import { useT as useTBase } from "@frost/shared/i18n/context";
import { registerDicts, type PluralBase, type TFunc } from "@frost/shared/i18n/core";
import { en as baseEn } from "@frost/shared/i18n/base/en";
import { it as baseIt } from "@frost/shared/i18n/base/it";
import { es as baseEs } from "@frost/shared/i18n/base/es";
import { fr as baseFr } from "@frost/shared/i18n/base/fr";
import { de as baseDe } from "@frost/shared/i18n/base/de";
import { ptBR as basePtBR } from "@frost/shared/i18n/base/pt-BR";
import { sv as baseSv } from "@frost/shared/i18n/base/sv";

const en = {
  ...baseEn,
  "update.available": "{{version}} is available.",
  "update.downloading": "Downloading…",
  "update.downloadingPct": "Downloading… {{pct}}%",
  "update.updating": "Updating…",
  "update.updateAndRestart": "Update & restart",
  "update.dismiss": "Dismiss update notification",
  "update.failed": "Update failed",
} as const;

type Key = keyof typeof en;
type Translation = Record<Key, string>;

const it: Translation = {
  ...baseIt,
  "update.available": "{{version}} è disponibile.",
  "update.downloading": "Download in corso…",
  "update.downloadingPct": "Download in corso… {{pct}}%",
  "update.updating": "Aggiornamento…",
  "update.updateAndRestart": "Aggiorna e riavvia",
  "update.dismiss": "Ignora la notifica di aggiornamento",
  "update.failed": "Aggiornamento non riuscito",
};
const es: Translation = {
  ...baseEs,
  "update.available": "{{version}} ya está disponible.",
  "update.downloading": "Descargando…",
  "update.downloadingPct": "Descargando… {{pct}} %",
  "update.updating": "Actualizando…",
  "update.updateAndRestart": "Actualizar y reiniciar",
  "update.dismiss": "Descartar la notificación de actualización",
  "update.failed": "La actualización falló",
};
const fr: Translation = {
  ...baseFr,
  "update.available": "{{version}} est disponible.",
  "update.downloading": "Téléchargement…",
  "update.downloadingPct": "Téléchargement… {{pct}} %",
  "update.updating": "Mise à jour…",
  "update.updateAndRestart": "Mettre à jour et redémarrer",
  "update.dismiss": "Ignorer la notification de mise à jour",
  "update.failed": "Échec de la mise à jour",
};
const de: Translation = {
  ...baseDe,
  "update.available": "{{version}} ist verfügbar.",
  "update.downloading": "Wird heruntergeladen…",
  "update.downloadingPct": "Wird heruntergeladen… {{pct}} %",
  "update.updating": "Wird aktualisiert…",
  "update.updateAndRestart": "Aktualisieren und neu starten",
  "update.dismiss": "Update-Benachrichtigung ausblenden",
  "update.failed": "Update fehlgeschlagen",
};
const ptBR: Translation = {
  ...basePtBR,
  "update.available": "{{version}} está disponível.",
  "update.downloading": "Baixando…",
  "update.downloadingPct": "Baixando… {{pct}}%",
  "update.updating": "Atualizando…",
  "update.updateAndRestart": "Atualizar e reiniciar",
  "update.dismiss": "Dispensar o aviso de atualização",
  "update.failed": "A atualização falhou",
};
const sv: Translation = {
  ...baseSv,
  "update.available": "{{version}} är tillgänglig.",
  "update.downloading": "Hämtar…",
  "update.downloadingPct": "Hämtar… {{pct}} %",
  "update.updating": "Uppdaterar…",
  "update.updateAndRestart": "Uppdatera och starta om",
  "update.dismiss": "Stäng uppdateringsmeddelandet",
  "update.failed": "Uppdateringen misslyckades",
};

// A side effect on import, as in the Studio's.
registerDicts({ en, it, es, fr, de, "pt-BR": ptBR, sv });

export type TKey = Key | PluralBase<Key>;

export function useT(): TFunc<TKey> {
  return useTBase<TKey>();
}

export { I18nProvider };
