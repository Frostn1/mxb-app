import type { Translation } from "..";

/**
 * Italian.
 *
 * Terminology follows what the MX Bikes community actually says rather than
 * dictionary equivalents: `mod`, `setup`, `preset` and `Stock` stay as-is —
 * they're the words riders use — while gear is translated (`casco`, `stivali`,
 * `maschera` for goggles, `livrea` for a bike paint).
 *
 * Product names (MXB App, FrostMod, MX Bikes) are never translated.
 */
export const it: Translation = {
  // ── Generico ───────────────────────────────────────────────────────────────
  "common.cancel": "Annulla",
  "common.back": "Indietro",
  "common.next": "Avanti",
  "common.skip": "Salta",
  "common.close": "Chiudi",
  "common.save": "Salva",
  "common.delete": "Elimina",
  "common.rename": "Rinomina",
  "common.retry": "Riprova",
  "common.tryAgain": "Riprova",
  "common.loading": "Caricamento…",
  "common.installed": "Installato",
  "common.select": "Seleziona",
  "common.deselect": "Deseleziona",
  "common.selectAll": "Seleziona tutto",
  "common.clear": "Svuota",
  "common.done": "Fatto",
  "common.apply": "Applica",
  "common.remove": "Rimuovi",
  "common.open": "Apri",
  "common.refresh": "Aggiorna",
  "common.dismiss": "Ignora",
  "common.later": "Più tardi",
  "common.active": "Attivo",

  // ── Controlli finestra ─────────────────────────────────────────────────────
  "window.minimize": "Riduci a icona",
  "window.maximize": "Ingrandisci",
  "window.close": "Chiudi",

  // ── Navigazione ────────────────────────────────────────────────────────────
  "nav.browse": "Esplora",
  "nav.shop": "Shop",
  "nav.library": "Libreria",
  "nav.locker": "Armadietto",
  "nav.presets": "Preset",
  "nav.rider": "Pilota",
  "nav.paints": "Livree",
  "nav.servers": "Server",
  "nav.manage": "Gestisci",
  "nav.settings": "Impostazioni",

  "sidebar.installing": "Installazione di “{{name}}”",
  "sidebar.queued": "+{{count}} in coda",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Controllo di FrostMod…",
  "frostmod.running": "FrostMod attivo",
  "frostmod.notRunning": "FrostMod non attivo",
  "frostmod.reloadGame": "Ricarica il gioco",
  "frostmod.start": "Avvia FrostMod",
  "frostmod.reloadedGame": "FrostMod ha ricaricato il gioco.",
  "frostmod.notRunningToast": "FrostMod non è in esecuzione.",
  "frostmod.started": "FrostMod avviato",
  "frostmod.alreadyRunning": "FrostMod è già in esecuzione",
  "frostmod.startFailed": "Impossibile avviare FrostMod",
  "frostmod.stop": "Arresta FrostMod",
  "frostmod.stopped": "FrostMod arrestato",
  "frostmod.stopFailed": "Impossibile arrestare FrostMod",
  "frostmod.stopFailedDesc":
    "È ancora in esecuzione: potrebbe essere stato avviato da un altro utente o con privilegi di amministratore.",
  "frostmod.installedToast": "FrostMod {{version}} installato",
  "frostmod.installedToastDesc":
    "Ricaricherà il gioco in tempo reale quando aggiungi mod.",
  "frostmod.installedToastRestart":
    "Riavvia MX Bikes per passare alla nuova versione — il gioco aperto sta ancora usando il vecchio FrostMod.",
  "frostmod.installFailed": "Impossibile installare FrostMod",
  "frostmod.newModsAdded": "Nuove mod aggiunte",
  "frostmod.modsAdded_one": "Nuova mod aggiunta",
  "frostmod.modsAdded_other": "{{count}} mod aggiunte",
  "frostmod.askedReload": "Richiesto a FrostMod di ricaricare il gioco.",
  "frostmod.andMore_one": "{{names}} e altra {{count}}",
  "frostmod.andMore_other": "{{names}} e altre {{count}}",
  "frostmod.watchDesc":
    "{{names}} — richiesto a FrostMod di ricaricare il gioco.",

  // ── Configurazione iniziale ────────────────────────────────────────────────
  "setup.title": "Benvenuto in MXB App",
  "setup.tagline": "Sfoglia le mod, installale con un clic e torna subito in sella.",
  "setup.modsFolder": "Cartella di {{game}}",
  "setup.autoDetect":
    "MXB App rileverà automaticamente la tua cartella {{hint}}. Puoi anche sceglierla tu.",
  "setup.chooseManually": "Scegli la cartella manualmente…",
  "setup.chooseDifferent": "Scegli un'altra cartella…",
  "setup.gameInstall": "Installazione di {{game}}",
  "setup.detecting": "Ricerca dell'installazione di {{game}}…",
  "setup.found": "Trovata",
  "setup.detectedAutomatically": "Rilevata automaticamente",
  "setup.installNotFound":
    "Non ho trovato automaticamente la tua installazione di {{game}} — serve per l'anteprima 3D del pilota. Scegliela manualmente, oppure impostala più tardi nelle Impostazioni.",
  "setup.chooseInstallManually":
    "Scegli manualmente la cartella d'installazione…",
  "setup.startBrowsing": "Inizia a sfogliare le mod",
  "setup.detectAndStart": "Rileva e inizia",
  "setup.pickModsFolder": "Seleziona la tua cartella di {{game}}",
  "setup.pickInstallFolder": "Seleziona la cartella d'installazione di {{game}}",

  // ── Benvenuto ──────────────────────────────────────────────────────────────
  "welcome.intro.title": "Benvenuto in MXB App",
  "welcome.intro.body":
    "Il tuo gestore di mod per MX Bikes. Tieni piste, moto e grafiche organizzate in un unico posto — niente più file zip sparsi sul desktop. Ti facciamo fare un giro in pochi secondi.",
  "welcome.getStarted": "Iniziamo",

  // ── Preset ─────────────────────────────────────────────────────────────────
  "presets.missing": "mancante",
  "presets.missingHint":
    "Questa mod non è installata — in gioco apparirà come stock",
  "presets.missingMods":
    "Mod mancanti: {{mods}}. Installale per vedere quelle parti.",
  "presets.help":
    "Salva un look completo del pilota e caricalo su una moto quando vuoi.",
  "presets.profile": "Profilo",
  "presets.namePlaceholder": "Nome del preset…",
  "presets.savePreset": "Salva preset",
  "presets.saveChanges": "Salva modifiche",
  "presets.saveChangesQ": "Salvare le modifiche?",
  "presets.replaceQ": "Sostituire il preset?",
  "presets.replace": "Sostituisci",
  "presets.loadCopy": "Carica una copia nell'editor",
  "presets.viewOnRider": "Vedi sul pilota",
  "presets.editNameOrOptions": "Modifica nome o opzioni",
  "presets.share": "Condividi",
  "presets.nameFirst": "Prima dai un nome al preset.",
  "presets.pickProfileAndBike": "Scegli un profilo e una moto a cui applicarlo.",
  "presets.updated": "Preset “{{name}}” aggiornato.",
  "presets.renamed": "Rinominato in “{{name}}” e modifiche salvate.",
  "presets.saved": "Preset “{{name}}” salvato.",
  "presets.editing":
    "Stai modificando “{{name}}” — cambia quello che vuoi, poi salva le modifiche.",
  "presets.appliedRefreshed":
    "“{{label}}” applicato a {{bike}} — aggiornato in tempo reale nel gioco.",
  "presets.appliedRefreshFailed":
    "“{{label}}” applicato a {{bike}} — salvato, ma l'aggiornamento istantaneo non è riuscito: riseleziona il profilo in gioco per caricarlo.",
  "presets.appliedGameRunning":
    "“{{label}}” applicato a {{bike}} — salvato. Riseleziona il profilo in MX Bikes (menu Profilo) per caricare il nuovo look.",
  "presets.appliedNextTime":
    "“{{label}}” applicato a {{bike}} — salvato. Verrà caricato alla prossima apertura del gioco.",
  "presets.appliedReselectBike":
    "“{{label}}” applicato a {{bike}} — le livree sono attive; riseleziona la moto in MX Bikes per vedere il modello.",
  "presets.phaseBundling": "Preparazione dei file…",
  "presets.phaseUploading": "Caricamento del pacchetto…",
  "presets.phaseDownloading": "Download del pacchetto…",
  "presets.phaseInstalling": "Installazione dei file…",
  "presets.bundleUploaded":
    "Pacchetto completo caricato — ora il codice include anche i file.",
  "presets.shareHintFull":
    "Questo codice include un pacchetto scaricabile — chi lo riceve sceglie Importazione completa e ottiene tutto, anche senza mod installate.",
  "presets.shareHintConfig":
    "Manda questo codice a chi vuoi. Lo importa da Preset → Importa. Servono le stesse mod installate perché si veda ogni parte.",
  "presets.generatingCode": "Generazione del codice…",
  "presets.nothingToBundle":
    "Nessun file installato da includere — questo look è tutto stock/font.",
  "presets.createFullBundle": "Crea pacchetto completo",
  "presets.copiedFull": "Codice con pacchetto completo copiato.",
  "presets.copiedShare": "Codice di condivisione copiato.",
  "presets.copyFailed":
    "Impossibile copiare — seleziona il codice e copialo a mano.",
  "presets.copyFullCode": "Copia codice completo",
  "presets.copyCode": "Copia codice",
  "presets.importTitle": "Importa preset",
  "presets.importBody": "Incolla un codice che ti hanno mandato.",
  "presets.configOnly": "Solo configurazione",
  "presets.import": "Importa",
  "presets.fullImport": "Importazione completa",
  "presets.editingBanner":
    "Stai modificando {{name}} — cambia il nome o qualsiasi slot, poi {{save}}.",
  "presets.bundleNotice":
    "Include un pacchetto completo (~{{size}} da {{host}}). Usa {{fullImport}} per scaricare e installare tutto — non serve avere già le mod.",

  // ── Slot dei preset ────────────────────────────────────────────────────────
  "slot.paint": "Livrea moto",
  "slot.modelSwap": "Cambio modello",
  "slot.bikeFont": "Font numeri",
  "slot.tyres": "Gomme",
  "slot.rider": "Profilo pilota",
  "slot.suitPaint": "Completo / kit",
  "slot.suitFont": "Font completo",
  "slot.glovesPaint": "Guanti",
  "slot.ridingStyle": "Stile di guida",
  "slot.helmet": "Casco",
  "slot.helmetPaint": "Grafica casco",
  "slot.gogglesPaint": "Maschera",
  "slot.boots": "Stivali",
  "slot.bootsPaint": "Grafica stivali",
  "slot.protection": "Protezioni",
  "slot.protectionPaint": "Grafica protezioni",
  "slotGroup.bike": "Moto",
  "slotGroup.rider": "Pilota",
  "slotGroup.head": "Testa",
  "slotGroup.body": "Corpo",

  // ── Studio pilota ──────────────────────────────────────────────────────────
  "rider.help":
    "Vesti il modello del pilota — casco, maschera, completo e stivali insieme.",
  "rider.namePlaceholder": "Dai un nome a questo pilota…",
  "rider.nameFirst": "Prima dai un nome a questo look.",
  "rider.showOnModel": "Mostra sul modello",

  // ── Tour guidato ───────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Fai un giro veloce",
  "tour.welcomeTour.body":
    "Bastano pochi secondi per vedere dov'è ogni cosa. Puoi saltare quando vuoi.",
  "tour.browse.title": "Sfoglia le mod",
  "tour.browse.body": "Cerca su {{site}} direttamente da qui e installa piste, moto o livree con un clic.",
  "tour.library.title": "La tua libreria",
  "tour.library.body":
    "Tutto ciò che hai installato, in un unico posto — aggiorna o rimuovi le mod senza mai toccare un file zip.",
  "tour.locker.title": "L'armadietto",
  "tour.locker.body":
    "Cambia i modelli delle moto quando vuoi. MXB App registra i pezzi in modo che il gioco li riconosca.",
  "tour.presets.title": "Preset",
  "tour.presets.body":
    "Salva le combinazioni di equipaggiamento e grafiche, poi applica un look completo con un clic — anche mentre stai guidando.",
  "tour.rider.title": "Studio pilota",
  "tour.rider.body":
    "Guarda l'anteprima del tuo equipaggiamento sul pilota 3D prima di portarlo in pista.",
  "tour.frostmod.title": "FrostMod, in diretta",
  "tour.frostmod.body":
    "Qui vedi lo stato di FrostMod. Ricarica MX Bikes dopo un'installazione, così i nuovi contenuti compaiono senza riavviare il gioco.",
  "tour.settings.title": "Impostazioni",
  "tour.settings.body":
    "Qui imposti la cartella di gioco, il comportamento in background e le opzioni di FrostMod. Da qui puoi anche rivedere questo tour.",
  "tour.done.title": "È tutto pronto",
  "tour.done.body":
    "Il tour finisce qui. Vai su Esplora e installa la tua prima mod.",

  // ── Errori ─────────────────────────────────────────────────────────────────
  "error.previewFailed": "Impossibile mostrare l'anteprima",
  "error.somethingWentWrong": "Qualcosa è andato storto",
  "error.unexpected": "Si è verificato un errore imprevisto.",
  "error.reloadApp": "Ricarica l'app",

  // ── Aggiornamenti ──────────────────────────────────────────────────────────
  "update.available": "{{version}} è disponibile.",
  "update.downloading": "Download in corso…",
  "update.downloadingPct": "Download in corso… {{pct}}%",
  "update.pitch":
    "Aggiorna per avere le ultime funzionalità e correzioni.",
  "update.updating": "Aggiornamento…",
  "update.updateAndRestart": "Aggiorna e riavvia",
  "update.dismiss": "Ignora la notifica di aggiornamento",
  "update.onLatest": "Hai già l'ultima versione",
  "update.checkFailed": "Impossibile controllare gli aggiornamenti",
  "update.failed": "Aggiornamento non riuscito",

  // ── Visualizzatore 3D ──────────────────────────────────────────────────────
  "viewer.preview3d": "Anteprima 3D",
  "viewer.expand": "Ingrandisci",
  "viewer.paint": "Grafica",
  "viewer.loadingModel": "Caricamento modello…",
  "viewer.loadingPaint": "Caricamento grafica…",
  "viewer.loadingRider": "Caricamento pilota…",
  "viewer.riderLoadFailed": "Anteprima non aggiornata — impossibile aggiornarla",
  "viewer.dragToRotate": "Trascina per ruotare",
  "viewer.scrollToZoom": "Scorri per zoomare",
  "viewer.rightDragToPan": "Trascina col destro per spostare",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Cerca…",
  "combobox.use": "Usa “{{value}}”",

  // ── Tipi di mod ────────────────────────────────────────────────────────────
  "modType.tracks": "Piste",
  "modType.bikes": "Moto",
  "modType.rider": "Pilota",
  "modType.tracksInline": "piste",
  "modType.bikesInline": "moto",
  "modType.riderInline": "equipaggiamento pilota",

  // ── Filtri categoria ───────────────────────────────────────────────────────
  "browseCat.all": "Tutto",
  "browseCat.beginner": "Principiante",
  "browseCat.intermediate": "Intermedio",
  "browseCat.pro": "Pro",
  "browseCat.assets": "Risorse",
  "browseCat.newBikes": "Nuove moto",
  "browseCat.liveries": "Livree",
  "browseCat.sounds": "Suoni",
  "browseCat.riderKit": "Kit pilota",
  "browseCat.helmets": "Caschi",
  "browseCat.helmetPaints": "Grafiche casco",
  "browseCat.gloves": "Guanti",
  "browseCat.boots": "Stivali",
  "browseCat.bootPaints": "Grafiche stivali",
  "browseCat.protection": "Protezioni",
  "browseCat.protectionPaints": "Grafiche protezioni",

  // ── Esplora ────────────────────────────────────────────────────────────────
  "browse.help":
    "Scopri e installa mod dal catalogo online — cerca, filtra per tipo e apri una mod per scaricarla nel gioco.",
  "browse.searchPlaceholder": "Cerca {{type}}…",
  "browseSort.newest": "Più recenti",
  "browseSort.oldest": "Meno recenti",
  "browseSort.popularAll": "Più popolari",
  "browseSort.popularMonth": "Popolari questo mese",
  "browseSort.popularWeek": "Popolari questa settimana",
  "browse.loadFailed": "Impossibile caricare le mod",
  "browse.empty": "Nessun risultato per {{type}}.",
  "browse.loadMore": "Carica altre",
  "browse.selectedCount": "{{count}} selezionate",
  "browse.queuing": "Accodamento…",
  "browse.quickInstallCount": "Installa rapidamente {{count}}",
  "browse.quickInstall": "Installazione rapida",
  "browse.quickReinstall": "Reinstallazione rapida",
  "browse.openDetails": "Apri i dettagli",
  "browse.reinstallOne": "Reinstallare “{{title}}”?",
  "browse.reinstallMany": "Reinstallare le mod che hai già?",
  "browse.reinstallOneBody":
    "Questa mod è già nella tua libreria. Reinstallandola verrà scaricata di nuovo e i file installati saranno sovrascritti.",
  "browse.reinstallManyBody":
    "{{installed}} delle {{total}} selezionate sono già installate. Continuando verranno reinstallate e sovrascritte.",
  "browse.reinstall": "Reinstalla",
  "browse.reinstallAll": "Reinstalla tutto",
  "browse.queued": "“{{title}}” in coda",
  "browse.queuedDesc": "Installazione in {{folder}}.",
  "browse.rootFolder": "principale",
  "browse.needsBrowser": "“{{title}}” va scaricata dal browser",
  "browse.needsBrowserDesc":
    "{{host}} blocca i download nell'app — apri la sua pagina per completare.",
  "browse.noDownload": "Nessun download trovato per “{{title}}”",
  "browse.quickInstallFailed":
    "Impossibile installare rapidamente “{{title}}”",
  "browse.queuedBulk_one": "{{count}} mod in coda",
  "browse.queuedBulk_other": "{{count}} mod in coda",
  "browse.queuedBulkDesc": "Verranno installate una dopo l'altra.",
  "browse.queuedBulkSkipped_one": "{{count}} saltata — host solo browser.",
  "browse.queuedBulkSkipped_other": "{{count}} saltate — host solo browser.",
  "browse.bulkFailed": "Impossibile installare rapidamente la selezione",
  "browse.bulkFailedDesc_one": "Va scaricata dal browser.",
  "browse.bulkFailedDesc_other":
    "Tutte e {{count}} vanno scaricate dal browser.",

  // ── Shop ───────────────────────────────────────────────────────────────────
  "shop.myDownloads": "I miei download",
  "shop.signInTitle": "Accedi a MX Bikes Shop",
  "shop.signInBody":
    "Accedi a mxbikes-shop.com per vedere e installare le piste che hai acquistato. Apriamo il sito vero — la tua password non passa mai da questa app.",
  "shop.signIn": "Accedi",
  "shop.logOut": "Esci",
  "shop.signedIn": "Accesso a MX Bikes Shop effettuato",
  "shop.sessionFailed":
    "Impossibile recuperare la tua sessione MX Bikes Shop",
  "shop.queuedDesc": "Installazione nella tua cartella piste.",
  "shop.loadFailed": "Impossibile caricare i tuoi download: {{error}}",
  "shop.empty": "Nessun download acquistato trovato sul tuo account.",
  // ── Catalogo MX Bikes Shop (solo consultazione; si acquista sul sito) ──────
  "shopCatalog.title": "Negozio",
  "shopCatalog.help":
    "Esplora il catalogo di mxbikes-shop.com: cerca, filtra e confronta i prezzi. L'acquisto e il download avvengono comunque sul sito del negozio; questa app ti mostra soltanto cosa c'è.",
  "shopCatalog.searchPlaceholder": "Cerca nel negozio…",
  "shopCatalog.allCategories": "Tutto",
  "shopCatalog.onSaleOnly": "In offerta",
  "shopCatalog.loadMore": "Carica altri",
  "shopCatalog.loadFailed": "Impossibile caricare il catalogo del negozio",
  "shopCatalog.empty": "Nel negozio non c'è nulla che corrisponda.",
  "shopCatalog.viewDetails": "Vedi dettagli",
  "shopCatalog.openOnStore": "Apri su mxbikes-shop.com",
  "shopCatalog.buyOnStore": "Acquista su mxbikes-shop.com",
  "shopCatalog.buyNote": "Si apre nel browser. Acquisto e download avvengono sul negozio.",
  "shopCatalog.noProductLink": "Questo articolo non ha una pagina prodotto che possiamo aprire.",
  "shopCatalog.noScreenshots": "Nessuno screenshot",
  "shopCatalog.about": "Informazioni su questo articolo",
  "shopCatalog.author": "Autore",
  "shopCatalog.category": "Categoria",
  "shopCatalog.updated": "Aggiornato",
  "shopCatalog.priceUnknown": "Prezzo non indicato",
  "shopCatalog.free": "Gratis",
  "shopCatalog.refresh": "Aggiorna",
  "shopCatalog.refreshing": "Aggiornamento…",
  "shopCatalog.stale": "Prezzi controllati l'ultima volta {{when}}.",
  "shopCatalog.staleHard":
    "Questi prezzi sono stati controllati l'ultima volta {{when}} e potrebbero non essere aggiornati. Aggiorna prima di fidartene.",
  "shopCatalog.saleEndsDays_one": "L'offerta finisce tra 1 giorno",
  "shopCatalog.saleEndsDays_other": "L'offerta finisce tra {{count}} giorni",
  "shopCatalog.saleEndsHours_one": "L'offerta finisce tra 1 ora",
  "shopCatalog.saleEndsHours_other": "L'offerta finisce tra {{count}} ore",
  "shopCatalog.saleEndsSoon": "L'offerta finisce presto",
  "shopCatalog.agoJustNow": "proprio ora",
  "shopCatalog.agoUnknown": "un po' di tempo fa",
  "shopCatalog.agoMinutes_one": "1 minuto fa",
  "shopCatalog.agoMinutes_other": "{{count}} minuti fa",
  "shopCatalog.agoHours_one": "1 ora fa",
  "shopCatalog.agoHours_other": "{{count}} ore fa",
  "shopCatalog.agoDays_one": "1 giorno fa",
  "shopCatalog.agoDays_other": "{{count}} giorni fa",
  "shopSort.newest": "Più recenti",
  "shopSort.recentlyUpdated": "Aggiornati di recente",
  "shopSort.priceAsc": "Prezzo: dal più basso",
  "shopSort.priceDesc": "Prezzo: dal più alto",
  "shopSort.onSale": "Prima le offerte",
  "shopSort.nameAsc": "Nome (A–Z)",

  // ── Finestra d'installazione ───────────────────────────────────────────────
  "installDialog.installTo": "Installa in",
  "installDialog.installToFolder": "Installa in {{folder}}",
  "installDialog.change": "Cambia",
  "installDialog.searchBikes": "Cerca moto…",
  "installDialog.searchFolders": "Cerca cartelle…",
  "installDialog.probably": "Probabilmente",
  "installDialog.allFolders": "Tutte le cartelle",
  "installDialog.noFolderMatch":
    "Nessuna cartella corrisponde — creala qui sotto.",
  "installDialog.rememberedFor": "Ricordato per {{type}}",
  "installDialog.downloadFrom": "Scarica da",
  "installDialog.downloadPerBike": "Download (per moto)",
  "installDialog.opensInBrowser":
    "Si apre nel browser — MXB App completa l'installazione",
  "installDialog.matchedBike": "Abbinato alla tua moto",
  "installDialog.differentBike": "Moto / pacchetto diverso",
  "installDialog.directFastest": "Diretto · il più veloce",
  "installDialog.direct": "Diretto",
  "installDialog.perBikeHint":
    "Ogni download è una moto diversa — selezionato automaticamente in base alla tua scelta. Scegli il pacchetto “all bikes” per averle tutte in una volta.",
  "installDialog.mirrorsHint":
    "Tutti i mirror contengono lo stesso file. Se uno non funziona, prova il successivo.",

  // ── Dettagli libreria ──────────────────────────────────────────────────────
  "libraryDetail.author": "Autore",
  "libraryDetail.length": "Lunghezza",
  "libraryDetail.altitude": "Altitudine",
  "libraryDetail.location": "Località",
  "libraryDetail.type": "Tipo",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Appartiene a",
  "libraryDetail.format": "Formato",
  "libraryDetail.extractedFolder": "Cartella estratta",
  "libraryDetail.paintFile": "File grafica",
  "libraryDetail.packagedPkz": "Pacchetto .pkz",
  "libraryDetail.size": "Dimensione",
  "libraryDetail.folder": "Cartella",
  "libraryDetail.lockedWord": "bloccata",
  "libraryDetail.lockedWithMeta":
    "Questa pista è {{locked}} dal suo creatore. Nome, dettagli e anteprima sono visibili qui, ma i file restano sigillati — non può essere estratta né vista in 3D.",
  "libraryDetail.lockedNoMeta":
    "Questa pista è {{locked}}, quindi nome, lunghezza e anteprima non si possono leggere dal file — solo nome file e dimensione.",

  // ── Pagina mod ─────────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Risoluzione",
  "modDetail.stageDownload": "Download",
  "modDetail.stageExtract": "Estrazione",
  "modDetail.stagePlace": "Posizionamento",
  "modDetail.stageReload": "Ricarica",
  "modDetail.modFiles": "File mod",
  "modDetail.copied": "Copiato",
  "modDetail.copy": "Copia",
  "modDetail.addToLibrary": "Aggiungi alla libreria",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Installa in",
  "modDetail.noDownloadLink": "Nessun link di download trovato in questa pagina — aprila su {{site}}.",
  "modDetail.frostmodHint":
    "FrostMod ricaricherà l'elenco {{kind}} al termine.",
  "modDetail.kindRider": "pilota",
  "modDetail.kindBike": "moto",
  "modDetail.kindTrack": "piste",
  "modDetail.details": "Dettagli",
  "modDetail.format": "Formato",
  "modDetail.mirrors": "Mirror",
  "modDetail.type": "Tipo",
  "modDetail.addedToLibrary": "Aggiunta alla tua libreria",
  "modDetail.extracting": "Estrazione…",
  "modDetail.addingToLibrary": "Aggiunta alla libreria…",
  "modDetail.resolving": "Risoluzione del download…",
  "modDetail.finishInBrowser": "Completa nel browser",
  "modDetail.viewOnSite": "Apri su {{site}}",

  // ── Impostazioni ───────────────────────────────────────────────────────────
  "settings.help":
    "Configura la cartella di gioco, gli aggiornamenti e le preferenze dell'app.",
  "settings.gameFolder": "Cartella di gioco",
  "settings.general": "Generali",
  "settings.appearance": "Aspetto",
  "settings.frostmod": "FrostMod",
  "settings.about": "Info e aggiornamenti",
  "settings.whatsNew": "Novità",
  "settings.modsFolderDesc":
    "Dove vengono installate le mod. Scegli la cartella che contiene le cartelle mods e profiles \u2014 quella sopra mods, non la cartella mods stessa. Cambiandola, la libreria viene riscansionata.",
  "settings.insideModsFolder": "Dentro la tua cartella {{game}}",
  "settings.notSet": "Non impostata",
  "settings.selectFolderFor": "Seleziona una cartella per {{game}}",
  "settings.gameDesc":
    "Quale titolo sta gestendo MXB App. Cartelle, libreria e preset appartengono tutti al gioco che scegli qui.",
  "settings.change": "Cambia…",
  "settings.set": "Imposta…",
  "settings.theme": "Tema",
  "settings.themeLight": "Chiaro",
  "settings.themeDark": "Scuro",
  "settings.themeSystem": "Sistema",
  "settings.language": "Lingua",
  "settings.languageSystem": "Sistema",
  "settings.runInBackground": "Continua in background",
  "settings.runInBackgroundDesc":
    "Chiudendo la finestra, MXB App resta nella barra di sistema così FrostMod rimane collegato. Esci dall'icona nella barra.",
  "settings.launchAtStartup": "Avvia all'accensione",
  "settings.launchAtStartupDesc":
    "Avvia MXB App automaticamente quando accedi.",
  "settings.instantRefresh": "Aggiornamento preset istantaneo",
  "settings.instantRefreshDesc":
    "Quando applichi un preset mentre {{game}} è in esecuzione, aggiorna il look in gioco all'istante — senza riavvio né riselezione del profilo. Se non ci riesce, ti verrà chiesto di riselezionare il profilo.",
  "settings.instantRefreshWindowsOnly":
    "Aggiornare il look in gioco senza riavviare richiede FrostMod, che è solo per Windows — ti verrà invece chiesto di riselezionare il profilo.",
  "settings.autoRunFrostmod": "Avvia FrostMod automaticamente",
  "settings.autoRunFrostmodDesc":
    "Avvia FrostMod in background ogni volta che apri MXB App.",
  "settings.watchModsReload": "Ricarica automatica alle modifiche",
  "settings.watchModsReloadDesc":
    "Ricarica il gioco automaticamente quando piste o moto vengono aggiunte alla cartella mod — anche se scaricate manualmente fuori da MXB App.",
  "settings.checking": "Controllo…",
  "settings.runningConnected": "In esecuzione · gioco collegato",
  "settings.notRunning": "Non in esecuzione",
  "settings.frostmodInstalled": "Installato{{suffix}}",
  "settings.notInstalled": "Non installato",
  "settings.checkingGitHub": "Controllo dell'ultima release su GitHub…",
  "settings.updateCheckFailed":
    "Impossibile controllare gli aggiornamenti — offline o GitHub non raggiungibile.",
  "settings.latestVersion": "Ultima: {{version}}",
  "settings.frostmodNeedsRepair":
    "I file installati non corrispondono a questa versione — reinstallando si risolve.",
  "settings.frostmodRepair": "Ripara installazione",
  "settings.frostmodUnsupportedForGame":
    "Questa versione di FrostMod non è sicura su {{game}} — aggiornala per usare FrostMod qui.",
  "settings.frostmodUpdateRequired": "Aggiornamento necessario",
  "settings.checkNewer": "Cerca una versione più recente di FrostMod",
  "settings.working": "Elaborazione…",
  "settings.installFrostmod": "Installa FrostMod",
  "settings.updateTo": "Aggiorna a {{version}}",
  "settings.reinstallLatest": "Reinstalla l'ultima",
  "settings.upToDate": "Aggiornato",
  "settings.madeWith": "Fatto con",
  "settings.updateFailed": "Impossibile aggiornare l'impostazione",
  "settings.startupUpdateFailed":
    "Impossibile aggiornare l'avvio automatico",
  "settings.folderUpdated": "Cartella di gioco aggiornata",
  "settings.folderUpdatedDesc": "La tua libreria verrà riscansionata.",
  "settings.folderUsedParent":
    "Quella era la cartella mods \u2014 è stata usata la cartella sopra: {{folder}}",
  "settings.setFolderFailed": "Impossibile impostare la cartella",
  "settings.reDetected": "Cartella {{game}} rilevata di nuovo",
  "settings.detectFolderFailed": "Impossibile rilevare la cartella",
  "settings.pickInstallFolder":
    "Seleziona la cartella d'installazione di {{game}} (contiene rider.pkz)",
  "settings.installSet": "Installazione di gioco impostata",
  "settings.installSetDesc":
    "L'anteprima 3D del pilota può ora caricare il modello reale del corpo.",
  "settings.setInstallFailed":
    "Impossibile impostare la cartella d'installazione",
  "settings.installNotFound": "Impossibile trovare {{game}}",
  "settings.installNotFoundDesc":
    "Nessuna installazione Steam rilevata — imposta la cartella manualmente.",
  "settings.installFound": "Installazione di {{game}} trovata",
  "settings.detectInstallFailed":
    "Impossibile rilevare la cartella d'installazione",
  "settings.pickProfilesFolder":
    "Seleziona la cartella dei profili di {{game}}",
  "settings.profilesSet": "Cartella dei profili impostata",
  "settings.profilesFound_one": "Trovato {{count}} profilo.",
  "settings.profilesFound_other": "Trovati {{count}} profili.",
  "settings.noProfilesThere": "Nessun profilo trovato lì",
  "settings.noProfilesThereDesc":
    "Salvata comunque, ma per creare preset serve una cartella che contenga le cartelle dei tuoi profile.ini.",
  "settings.setProfilesFailed":
    "Impossibile impostare la cartella dei profili",
  "settings.profilesReverted":
    "Ripristinata la cartella dei profili predefinita",
  "settings.resetProfilesFailed":
    "Impossibile reimpostare la cartella dei profili",
  "settings.frostmodNotRunningHint":
    "FrostMod non è in esecuzione — avvialo per ricaricare le mod a caldo.",
  "settings.reloadUnavailable":
    "La ricarica non è disponibile su questa piattaforma.",

  // ── Avvio del gioco ────────────────────────────────────────────────────────
  "game.play": "Gioca",
  "game.starting": "Avvio…",
  "game.running": "{{game}} in esecuzione",
  "game.launch": "Avvia {{game}}",
  "game.alreadyRunning": "{{game}} è già in esecuzione",
  "game.launching": "Avvio di {{game}}…",
  "game.launchFailed": "Impossibile avviare {{game}}",
  "join.title": "Entra in un server",
  "join.desc":
    "Inserisci l'indirizzo di un server per avviare {{game}} collegandoti direttamente.",
  "join.address": "Indirizzo del server",
  "join.action": "Entra",
  "join.joining": "Connessione…",
  "join.launching": "Connessione a {{address}}…",
  "join.alreadyRunning":
    "Chiudi prima {{game}} — un gioco già avviato non può essere collegato a un server.",
  "join.failed": "Impossibile entrare in quel server",

  "servers.title": "Server",
  "servers.subtitle":
    "Gestisci i server dedicati che ospiti. Su ognuno serve l'agent MXB installato.",
  "servers.empty": "Ancora nessun server. Aggiungine uno per gestirlo da qui.",
  "servers.add": "Aggiungi un server",
  "servers.remove": "Rimuovi questo server",
  "servers.namePlaceholder": "Nome del server",
  "servers.tokenPlaceholder": "Token dell'agent",
  "servers.track": "Pista",
  "servers.slots": "Posti",
  "servers.uptime": "Attivo da",
  "servers.restarts": "Riavvii",
  "servers.stopped": "Fermo",
  "servers.start": "Avvia",
  "servers.stop": "Ferma",
  "servers.restart": "Riavvia",
  "servers.setTrack": "Imposta pista",
  "servers.trackPlaceholder": "ID pista",
  "servers.actionDone": "Fatto",
  "servers.actionFailed": "Non ha funzionato",
  "servers.trackChanged": "Pista impostata su {{track}} — il server è stato riavviato.",
  "servers.saveFailed": "Impossibile salvare l'elenco dei server",

  "settings.experimental": "Sperimentale",
  "settings.experimentalServers": "Server e sincronizzazione livree",
  "settings.experimentalServersDesc":
    "Non finito. Aggiunge la scheda Server, ti permette di gestire server dedicati e sincronizza le livree perché tutti sul server si vedano correttamente.",
  "settings.experimentalForced":
    "Attivato per questa sessione da MXB_EXPERIMENTAL — l'impostazione non ha effetto finché non lo rimuovi.",
  "settings.betaBadge": "Beta",

  "sync.title": "Sincronizzazione livree",
  "sync.desc":
    "MX Bikes non invia mai le livree, quindi gli altri piloti appaiono con quelle di serie se non hai già il loro file esatto. Pubblica la tua e scarica quelle degli altri.",
  "sync.enroll": "Registrati",
  "sync.enrolled": "Registrato come {{name}}",
  "sync.enrollFailed": "Registrazione non riuscita",
  "sync.codePlaceholder": "Codice invito",
  "sync.riderNamePlaceholder": "Nome pilota in gioco",
  "sync.riderNameHint":
    "Deve corrispondere esattamente al tuo nome pilota in MX Bikes — è così che le app degli altri sanno quali livree sono tue.",
  "sync.ridingAs": "Pubblichi come {{name}}",
  "sync.pull": "Sincronizza livree",
  "sync.setGuid": "Salva GUID",
  "sync.guidPlaceholder": "Il tuo GUID di MX Bikes",
  "sync.guidHint":
    "Il tuo GUID di MX Bikes (facoltativo). Ti identifica anche se cambi nome pilota, e il server lo registra a ogni connessione.",
  "sync.guidSaved": "GUID salvato",
  "sync.pulled": "Installate {{installed}} da {{riders}} piloti ({{had}} già presenti)",
  "sync.pullFailed": "Sincronizzazione non riuscita",
  "sync.rejected": "Saltate {{count}} con una destinazione non sicura",

  // ── Stringhe sfuggite alla prima scansione (JSX su più righe) ──────────────
  "libraryDetail.noEmbedded": "Nessun dettaglio incorporato trovato per questo elemento.",
  "modDetail.downloadFromHost": "Scarica da {{host}}",
  "modDetail.openHost": "Apri {{host}}",
  "modDetail.thenAddFile": "Poi aggiungi il file",
  "modDetail.chooseDownloaded": "Scegli il file scaricato",
  "presets.chooseProfilesFolder": "Scegli la cartella dei profili…",
  "presets.viewInRider": "Vedi nel Pilota",
  "presets.noModelSwapsHere": "Nessun cambio modello registrato per questa moto —",
  "presets.setUpInLocker": "impostali nell'Armadietto",
  "presets.makeActiveBike": "Rendi questa la moto attiva",
  "presets.nameClash":
    "Esiste già un altro preset chiamato “{{name}}” — salvando sovrascriverai anche quello.",
  "presets.shareWarning":
    "Carica su un link pubblico e temporaneo — ridistribuisce file di mod fatti da altri, quindi condividi con criterio.",
  "settings.profilesDesc":
    "I preset leggono i tuoi profili da qui — il percorso qui sotto è quello che l'app sta usando adesso. È la cartella {{profiles}} dentro la tua cartella {{game}}, oppure {{documents}} se hai spostato la cartella delle mod. Impostalo solo se il tuo è altrove.",
  "settings.resetToDefault": "Ripristina il predefinito",
  "settings.gameInstallDesc":
    "Cartella d'installazione del gioco (facoltativa) — dove è installato {{game}} (contiene {{file}}). Impostala per caricare il corpo reale del pilota nell'anteprima 3D.",
  "viewer.stockGearNote":
    "Mostrato sul {{part}} stock del gioco. Una grafica fatta per un altro modello potrebbe non combaciare alla perfezione.",
  "viewer.paintNoChange":
    "Nessuna delle texture di questa grafica è usata dalle parti mostrate qui, quindi l'anteprima non cambia. Potrebbe comunque colorare ruote o catena, che questa vista non mostra.",
  "viewer.noPaintPreview": "Nessuna anteprima della grafica ({{err}})",

  // ── Libreria ───────────────────────────────────────────────────────────────
  "library.help":
    "Le tue mod installate. Controlla cosa è installato e rimuovi ciò che non ti serve più.",
  "library.rootFolder": "(principale)",
  "library.byAuthor": "di {{author}}",
  "library.locked": "Bloccato — il contenuto non è leggibile",
  "library.searchPlaceholder": "Cerca tra le installate…",
  "library.scanning": "Scansione della libreria…",
  "library.empty":
    "Nessuna mod {{type}} installata — vai su Esplora e aggiungine una.",
  "library.noMatches": "Nessun risultato.",
  "library.quick3d": "Vedi in 3D",
  "library.selectNone": "Deseleziona tutto",
  "library.move": "Sposta",
  "library.uninstall": "Disinstalla",
  "library.uninstallAction": "Disinstalla…",
  "library.moveToFolder": "Sposta nella cartella…",
  "library.showInExplorer": "Mostra in Esplora file",
  "library.moveDialogTitle": "Sposta nella cartella",
  "library.moveCount_one": "Sposta {{count}} elemento",
  "library.moveCount_other": "Sposta {{count}} elementi",
  "library.chooseDestination": "Scegli una cartella di destinazione",
  "library.newFolder": "Nuova cartella…",
  "library.newFolderName": "Nome della nuova cartella",
  "library.createAndMove": "Crea e sposta",
  "library.confirmUninstall": "Disinstallare {{name}}?",
  "library.confirmUninstallBody":
    "L'elemento viene spostato nel Cestino — puoi ripristinarlo da lì.",
  "library.confirmBulkUninstall_one": "Disinstallare {{count}} elemento?",
  "library.confirmBulkUninstall_other": "Disinstallare {{count}} elementi?",
  "library.confirmBulkUninstallBody":
    "Ogni elemento viene spostato nel Cestino — puoi ripristinarli da lì.",
  "library.uninstallCount": "Disinstalla {{count}}",
  "library.moveFailed": "Impossibile spostare la mod",
  "library.uninstallFailed": "Impossibile disinstallare",
  "library.openFailed": "Impossibile aprire",
  "library.uninstalledOne": "{{name}} disinstallata",
  "library.movedToBin": "Spostata nel Cestino.",
  "library.someNotRemoved": "Alcuni elementi non sono stati rimossi.",
  "library.bulkUninstalled_one": "{{count}} elemento disinstallato",
  "library.bulkUninstalled_other": "{{count}} elementi disinstallati",
  "library.bulkUninstallPartial": "Disinstallati {{ok}}, {{fail}} falliti",
  "library.bulkMovePartial": "Spostati {{ok}}, {{fail}} falliti",
  "library.bulkMoved_one": "Spostato {{count}} elemento in {{folder}}",
  "library.bulkMoved_other": "Spostati {{count}} elementi in {{folder}}",

  // ── Armadietto ─────────────────────────────────────────────────────────────
  "locker.help":
    "Cambia il modello e il suono del motore di ogni moto tra i set che hai installato.",
  "locker.rescan": "Riscansiona",
  "locker.restore": "Ripristina",
  "locker.register": "Registra",
  "locker.scanning": "Scansione delle moto…",
  "locker.scanForSwaps": "Cerca set da scambiare",
  "locker.orphanBanner":
    "A {{bike}} mancano i file di setup — una versione precedente li ha spostati in una cartella di swap, e questo impedisce del tutto il caricamento della moto in gioco. {{files}}",
  "locker.looseBanner_one":
    "{{count}} set modello / suono trovato sparso tra le tue moto — registralo in {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} set modello / suono trovati sparsi tra le tue moto — registrali in {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "Ancora nessuna moto scambiabile.",
  "locker.emptyIntro":
    "Servono due condizioni prima che uno scambio sia possibile:",
  "locker.unpacked": "estratta",
  "locker.emptyRuleUnpacked":
    "La moto è {{unpacked}} in {{path}}— un {{pkz}} compresso non può essere scambiato. Estraine una dalla Libreria.",
  "locker.emptyRuleMesh":
    "Ogni modello alternativo sta nella sua cartella dentro quella moto e contiene una mesh ({{edf}}). Mettila ovunque nella cartella della moto e premi Cerca qui sotto — ti proporremo di archiviarla in {{folder}}.",
  "locker.summary": "{{model}} · suono “{{sound}}”",
  "locker.modelNamed": "modello “{{name}}”",
  "locker.noModelSwaps": "nessun cambio modello",
  "locker.models": "Modelli",
  "locker.sounds": "Suoni",
  "locker.onlyOneModel": "Un solo modello — installane altri per scambiare",
  "locker.onlyStock":
    "Solo Stock — installa una mod audio per scambiare",
  "locker.noModel": "Nessun modello",
  "locker.stock": "Stock",
  "locker.activeModel": "Modello attivo",
  "locker.activeSound": "Suono attivo",
  "locker.switchToNoModel":
    "Passa a nessun modello — rimuove i file del modello attuale",
  "locker.switchToStock":
    "Passa a Stock — rimuove la mod audio (torna il suono originale)",
  "locker.missingModelEdf": "Questo set non ha model.edf",
  "locker.missingSoundFiles": "A questo set mancano engine.scl o sfx.cfg",
  "locker.switchTo": "Passa a {{name}}",
  "locker.tiedToModel": "Legato al modello {{models}}",
  "locker.boundHint":
    "“{{sound}}” è legato al modello “{{model}}” — segue quel modello. Clicca per slegarlo.",
  "locker.unboundHint":
    "Lega il suono attivo “{{sound}}” al modello “{{model}}” così passando a quel modello arriva anche il suono.",
  "locker.tieAction": "Lega “{{sound}}” a “{{model}}”",
  "locker.untieAction": "Slega “{{sound}}” da “{{model}}”",
  "locker.restored": "Ripristinati i file di setup di {{bike}}.",
  "locker.restoredNote_one":
    "{{count}} file rimesso a posto — la moto dovrebbe caricarsi di nuovo.",
  "locker.restoredNote_other":
    "{{count}} file rimessi a posto — la moto dovrebbe caricarsi di nuovo.",
  "locker.switchedModel":
    "Modello di {{bike}} cambiato in “{{target}}”.",
  "locker.switchedSound": "Suono di {{bike}} cambiato in “{{target}}”.",
  "locker.tied": "“{{sound}}” legato al modello “{{model}}”.",
  "locker.untied": "“{{sound}}” slegato dal modello “{{model}}”.",
  "locker.refreshedLive": "Aggiornato in tempo reale nel gioco.",
  "locker.refreshFailed":
    "Aggiornamento istantaneo fallito — riseleziona il profilo in gioco per caricarlo.",
  "locker.reselectProfile":
    "Riseleziona il tuo profilo in MX Bikes per caricare lo scambio.",
  "locker.loadsNextTime":
    "Verrà caricato alla prossima apertura del gioco.",
  "locker.modelRefreshing":
    "Aggiornamento in gioco — se è la moto che hai selezionata, cambia adesso.",
  "locker.modelFrostmodNotRunning":
    "Avvia FrostMod per vedere i cambi modello in tempo reale — per ora riseleziona la moto in gioco.",
  "locker.modelReselectBike":
    "Modello cambiato — riseleziona la moto in MX Bikes per vederlo.",
  "locker.modelFrostmodUnreachable":
    "Impossibile raggiungere FrostMod — riseleziona la moto in gioco per caricarla.",
  "locker.modelRefreshWindowsOnly":
    "L'aggiornamento del modello in tempo reale è solo per Windows — riseleziona la moto in gioco.",
  "locker.modelInstantRefreshOff":
    "Riseleziona la moto in MX Bikes per caricarla (l'aggiornamento istantaneo è disattivato).",

  // ── Registrazione set sparsi ───────────────────────────────────────────────
  "swaps.model": "modello",
  "swaps.modelSets_one": "{{count}} cambio modello",
  "swaps.modelSets_other": "{{count}} cambi modello",
  "swaps.soundSets_one": "{{count}} mod audio",
  "swaps.soundSets_other": "{{count}} mod audio",
  "swaps.and": "{{a}} e {{b}}",
  "swaps.noSets": "0 set",
  "swaps.foundTitle": "Trovati {{summary}}",
  "swaps.description":
    "Queste cartelle sono sparse dentro le tue moto. Registrale per spostarle ciascuna nella libreria giusta — {{modelsFolder}} per i modelli, {{soundsFolder}} per i suoni — così compaiono nell'Armadietto.",
  "swaps.registered_one": "Registrato {{count}} set.",
  "swaps.registered_other": "Registrati {{count}} set.",
  "swaps.nothingMoved": "Non è stato spostato nulla.",
  "swaps.skipped_one": "{{count}} saltato (nome già in uso).",
  "swaps.skipped_other": "{{count}} saltati (nomi già in uso).",
  "swaps.foldersCreated_one":
    "Create le cartelle di libreria per {{count}} moto.",
  "swaps.foldersCreated_other":
    "Create le cartelle di libreria per {{count}} moto.",
  "swaps.foldersCreatedDesc":
    "Le tue cartelle modello / suono sono rimaste dove sono.",
  "swaps.justCreateFolders": "Crea solo le cartelle",
  "swaps.registerAndMove": "Registra e sposta",
  "swaps.fileCount_one": "{{count}} file",
  "swaps.fileCount_other": "{{count}} file",

  // ── Installazione ──────────────────────────────────────────────────────────
  "install.installed": "{{title}} installata",
  "install.reloadedDesc":
    "Gioco ricaricato tramite FrostMod — è già attiva.",
  "install.addedDesc": "Aggiunta alla tua libreria.",
  "install.failed": "Installazione fallita — {{title}}",
  "install.openModPage": "Apri la pagina della mod",
  "install.clickToOpen": "Clicca per aprire la pagina della mod",

  // ── Categorie (singolare) ──────────────────────────────────────────────────
  "category.track": "Pista",
  "category.bike": "Moto",
  "category.bikePaint": "Livrea",
  "category.bikeModelSwap": "Cambio modello",
  "category.sound": "Suono",
  "category.helmet": "Casco",
  "category.helmetPaint": "Grafica casco",
  "category.goggles": "Maschera",
  "category.boots": "Stivali",
  "category.bootPaint": "Grafica stivali",
  "category.protection": "Protezioni",
  "category.protectionPaint": "Grafica protezioni",
  "category.gloves": "Guanti",
  "category.outfit": "Completo / kit",
  "category.misc": "Altro",

  // ── Intestazioni di sezione (plurale) ──────────────────────────────────────
  "section.bikePaint": "Livree",
  "section.bikeModelSwap": "Cambi modello",
  "section.sound": "Suoni",
  "section.helmet": "Caschi",
  "section.helmetPaint": "Grafiche casco",
  "section.boots": "Stivali",
  "section.bootPaint": "Grafiche stivali",
  "section.protection": "Protezioni",
  "section.protectionPaint": "Grafiche protezioni",
  "section.gloves": "Guanti",
  "section.outfit": "Completo / kit",

  // ── Destinazioni d'installazione ───────────────────────────────────────────
  "dest.bikesRoot": "Moto (principale)",
  "dest.tracksRoot": "Piste (principale)",
  "dest.bikeFolder": "{{name}} — cartella moto",
  "dest.bikePaints": "{{name}} — grafiche",
  "dest.helmetsNewModel": "Caschi (nuovo modello)",
  "dest.bootsNewModel": "Stivali (nuovo modello)",
  "dest.protectionNewModel": "Protezioni (nuovo modello)",
  "dest.riderModelsNew": "Modelli pilota (nuovo modello)",
  "dest.animationsNewStyle": "Stili di guida (nuova animazione)",
  "dest.helmetPaintsFor": "{{name}} · grafiche casco",
  "dest.gogglesFor": "{{name}} · maschera",
  "dest.bootPaintsFor": "{{name}} · grafiche stivali",
  "dest.protectionPaintsFor": "{{name}} · grafiche protezioni",
  "dest.outfitFor": "{{name}} · completo / kit",
  "dest.suitPaintsFor": "{{name}} · grafiche tuta",
  "dest.glovesFor": "{{name}} · guanti",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay in gioco",
  "overlay.enable": "Attiva l'overlay in gioco",
  "overlay.enableDesc": "Premi una scorciatoia mentre {{game}} è in esecuzione per aprire Preset, Locker e Browse sopra il gioco — senza alt-tab. I preset e i cambi modello si applicano al gioco in esecuzione.",
  "overlay.shortcut": "Scorciatoia overlay",
  "overlay.shortcutDesc": "Funziona anche quando il gioco ha il focus. Esc chiude l'overlay e restituisce il controllo.",
  "overlay.borderlessTitle": "Avvia {{game}} senza bordi o in finestra",
  "overlay.borderlessNote": "Nulla può essere disegnato sopra un gioco che tiene lo schermo in fullscreen esclusivo — overlay compreso. Imposta {{game}} su Borderless (o Windowed) in Options → Video e l'overlay comparirà sopra il gioco come previsto.",
  "overlay.gameRunning": "{{game}} è in esecuzione",
  "overlay.gameNotRunning": "{{game}} non è in esecuzione",
  "overlay.showNow": "Mostra l'overlay ora",
  "overlay.showFailed": "Impossibile aprire l'overlay",
  "overlay.hotkeyTaken": "Un'altra app sta usando questa scorciatoia",
  "overlay.hotkeyTakenDesc": "La combinazione va all'app che l'ha richiesta per prima, quindi l'overlay non si apre mai. Scegline un'altra qui sopra — di solito è il mute di Discord.",
  "overlay.fullscreenNow": "{{game}} è in fullscreen esclusivo in questo momento",
  "overlay.fullscreenNowDesc": "L'overlay si apre lo stesso — è il gioco a essere disegnato sopra. Passa a senza bordi o finestra in Options → Video.",
  "overlay.notWorking": "L'hai premuta e non è successo nulla?",
  "overlay.notWorkingDesc": "Controlla la scorciatoia qui sopra: un'altra app potrebbe già avere quella combinazione, e sceglierne una libera è ciò che risolve.",
  "overlay.pressKeys": "Premi i tasti…",
  "overlay.needModifier": "Aggiungi un modificatore",
  "overlay.needModifierDesc": "Tieni premuto Ctrl, Alt o Shift, così la scorciatoia non scatta mentre scrivi.",
  "overlay.shortcutUpdated": "Scorciatoia overlay aggiornata",
  "overlay.shortcutRejected": "Impossibile usare questa scorciatoia",
  "overlay.registerFailed": "Impossibile registrare la scorciatoia dell'overlay",
  "overlay.toClose": "{{hotkey}} per chiudere",
  "overlay.closeTitle": "Chiudi overlay (Esc)",
  "overlay.openMain": "Apri l'app completa",
  "overlay.openMainTitle": "Chiudi l'overlay e apri la finestra principale di MXB App",
  "overlay.needsSetup": "Completa prima la configurazione di MXB App nella finestra principale — deve sapere dov'è la tua cartella {{game}}.",
  "overlay.fullscreenBlocked": "L'overlay non può apparire sopra il fullscreen esclusivo",
  "overlay.fullscreenBlockedDesc": "Imposta {{game}} senza bordi o in finestra in Options → Video, poi riprova con la scorciatoia.",

  // Vetrina della release — la finestra "novità" mostrata una volta dopo un aggiornamento.
  "showcase.eyebrow": "Appena aggiornato",
  "showcase.title": "Novità della {{version}}",
  "showcase.subtitle": "Prima la novità grossa. Tutto il resto della release è nelle note.",
  "showcase.whileGameRunning": "mentre MX Bikes è in esecuzione",
  "showcase.releaseNotes": "Leggi le note di rilascio",
  "showcase.gotIt": "Ho capito",
  "showcase.v080.hero.title": "MXB App gestisce anche GP Bikes",
  "showcase.v080.hero.body":
    "Scegli il gioco al primo avvio, o cambialo quando vuoi nelle Impostazioni: tutta l'app lo segue — Libreria, Gestisci, Preset, Play e una scheda Sfoglia servita da gpb-mods.com. Le cartelle pilota di GP vengono lette come quelle di GP, non di MX Bikes, e anche lì FrostMod ricarica al volo. Ogni gioco tiene le proprie cartelle, quindi la tua configurazione di MX Bikes resta intatta.",
  "showcase.v080.shop":
    "La scheda Shop naviga mxbikes-shop.com e installa ciò che hai acquistato, senza uscire dall'app.",
  "showcase.v080.dropzone":
    "Trascina qualsiasi cosa sulla finestra. Capisce cos'è ogni file, mostra dove finirà e cosa sostituirebbe, e ti lascia ricollocare ogni riga prima di installare.",
  "showcase.v080.destinations":
    "I mod finiscono nella cartella che il gioco legge davvero — una livrea sulla sua moto, una grafica casco sul suo casco, una tuta GP sul tuo modello pilota.",
  "showcase.v080.protection":
    "Lo slot protezioni funziona: ogni pezzo disegnato dritto e completo, e installato dove il gioco lo cerca.",
  "showcase.v080.faster":
    "Le anteprime sono in cache e disegnate alla dimensione mostrata, così Sfoglia e Shop si aprono molto più in fretta.",
  "showcase.v070.hero.title": "Un overlay in gioco, su una scorciatoia",
  "showcase.v070.hero.body": "Apre Preset, Locker e Browse sopra MX Bikes — senza alt-tab. Esc restituisce subito il controllo e un preset scelto qui arriva sulla sessione che stai già guidando. Gioca senza bordi o in finestra: sopra il fullscreen esclusivo non si può disegnare nulla.",
  "showcase.v070.hero.action": "Configura l'overlay",
  "showcase.v070.languages": "MXB App parla sei lingue — scegli la tua in Impostazioni → Aspetto.",
  "showcase.v070.browse": "Browse ordina per più popolari e le schede mostrano le stelle di valutazione.",
  "showcase.v070.play": "Un pulsante Play nella barra laterale avvia MX Bikes.",
  "showcase.v070.paint": "Le moto tornano a indossare la livrea giusta — Kawasaki KX e Yamaha YZ sono sistemate.",
  "manage.help":
    "MX Bikes carica ogni mod della cartella all'avvio. Assegna a un preset la pista su cui corre, premi Modalità gara e tutto il resto si fa da parte — niente viene eliminato, si sposta solo in una cartella di sosta finché non lo riporti indietro.",
  "manage.tabRace": "Preset gara",
  "manage.tabMods": "Mod",
  "manage.disabledCount_one": "{{count}} mod disattivata",
  "manage.disabledCount_other": "{{count}} mod disattivate",
  "manage.restoreAll": "Riattiva tutto",
  "manage.restoreTitle": "Rimettere a posto tutte le mod?",
  "manage.restoreBody":
    "Tutte le {{count}} mod disattivate tornano esattamente nelle cartelle da cui sono partite. MX Bikes le caricherà di nuovo tutte.",
  "manage.restored_one": "Riportata {{count}} mod.",
  "manage.restored_other": "Riportate {{count}} mod.",
  "manage.applyLookTo": "Applica l'aspetto a",
  "manage.applyLookHelp":
    "La modalità gara scrive livrea e attrezzatura del preset su questo profilo e questa moto, come fa la scheda Preset. Lascia vuoto uno dei due per spostare solo i contenuti senza toccare il tuo aspetto.",
  "manage.noPresets": "Nessun preset salvato — creane uno nella scheda Preset.",
  "manage.noContentYet": "Nessun contenuto gara — aggiungi una pista per usare la modalità gara",
  "manage.noTrack": "Nessuna pista",
  "manage.pinnedCount_one": "{{count}} fissata",
  "manage.pinnedCount_other": "{{count}} fissate",
  "manage.editContent": "Modifica contenuti",
  "manage.raceMode": "Modalità gara",
  "manage.raceTitle": "Correre con “{{name}}”?",
  "manage.raceBody":
    "Mantiene {{keep}} mod e ne sposta {{disable}}, così MX Bikes carica solo i contenuti di questa gara.",
  "manage.raceReEnable_one": "{{count}} mod disattivata che serve a questo preset torna attiva.",
  "manage.raceReEnable_other": "{{count}} mod disattivate che servono a questo preset tornano attive.",
  "manage.raceLook": "Livrea e attrezzatura vanno su {{bike}} nel profilo {{profile}}.",
  "manage.raceNoLook": "Solo contenuti — scegli sopra profilo e moto per applicare anche l'aspetto.",
  "manage.raceNoBike":
    "Nessuna mod moto verrà mantenuta — resteresti con le moto di serie del gioco. Fissa la moto che usi in Sempre attive.",
  "manage.raceGameRunning":
    "MX Bikes è aperto. I file che tiene in uso non si possono spostare — chiudi prima il gioco.",
  "manage.raceUnresolved": "Non installati, quindi resteranno di serie: {{slots}}",
  "manage.raceGo": "Prepara la gara",
  "manage.raceApplied": "Pronto a correre “{{name}}” — {{count}} mod messe da parte.",
  "manage.contentSaved": "Contenuti gara salvati per “{{name}}”.",
  "manage.contentTitle": "Contenuti gara di “{{name}}”",
  "manage.contentBody":
    "Livrea, attrezzatura e model swap del preset vengono trovati da soli. Qui va il resto: la pista, i modelli di attrezzatura da tenere in più e i pacchetti che una gara richiede comunque.",
  "manage.paneTracks": "Piste",
  "manage.paneHelmets": "Caschi",
  "manage.paneBoots": "Stivali",
  "manage.paneProtection": "Protezioni",
  "manage.paneKeep": "Sempre attive",
  "manage.paneTracksHint": "La pista (o le piste) per cui è pensato questo preset.",
  "manage.paneGearHint":
    "Modelli extra da lasciare nel selettore del gioco. L'attrezzatura del preset viene mantenuta da sola: spunta qui ciò che vuoi ancora poter scegliere. Tutto ciò che resta non spuntato si fa da parte.",
  "manage.paneKeepHint":
    "Mod da tenere attive qualunque cosa accada — il pacchetto OEM, la moto di questo preset, una mod audio.",
  "manage.notInstalled": "non installata",
  "manage.off": "off",
  "manage.enabledOne": "{{name}} attivata.",
  "manage.disabledOne": "{{name}} disattivata.",
  "manage.enabledMany_one": "Attivata {{count}} mod.",
  "manage.enabledMany_other": "Attivate {{count}} mod.",
  "manage.disabledMany_one": "Disattivata {{count}} mod.",
  "manage.disabledMany_other": "Disattivate {{count}} mod.",
  "manage.enableShown": "Attiva le visibili ({{count}})",
  "manage.disableShown": "Disattiva le visibili ({{count}})",
  "manage.noMods": "Nessuna mod installata.",
  "manage.someFailed_one": "{{count}} mod non si è potuta spostare: {{first}}",
  "manage.someFailed_other": "{{count}} mod non si sono potute spostare: {{first}}",
  "manage.deleteTitle": "Eliminare {{name}}?",
  "manage.deleteBody": "Finisce nel cestino, quindi puoi ancora recuperarla da lì.",
  "manage.deleted": "{{name}} eliminata.",
  "game.label": "Gioco",
  "game.switch": "Cambia gioco",
  "game.switchFailed": "Impossibile cambiare gioco",
  "settings.instantRefreshMxOnly": "Solo MX Bikes — {{game}} non ricarica i profili a caldo.",
  "modType.misc": "Varie",
  "modType.miscInline": "extra",
  "browseCat.raceTracks": "Circuiti",
  "browseCat.kartTracks": "Piste di kart",
  "browseCat.others": "Altro",
  "browseCat.riderModels": "Modelli pilota",
  "browseCat.suitPaints": "Livree tuta",
  "browseCat.helmetModels": "Modelli casco",
  "browseCat.plugins": "Plugin",
  "browseCat.tools": "Strumenti",
  "browseCat.menuBackgrounds": "Sfondi del menu",
  "category.animation": "Stile di guida",
  "section.animation": "Stili di guida",
  "modDetail.restartHint": "Riavvia {{game}} per rilevare i nuovi {{kind}}.",
  "modDetail.protonHint": "I file di Proton Drive sono cifrati, quindi non possono essere scaricati automaticamente.",
  "setup.whichGame": "Quale gioco stai configurando? Potrai aggiungere l'altro più avanti.",
  "setup.switchLater": "Puoi cambiare gioco quando vuoi nelle Impostazioni.",
  "setup.chooseDifferentGame": "Scegli un altro gioco",
  // ── Dropzone ───────────────────────────────────────────────────────────────
  "drop.dropHere": "Rilascia per installare",
  "drop.dropHint": "Archivi, .pkz, grafiche, cartelle — qualsiasi cosa di {{game}}",
  "drop.scanning": "Sto capendo di cosa si tratta…",
  "drop.found_one": "Trovato {{count}} elemento",
  "drop.found_other": "Trovati {{count}} elementi",
  "drop.reviewHint": "Controlla le destinazioni, poi installa.",
  "drop.install_one": "Installa {{count}}",
  "drop.install_other": "Installa {{count}}",
  "drop.fileCount_one": "{{count}} file",
  "drop.fileCount_other": "{{count}} file",
  "drop.replaces_one": "Sostituisce {{count}} file esistente",
  "drop.replaces_other": "Sostituisce {{count}} file esistenti",
  "drop.willReplace_one": "{{count}} file esistente verrà sostituito",
  "drop.willReplace_other": "{{count}} file esistenti verranno sostituiti",
  "drop.nothingOverwritten": "Non verrà sostituito nulla di esistente.",
  "drop.needChoice_one": "{{count}} elemento ha ancora bisogno di una destinazione",
  "drop.needChoice_other": "{{count}} elementi hanno ancora bisogno di una destinazione",
  "drop.skipped_one": "{{count}} file saltato",
  "drop.skipped_other": "{{count}} file saltati",
  "drop.pickDestinationFirst": "Scegli dove va prima di installare.",
  "drop.chooseDestination": "Scegli una destinazione",
  "drop.searchDestinations": "Cerca moto e attrezzatura…",
  "drop.noDestinations": "Non c'è ancora nulla di installato su cui metterlo.",
  "drop.destAsPackaged": "Com'è impacchettato",
  "drop.include": "Includi questo elemento",
  "drop.exclude": "Lascia fuori questo elemento",
  "drop.installed_one": "Installato {{count}} elemento",
  "drop.installed_other": "Installati {{count}} elementi",
  "drop.itemFailed": "Impossibile installare {{name}}",
  "drop.installFailed": "Installazione non riuscita",
  "drop.scanFailed": "Impossibile leggere ciò che hai rilasciato",
  "drop.previewFailed": "Impossibile controllare quella destinazione",
  "drop.nothingUsable": "Niente di installabile in quel rilascio",
  "drop.kind.modsTree": "Cartella mods",
  "drop.kind.track": "Pista",
  "drop.kind.bike": "Moto",
  "drop.kind.bikePaint": "Grafica",
  "drop.kind.soundSet": "Suono",
  "drop.kind.riderGear": "Attrezzatura",
  "drop.kind.reshadePreset": "Preset ReShade",
  "drop.kind.unknown": "Sconosciuto",
  "drop.reason.modsTree": "Contiene una cartella mods completa",
  "drop.reason.categoryDirs": "Contiene cartelle moto/piste/pilota",
  "drop.reason.paintsBundle": "Contiene una cartella paints",
  "drop.reason.soundMarkers": "Trovati engine.scl e sfx.cfg",
  "drop.reason.trackMarkers": "Trovati file di pista",
  "drop.reason.trackPackage": "Pista impacchettata",
  "drop.reason.bikeConfig": "Trovata una configurazione moto",
  "drop.reason.loosePaint": "Grafiche sciolte — nulla dice di quale modello siano",
  "drop.reason.gearFolders": "Trovate cartelle di attrezzatura",
  "drop.reason.riderTexture": "Colora il corpo del pilota — una tuta",
  "drop.reason.gearTexture": "Colora un pezzo di attrezzatura",
  "drop.reason.reshadePreset": "Elenca tecniche ReShade",
  "drop.reason.unrecognised": "Non riconosciuto — dovrai collocarlo tu",

  // ── ReShade ────────────────────────────────────────────────────────────────
  "settings.reshade": "ReShade",
  "settings.reshadeDesc": "Preset di post-processing — l'aspetto di {{game}} a schermo.",
  "modType.reshade": "ReShade",
  "modType.reshadeInline": "preset ReShade",
  "reshade.needsGameFolder":
    "Imposta la cartella di {{game}} qui sopra e i preset ReShade compariranno qui.",
  "reshade.intro":
    "ReShade aggiunge il post-processing a {{game}}. È uno strumento gratuito a parte: installalo una volta, poi scegli un preset qui.",
  "reshade.wrongApi":
    "ReShade è installato come {{dll}}, che {{game}} non carica mai — usa OpenGL. Riavvia l'installer di ReShade e scegli OpenGL.",
  "reshade.step1": "Scarica l'installer da reshade.me.",
  "reshade.step2": "Avvialo e seleziona {{exe}} nella cartella di {{game}}.",
  "reshade.step3": "Scegli OpenGL quando te lo chiede — non DirectX.",
  "reshade.getIt": "Scarica ReShade",
  "reshade.recheck": "Ricontrolla",
  "reshade.installed": "Installato",
  "reshade.installedVersion": "Installato · {{version}}",
  "reshade.off": "Off — nessun effetto",
  "reshade.delete": "Elimina preset",
  "reshade.deleted": "{{name}} eliminato",
  "reshade.applied": "{{name}} è ora attivo",
  "reshade.appliedNextLaunch": "{{name}} è impostato — si applica al prossimo avvio",
  "reshade.loosePreset": "Nella tua cartella di gioco — non installato da MXB App",
  "reshade.missingEffects_one": "Richiede {{list}}, che non è installato",
  "reshade.missingEffects_other": "Richiede {{count}} effetti non installati: {{list}}",
  "reshade.noShaders":
    "Nessun effetto ReShade è installato, quindi i preset resteranno senza effetto. Riavvia l'installer di ReShade e scegli un pacchetto di shader.",
  "reshade.noPresets":
    "Ancora nessun preset — installane da Sfoglia, o trascina qui un .ini.",
  "reshade.browseHint": "Altri preset in Sfoglia → ReShade.",
  "reshade.nextLaunchHint":
    "{{game}} è in esecuzione — la modifica si applica al prossimo avvio.",
  // ── Paint studio ───────────────────────────────────────────────────────────
  "paints.help":
    "Trasforma file .tga o .png disegnati in GIMP o Photoshop in un .pnt che il gioco carica — e scompatta una livrea esistente da cui partire.",
  "paints.unpack": "Scompatta una livrea…",
  "paints.unpacked": "Estratte {{count}} texture — modificale, poi salva.",
  "paints.whereTitle": "Dove va",
  "paints.kind.bike": "Livrea moto",
  "paints.kind.helmet": "Casco",
  "paints.kind.goggles": "Maschera",
  "paints.kind.boots": "Stivali",
  "paints.kind.protection": "Protezioni",
  "paints.kind.kit": "Completo pilota",
  "paints.kind.gloves": "Guanti",
  "paints.model": "Per",
  "paints.profile": "Profilo pilota",
  "paints.noModels": "Non c'è ancora nulla da dipingere.",
  "paints.destPath": "Installa in mods/{{rel}}",
  "paints.saveElsewhere": "Salva invece in una cartella…",
  "paints.saveTitle": "Nome e salvataggio",
  "paints.namePlaceholder": "Dai un nome a questa livrea…",
  "paints.save": "Salva livrea",
  "paints.saved": "Salvata in {{path}}",
  "paints.preview3d": "Anteprima 3D",
  "paints.openFolder": "Apri cartella",
  "paints.sheetsTitle": "Texture",
  "paints.reload": "Ricarica dal disco",
  "paints.addImages": "Aggiungi immagini…",
  "paints.expected": "Le livree qui usano:",
  "paints.empty":
    "Aggiungi un .tga o .png per ogni texture. Contano i nomi, non i file: una texture chiamata “livery” finisce sulla parte che chiede “livery”. Scompattando una livrea esistente ottieni i nomi giusti.",
  "paints.resized": "Ridimensionata {{from}} → {{to}} — il gioco richiede potenze di due.",
  "paints.unknownName": "Nessuna livrea qui usa questo nome: potrebbe non comparire sul modello.",
  "paints.needSheets": "Aggiungi almeno un'immagine.",
  "paints.needName": "Dai un nome a questa livrea.",
  "paints.needTextureNames": "Ogni texture ha bisogno di un nome.",
  "paints.duplicateName": "Due texture si chiamano “{{name}}”.",
  "paints.needTarget": "Scegli dove va la livrea.",
  "paints.replaceTitle": "Sostituire questa livrea?",
  "paints.replaceBody": "{{path}} esiste già. Salvando la sostituisci.",
  "paints.replace": "Sostituisci",
};
