import type { Translation } from "..";

/**
 * German (informal "du" — this is a modding tool for a game community, not a bank).
 *
 * Community terminology rather than dictionary equivalents: `mod`, `Setup`,
 * `Preset` and `Stock` stay as loanwords, while gear is translated — `Helm`,
 * `Stiefel`, `Brille` for goggles, `Protektoren` for protection (the actual MX term,
 * not "Schutz"), `Lackierung` for a bike paint.
 *
 * Note `modType.*Inline`: German capitalizes nouns *everywhere*, so these keep their
 * capitals where English lowercases them mid-sentence. That's the whole reason the
 * inline forms are separate keys rather than `label.toLowerCase()`.
 *
 * German runs ~30% longer than English — this is the locale that stresses layout.
 * Product names (MXB App, FrostMod, MX Bikes) are never translated.
 */
export const de: Translation = {
  // ── Allgemein ──────────────────────────────────────────────────────────────
  "common.cancel": "Abbrechen",
  "common.back": "Zurück",
  "common.next": "Weiter",
  "common.skip": "Überspringen",
  "common.close": "Schließen",
  "common.save": "Speichern",
  "common.delete": "Löschen",
  "common.rename": "Umbenennen",
  "common.retry": "Erneut versuchen",
  "common.tryAgain": "Erneut versuchen",
  "common.loading": "Wird geladen…",
  "common.installed": "Installiert",
  "common.select": "Auswählen",
  "common.deselect": "Abwählen",
  "common.selectAll": "Alle auswählen",
  "common.clear": "Leeren",
  "common.done": "Fertig",
  "common.apply": "Anwenden",
  "common.remove": "Entfernen",
  "common.open": "Öffnen",
  "common.refresh": "Aktualisieren",
  "common.dismiss": "Ausblenden",
  "common.later": "Später",
  "common.active": "Aktiv",

  // ── Fenstersteuerung ───────────────────────────────────────────────────────
  "window.minimize": "Minimieren",
  "window.maximize": "Maximieren",
  "window.close": "Schließen",

  // ── Navigation ─────────────────────────────────────────────────────────────
  "nav.browse": "Entdecken",
  "nav.shop": "Shop",
  "nav.library": "Bibliothek",
  "nav.locker": "Spind",
  "nav.presets": "Presets",
  "nav.rider": "Fahrer",
  "nav.servers": "Server",
  "nav.manage": "Verwalten",
  "nav.settings": "Einstellungen",

  "sidebar.installing": "„{{name}}“ wird installiert",
  "sidebar.queued": "+{{count}} in der Warteschlange",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "FrostMod wird geprüft…",
  "frostmod.running": "FrostMod läuft",
  "frostmod.notRunning": "FrostMod läuft nicht",
  "frostmod.reloadGame": "Spiel neu laden",
  "frostmod.start": "FrostMod starten",
  "frostmod.reloadedGame": "FrostMod hat das Spiel neu geladen.",
  "frostmod.notRunningToast": "FrostMod läuft nicht.",
  "frostmod.started": "FrostMod gestartet",
  "frostmod.alreadyRunning": "FrostMod läuft bereits",
  "frostmod.startFailed": "FrostMod konnte nicht gestartet werden",
  "frostmod.installedToast": "FrostMod {{version}} installiert",
  "frostmod.installedToastDesc":
    "Es lädt das Spiel live neu, sobald du Mods hinzufügst.",
  "frostmod.installedToastRestart":
    "Starte MX Bikes neu, damit sie greift — das laufende Spiel nutzt noch das alte FrostMod.",
  "frostmod.installFailed": "FrostMod konnte nicht installiert werden",
  "frostmod.newModsAdded": "Neue Mods hinzugefügt",
  "frostmod.modsAdded_one": "Neuer Mod hinzugefügt",
  "frostmod.modsAdded_other": "{{count}} Mods hinzugefügt",
  "frostmod.askedReload": "FrostMod wurde zum Neuladen aufgefordert.",
  "frostmod.andMore_one": "{{names}} und {{count}} weiterer",
  "frostmod.andMore_other": "{{names}} und {{count}} weitere",
  "frostmod.watchDesc":
    "{{names}} — FrostMod wurde zum Neuladen aufgefordert.",

  // ── Ersteinrichtung ────────────────────────────────────────────────────────
  "setup.title": "Willkommen bei MXB App",
  "setup.tagline":
    "Durchstöbere mxb-mods, installiere mit einem Klick und lass FrostMod das Spiel für dich neu laden.",
  "setup.modsFolder": "MX-Bikes-Ordner",
  "setup.autoDetect":
    "MXB App erkennt deinen Ordner {{hint}} automatisch. Du kannst ihn auch selbst auswählen.",
  "setup.chooseManually": "Ordner manuell auswählen…",
  "setup.chooseDifferent": "Anderen Ordner auswählen…",
  "setup.gameInstall": "MX-Bikes-Installation",
  "setup.detecting": "Deine MX-Bikes-Installation wird gesucht…",
  "setup.found": "Gefunden",
  "setup.detectedAutomatically": "Automatisch erkannt",
  "setup.installNotFound":
    "Deine MX-Bikes-Installation konnte nicht automatisch gefunden werden — sie liefert die 3D-Fahrervorschau. Wähle sie manuell aus oder lege sie später in den Einstellungen fest.",
  "setup.chooseInstallManually":
    "Installationsordner manuell auswählen…",
  "setup.startBrowsing": "Mods entdecken",
  "setup.detectAndStart": "Erkennen und loslegen",
  "setup.pickModsFolder": "Wähle deinen MX-Bikes-Ordner",
  "setup.pickInstallFolder": "Wähle deinen MX-Bikes-Installationsordner",

  // ── Willkommen ─────────────────────────────────────────────────────────────
  "welcome.intro.title": "Willkommen bei MXB App",
  "welcome.intro.body":
    "Dein Mod-Manager für MX Bikes. Halte Strecken, Motorräder und Lackierungen an einem Ort organisiert — keine ZIP-Dateien mehr über den ganzen Desktop verstreut. Wir zeigen dir das Wichtigste in ein paar Sekunden.",
  "welcome.getStarted": "Los geht's",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "fehlt",
  "presets.missingHint":
    "Dieser Mod ist nicht installiert — im Spiel erscheint er als Stock",
  "presets.missingMods":
    "Fehlende Mods: {{mods}}. Installiere sie, damit diese Teile angezeigt werden.",
  "presets.help":
    "Speichere einen kompletten Fahrer-Look und lade ihn auf Kommando auf ein Motorrad.",
  "presets.profile": "Profil",
  "presets.namePlaceholder": "Preset-Name…",
  "presets.savePreset": "Preset speichern",
  "presets.saveChanges": "Änderungen speichern",
  "presets.saveChangesQ": "Änderungen speichern?",
  "presets.replaceQ": "Preset ersetzen?",
  "presets.replace": "Ersetzen",
  "presets.loadCopy": "Kopie in den Editor laden",
  "presets.viewOnRider": "Am Fahrer ansehen",
  "presets.editNameOrOptions": "Name oder Optionen bearbeiten",
  "presets.share": "Teilen",
  "presets.nameFirst": "Gib dem Preset zuerst einen Namen.",
  "presets.pickProfileAndBike":
    "Wähle ein Profil und ein Motorrad zum Anwenden.",
  "presets.updated": "Preset „{{name}}“ aktualisiert.",
  "presets.renamed":
    "In „{{name}}“ umbenannt und Änderungen gespeichert.",
  "presets.saved": "Preset „{{name}}“ gespeichert.",
  "presets.editing":
    "„{{name}}“ wird bearbeitet — ändere, was du willst, und speichere dann.",
  "presets.appliedRefreshed":
    "„{{label}}“ auf {{bike}} angewendet — live im Spiel aktualisiert.",
  "presets.appliedRefreshFailed":
    "„{{label}}“ auf {{bike}} angewendet — gespeichert, aber die sofortige Aktualisierung ist fehlgeschlagen: wähle dein Profil im Spiel neu aus, um es zu laden.",
  "presets.appliedGameRunning":
    "„{{label}}“ auf {{bike}} angewendet — gespeichert. Wähle dein Profil in MX Bikes (Profilmenü) neu aus, um den neuen Look zu laden.",
  "presets.appliedNextTime":
    "„{{label}}“ auf {{bike}} angewendet — gespeichert. Es wird beim nächsten Start des Spiels geladen.",
  "presets.appliedReselectBike":
    "„{{label}}“ auf {{bike}} angewendet — die Lackierungen sind live; wähle das Motorrad in MX Bikes neu aus, um das Modell zu sehen.",
  "presets.phaseBundling": "Dateien werden verpackt…",
  "presets.phaseUploading": "Paket wird hochgeladen…",
  "presets.phaseDownloading": "Paket wird heruntergeladen…",
  "presets.phaseInstalling": "Dateien werden installiert…",
  "presets.bundleUploaded":
    "Komplettpaket hochgeladen — der Code enthält jetzt auch die Dateien.",
  "presets.shareHintFull":
    "Dieser Code enthält ein herunterladbares Paket — der Empfänger wählt Vollständiger Import und bekommt alles, auch ganz ohne installierte Mods.",
  "presets.shareHintConfig":
    "Schick diesen Code an wen du willst. Importiert wird unter Presets → Importieren. Für jedes Teil werden dieselben Mods benötigt.",
  "presets.generatingCode": "Code wird erzeugt…",
  "presets.nothingToBundle":
    "Keine installierten Dateien zum Verpacken — dieser Look besteht nur aus Stock/Schriften.",
  "presets.createFullBundle": "Komplettpaket erstellen",
  "presets.copiedFull": "Code mit Komplettpaket kopiert.",
  "presets.copiedShare": "Teilen-Code kopiert.",
  "presets.copyFailed":
    "Kopieren nicht möglich — markiere den Code und kopiere ihn von Hand.",
  "presets.copyFullCode": "Vollständigen Code kopieren",
  "presets.copyCode": "Code kopieren",
  "presets.importTitle": "Preset importieren",
  "presets.importBody": "Füge einen Code ein, den dir jemand geschickt hat.",
  "presets.configOnly": "Nur Konfiguration",
  "presets.import": "Importieren",
  "presets.fullImport": "Vollständiger Import",
  "presets.editingBanner":
    "{{name}} wird bearbeitet — ändere den Namen oder einen Slot und dann {{save}}.",
  "presets.bundleNotice":
    "Enthält ein komplettes Paket (~{{size}} von {{host}}). Nutze {{fullImport}}, um alles herunterzuladen und zu installieren — vorher werden keine Mods benötigt.",

  // ── Preset-Slots ───────────────────────────────────────────────────────────
  "slot.paint": "Motorrad-Lackierung",
  "slot.modelSwap": "Modellwechsel",
  "slot.bikeFont": "Startnummern-Schrift",
  "slot.tyres": "Reifen",
  "slot.rider": "Fahrerprofil",
  "slot.suitPaint": "Outfit / Kit",
  "slot.suitFont": "Outfit-Schrift",
  "slot.glovesPaint": "Handschuhe",
  "slot.ridingStyle": "Fahrstil",
  "slot.helmet": "Helm",
  "slot.helmetPaint": "Helm-Design",
  "slot.gogglesPaint": "Brille",
  "slot.boots": "Stiefel",
  "slot.bootsPaint": "Stiefel-Design",
  "slot.protection": "Protektoren",
  "slot.protectionPaint": "Protektoren-Design",
  "slotGroup.bike": "Motorrad",
  "slotGroup.rider": "Fahrer",
  "slotGroup.head": "Kopf",
  "slotGroup.body": "Körper",

  // ── Fahrer-Studio ──────────────────────────────────────────────────────────
  "rider.help":
    "Kleide das Fahrermodell ein — Helm, Brille, Outfit und Stiefel zusammen.",
  "rider.namePlaceholder": "Diesem Fahrer einen Namen geben…",
  "rider.nameFirst": "Gib diesem Fahrer-Look zuerst einen Namen.",
  "rider.showOnModel": "Am Modell zeigen",

  // ── Rundgang ───────────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Mach einen kurzen Rundgang",
  "tour.welcomeTour.body":
    "Ein paar Sekunden, um zu sehen, wo alles liegt. Du kannst jederzeit abbrechen.",
  "tour.browse.title": "Mods entdecken",
  "tour.browse.body":
    "Durchsuche mxb-mods.com direkt hier und installiere jede Strecke, jedes Motorrad und jede Lackierung mit einem einzigen Klick.",
  "tour.library.title": "Deine Bibliothek",
  "tour.library.body":
    "Alles, was du installiert hast, an einem Ort — Mods aktualisieren oder entfernen, ohne je eine ZIP-Datei anzufassen.",
  "tour.locker.title": "Der Spind",
  "tour.locker.body":
    "Tausche Motorradmodelle beliebig aus. MXB App registriert die Teile, damit das Spiel sie erkennt.",
  "tour.presets.title": "Presets",
  "tour.presets.body":
    "Speichere Ausrüstungs- und Design-Kombinationen und wende einen kompletten Look mit einem Klick an — sogar während du fährst.",
  "tour.rider.title": "Fahrer-Studio",
  "tour.rider.body":
    "Sieh dir Ausrüstung und Designs am 3D-Fahrer an, bevor du sie mit auf die Strecke nimmst.",
  "tour.frostmod.title": "FrostMod, live",
  "tour.frostmod.body":
    "Hier siehst du den Status von FrostMod. Es lädt MX Bikes nach einer Installation live neu, sodass neue Inhalte ohne Neustart erscheinen.",
  "tour.settings.title": "Einstellungen",
  "tour.settings.body":
    "Hier legst du deinen Spielordner, das Verhalten im Hintergrund und die FrostMod-Optionen fest. Diesen Rundgang kannst du von hier aus ebenfalls wiederholen.",
  "tour.done.title": "Alles bereit",
  "tour.done.body":
    "Das war der Rundgang. Auf zu Entdecken und installiere deinen ersten Mod.",

  // ── Fehler ─────────────────────────────────────────────────────────────────
  "error.previewFailed": "Vorschau konnte nicht dargestellt werden",
  "error.somethingWentWrong": "Etwas ist schiefgelaufen",
  "error.unexpected": "Ein unerwarteter Fehler ist aufgetreten.",
  "error.reloadApp": "App neu laden",

  // ── Updates ────────────────────────────────────────────────────────────────
  "update.available": "{{version}} ist verfügbar.",
  "update.downloading": "Wird heruntergeladen…",
  "update.downloadingPct": "Wird heruntergeladen… {{pct}} %",
  "update.pitch":
    "Aktualisiere, um die neuesten Funktionen und Fehlerbehebungen zu erhalten.",
  "update.updating": "Wird aktualisiert…",
  "update.updateAndRestart": "Aktualisieren und neu starten",
  "update.dismiss": "Update-Benachrichtigung ausblenden",
  "update.onLatest": "Du hast bereits die neueste Version",
  "update.checkFailed": "Updates konnten nicht geprüft werden",
  "update.failed": "Update fehlgeschlagen",

  // ── 3D-Ansicht ─────────────────────────────────────────────────────────────
  "viewer.preview3d": "3D-Vorschau",
  "viewer.expand": "Vergrößern",
  "viewer.paint": "Design",
  "viewer.loadingModel": "Modell wird geladen…",
  "viewer.loadingPaint": "Design wird geladen…",
  "viewer.loadingRider": "Fahrer wird geladen…",
  "viewer.riderLoadFailed": "Vorschau ist veraltet — sie konnte nicht aktualisiert werden",
  "viewer.dragToRotate": "Ziehen zum Drehen",
  "viewer.scrollToZoom": "Scrollen zum Zoomen",
  "viewer.rightDragToPan": "Rechts ziehen zum Verschieben",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Suchen…",
  "combobox.use": "„{{value}}“ verwenden",

  // ── Mod-Typen ──────────────────────────────────────────────────────────────
  "modType.tracks": "Strecken",
  "modType.bikes": "Motorräder",
  "modType.rider": "Fahrer",
  // Deutsche Substantive werden immer großgeschrieben — auch mitten im Satz.
  "modType.tracksInline": "Strecken",
  "modType.bikesInline": "Motorräder",
  "modType.riderInline": "Fahrerausrüstung",

  // ── Kategoriefilter ────────────────────────────────────────────────────────
  "browseCat.all": "Alle",
  "browseCat.beginner": "Anfänger",
  "browseCat.intermediate": "Fortgeschritten",
  "browseCat.pro": "Profi",
  "browseCat.assets": "Assets",
  "browseCat.newBikes": "Neue Motorräder",
  "browseCat.liveries": "Lackierungen",
  "browseCat.sounds": "Sounds",
  "browseCat.riderKit": "Fahrer-Kit",
  "browseCat.helmets": "Helme",
  "browseCat.helmetPaints": "Helm-Designs",
  "browseCat.gloves": "Handschuhe",
  "browseCat.boots": "Stiefel",
  "browseCat.bootPaints": "Stiefel-Designs",
  "browseCat.protection": "Protektoren",
  "browseCat.protectionPaints": "Protektoren-Designs",

  // ── Entdecken ──────────────────────────────────────────────────────────────
  "browse.help":
    "Entdecke und installiere Mods aus dem Online-Katalog — suchen, nach Typ filtern und einen Mod öffnen, um ihn ins Spiel zu laden.",
  "browse.searchPlaceholder": "{{type}} suchen…",
  "browseSort.newest": "Neueste",
  "browseSort.oldest": "Älteste",
  "browseSort.popularAll": "Beliebteste",
  "browseSort.popularMonth": "Beliebt diesen Monat",
  "browseSort.popularWeek": "Beliebt diese Woche",
  "browse.loadFailed": "Mods konnten nicht geladen werden",
  "browse.empty": "Keine {{type}} gefunden.",
  "browse.loadMore": "Mehr laden",
  "browse.selectedCount": "{{count}} ausgewählt",
  "browse.queuing": "Wird eingereiht…",
  "browse.quickInstallCount": "{{count}} schnell installieren",
  "browse.quickInstall": "Schnellinstallation",
  "browse.quickReinstall": "Schnelle Neuinstallation",
  "browse.openDetails": "Details öffnen",
  "browse.reinstallOne": "„{{title}}“ neu installieren?",
  "browse.reinstallMany": "Bereits vorhandene Mods neu installieren?",
  "browse.reinstallOneBody":
    "Dieser Mod ist bereits in deiner Bibliothek. Beim Neuinstallieren wird er erneut heruntergeladen und die installierten Dateien werden überschrieben.",
  "browse.reinstallManyBody":
    "{{installed}} der {{total}} ausgewählten sind bereits installiert. Wenn du fortfährst, werden sie neu installiert und überschrieben.",
  "browse.reinstall": "Neu installieren",
  "browse.reinstallAll": "Alle neu installieren",
  "browse.queued": "„{{title}}“ eingereiht",
  "browse.queuedDesc": "Wird in {{folder}} installiert.",
  "browse.rootFolder": "Hauptordner",
  "browse.needsBrowser":
    "„{{title}}“ muss über den Browser heruntergeladen werden",
  "browse.needsBrowserDesc":
    "{{host}} blockiert Downloads in der App — öffne die Seite, um fertigzustellen.",
  "browse.noDownload": "Kein Download für „{{title}}“ gefunden",
  "browse.quickInstallFailed":
    "„{{title}}“ konnte nicht schnell installiert werden",
  "browse.queuedBulk_one": "{{count}} Mod eingereiht",
  "browse.queuedBulk_other": "{{count}} Mods eingereiht",
  "browse.queuedBulkDesc": "Sie werden nacheinander installiert.",
  "browse.queuedBulkSkipped_one":
    "{{count}} übersprungen — nur über den Browser verfügbar.",
  "browse.queuedBulkSkipped_other":
    "{{count}} übersprungen — nur über den Browser verfügbar.",
  "browse.bulkFailed": "Die Auswahl konnte nicht schnell installiert werden",
  "browse.bulkFailedDesc_one":
    "Er muss über den Browser heruntergeladen werden.",
  "browse.bulkFailedDesc_other":
    "Alle {{count}} müssen über den Browser heruntergeladen werden.",

  // ── Shop ───────────────────────────────────────────────────────────────────
  "shop.myDownloads": "Meine Downloads",
  "shop.signInTitle": "Bei MX Bikes Shop anmelden",
  "shop.signInBody":
    "Melde dich bei mxbikes-shop.com an, um deine gekauften Strecken zu sehen und zu installieren. Wir öffnen die echte Seite — dein Passwort kommt nie mit dieser App in Berührung.",
  "shop.signIn": "Anmelden",
  "shop.logOut": "Abmelden",
  "shop.signedIn": "Bei MX Bikes Shop angemeldet",
  "shop.sessionFailed":
    "Deine MX-Bikes-Shop-Sitzung konnte nicht übernommen werden",
  "shop.queuedDesc": "Wird in deinen Streckenordner installiert.",
  "shop.loadFailed":
    "Deine Downloads konnten nicht geladen werden: {{error}}",
  "shop.empty": "Noch keine gekauften Downloads in deinem Konto gefunden.",
  // ── MX Bikes Shop-Katalog (nur Stöbern; gekauft wird im Shop) ──────────────
  "shopCatalog.title": "Shop",
  "shopCatalog.help":
    "Stöbere im Katalog von mxbikes-shop.com — suchen, filtern und Preise vergleichen. Gekauft und heruntergeladen wird weiterhin auf der Seite des Shops; diese App zeigt dir nur, was es dort gibt.",
  "shopCatalog.searchPlaceholder": "Shop durchsuchen…",
  "shopCatalog.allCategories": "Alle",
  "shopCatalog.onSaleOnly": "Im Angebot",
  "shopCatalog.loadMore": "Mehr laden",
  "shopCatalog.loadFailed": "Shop-Katalog konnte nicht geladen werden",
  "shopCatalog.empty": "Nichts im Shop passt dazu.",
  "shopCatalog.viewDetails": "Details ansehen",
  "shopCatalog.openOnStore": "Auf mxbikes-shop.com öffnen",
  "shopCatalog.buyOnStore": "Auf mxbikes-shop.com kaufen",
  "shopCatalog.buyNote": "Öffnet im Browser. Kauf und Download laufen über den Shop.",
  "shopCatalog.noProductLink": "Für diesen Artikel gibt es keine Produktseite, die wir öffnen können.",
  "shopCatalog.noScreenshots": "Keine Screenshots",
  "shopCatalog.about": "Über diesen Artikel",
  "shopCatalog.author": "Ersteller",
  "shopCatalog.category": "Kategorie",
  "shopCatalog.updated": "Aktualisiert",
  "shopCatalog.priceUnknown": "Kein Preis angegeben",
  "shopCatalog.free": "Kostenlos",
  "shopCatalog.refresh": "Aktualisieren",
  "shopCatalog.refreshing": "Wird aktualisiert…",
  "shopCatalog.stale": "Preise zuletzt geprüft {{when}}.",
  "shopCatalog.staleHard":
    "Diese Preise wurden zuletzt {{when}} geprüft und sind möglicherweise veraltet. Aktualisiere sie, bevor du dich darauf verlässt.",
  "shopCatalog.saleEndsDays_one": "Angebot endet in 1 Tag",
  "shopCatalog.saleEndsDays_other": "Angebot endet in {{count}} Tagen",
  "shopCatalog.saleEndsHours_one": "Angebot endet in 1 Stunde",
  "shopCatalog.saleEndsHours_other": "Angebot endet in {{count}} Stunden",
  "shopCatalog.saleEndsSoon": "Angebot endet bald",
  "shopCatalog.agoJustNow": "gerade eben",
  "shopCatalog.agoUnknown": "vor einer Weile",
  "shopCatalog.agoMinutes_one": "vor 1 Minute",
  "shopCatalog.agoMinutes_other": "vor {{count}} Minuten",
  "shopCatalog.agoHours_one": "vor 1 Stunde",
  "shopCatalog.agoHours_other": "vor {{count}} Stunden",
  "shopCatalog.agoDays_one": "vor 1 Tag",
  "shopCatalog.agoDays_other": "vor {{count}} Tagen",
  "shopSort.newest": "Neueste",
  "shopSort.recentlyUpdated": "Kürzlich aktualisiert",
  "shopSort.priceAsc": "Preis: aufsteigend",
  "shopSort.priceDesc": "Preis: absteigend",
  "shopSort.onSale": "Angebote zuerst",
  "shopSort.nameAsc": "Name (A–Z)",

  // ── Installationsdialog ────────────────────────────────────────────────────
  "installDialog.installTo": "Installieren nach",
  "installDialog.installToFolder": "Nach {{folder}} installieren",
  "installDialog.change": "Ändern",
  "installDialog.searchBikes": "Motorräder suchen…",
  "installDialog.searchFolders": "Ordner suchen…",
  "installDialog.probably": "Wahrscheinlich",
  "installDialog.allFolders": "Alle Ordner",
  "installDialog.noFolderMatch":
    "Kein Ordner passt — lege ihn unten an.",
  "installDialog.rememberedFor": "Gemerkt für {{type}}",
  "installDialog.downloadFrom": "Herunterladen von",
  "installDialog.downloadPerBike": "Download (pro Motorrad)",
  "installDialog.opensInBrowser":
    "Öffnet im Browser — MXB App schließt die Installation ab",
  "installDialog.matchedBike": "Passend zu deinem Motorrad",
  "installDialog.differentBike": "Anderes Motorrad / Paket",
  "installDialog.directFastest": "Direkt · am schnellsten",
  "installDialog.direct": "Direkt",
  "installDialog.perBikeHint":
    "Jeder Download ist ein anderes Motorrad — automatisch passend zu deiner Auswahl. Wähle das Paket „all bikes“, um alle auf einmal zu bekommen.",
  "installDialog.mirrorsHint":
    "Alle Spiegelserver enthalten dieselbe Datei. Wenn einer fehlschlägt, probiere den nächsten.",

  // ── Bibliotheksdetails ─────────────────────────────────────────────────────
  "libraryDetail.author": "Autor",
  "libraryDetail.length": "Länge",
  "libraryDetail.altitude": "Höhe",
  "libraryDetail.location": "Ort",
  "libraryDetail.type": "Typ",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Gehört zu",
  "libraryDetail.format": "Format",
  "libraryDetail.extractedFolder": "Entpackter Ordner",
  "libraryDetail.paintFile": "Design-Datei",
  "libraryDetail.packagedPkz": "Gepackte .pkz",
  "libraryDetail.size": "Größe",
  "libraryDetail.folder": "Ordner",
  "libraryDetail.lockedWord": "gesperrt",
  "libraryDetail.lockedWithMeta":
    "Diese Strecke wurde von ihrem Ersteller {{locked}}. Name, Details und Vorschau werden hier angezeigt, die Dateien bleiben aber versiegelt — sie lässt sich weder entpacken noch in 3D ansehen.",
  "libraryDetail.lockedNoMeta":
    "Diese Strecke ist {{locked}}, deshalb lassen sich Name, Länge und Vorschau nicht aus der Datei lesen — nur Dateiname und Größe.",

  // ── Mod-Seite ──────────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Auflösen",
  "modDetail.stageDownload": "Herunterladen",
  "modDetail.stageExtract": "Entpacken",
  "modDetail.stagePlace": "Ablegen",
  "modDetail.stageReload": "Neu laden",
  "modDetail.modFiles": "Mod-Dateien",
  "modDetail.copied": "Kopiert",
  "modDetail.copy": "Kopieren",
  "modDetail.addToLibrary": "Zur Bibliothek hinzufügen",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Installiert nach",
  "modDetail.noDownloadLink":
    "Auf dieser Seite wurde kein Download-Link gefunden — öffne sie auf mxb-mods.com.",
  "modDetail.frostmodHint":
    "FrostMod lädt die Liste ({{kind}}) neu, sobald das fertig ist.",
  "modDetail.kindRider": "Fahrer",
  "modDetail.kindBike": "Motorräder",
  "modDetail.kindTrack": "Strecken",
  "modDetail.details": "Details",
  "modDetail.format": "Format",
  "modDetail.mirrors": "Spiegelserver",
  "modDetail.type": "Typ",
  "modDetail.addedToLibrary": "Zu deiner Bibliothek hinzugefügt",
  "modDetail.extracting": "Wird entpackt…",
  "modDetail.addingToLibrary": "Wird zur Bibliothek hinzugefügt…",
  "modDetail.resolving": "Download wird aufgelöst…",
  "modDetail.finishInBrowser": "Im Browser abschließen",
  "modDetail.viewOnSite": "Auf mxb-mods.com ansehen",

  // ── Einstellungen ──────────────────────────────────────────────────────────
  "settings.help":
    "Konfiguriere deinen Spielordner, Updates und App-Einstellungen.",
  "settings.gameFolder": "Spielordner",
  "settings.general": "Allgemein",
  "settings.appearance": "Darstellung",
  "settings.frostmod": "FrostMod",
  "settings.about": "Info & Updates",
  "settings.whatsNew": "Was ist neu",
  "settings.modsFolderDesc":
    "Wohin Mods installiert werden. Eine Änderung scannt deine Bibliothek neu.",
  "settings.insideModsFolder": "In deinem MX-Bikes-Ordner",
  "settings.notSet": "Nicht festgelegt",
  "settings.change": "Ändern…",
  "settings.set": "Festlegen…",
  "settings.theme": "Design",
  "settings.themeLight": "Hell",
  "settings.themeDark": "Dunkel",
  "settings.themeSystem": "System",
  "settings.language": "Sprache",
  "settings.languageSystem": "System",
  "settings.runInBackground": "Im Hintergrund weiterlaufen",
  "settings.runInBackgroundDesc":
    "Beim Schließen des Fensters läuft MXB App im Infobereich weiter, damit FrostMod verbunden bleibt. Beenden über das Symbol im Infobereich.",
  "settings.launchAtStartup": "Beim Systemstart starten",
  "settings.launchAtStartupDesc":
    "MXB App automatisch starten, wenn du dich anmeldest.",
  "settings.instantRefresh": "Sofortige Preset-Aktualisierung",
  "settings.instantRefreshDesc":
    "Wenn du ein Preset anwendest, während MX Bikes läuft, wird der Look sofort im Spiel aktualisiert — ohne Neustart und ohne das Profil neu auszuwählen. Falls das nicht klappt, wirst du gebeten, dein Profil neu auszuwählen.",
  "settings.instantRefreshWindowsOnly":
    "Den Look ohne Neustart im Spiel zu aktualisieren erfordert FrostMod, und das gibt es nur für Windows — du wirst stattdessen gebeten, dein Profil neu auszuwählen.",
  "settings.autoRunFrostmod": "FrostMod automatisch starten",
  "settings.autoRunFrostmodDesc":
    "FrostMod im Hintergrund starten, sobald MXB App geöffnet wird.",
  "settings.watchModsReload": "Automatisch neu laden bei Ordneränderungen",
  "settings.watchModsReloadDesc":
    "Das Spiel automatisch neu laden, wenn Strecken oder Motorräder in deinen Mod-Ordner kommen — auch wenn sie außerhalb von MXB App manuell heruntergeladen wurden.",
  "settings.checking": "Wird geprüft…",
  "settings.runningConnected": "Läuft · Spiel verbunden",
  "settings.notRunning": "Läuft nicht",
  "settings.frostmodInstalled": "Installiert{{suffix}}",
  "settings.notInstalled": "Nicht installiert",
  "settings.checkingGitHub":
    "GitHub wird auf die neueste Version geprüft…",
  "settings.updateCheckFailed":
    "Updates konnten nicht geprüft werden — offline oder GitHub nicht erreichbar.",
  "settings.latestVersion": "Neueste: {{version}}",
  "settings.frostmodNeedsRepair":
    "Die installierten Dateien passen nicht zu dieser Version — eine Neuinstallation behebt das.",
  "settings.frostmodRepair": "Installation reparieren",
  "settings.checkNewer": "Nach einer neueren FrostMod-Version suchen",
  "settings.working": "Wird ausgeführt…",
  "settings.installFrostmod": "FrostMod installieren",
  "settings.updateTo": "Auf {{version}} aktualisieren",
  "settings.reinstallLatest": "Neueste neu installieren",
  "settings.upToDate": "Aktuell",
  "settings.madeWith": "Gemacht mit",
  "settings.updateFailed": "Einstellung konnte nicht geändert werden",
  "settings.startupUpdateFailed":
    "Autostart-Einstellung konnte nicht geändert werden",
  "settings.folderUpdated": "Spielordner aktualisiert",
  "settings.folderUpdatedDesc": "Deine Bibliothek wird neu gescannt.",
  "settings.setFolderFailed": "Ordner konnte nicht festgelegt werden",
  "settings.reDetected": "MX-Bikes-Ordner erneut erkannt",
  "settings.detectFolderFailed": "Ordner konnte nicht erkannt werden",
  "settings.pickInstallFolder":
    "Wähle deinen MX-Bikes-Installationsordner (enthält rider.pkz)",
  "settings.installSet": "Spielinstallation festgelegt",
  "settings.installSetDesc":
    "Die 3D-Fahrervorschau kann jetzt das echte Körpermodell laden.",
  "settings.setInstallFailed":
    "Installationsordner konnte nicht festgelegt werden",
  "settings.installNotFound": "MX Bikes konnte nicht gefunden werden",
  "settings.installNotFoundDesc":
    "Keine Steam-Installation erkannt — lege den Ordner manuell fest.",
  "settings.installFound": "Deine MX-Bikes-Installation wurde gefunden",
  "settings.detectInstallFailed":
    "Installationsordner konnte nicht erkannt werden",
  "settings.pickProfilesFolder": "Wähle deinen MX-Bikes-Profilordner",
  "settings.profilesSet": "Profilordner festgelegt",
  "settings.profilesFound_one": "{{count}} Profil gefunden.",
  "settings.profilesFound_other": "{{count}} Profile gefunden.",
  "settings.noProfilesThere": "Dort wurden keine Profile gefunden",
  "settings.noProfilesThereDesc":
    "Trotzdem gespeichert, aber zum Erstellen von Presets wird ein Ordner benötigt, der deine profile.ini-Ordner enthält.",
  "settings.setProfilesFailed":
    "Profilordner konnte nicht festgelegt werden",
  "settings.profilesReverted":
    "Auf den Standard-Profilordner zurückgesetzt",
  "settings.resetProfilesFailed":
    "Profilordner konnte nicht zurückgesetzt werden",
  "settings.frostmodNotRunningHint":
    "FrostMod läuft nicht — starte es, um Mods live nachzuladen.",
  "settings.reloadUnavailable":
    "Neu laden ist auf dieser Plattform nicht verfügbar.",

  // ── Spielstart ─────────────────────────────────────────────────────────────
  "game.play": "Spielen",
  "game.starting": "Wird gestartet…",
  "game.running": "MX Bikes läuft",
  "game.launch": "MX Bikes starten",
  "game.alreadyRunning": "MX Bikes läuft bereits",
  "game.launching": "MX Bikes wird gestartet…",
  "game.launchFailed": "MX Bikes konnte nicht gestartet werden",
  "join.title": "Server beitreten",
  "join.desc":
    "Gib eine Serveradresse ein, um MX Bikes direkt damit verbunden zu starten.",
  "join.address": "Serveradresse",
  "join.action": "Beitreten",
  "join.joining": "Verbinden…",
  "join.launching": "Verbinde mit {{address}}…",
  "join.alreadyRunning":
    "Schließe zuerst MX Bikes — ein laufendes Spiel kann nicht zu einem Server geschickt werden.",
  "join.failed": "Diesem Server konnte nicht beigetreten werden",

  "servers.title": "Server",
  "servers.subtitle":
    "Verwalte deine eigenen Dedicated Server. Auf jedem muss der MXB-Agent installiert sein.",
  "servers.empty": "Noch keine Server. Füge einen hinzu, um ihn von hier aus zu verwalten.",
  "servers.add": "Server hinzufügen",
  "servers.remove": "Diesen Server entfernen",
  "servers.namePlaceholder": "Servername",
  "servers.tokenPlaceholder": "Agent-Token",
  "servers.track": "Strecke",
  "servers.slots": "Plätze",
  "servers.uptime": "Laufzeit",
  "servers.restarts": "Neustarts",
  "servers.stopped": "Gestoppt",
  "servers.start": "Starten",
  "servers.stop": "Stoppen",
  "servers.restart": "Neu starten",
  "servers.setTrack": "Strecke setzen",
  "servers.trackPlaceholder": "Strecken-ID",
  "servers.actionDone": "Erledigt",
  "servers.actionFailed": "Das hat nicht geklappt",
  "servers.trackChanged": "Strecke auf {{track}} gesetzt — der Server wurde neu gestartet.",
  "servers.saveFailed": "Deine Serverliste konnte nicht gespeichert werden",

  "settings.experimental": "Experimentell",
  "settings.experimentalServers": "Server und Paint-Sync",
  "settings.experimentalServersDesc":
    "Unfertig. Fügt den Server-Tab hinzu, lässt dich Dedicated Server betreiben und gleicht Paints ab, damit alle auf einem Server richtig aussehen.",
  "settings.experimentalForced":
    "Für diesen Lauf durch MXB_EXPERIMENTAL aktiviert — die Einstellung wirkt erst, wenn du die Variable entfernst.",
  "settings.betaBadge": "Beta",

  "sync.title": "Paint-Sync",
  "sync.desc":
    "MX Bikes überträgt Paints nie, also erscheinen andere Fahrer im Standard-Look, wenn du ihre Datei nicht schon hast. Veröffentliche deine und hol dir die der anderen.",
  "sync.enroll": "Registrieren",
  "sync.enrolled": "Registriert als {{name}}",
  "sync.enrollFailed": "Registrierung fehlgeschlagen",
  "sync.codePlaceholder": "Einladungscode",
  "sync.riderNamePlaceholder": "Fahrername im Spiel",
  "sync.riderNameHint":
    "Muss exakt deinem Fahrernamen in MX Bikes entsprechen — daran erkennen die Apps der anderen, welche Paints dir gehören.",
  "sync.ridingAs": "Veröffentlicht als {{name}}",
  "sync.pull": "Paints abgleichen",
  "sync.setGuid": "GUID speichern",
  "sync.guidPlaceholder": "Deine MX-Bikes-GUID",
  "sync.guidHint":
    "Deine MX-Bikes-GUID (optional). Sie identifiziert dich auch nach einer Namensänderung, und der Server protokolliert sie bei jeder Verbindung.",
  "sync.guidSaved": "GUID gespeichert",
  "sync.pulled": "{{installed}} von {{riders}} Fahrern installiert ({{had}} schon vorhanden)",
  "sync.pullFailed": "Abgleich fehlgeschlagen",
  "sync.rejected": "{{count}} mit unsicherem Ziel übersprungen",

  // ── Vom ersten Durchlauf übersehene Strings (mehrzeiliges JSX) ─────────────
  "libraryDetail.noEmbedded": "Für dieses Element wurden keine eingebetteten Details gefunden.",
  "modDetail.downloadFromHost": "Von {{host}} herunterladen",
  "modDetail.openHost": "{{host}} öffnen",
  "modDetail.thenAddFile": "Füge dann die Datei hinzu",
  "modDetail.chooseDownloaded": "Heruntergeladene Datei auswählen",
  "presets.chooseProfilesFolder": "Profilordner auswählen…",
  "presets.viewInRider": "Im Fahrer ansehen",
  "presets.noModelSwapsHere": "Für dieses Motorrad sind keine Modellwechsel registriert —",
  "presets.setUpInLocker": "richte sie im Spind ein",
  "presets.makeActiveBike": "Dieses Motorrad aktiv setzen",
  "presets.nameClash":
    "Ein anderes Preset heißt bereits „{{name}}“ — beim Speichern wird es ebenfalls überschrieben.",
  "presets.shareWarning":
    "Lädt zu einem öffentlichen, temporären Link hoch — dabei werden Mod-Dateien anderer weiterverbreitet, also teile verantwortungsvoll.",
  "settings.profilesDesc":
    "Presets lesen deine Profile von hier — der Pfad unten ist der, in dem die App gerade nachsieht. Das ist der Ordner {{profiles}} in deinem MX-Bikes-Ordner, oder {{documents}}, wenn du deinen Mod-Ordner verschoben hast. Setze ihn nur, wenn deiner woanders liegt.",
  "settings.resetToDefault": "Auf Standard zurücksetzen",
  "settings.gameInstallDesc":
    "Spiel-Installationsordner (optional) — wo MX Bikes installiert ist (enthält {{file}}). Setze ihn, um den echten Fahrerkörper in der 3D-Vorschau zu laden.",
  "viewer.stockGearNote":
    "Auf dem Standard-{{part}} des Spiels gezeigt. Ein Design für ein anderes Modell passt möglicherweise nicht exakt.",
  "viewer.paintNoChange":
    "Keine der Texturen dieses Designs wird von den hier gezeigten Teilen verwendet, deshalb ändert sich die Vorschau nicht. Es kann trotzdem Räder oder Kette einfärben, die diese Ansicht nicht darstellt.",
  "viewer.noPaintPreview": "Keine Design-Vorschau ({{err}})",

  // ── Bibliothek ─────────────────────────────────────────────────────────────
  "library.help":
    "Deine installierten Mods. Sieh nach, was installiert ist, und entferne, was du nicht mehr willst.",
  "library.rootFolder": "(Hauptordner)",
  "library.byAuthor": "von {{author}}",
  "library.locked": "Gesperrt — Inhalt kann nicht gelesen werden",
  "library.searchPlaceholder": "Installierte durchsuchen…",
  "library.scanning": "Deine Bibliothek wird gescannt…",
  "library.empty":
    "Noch keine {{type}} installiert — geh zu Entdecken und füge etwas hinzu.",
  "library.noMatches": "Keine Treffer.",
  "library.quick3d": "Schnelle 3D-Ansicht",
  "library.selectNone": "Auswahl aufheben",
  "library.move": "Verschieben",
  "library.uninstall": "Deinstallieren",
  "library.uninstallAction": "Deinstallieren…",
  "library.moveToFolder": "In Ordner verschieben…",
  "library.showInExplorer": "Im Explorer anzeigen",
  "library.moveDialogTitle": "In Ordner verschieben",
  "library.moveCount_one": "{{count}} Element verschieben",
  "library.moveCount_other": "{{count}} Elemente verschieben",
  "library.chooseDestination": "Wähle einen Zielordner",
  "library.newFolder": "Neuer Ordner…",
  "library.newFolderName": "Name des neuen Ordners",
  "library.createAndMove": "Erstellen und verschieben",
  "library.confirmUninstall": "{{name}} deinstallieren?",
  "library.confirmUninstallBody":
    "Das Element wird in den Papierkorb verschoben — von dort kannst du es wiederherstellen.",
  "library.confirmBulkUninstall_one": "{{count}} Element deinstallieren?",
  "library.confirmBulkUninstall_other":
    "{{count}} Elemente deinstallieren?",
  "library.confirmBulkUninstallBody":
    "Jedes Element wird in den Papierkorb verschoben — von dort kannst du sie wiederherstellen.",
  "library.uninstallCount": "{{count}} deinstallieren",
  "library.moveFailed": "Mod konnte nicht verschoben werden",
  "library.uninstallFailed": "Deinstallation fehlgeschlagen",
  "library.openFailed": "Konnte nicht geöffnet werden",
  "library.uninstalledOne": "{{name}} deinstalliert",
  "library.movedToBin": "In den Papierkorb verschoben.",
  "library.someNotRemoved":
    "Einige Elemente konnten nicht entfernt werden.",
  "library.bulkUninstalled_one": "{{count}} Element deinstalliert",
  "library.bulkUninstalled_other": "{{count}} Elemente deinstalliert",
  "library.bulkUninstallPartial":
    "{{ok}} deinstalliert, {{fail}} fehlgeschlagen",
  "library.bulkMovePartial": "{{ok}} verschoben, {{fail}} fehlgeschlagen",
  "library.bulkMoved_one": "{{count}} Element nach {{folder}} verschoben",
  "library.bulkMoved_other":
    "{{count}} Elemente nach {{folder}} verschoben",

  // ── Spind ──────────────────────────────────────────────────────────────────
  "locker.help":
    "Wechsle Modell und Motorsound jedes Motorrads zwischen den Sets, die du installiert hast.",
  "locker.rescan": "Neu scannen",
  "locker.restore": "Wiederherstellen",
  "locker.register": "Registrieren",
  "locker.scanning": "Motorräder werden gescannt…",
  "locker.scanForSwaps": "Nach Sets suchen",
  "locker.orphanBanner":
    "{{bike}} fehlen die Setup-Dateien — eine frühere Version hat sie in einen Swap-Ordner verschoben, wodurch das Motorrad im Spiel überhaupt nicht mehr lädt. {{files}}",
  "locker.looseBanner_one":
    "{{count}} Modell-/Sound-Set lose in deinen Motorrädern gefunden — registriere es in {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} Modell-/Sound-Sets lose in deinen Motorrädern gefunden — registriere sie in {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "Noch keine tauschbaren Motorräder.",
  "locker.emptyIntro":
    "Zwei Dinge müssen zutreffen, damit ein Tausch möglich ist:",
  "locker.unpacked": "entpackt",
  "locker.emptyRuleUnpacked":
    "Das Motorrad ist {{unpacked}} nach {{path}}— eine gepackte {{pkz}} lässt sich nicht tauschen. Entpacke eines über die Bibliothek.",
  "locker.emptyRuleMesh":
    "Jedes Alternativmodell liegt in einem eigenen Ordner innerhalb dieses Motorrads und enthält ein Mesh ({{edf}}). Lege es irgendwo im Motorradordner ab und klicke unten auf Suchen — wir bieten dir dann an, es unter {{folder}} einzuordnen.",
  "locker.summary": "{{model}} · Sound „{{sound}}“",
  "locker.modelNamed": "Modell „{{name}}“",
  "locker.noModelSwaps": "keine Modellwechsel",
  "locker.models": "Modelle",
  "locker.sounds": "Sounds",
  "locker.onlyOneModel":
    "Nur ein Modell — installiere weitere zum Tauschen",
  "locker.onlyStock":
    "Nur Stock — installiere einen Sound-Mod zum Tauschen",
  "locker.noModel": "Kein Modell",
  "locker.stock": "Stock",
  "locker.activeModel": "Aktives Modell",
  "locker.activeSound": "Aktiver Sound",
  "locker.switchToNoModel":
    "Auf kein Modell wechseln — entfernt die aktuellen Modelldateien",
  "locker.switchToStock":
    "Auf Stock wechseln — entfernt den Sound-Mod (der Originalsound spielt)",
  "locker.missingModelEdf": "Diesem Set fehlt model.edf",
  "locker.missingSoundFiles":
    "Diesem Set fehlt engine.scl oder sfx.cfg",
  "locker.switchTo": "Auf {{name}} wechseln",
  "locker.tiedToModel": "Verknüpft mit Modell {{models}}",
  "locker.boundHint":
    "„{{sound}}“ ist mit Modell „{{model}}“ verknüpft — er wandert mit diesem Modell mit. Zum Lösen klicken.",
  "locker.unboundHint":
    "Verknüpfe den aktiven Sound „{{sound}}“ mit Modell „{{model}}“, damit beim Wechsel dorthin auch der Sound mitkommt.",
  "locker.tieAction": "„{{sound}}“ mit „{{model}}“ verknüpfen",
  "locker.untieAction": "„{{sound}}“ von „{{model}}“ lösen",
  "locker.restored": "Setup-Dateien von {{bike}} wiederhergestellt.",
  "locker.restoredNote_one":
    "{{count}} Datei zurückgelegt — das Motorrad sollte wieder laden.",
  "locker.restoredNote_other":
    "{{count}} Dateien zurückgelegt — das Motorrad sollte wieder laden.",
  "locker.switchedModel":
    "Modell von {{bike}} auf „{{target}}“ gewechselt.",
  "locker.switchedSound":
    "Sound von {{bike}} auf „{{target}}“ gewechselt.",
  "locker.tied": "„{{sound}}“ mit Modell „{{model}}“ verknüpft.",
  "locker.untied": "„{{sound}}“ von Modell „{{model}}“ gelöst.",
  "locker.refreshedLive": "Live im Spiel aktualisiert.",
  "locker.refreshFailed":
    "Sofortige Aktualisierung fehlgeschlagen — wähle dein Profil im Spiel neu aus, um sie zu laden.",
  "locker.reselectProfile":
    "Wähle dein Profil in MX Bikes neu aus, um den Tausch zu laden.",
  "locker.loadsNextTime":
    "Wird beim nächsten Start des Spiels geladen.",
  "locker.modelRefreshing":
    "Wird im Spiel aktualisiert — wenn es dein ausgewähltes Motorrad ist, ändert es sich jetzt.",
  "locker.modelFrostmodNotRunning":
    "Starte FrostMod, um Modellwechsel live zu sehen — wähle das Motorrad vorerst im Spiel neu aus.",
  "locker.modelReselectBike":
    "Modell gewechselt — wähle das Motorrad in MX Bikes neu aus, um es zu sehen.",
  "locker.modelFrostmodUnreachable":
    "FrostMod war nicht erreichbar — wähle das Motorrad im Spiel neu aus, um es zu laden.",
  "locker.modelRefreshWindowsOnly":
    "Die Live-Modellaktualisierung gibt es nur unter Windows — wähle das Motorrad im Spiel neu aus.",
  "locker.modelInstantRefreshOff":
    "Wähle das Motorrad in MX Bikes neu aus, um es zu laden (die sofortige Aktualisierung ist aus).",

  // ── Registrierung loser Sets ───────────────────────────────────────────────
  "swaps.model": "Modell",
  "swaps.modelSets_one": "{{count}} Modellwechsel",
  "swaps.modelSets_other": "{{count}} Modellwechsel",
  "swaps.soundSets_one": "{{count}} Sound-Mod",
  "swaps.soundSets_other": "{{count}} Sound-Mods",
  "swaps.and": "{{a}} und {{b}}",
  "swaps.noSets": "0 Sets",
  "swaps.foundTitle": "{{summary}} gefunden",
  "swaps.description":
    "Diese Ordner liegen lose in deinen Motorrädern. Registriere sie, um jeden in die richtige Bibliothek zu verschieben — {{modelsFolder}} für Modelle, {{soundsFolder}} für Sounds — damit sie im Spind auftauchen.",
  "swaps.registered_one": "{{count}} Set registriert.",
  "swaps.registered_other": "{{count}} Sets registriert.",
  "swaps.nothingMoved": "Es wurde nichts verschoben.",
  "swaps.skipped_one": "{{count}} übersprungen (Name bereits vergeben).",
  "swaps.skipped_other":
    "{{count}} übersprungen (Namen bereits vergeben).",
  "swaps.foldersCreated_one":
    "Bibliotheksordner für {{count}} Motorrad erstellt.",
  "swaps.foldersCreated_other":
    "Bibliotheksordner für {{count}} Motorräder erstellt.",
  "swaps.foldersCreatedDesc":
    "Deine Modell-/Sound-Ordner sind dort geblieben, wo sie waren.",
  "swaps.justCreateFolders": "Nur Ordner erstellen",
  "swaps.registerAndMove": "Registrieren und verschieben",
  "swaps.fileCount_one": "{{count}} Datei",
  "swaps.fileCount_other": "{{count}} Dateien",

  // ── Installation ───────────────────────────────────────────────────────────
  "install.installed": "{{title}} installiert",
  "install.reloadedDesc":
    "Spiel über FrostMod neu geladen — es ist jetzt aktiv.",
  "install.addedDesc": "Zu deiner Bibliothek hinzugefügt.",
  "install.failed": "Installation fehlgeschlagen — {{title}}",
  "install.openModPage": "Die Mod-Seite öffnen",
  "install.clickToOpen": "Klicken, um die Mod-Seite zu öffnen",

  // ── Kategorien (Singular) ──────────────────────────────────────────────────
  "category.track": "Strecke",
  "category.bike": "Motorrad",
  "category.bikePaint": "Lackierung",
  "category.bikeModelSwap": "Modellwechsel",
  "category.sound": "Sound",
  "category.helmet": "Helm",
  "category.helmetPaint": "Helm-Design",
  "category.goggles": "Brille",
  "category.boots": "Stiefel",
  "category.bootPaint": "Stiefel-Design",
  "category.protection": "Protektoren",
  "category.protectionPaint": "Protektoren-Design",
  "category.gloves": "Handschuhe",
  "category.outfit": "Outfit / Kit",
  "category.misc": "Sonstiges",

  // ── Abschnittsüberschriften (Plural) ───────────────────────────────────────
  "section.bikePaint": "Lackierungen",
  "section.bikeModelSwap": "Modellwechsel",
  "section.sound": "Sounds",
  "section.helmet": "Helme",
  "section.helmetPaint": "Helm-Designs",
  "section.boots": "Stiefel",
  "section.bootPaint": "Stiefel-Designs",
  "section.protection": "Protektoren",
  "section.protectionPaint": "Protektoren-Designs",
  "section.gloves": "Handschuhe",
  "section.outfit": "Outfit / Kit",

  // ── Installationsziele ─────────────────────────────────────────────────────
  "dest.bikesRoot": "Motorräder (Hauptordner)",
  "dest.tracksRoot": "Strecken (Hauptordner)",
  "dest.bikeFolder": "{{name}} — Motorradordner",
  "dest.bikePaints": "{{name}} — Lackierungen",
  "dest.helmetsNewModel": "Helme (neues Modell)",
  "dest.bootsNewModel": "Stiefel (neues Modell)",
  "dest.protectionNewModel": "Protektoren (neues Modell)",
  "dest.helmetPaintsFor": "{{name}} · Helm-Designs",
  "dest.gogglesFor": "{{name}} · Brille",
  "dest.bootPaintsFor": "{{name}} · Stiefel-Designs",
  "dest.protectionPaintsFor": "{{name}} · Protektoren-Designs",
  "dest.outfitFor": "{{name}} · Outfit / Kit",
  "dest.glovesFor": "{{name}} · Handschuhe",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "In-Game-Overlay",
  "overlay.enable": "In-Game-Overlay aktivieren",
  "overlay.enableDesc": "Drücke ein Tastenkürzel, während MX Bikes läuft, um Presets, Locker und Browse über dem Spiel zu öffnen — ohne Alt-Tab. Presets und Modellwechsel greifen im laufenden Spiel.",
  "overlay.shortcut": "Overlay-Tastenkürzel",
  "overlay.shortcutDesc": "Funktioniert auch, wenn das Spiel den Fokus hat. Esc schließt das Overlay und gibt die Steuerung zurück.",
  "overlay.borderlessTitle": "Spiele MX Bikes randlos oder im Fenster",
  "overlay.borderlessNote": "Über einem Spiel, das den Bildschirm im exklusiven Vollbild hält, lässt sich nichts zeichnen — auch das Overlay nicht. Stelle MX Bikes unter Options → Video auf Borderless (oder Windowed), dann erscheint es wie erwartet über dem Spiel.",
  "overlay.gameRunning": "MX Bikes läuft",
  "overlay.gameNotRunning": "MX Bikes läuft nicht",
  "overlay.showNow": "Overlay jetzt zeigen",
  "overlay.showFailed": "Overlay ließ sich nicht öffnen",
  "overlay.hotkeyTaken": "Eine andere App benutzt dieses Kürzel",
  "overlay.hotkeyTakenDesc": "Die Kombination bekommt die App, die sie zuerst angemeldet hat — das Overlay öffnet sich deshalb nie. Wähle oben eine andere; meist ist es Discords Stummschaltung.",
  "overlay.fullscreenNow": "MX Bikes läuft gerade im exklusiven Vollbild",
  "overlay.fullscreenNowDesc": "Das Overlay öffnet sich trotzdem — das Spiel wird nur darüber gezeichnet. Wechsle unter Options → Video auf randlos oder Fenstermodus.",
  "overlay.notWorking": "Gedrückt und nichts passiert?",
  "overlay.notWorkingDesc": "Prüfe das Kürzel oben: eine andere App hat diese Kombination womöglich schon, und eine freie zu wählen ist die Lösung.",
  "overlay.pressKeys": "Tasten drücken…",
  "overlay.needModifier": "Modifikator hinzufügen",
  "overlay.needModifierDesc": "Halte Ctrl, Alt oder Shift, damit das Kürzel nicht beim Tippen auslöst.",
  "overlay.shortcutUpdated": "Overlay-Tastenkürzel aktualisiert",
  "overlay.shortcutRejected": "Dieses Tastenkürzel geht nicht",
  "overlay.registerFailed": "Overlay-Tastenkürzel konnte nicht registriert werden",
  "overlay.toClose": "{{hotkey}} zum Schließen",
  "overlay.closeTitle": "Overlay schließen (Esc)",
  "overlay.openMain": "Vollständige App öffnen",
  "overlay.openMainTitle": "Overlay schließen und das Hauptfenster von MXB App öffnen",
  "overlay.needsSetup": "Richte MXB App zuerst im Hauptfenster fertig ein — sie muss wissen, wo dein MX-Bikes-Ordner liegt.",
  "overlay.fullscreenBlocked": "Das Overlay kann nicht über exklusivem Vollbild erscheinen",
  "overlay.fullscreenBlockedDesc": "Stelle MX Bikes unter Options → Video auf randlos oder Fenstermodus und drücke das Kürzel erneut.",

  // Release-Vorstellung — das „Neu"-Fenster, das einmal nach einem Update erscheint.
  "showcase.eyebrow": "Gerade aktualisiert",
  "showcase.title": "Neu in {{version}}",
  "showcase.subtitle": "Das Große zuerst. Alles andere aus dieser Version steht in den Notes.",
  "showcase.whileGameRunning": "während MX Bikes läuft",
  "showcase.releaseNotes": "Release Notes lesen",
  "showcase.gotIt": "Alles klar",
  "showcase.v070.hero.title": "Ein Overlay im Spiel, auf einem Kürzel",
  "showcase.v070.hero.body": "Holt Preset, Locker und Browse über MX Bikes — ohne Alt-Tab. Esc gibt die Kontrolle sofort zurück, und ein hier gewähltes Preset landet in der Session, die du gerade fährst. Spiele randlos oder im Fenster: über exklusivem Vollbild lässt sich nichts zeichnen.",
  "showcase.v070.hero.action": "Overlay einrichten",
  "showcase.v070.languages": "MXB App spricht sechs Sprachen — wähle deine unter Einstellungen → Darstellung.",
  "showcase.v070.browse": "Browse sortiert nach den beliebtesten Mods, und die Karten zeigen Sternebewertungen.",
  "showcase.v070.play": "Ein Play-Button in der Seitenleiste startet MX Bikes.",
  "showcase.v070.paint": "Bikes tragen wieder ihr richtiges Design — Kawasaki KX und Yamaha YZ sind repariert.",
  "manage.help":
    "MX Bikes lädt beim Start jede Mod in deinem Ordner. Gib einem Preset die Strecke, auf der es fährt, klick auf Rennmodus, und alles andere tritt beiseite — gelöscht wird nichts, es wandert nur in einen Parkordner, bis du es zurückholst.",
  "manage.tabRace": "Rennpresets",
  "manage.tabMods": "Mods",
  "manage.disabledCount_one": "{{count}} Mod deaktiviert",
  "manage.disabledCount_other": "{{count}} Mods deaktiviert",
  "manage.restoreAll": "Alles aktivieren",
  "manage.restoreTitle": "Alle Mods zurückholen?",
  "manage.restoreBody":
    "Alle {{count}} deaktivierten Mods kehren in genau die Ordner zurück, aus denen sie kamen. MX Bikes lädt sie dann wieder alle.",
  "manage.restored_one": "{{count}} Mod zurückgeholt.",
  "manage.restored_other": "{{count}} Mods zurückgeholt.",
  "manage.applyLookTo": "Look anwenden auf",
  "manage.applyLookHelp":
    "Der Rennmodus schreibt Lackierung und Ausrüstung des Presets auf dieses Profil und dieses Bike — genau wie der Presets-Tab. Lass eines davon leer, um nur die Inhalte zu verschieben, ohne deinen Look anzufassen.",
  "manage.noPresets": "Noch keine gespeicherten Presets — leg zuerst eines im Presets-Tab an.",
  "manage.noContentYet": "Noch keine Renninhalte — füge eine Strecke hinzu, um den Rennmodus zu nutzen",
  "manage.noTrack": "Keine Strecke",
  "manage.pinnedCount_one": "{{count}} angeheftet",
  "manage.pinnedCount_other": "{{count}} angeheftet",
  "manage.editContent": "Inhalte bearbeiten",
  "manage.raceMode": "Rennmodus",
  "manage.raceTitle": "Mit „{{name}}“ fahren?",
  "manage.raceBody":
    "Behält {{keep}} Mods und schiebt {{disable}} beiseite, damit MX Bikes nur die Inhalte dieses Rennens lädt.",
  "manage.raceReEnable_one": "{{count}} deaktivierte Mod, die dieses Preset braucht, kommt zurück.",
  "manage.raceReEnable_other": "{{count}} deaktivierte Mods, die dieses Preset braucht, kommen zurück.",
  "manage.raceLook": "Lackierung und Ausrüstung gehen auf {{bike}} im Profil {{profile}}.",
  "manage.raceNoLook": "Nur Inhalte — wähle oben Profil und Bike, um auch den Look anzuwenden.",
  "manage.raceNoBike":
    "Keine Bike-Mod wird behalten — es blieben nur die Bikes des Spiels. Heft das Bike, das du fährst, unter Immer behalten an.",
  "manage.raceGameRunning":
    "MX Bikes läuft. Dateien, die das Spiel offen hält, lassen sich nicht verschieben — schließ es zuerst.",
  "manage.raceUnresolved": "Nicht installiert, erscheinen also serienmäßig: {{slots}}",
  "manage.raceGo": "Rennen vorbereiten",
  "manage.raceApplied": "Bereit für „{{name}}“ — {{count}} Mods beiseitegeschoben.",
  "manage.contentSaved": "Renninhalte für „{{name}}“ gespeichert.",
  "manage.contentTitle": "Renninhalte für „{{name}}“",
  "manage.contentBody":
    "Lackierung, Ausrüstung und Model-Swap des Presets werden von allein gefunden. Hier steht der Rest: die Strecke, zusätzliche Ausrüstungsmodelle, die bleiben sollen, und die Packs, die ein Rennen ohnehin braucht.",
  "manage.paneTracks": "Strecken",
  "manage.paneHelmets": "Helme",
  "manage.paneBoots": "Stiefel",
  "manage.paneProtection": "Protektoren",
  "manage.paneKeep": "Immer behalten",
  "manage.paneTracksHint": "Die Strecke (oder Strecken), für die dieses Preset gedacht ist.",
  "manage.paneGearHint":
    "Zusätzliche Modelle, die in der Auswahl des Spiels bleiben. Die Ausrüstung des Presets wird ohnehin behalten — hake hier an, worauf du sonst noch zugreifen möchtest. Alles ohne Haken tritt zur Seite.",
  "manage.paneKeepHint":
    "Mods, die aktiv bleiben, egal was sonst passiert — das OEM-Pack, das Bike dieses Presets, eine Sound-Mod.",
  "manage.notInstalled": "nicht installiert",
  "manage.off": "aus",
  "manage.enabledOne": "{{name}} aktiviert.",
  "manage.disabledOne": "{{name}} deaktiviert.",
  "manage.enabledMany_one": "{{count}} Mod aktiviert.",
  "manage.enabledMany_other": "{{count}} Mods aktiviert.",
  "manage.disabledMany_one": "{{count}} Mod deaktiviert.",
  "manage.disabledMany_other": "{{count}} Mods deaktiviert.",
  "manage.enableShown": "Angezeigte aktivieren ({{count}})",
  "manage.disableShown": "Angezeigte deaktivieren ({{count}})",
  "manage.noMods": "Noch keine Mods installiert.",
  "manage.someFailed_one": "{{count}} Mod ließ sich nicht verschieben: {{first}}",
  "manage.someFailed_other": "{{count}} Mods ließen sich nicht verschieben: {{first}}",
  "manage.deleteTitle": "{{name}} löschen?",
  "manage.deleteBody": "Sie landet im Papierkorb, von dort kannst du sie noch zurückholen.",
  "manage.deleted": "{{name}} gelöscht.",
};
