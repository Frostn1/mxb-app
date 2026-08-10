import type { Translation } from "..";

/**
 * French.
 *
 * Community terminology rather than dictionary equivalents: `mod`, `setup`,
 * `preset` and `Stock` stay as loanwords (that's what riders say), while gear is
 * translated — `casque`, `bottes`, `masque` for goggles, `déco` for a paint.
 *
 * Note the plural forms: French `_one` covers **0 and 1**, which `Intl.PluralRules`
 * handles for us — so "0 fichier" (singular) comes out correct without a special case.
 *
 * Product names (MXB App, FrostMod, MX Bikes) are never translated.
 */
export const fr: Translation = {
  // ── Générique ──────────────────────────────────────────────────────────────
  "common.cancel": "Annuler",
  "common.back": "Retour",
  "common.next": "Suivant",
  "common.skip": "Passer",
  "common.close": "Fermer",
  "common.save": "Enregistrer",
  "common.delete": "Supprimer",
  "common.rename": "Renommer",
  "common.retry": "Réessayer",
  "common.tryAgain": "Réessayer",
  "common.loading": "Chargement…",
  "common.installed": "Installé",
  "common.select": "Sélectionner",
  "common.deselect": "Désélectionner",
  "common.selectAll": "Tout sélectionner",
  "common.clear": "Effacer",
  "common.done": "Terminé",
  "common.apply": "Appliquer",
  "common.remove": "Retirer",
  "common.open": "Ouvrir",
  "common.refresh": "Actualiser",
  "common.dismiss": "Ignorer",
  "common.later": "Plus tard",
  "common.active": "Actif",

  // ── Contrôles de fenêtre ───────────────────────────────────────────────────
  "window.minimize": "Réduire",
  "window.maximize": "Agrandir",
  "window.close": "Fermer",

  // ── Navigation ─────────────────────────────────────────────────────────────
  "nav.browse": "Parcourir",
  "nav.shop": "Boutique",
  "nav.library": "Bibliothèque",
  "nav.locker": "Casier",
  "nav.presets": "Presets",
  "nav.rider": "Pilote",
  "nav.designer": "Designer",
  "nav.paints": "Décos",
  "nav.studio": "Studio",
  "nav.servers": "Serveurs",
  "nav.manage": "Gérer",
  "nav.settings": "Réglages",

  "sidebar.installing": "Installation de « {{name}} »",
  "sidebar.queued": "+{{count}} en attente",
  "sidebar.expand": "Développer la barre latérale",
  "sidebar.collapse": "Réduire la barre latérale",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Vérification de FrostMod…",
  "frostmod.running": "FrostMod actif",
  "frostmod.notRunning": "FrostMod inactif",
  "frostmod.reloadGame": "Recharger le jeu",
  "frostmod.start": "Démarrer FrostMod",
  "frostmod.reloadedGame": "FrostMod a rechargé le jeu.",
  "frostmod.notRunningToast": "FrostMod n'est pas en cours d'exécution.",
  "frostmod.started": "FrostMod démarré",
  "frostmod.alreadyRunning": "FrostMod est déjà en cours d'exécution",
  "frostmod.startFailed": "Impossible de démarrer FrostMod",
  "frostmod.stop": "Arrêter FrostMod",
  "frostmod.stopped": "FrostMod arrêté",
  "frostmod.stopFailed": "Impossible d'arrêter FrostMod",
  "frostmod.stopFailedDesc":
    "Il est toujours en cours d'exécution : il a peut-être été démarré par un autre utilisateur ou avec des droits administrateur.",
  "frostmod.installedToast": "FrostMod {{version}} installé",
  "frostmod.installedToastDesc":
    "Il rechargera le jeu à chaud dès que vous ajouterez des mods.",
  "frostmod.installedToastRestart":
    "Redémarrez MX Bikes pour en profiter — le jeu en cours utilise encore l'ancien FrostMod.",
  "frostmod.installFailed": "Impossible d'installer FrostMod",
  "frostmod.newModsAdded": "Nouveaux mods ajoutés",
  "frostmod.modsAdded_one": "Nouveau mod ajouté",
  "frostmod.modsAdded_other": "{{count}} mods ajoutés",
  "frostmod.askedReload": "Demande de rechargement envoyée à FrostMod.",
  "frostmod.andMore_one": "{{names}} et {{count}} autre",
  "frostmod.andMore_other": "{{names}} et {{count}} autres",
  "frostmod.watchDesc":
    "{{names}} — demande de rechargement envoyée à FrostMod.",

  // ── Configuration initiale ─────────────────────────────────────────────────
  "setup.title": "Bienvenue dans MXB App",
  "setup.tagline": "Parcourez les mods, installez-les en un clic et remontez vite en selle.",
  "setup.modsFolder": "Dossier {{game}}",
  "setup.autoDetect":
    "MXB App détectera automatiquement votre dossier {{hint}}. Vous pouvez aussi le choisir vous-même.",
  "setup.chooseManually": "Choisir le dossier manuellement…",
  "setup.chooseDifferent": "Choisir un autre dossier…",
  "setup.gameInstall": "Installation de {{game}}",
  "setup.detecting": "Recherche de votre installation de {{game}}…",
  "setup.found": "Trouvée",
  "setup.detectedAutomatically": "Détectée automatiquement",
  "setup.installNotFound":
    "Impossible de trouver automatiquement votre installation de {{game}} — elle alimente l'aperçu 3D du pilote. Choisissez-la manuellement, ou définissez-la plus tard dans les Réglages.",
  "setup.chooseInstallManually":
    "Choisir le dossier d'installation manuellement…",
  "setup.startBrowsing": "Commencer à parcourir les mods",
  "setup.detectAndStart": "Détecter et commencer",
  "setup.pickModsFolder": "Sélectionnez votre dossier {{game}}",
  "setup.pickInstallFolder": "Sélectionnez le dossier d'installation de {{game}}",

  // ── Bienvenue ──────────────────────────────────────────────────────────────
  "welcome.intro.title": "Bienvenue dans MXB App",
  "welcome.intro.body":
    "Votre gestionnaire de mods pour MX Bikes. Gardez circuits, motos et décos organisés au même endroit — fini les fichiers zip éparpillés sur le bureau. On vous fait faire le tour en quelques secondes.",
  "welcome.getStarted": "C'est parti",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "manquant",
  "presets.missingHint":
    "Ce mod n'est pas installé — il apparaîtra en Stock dans le jeu",
  "presets.missingMods":
    "Mods manquants : {{mods}}. Installez-les pour voir ces éléments.",
  "presets.help":
    "Enregistrez un look de pilote complet et chargez-le sur une moto à la demande.",
  "presets.profile": "Profil",
  "presets.namePlaceholder": "Nom du preset…",
  "presets.savePreset": "Enregistrer le preset",
  "presets.saveChanges": "Enregistrer les modifications",
  "presets.saveChangesQ": "Enregistrer les modifications ?",
  "presets.replaceQ": "Remplacer le preset ?",
  "presets.replace": "Remplacer",
  "presets.loadCopy": "Charger une copie dans l'éditeur",
  "presets.viewOnRider": "Voir sur le pilote",
  "presets.editNameOrOptions": "Modifier le nom ou les options",
  "presets.share": "Partager",
  "presets.nameFirst": "Donnez d'abord un nom au preset.",
  "presets.pickProfileAndBike":
    "Choisissez un profil et une moto sur lesquels l'appliquer.",
  "presets.updated": "Preset « {{name}} » mis à jour.",
  "presets.renamed":
    "Renommé en « {{name}} » et modifications enregistrées.",
  "presets.saved": "Preset « {{name}} » enregistré.",
  "presets.editing":
    "Modification de « {{name}} » — changez ce que vous voulez, puis enregistrez.",
  "presets.appliedRefreshed":
    "« {{label}} » appliqué à {{bike}} — actualisé en direct dans le jeu.",
  "presets.appliedRefreshFailed":
    "« {{label}} » appliqué à {{bike}} — enregistré, mais l'actualisation instantanée a échoué : resélectionnez votre profil en jeu pour le charger.",
  "presets.appliedGameRunning":
    "« {{label}} » appliqué à {{bike}} — enregistré. Resélectionnez votre profil dans MX Bikes (menu Profil) pour charger le nouveau look.",
  "presets.appliedNextTime":
    "« {{label}} » appliqué à {{bike}} — enregistré. Il sera chargé à la prochaine ouverture du jeu.",
  "presets.appliedReselectBike":
    "« {{label}} » appliqué à {{bike}} — les décos sont en place ; resélectionnez la moto dans MX Bikes pour voir le modèle.",
  "presets.phaseBundling": "Préparation des fichiers…",
  "presets.phaseUploading": "Envoi du paquet…",
  "presets.phaseDownloading": "Téléchargement du paquet…",
  "presets.phaseInstalling": "Installation des fichiers…",
  "presets.bundleUploaded":
    "Paquet complet envoyé — le code inclut désormais les fichiers.",
  "presets.shareHintFull":
    "Ce code inclut un paquet téléchargeable — le destinataire choisit Import complet et récupère tout, même sans aucun mod installé.",
  "presets.shareHintConfig":
    "Envoyez ce code à qui vous voulez. L'import se fait dans Presets → Importer. Il faudra les mêmes mods installés pour que tout s'affiche.",
  "presets.generatingCode": "Génération du code…",
  "presets.nothingToBundle":
    "Aucun fichier installé à empaqueter — ce look est entièrement en Stock/polices.",
  "presets.createFullBundle": "Créer un paquet complet",
  "presets.copiedFull": "Code du paquet complet copié.",
  "presets.copiedShare": "Code de partage copié.",
  "presets.copyFailed":
    "Copie impossible — sélectionnez le code et copiez-le manuellement.",
  "presets.copyFullCode": "Copier le code complet",
  "presets.copyCode": "Copier le code",
  "presets.importTitle": "Importer un preset",
  "presets.importBody": "Collez un code de partage qu'on vous a envoyé.",
  "presets.configOnly": "Configuration seule",
  "presets.import": "Importer",
  "presets.fullImport": "Import complet",
  "presets.editingBanner":
    "Modification de {{name}} — changez le nom ou n'importe quel emplacement, puis {{save}}.",
  "presets.bundleNotice":
    "Inclut un paquet complet (~{{size}} depuis {{host}}). Utilisez {{fullImport}} pour tout télécharger et installer — aucun mod requis au préalable.",

  // ── Emplacements de preset ─────────────────────────────────────────────────
  "slot.paint": "Livrée moto",
  "slot.modelSwap": "Changement de modèle",
  "slot.bikeFont": "Police des numéros",
  "slot.tyres": "Pneus",
  "slot.rider": "Profil pilote",
  "slot.suitPaint": "Tenue / kit",
  "slot.suitFont": "Police de la tenue",
  "slot.glovesPaint": "Gants",
  "slot.ridingStyle": "Style de pilotage",
  "slot.helmet": "Casque",
  "slot.helmetPaint": "Déco casque",
  "slot.gogglesPaint": "Masque",
  "slot.boots": "Bottes",
  "slot.bootsPaint": "Déco bottes",
  "slot.protection": "Protections",
  "slot.protectionPaint": "Déco protections",
  "slotGroup.bike": "Moto",
  "slotGroup.rider": "Pilote",
  "slotGroup.head": "Tête",
  "slotGroup.body": "Corps",

  // ── Studio pilote ──────────────────────────────────────────────────────────
  "rider.help":
    "Habillez le modèle du pilote — casque, masque, tenue et bottes d'un seul coup.",
  "rider.namePlaceholder": "Nommez ce pilote…",
  "rider.nameFirst": "Nommez d'abord ce look de pilote.",
  "rider.showOnModel": "Afficher sur le modèle",
  "rider.repairTitle": "Un mod {{area}} a été installé en vrac",
  "rider.repairBody":
    "Ses fichiers sont directement dans {{area}} au lieu d'un dossier, donc ni le jeu ni cette app ne peuvent le charger. Les rassembler dans « {{model}} » ?",
  "rider.repairAction": "Réparer",
  "rider.repairDone_one": "{{count}} fichier rassemblé dans « {{model}} ».",
  "rider.repairDone_other": "{{count}} fichiers rassemblés dans « {{model}} ».",
  "rider.repairNothing": "Plus rien à rassembler.",

  // ── Visite guidée ──────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Faites un tour rapide",
  "tour.welcomeTour.body":
    "Quelques secondes pour voir où se trouve chaque chose. Vous pouvez passer à tout moment.",
  "tour.browse.title": "Parcourir les mods",
  "tour.browse.body": "Cherchez sur {{site}} directement ici et installez circuits, motos ou peintures en un clic.",
  "tour.library.title": "Votre bibliothèque",
  "tour.library.body":
    "Tout ce que vous avez installé, au même endroit — mettez à jour ou supprimez des mods sans jamais toucher un fichier zip.",
  "tour.locker.title": "Le casier",
  "tour.locker.body":
    "Changez les modèles de moto à volonté. MXB App enregistre les pièces pour que le jeu les reconnaisse.",
  "tour.presets.title": "Presets",
  "tour.presets.body":
    "Enregistrez vos combinaisons d'équipement et de décos, puis appliquez un look complet en un clic — même en pleine session.",
  "tour.rider.title": "Studio pilote",
  "tour.rider.body":
    "Prévisualisez votre équipement et vos décos sur le pilote 3D avant de les emmener sur la piste.",
  "tour.frostmod.title": "FrostMod, en direct",
  "tour.frostmod.body":
    "Ceci affiche l'état de FrostMod. Il recharge MX Bikes à chaud après une installation, pour que le nouveau contenu apparaisse sans redémarrer le jeu.",
  "tour.servers.title": "Être vu correctement en ligne",
  "tour.servers.body": "MX Bikes n'envoie jamais les peintures entre joueurs : tout le monde apparaît avec l'équipement par défaut si vous n'avez pas déjà leur fichier exact. Inscrivez-vous ici et l'app publie votre look et récupère celui des autres — et vous pouvez lancer un serveur dédié depuis la même page.",
  "tour.settings.title": "Réglages",
  "tour.settings.body":
    "Définissez ici votre dossier de jeu, le comportement en arrière-plan et les options FrostMod. Vous pouvez aussi rejouer cette visite depuis cet écran.",
  "tour.done.title": "Tout est prêt",
  "tour.done.body":
    "La visite est terminée. Direction Parcourir pour installer votre premier mod.",

  // ── Erreurs ────────────────────────────────────────────────────────────────
  "error.previewFailed": "Impossible d'afficher l'aperçu",
  "error.somethingWentWrong": "Une erreur est survenue",
  "error.unexpected": "Une erreur inattendue s'est produite.",
  "error.reloadApp": "Recharger l'application",

  // ── Mises à jour ───────────────────────────────────────────────────────────
  "update.available": "{{version}} est disponible.",
  "update.downloading": "Téléchargement…",
  "update.downloadingPct": "Téléchargement… {{pct}} %",
  "update.pitch":
    "Mettez à jour pour obtenir les dernières fonctionnalités et corrections.",
  "update.updating": "Mise à jour…",
  "update.updateAndRestart": "Mettre à jour et redémarrer",
  "update.dismiss": "Ignorer la notification de mise à jour",
  "update.onLatest": "Vous avez déjà la dernière version",

  // ── Runtime Visual C++ manquant ────────────────────────────────────────────
  "runtime.componentVc90": "Microsoft Visual C++ 2008 (x64)",
  "runtime.componentVc140": "Microsoft Visual C++ 2015–2022 (x64)",
  "runtime.bannerGame":
    "MX Bikes a besoin de {{what}} pour que FrostMod puisse s'y greffer.",
  "runtime.bannerFrostmod": "FrostMod a besoin de {{what}} pour fonctionner.",
  "runtime.pitch":
    "Sans ça, Windows affiche l'erreur « dll was not found ». Réglé en quelques secondes.",
  "runtime.fixIt": "L'installer",
  "runtime.installing": "Installation…",
  "runtime.dismiss": "Masquer cet avertissement",
  "runtime.installed": "Composant installé",
  "runtime.installedDesc":
    "FrostMod devrait maintenant atteindre le jeu. Relancez MX Bikes s'il est déjà ouvert.",
  "runtime.cancelled": "Rien n'a été installé",
  "runtime.cancelledDesc":
    "Windows a besoin de votre autorisation. Ouverture du téléchargement Microsoft à la place.",
  "runtime.installFailed": "Impossible d'installer le composant",
  "runtime.downloadManually": "Le télécharger soi-même",
  "update.checkFailed": "Impossible de vérifier les mises à jour",
  "update.failed": "Échec de la mise à jour",

  // ── Visualiseur 3D ─────────────────────────────────────────────────────────
  "viewer.preview3d": "Aperçu 3D",
  "viewer.expand": "Agrandir",
  "viewer.paint": "Déco",
  "viewer.loadingModel": "Chargement du modèle…",
  "viewer.loadingPaint": "Chargement de la déco…",
  "viewer.loadingRider": "Chargement du pilote…",
  "viewer.riderLoadFailed": "Aperçu obsolète — impossible de le mettre à jour",
  "viewer.dragToRotate": "Glisser pour pivoter",
  "viewer.scrollToZoom": "Molette pour zoomer",
  "viewer.rightDragToPan": "Clic droit glissé pour déplacer",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Rechercher…",
  "combobox.use": "Utiliser « {{value}} »",

  // ── Types de mods ──────────────────────────────────────────────────────────
  "modType.tracks": "Circuits",
  "modType.bikes": "Motos",
  "modType.rider": "Pilote",
  "modType.tracksInline": "circuits",
  "modType.bikesInline": "motos",
  "modType.riderInline": "équipement pilote",

  // ── Filtres de catégorie ───────────────────────────────────────────────────
  "browseCat.all": "Tout",
  "browseCat.beginner": "Débutant",
  "browseCat.intermediate": "Intermédiaire",
  "browseCat.pro": "Pro",
  "browseCat.assets": "Ressources",
  "browseCat.newBikes": "Nouvelles motos",
  "browseCat.liveries": "Livrées",
  "browseCat.sounds": "Sons",
  "browseCat.riderKit": "Kit pilote",
  "browseCat.helmets": "Casques",
  "browseCat.helmetPaints": "Décos casque",
  "browseCat.gloves": "Gants",
  "browseCat.boots": "Bottes",
  "browseCat.bootPaints": "Décos bottes",
  "browseCat.protection": "Protections",
  "browseCat.protectionPaints": "Décos protections",

  // ── Parcourir ──────────────────────────────────────────────────────────────
  "browse.help":
    "Découvrez et installez des mods depuis le catalogue en ligne — cherchez, filtrez par type, et ouvrez un mod pour le télécharger dans le jeu.",
  "browse.searchPlaceholder": "Rechercher des {{type}}…",
  "browseSort.newest": "Plus récents",
  "browseSort.oldest": "Plus anciens",
  "browseSort.popularAll": "Plus populaires",
  "browseSort.popularMonth": "Populaires ce mois-ci",
  "browseSort.popularWeek": "Populaires cette semaine",
  "browse.loadFailed": "Impossible de charger les mods",
  "browse.empty": "Aucun résultat pour {{type}}.",
  "browse.loadMore": "Charger plus",
  "browse.selectedCount": "{{count}} sélectionné(s)",
  "browse.queuing": "Mise en file…",
  "browse.quickInstallCount": "Installation rapide de {{count}}",
  "browse.quickInstall": "Installation rapide",
  "browse.quickReinstall": "Réinstallation rapide",
  "browse.openDetails": "Ouvrir les détails",
  "browse.reinstallOne": "Réinstaller « {{title}} » ?",
  "browse.reinstallMany": "Réinstaller les mods que vous avez déjà ?",
  "browse.reinstallOneBody":
    "Ce mod est déjà dans votre bibliothèque. Le réinstaller le télécharge à nouveau et écrase les fichiers installés.",
  "browse.reinstallManyBody":
    "{{installed}} des {{total}} sélectionnés sont déjà installés. Continuer les réinstalle et les écrase.",
  "browse.reinstall": "Réinstaller",
  "browse.reinstallAll": "Tout réinstaller",
  "browse.queued": "« {{title}} » en file d'attente",
  "browse.queuedDesc": "Installation dans {{folder}}.",
  "browse.rootFolder": "racine",
  "browse.needsBrowser":
    "« {{title}} » doit être téléchargé depuis le navigateur",
  "browse.needsBrowserDesc":
    "{{host}} bloque les téléchargements dans l'application — ouvrez sa page pour terminer.",
  "browse.noDownload": "Aucun téléchargement trouvé pour « {{title}} »",
  "browse.quickInstallFailed":
    "Impossible d'installer rapidement « {{title}} »",
  "browse.queuedBulk_one": "{{count}} mod en file d'attente",
  "browse.queuedBulk_other": "{{count}} mods en file d'attente",
  "browse.queuedBulkDesc": "Ils s'installeront l'un après l'autre.",
  "browse.queuedBulkSkipped_one":
    "{{count}} ignoré — hébergeur navigateur uniquement.",
  "browse.queuedBulkSkipped_other":
    "{{count}} ignorés — hébergeur navigateur uniquement.",
  "browse.bulkFailed": "Impossible d'installer rapidement la sélection",
  "browse.bulkFailedDesc_one":
    "Il doit être téléchargé depuis le navigateur.",
  "browse.bulkFailedDesc_other":
    "Les {{count}} doivent être téléchargés depuis le navigateur.",

  // ── Boutique (MX Bikes Shop — téléchargements achetés) ─────────────────────
  "shop.help":
    "Parcourez le catalogue de mxbikes-shop.com et installez ce que vous avez déjà acheté. L'achat se fait toujours sur le site de la boutique ; connectez-vous dans Mes achats pour installer vos commandes depuis ici.",
  "shopTab.catalog": "Catalogue",
  "shopTab.purchases": "Mes achats",
  "shop.myDownloads": "Mes achats",
  "shop.signInTitle": "Connectez-vous à MX Bikes Shop",
  "shop.signInBody":
    "Connectez-vous à mxbikes-shop.com pour voir et installer tout ce que vous avez acheté. Nous ouvrons le vrai site — votre mot de passe ne touche jamais cette application.",
  "shop.signIn": "Se connecter",
  "shop.logOut": "Se déconnecter",
  "shop.signedIn": "Connecté à MX Bikes Shop",
  "shop.sessionFailed": "Impossible de récupérer votre session MX Bikes Shop",
  "shop.loadFailed": "Impossible de charger vos achats : {{error}}",
  "shop.empty": "Aucun téléchargement acheté trouvé sur votre compte.",
  "purchases.count_one": "{{count}} achat",
  "purchases.count_other": "{{count}} achats",
  "purchases.fileCount_one": "{{count}} fichier",
  "purchases.fileCount_other": "{{count}} fichiers",
  "purchases.install": "Installer",
  "purchases.reinstall": "Réinstaller",
  "purchases.installed": "Installé",
  "purchases.downloading": "Téléchargement…",
  "purchases.downloadFailed": "Impossible de télécharger {{title}}",
  "purchases.searchPlaceholder": "Rechercher dans vos achats…",
  "purchases.otherCategory": "Autres",
  "purchases.notInstalledOnly": "Non installés",
  "purchases.noMatches": "Aucun de vos achats ne correspond.",
  "purchases.viewDetails": "Voir les détails",
  "purchaseSort.recentlyPurchased": "Achetés récemment",
  "purchaseSort.nameAsc": "Nom (A–Z)",
  "purchaseSort.notInstalled": "Non installés d’abord",
  // ── Catalogue MX Bikes Shop (consultation seule ; l'achat se fait sur le site) ─
  "shopCatalog.searchPlaceholder": "Rechercher dans la boutique…",
  "shopCatalog.allCategories": "Tout",
  "shopCatalog.onSaleOnly": "En promo",
  "shopCatalog.loadMore": "Charger plus",
  "shopCatalog.loadFailed": "Impossible de charger le catalogue de la boutique",
  "shopCatalog.empty": "Rien dans la boutique ne correspond.",
  "shopCatalog.viewDetails": "Voir les détails",
  "shopCatalog.openOnStore": "Ouvrir sur mxbikes-shop.com",
  "shopCatalog.buyOnStore": "Acheter sur mxbikes-shop.com",
  "shopCatalog.buyNote": "S'ouvre dans votre navigateur. L'achat et le téléchargement se font sur la boutique.",
  "shopCatalog.noProductLink": "Cet article n'a pas de page produit que nous puissions ouvrir.",
  "shopCatalog.noScreenshots": "Aucune capture",
  "shopCatalog.about": "À propos de cet article",
  "shopCatalog.author": "Créateur",
  "shopCatalog.category": "Catégorie",
  "shopCatalog.updated": "Mis à jour",
  "shopCatalog.priceUnknown": "Prix non indiqué",
  "shopCatalog.free": "Gratuit",
  "shopCatalog.refresh": "Actualiser",
  "shopCatalog.refreshing": "Actualisation…",
  "shopCatalog.stale": "Prix vérifiés pour la dernière fois {{when}}.",
  "shopCatalog.staleHard":
    "Ces prix ont été vérifiés pour la dernière fois {{when}} et peuvent être obsolètes. Actualisez avant de vous y fier.",
  "shopCatalog.saleEndsDays_one": "La promo se termine dans 1 jour",
  "shopCatalog.saleEndsDays_other": "La promo se termine dans {{count}} jours",
  "shopCatalog.saleEndsHours_one": "La promo se termine dans 1 heure",
  "shopCatalog.saleEndsHours_other": "La promo se termine dans {{count}} heures",
  "shopCatalog.saleEndsSoon": "La promo se termine bientôt",
  "shopCatalog.agoJustNow": "à l'instant",
  "shopCatalog.agoUnknown": "il y a un moment",
  "shopCatalog.agoMinutes_one": "il y a 1 minute",
  "shopCatalog.agoMinutes_other": "il y a {{count}} minutes",
  "shopCatalog.agoHours_one": "il y a 1 heure",
  "shopCatalog.agoHours_other": "il y a {{count}} heures",
  "shopCatalog.agoDays_one": "il y a 1 jour",
  "shopCatalog.agoDays_other": "il y a {{count}} jours",
  "shopSort.newest": "Plus récents",
  "shopSort.recentlyUpdated": "Récemment mis à jour",
  "shopSort.priceAsc": "Prix : croissant",
  "shopSort.priceDesc": "Prix : décroissant",
  "shopSort.onSale": "Promos d'abord",
  "shopSort.nameAsc": "Nom (A–Z)",

  // ── Fenêtre d'installation ─────────────────────────────────────────────────
  "installDialog.installTo": "Installer dans",
  "installDialog.installToFolder": "Installer dans {{folder}}",
  "installDialog.change": "Modifier",
  "installDialog.searchBikes": "Rechercher une moto…",
  "installDialog.searchFolders": "Rechercher un dossier…",
  "installDialog.probably": "Probablement",
  "installDialog.allFolders": "Tous les dossiers",
  "installDialog.noFolderMatch":
    "Aucun dossier ne correspond — créez-le ci-dessous.",
  "installDialog.rememberedFor": "Mémorisé pour {{type}}",
  "installDialog.downloadFrom": "Télécharger depuis",
  "installDialog.downloadPerBike": "Téléchargement (par moto)",
  "installDialog.opensInBrowser":
    "S'ouvre dans le navigateur — MXB App termine l'installation",
  "installDialog.matchedBike": "Associé à votre moto",
  "installDialog.differentBike": "Moto / pack différent",
  "installDialog.directFastest": "Direct · le plus rapide",
  "installDialog.direct": "Direct",
  "installDialog.perBikeHint":
    "Chaque téléchargement correspond à une moto différente — sélectionné automatiquement selon votre choix. Choisissez le pack « all bikes » pour toutes les motos d'un coup.",
  "installDialog.mirrorsHint":
    "Tous les miroirs contiennent le même fichier. Si l'un échoue, essayez le suivant.",

  // ── Détails de bibliothèque ────────────────────────────────────────────────
  "libraryDetail.author": "Auteur",
  "libraryDetail.length": "Longueur",
  "libraryDetail.altitude": "Altitude",
  "libraryDetail.location": "Lieu",
  "libraryDetail.type": "Type",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Appartient à",
  "libraryDetail.format": "Format",
  "libraryDetail.extractedFolder": "Dossier extrait",
  "libraryDetail.paintFile": "Fichier de déco",
  "libraryDetail.packagedPkz": "Paquet .pkz",
  "libraryDetail.size": "Taille",
  "libraryDetail.folder": "Dossier",
  "libraryDetail.lockedWord": "verrouillé",
  "libraryDetail.lockedWithMeta":
    "Ce circuit est {{locked}} par son créateur. Son nom, ses détails et son aperçu sont affichés ici, mais les fichiers restent scellés — il ne peut être ni extrait ni prévisualisé en 3D.",
  "libraryDetail.lockedNoMeta":
    "Ce circuit est {{locked}}, donc son nom, sa longueur et son aperçu ne peuvent pas être lus depuis le fichier — seulement son nom de fichier et sa taille.",

  // ── Page de mod ────────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Résolution",
  "modDetail.stageDownload": "Téléchargement",
  "modDetail.stageExtract": "Extraction",
  "modDetail.stagePlace": "Placement",
  "modDetail.stageReload": "Rechargement",
  "modDetail.modFiles": "Fichiers de mod",
  "modDetail.copied": "Copié",
  "modDetail.copy": "Copier",
  "modDetail.addToLibrary": "Ajouter à la bibliothèque",
  "modDetail.host": "Hébergeur",
  "modDetail.installsTo": "Installe dans",
  "modDetail.noDownloadLink": "Aucun lien de téléchargement trouvé sur cette page — ouvrez-la sur {{site}}.",
  "modDetail.frostmodHint":
    "FrostMod rechargera la liste des {{kind}} une fois terminé.",
  "modDetail.kindRider": "pilotes",
  "modDetail.kindBike": "motos",
  "modDetail.kindTrack": "circuits",
  "modDetail.details": "Détails",
  "modDetail.format": "Format",
  "modDetail.mirrors": "Miroirs",
  "modDetail.type": "Type",
  "modDetail.addedToLibrary": "Ajouté à votre bibliothèque",
  "modDetail.extracting": "Extraction…",
  "modDetail.addingToLibrary": "Ajout à la bibliothèque…",
  "modDetail.resolving": "Résolution du téléchargement…",
  "modDetail.finishInBrowser": "Terminez dans votre navigateur",
  "modDetail.viewOnSite": "Voir sur {{site}}",

  // ── Réglages ───────────────────────────────────────────────────────────────
  "settings.help":
    "Configurez votre dossier de jeu, les mises à jour et les préférences de l'application.",
  "settings.gameFolder": "Dossier de jeu",
  "settings.general": "Général",
  "settings.appearance": "Apparence",
  "settings.frostmod": "FrostMod",
  "settings.about": "À propos et mises à jour",
  "settings.whatsNew": "Nouveautés",
  "settings.modsFolderDesc":
    "Là où les mods sont installés. Choisissez le dossier qui contient les dossiers mods et profiles \u2014 celui au-dessus de mods, pas le dossier mods lui-même. Le modifier relance l'analyse de votre bibliothèque.",
  "settings.insideModsFolder": "Dans votre dossier {{game}}",
  "settings.notSet": "Non défini",
  "settings.selectFolderFor": "Sélectionnez un dossier pour {{game}}",
  "settings.gameDesc":
    "Le jeu que MXB App pilote. Vos dossiers, votre bibliothèque et vos presets appartiennent tous au jeu choisi ici.",
  "settings.change": "Modifier…",
  "settings.set": "Définir…",
  "settings.theme": "Thème",
  "settings.themeLight": "Clair",
  "settings.themeDark": "Sombre",
  "settings.themeSystem": "Système",
  "settings.language": "Langue",
  "settings.languageSystem": "Système",
  "settings.runInBackground": "Continuer en arrière-plan",
  "settings.runInBackgroundDesc":
    "Fermer la fenêtre place MXB App dans la barre d'état pour que FrostMod reste connecté. Quittez depuis l'icône de la barre.",
  "settings.launchAtStartup": "Lancer au démarrage",
  "settings.launchAtStartupDesc":
    "Démarrer MXB App automatiquement à votre connexion.",
  "settings.instantRefresh": "Actualisation instantanée des presets",
  "settings.instantRefreshDesc":
    "Quand vous appliquez un preset pendant que {{game}} tourne, actualise le look en jeu instantanément — sans redémarrage ni resélection de profil. Si ce n'est pas possible, il vous sera demandé de resélectionner votre profil.",
  "settings.instantRefreshWindowsOnly":
    "Actualiser le look en jeu sans redémarrer nécessite FrostMod, qui est réservé à Windows — il vous sera demandé de resélectionner votre profil à la place.",
  "settings.autoRunFrostmod": "Lancer FrostMod automatiquement",
  "settings.autoRunFrostmodDesc":
    "Démarrer FrostMod en arrière-plan à chaque ouverture de MXB App.",
  "settings.watchModsReload":
    "Rechargement auto lors des changements de dossier",
  "settings.watchModsReloadDesc":
    "Recharger le jeu automatiquement quand des circuits ou des motos sont ajoutés à votre dossier de mods — même téléchargés manuellement hors de MXB App.",
  "settings.checking": "Vérification…",
  "settings.runningConnected": "En cours · jeu connecté",
  "settings.notRunning": "Inactif",
  "settings.frostmodInstalled": "Installé{{suffix}}",
  "settings.notInstalled": "Non installé",
  "settings.checkingGitHub":
    "Vérification de la dernière version sur GitHub…",
  "settings.updateCheckFailed":
    "Impossible de vérifier les mises à jour — hors ligne ou GitHub indisponible.",
  "settings.latestVersion": "Dernière : {{version}}",
  "settings.frostmodRuntimeMissing":
    "Il manque à Windows un composant Visual C++ dont FrostMod a besoin — installez-le pour faire disparaître l'erreur « dll was not found ».",
  "settings.frostmodNeedsRepair":
    "Les fichiers installés ne correspondent pas à cette version — une réinstallation corrige ça.",
  "settings.frostmodRepair": "Réparer l'installation",
  "settings.frostmodUnsupportedForGame":
    "Cette version de FrostMod n'est pas sûre sur {{game}} — mets-la à jour pour utiliser FrostMod ici.",
  "settings.frostmodUpdateRequired": "Mise à jour requise",
  "settings.checkNewer": "Chercher une version plus récente de FrostMod",
  "settings.working": "Traitement…",
  "settings.installFrostmod": "Installer FrostMod",
  "settings.updateTo": "Mettre à jour vers {{version}}",
  "settings.reinstallLatest": "Réinstaller la dernière",
  "settings.upToDate": "À jour",
  "settings.madeWith": "Fait avec",
  "settings.updateFailed": "Impossible de mettre à jour le réglage",
  "settings.startupUpdateFailed":
    "Impossible de mettre à jour le lancement au démarrage",
  "settings.folderUpdated": "Dossier de jeu mis à jour",
  "settings.folderUpdatedDesc": "Votre bibliothèque va être réanalysée.",
  "settings.folderUsedParent":
    "C’était le dossier mods \u2014 le dossier au-dessus a été utilisé : {{folder}}",
  "settings.setFolderFailed": "Impossible de définir le dossier",
  "settings.reDetected": "Dossier {{game}} détecté à nouveau",
  "settings.detectFolderFailed": "Impossible de détecter le dossier",
  "settings.pickInstallFolder":
    "Sélectionnez votre dossier d'installation de {{game}} (contient rider.pkz)",
  "settings.installSet": "Installation du jeu définie",
  "settings.installSetDesc":
    "L'aperçu 3D du pilote peut désormais charger le vrai modèle du corps.",
  "settings.setInstallFailed":
    "Impossible de définir le dossier d'installation",
  "settings.installNotFound": "Impossible de trouver {{game}}",
  "settings.installNotFoundDesc":
    "Aucune installation Steam détectée — définissez le dossier manuellement.",
  "settings.installFound": "Installation de {{game}} trouvée",
  "settings.detectInstallFailed":
    "Impossible de détecter le dossier d'installation",
  "settings.wineRunnerDesc":
    "{{game}} est un jeu Windows : sur un Mac, il tourne dans une bottle CrossOver, Whisky ou Wine. C'est par là que Jouer le lance.",
  "settings.wineRunnerNone": "Aucun runner Wine trouvé",
  "settings.pickWineRunner":
    "Sélectionnez un binaire Wine (par ex. le wine de CrossOver)",
  "settings.wineRunnerFailed": "Impossible de définir le runner Wine",
  "settings.wineBottlesFound_one":
    "{{count}} bottle trouvée où chercher votre installation.",
  "settings.wineBottlesFound_other":
    "{{count}} bottles trouvées où chercher votre installation.",
  "settings.wineBottlesNone":
    "Aucune bottle trouvée — installez d'abord {{game}} dans CrossOver, Whisky ou Wine.",
  "settings.pickProfilesFolder":
    "Sélectionnez votre dossier de profils {{game}}",
  "settings.profilesSet": "Dossier de profils défini",
  "settings.profilesFound_one": "{{count}} profil trouvé.",
  "settings.profilesFound_other": "{{count}} profils trouvés.",
  "settings.noProfilesThere": "Aucun profil trouvé à cet endroit",
  "settings.noProfilesThereDesc":
    "Enregistré quand même, mais la création de presets nécessite un dossier contenant vos dossiers profile.ini.",
  "settings.setProfilesFailed":
    "Impossible de définir le dossier de profils",
  "settings.profilesReverted": "Retour au dossier de profils par défaut",
  "settings.resetProfilesFailed":
    "Impossible de réinitialiser le dossier de profils",
  "settings.frostmodNotRunningHint":
    "FrostMod n'est pas actif — démarrez-le pour recharger les mods à chaud.",
  "settings.reloadUnavailable":
    "Le rechargement n'est pas disponible sur cette plateforme.",

  // ── Lancement du jeu ───────────────────────────────────────────────────────
  "game.play": "Jouer",
  "game.starting": "Démarrage…",
  "game.running": "{{game}} en cours",
  "game.launch": "Lancer {{game}}",
  "game.alreadyRunning": "{{game}} est déjà en cours d'exécution",
  "game.launching": "Lancement de {{game}}…",
  "game.launchFailed": "Impossible de lancer {{game}}",
  "join.title": "Rejoindre un serveur",
  "join.desc":
    "Saisissez l'adresse d'un serveur pour lancer {{game}} en vous y connectant directement.",
  "join.address": "Adresse du serveur",
  "join.action": "Rejoindre",
  "join.joining": "Connexion…",
  "join.launching": "Connexion à {{address}}…",
  "join.alreadyRunning":
    "Fermez d'abord {{game}} — un jeu déjà lancé ne peut pas être envoyé vers un serveur.",
  "join.failed": "Impossible de rejoindre ce serveur",
  "join.manual": "Rejoindre un serveur non listé",
  "join.noServers": "Aucun serveur listé pour l'instant — saisissez une adresse qu'on vous a donnée.",

  "servers.title": "Serveurs",
  "servers.subtitle":
    "Gérez les serveurs dédiés que vous hébergez. Chacun doit avoir l'agent MXB installé.",
  "servers.empty": "Aucun serveur pour l'instant. Ajoutez-en un pour le gérer d'ici.",
  "servers.add": "Ajouter un serveur",
  "servers.remove": "Retirer ce serveur",
  "servers.namePlaceholder": "Nom du serveur",
  "servers.tokenPlaceholder": "Jeton de l'agent",
  "servers.track": "Circuit",
  "servers.slots": "Places",
  "servers.uptime": "Actif depuis",
  "servers.restarts": "Redémarrages",
  "servers.stopped": "Arrêté",
  "servers.start": "Démarrer",
  "servers.stop": "Arrêter",
  "servers.restart": "Redémarrer",
  "servers.setTrack": "Changer de circuit",
  "servers.trackPlaceholder": "ID du circuit",
  "servers.actionDone": "C'est fait",
  "servers.actionFailed": "Ça n'a pas fonctionné",
  "servers.trackChanged": "Circuit réglé sur {{track}} — le serveur a redémarré.",
  "servers.saveFailed": "Impossible d'enregistrer votre liste de serveurs",
  "servers.trackLoading": "Lecture des circuits…",
  "servers.trackEmpty": "Aucun circuit sur cet hôte",
  "servers.nameOptional": "Nom du serveur (facultatif — lu depuis l'hôte)",
  "servers.probing": "Vérification de cet agent…",
  "servers.probeFailed": "Impossible de joindre cet agent",
  "servers.probed": "{{name}} trouvé",
  "servers.pairingWhere":
    "Lancez mxb-agent sur la machine qui héberge votre serveur. Il affiche cette ligne à chaque démarrage — copiez-la entièrement.",
  "servers.manualEntry": "Je n'ai pas de code d'appairage — saisir les détails à la main",
  "servers.publish": "Ajouter à la liste des serveurs",
  "servers.unpublish": "Retirer de la liste",
  "servers.listed": "Dans la liste publique — n'importe qui peut le trouver et le rejoindre.",
  "servers.notListed": "Pas encore dans la liste publique des serveurs.",
  "servers.published": "Ajouté — les autres joueurs peuvent le trouver",
  "servers.publishedUnreachable":
    "Enregistré, mais nous n'avons pas pu le joindre depuis internet, donc il n'est pas encore listé. Vérifiez que l'agent tourne et que son port est ouvert.",
  "servers.publishFailed": "Impossible de modifier la liste des serveurs",
  "servers.unpublished": "Retiré de la liste des serveurs",
  "servers.createTitle": "Créer un serveur",
  "servers.createDesc":
    "Lancez un serveur dédié dans le cloud sans posséder de machine. Il s'éteint tout seul quand plus personne n'y roule depuis un moment, donc il ne fait pas grimper la facture la nuit.",
  "servers.create": "Créer",
  "servers.creating": "Création en cours — quelques minutes avant qu'il soit prêt",
  "servers.createFailed": "Impossible de créer ce serveur",
  "servers.runningCount_one": "{{count}} actif",
  "servers.runningCount_other": "{{count}} actifs",
  "servers.pairingPlaceholder": "Collez le code d'appairage",
  "servers.pairingHint":
    "L'agent affiche cette ligne au démarrage. Collez-la ici et l'adresse et le jeton se remplissent tout seuls — ou saisissez-les à la main ci-dessous.",

  "settings.experimental": "Expérimental",
  "settings.experimentalServers": "Serveurs et synchronisation des décos",
  "settings.experimentalServersDesc":
    "Inachevé. Ajoute l'onglet Serveurs, vous permet d'héberger des serveurs dédiés et synchronise les décos pour que tout le monde s'affiche correctement.",
  "settings.experimentalForced":
    "Activé pour cette session par MXB_EXPERIMENTAL — le réglage reste sans effet tant qu'il est défini.",
  "settings.betaBadge": "Bêta",

  "sync.title": "Synchronisation des décos",
  "sync.desc":
    "MX Bikes n'envoie jamais les décos : les autres pilotes apparaissent en déco d'origine si vous n'avez pas déjà leur fichier exact. Publiez la vôtre et récupérez celles des autres.",
  "sync.enroll": "S'inscrire",
  "sync.enrolled": "Inscrit en tant que {{name}}",
  "sync.enrollFailed": "Inscription impossible",
  "sync.codePlaceholder": "Code d'invitation",
  "sync.riderNamePlaceholder": "Nom de pilote en jeu",
  "sync.riderNameHint":
    "Il doit correspondre exactement à votre nom de pilote dans MX Bikes — c'est ainsi que les apps des autres savent quelles décos sont les vôtres.",
  "sync.ridingAs": "Publié sous {{name}}",
  "sync.pull": "Synchroniser les décos",
  "sync.setGuid": "Enregistrer le GUID",
  "sync.guidPlaceholder": "Votre GUID MX Bikes",
  "sync.guidHint":
    "Votre GUID MX Bikes (facultatif). Il vous identifie même si vous changez de nom de pilote, et le serveur l'enregistre à chaque connexion.",
  "sync.guidSaved": "GUID enregistré",
  "sync.pulled": "{{installed}} installées depuis {{riders}} pilotes ({{had}} déjà présentes)",
  "sync.pullFailed": "Synchronisation impossible",
  "sync.rejected": "{{count}} ignorées : destination non sûre",
  "sync.pickProfile": "Vous roulez sous",
  "sync.pickProfileHint":
    "Vos profils MX Bikes, tels que l'app les a trouvés. En choisir un, c'est ce qui indique aux apps des autres joueurs quelles peintures sont les vôtres.",
  "sync.noProfiles":
    "Aucun profil MX Bikes trouvé : saisissez votre nom de pilote exactement tel qu'il apparaît dans le jeu.",
  "sync.guidClaimed": "Identifié par le GUID {{guid}}",
  "sync.guidPending":
    "Votre GUID est récupéré tout seul la première fois qu'un de vos serveurs vous voit vous connecter. D'ici là, c'est votre nom de pilote qui vous identifie.",
  "sync.guidManual": "Le saisir manuellement",
  "sync.whereCode":
    "Le paint sync est sur invitation pour l'instant. Les codes sont distribués sur le Discord — demandez-y et collez ci-dessus celui qu'on vous donne.",
  "sync.getCode": "Demander sur le Discord",
  "sync.sidebarOk": "Synchronisé · {{count}} pilotes",
  "sync.sidebarUnpublished": "Votre look n'est pas publié",
  "sync.agoJustNow": "à l'instant",
  "sync.agoMinutes_one": "il y a {{count}} minute",
  "sync.agoMinutes_other": "il y a {{count}} minutes",
  "sync.agoHours_one": "il y a {{count}} heure",
  "sync.agoHours_other": "il y a {{count}} heures",
  "sync.agoDays_one": "il y a {{count}} jour",
  "sync.agoDays_other": "il y a {{count}} jours",
  "sync.publishing": "Envoi de votre look…",
  "sync.pulling": "Récupération des peintures des autres…",
  "sync.publishNow": "Publier maintenant",
  "sync.published": "{{paints}} peintures publiées sur {{bikes}} motos",
  "sync.publishFailed": "Impossible de publier vos peintures",
  "sync.publishedState": "Votre look est publié — {{bikes}} motos, {{paints}} peintures",
  "sync.lastPublished": "Envoyé {{ago}}. Il repart tout seul dès que vous changez quelque chose.",
  "sync.neverPublished": "Votre look n'a pas encore été publié",
  "sync.neverPublishedWhy": "Tant qu'il ne l'est pas, tout le monde sur le serveur vous voit avec la moto et l'équipement par défaut.",
  "sync.pulledState": "Vous avez les peintures de {{count}} pilotes",
  "sync.lastPulled": "Dernière vérification {{ago}}. Elle repart seule quand vous appuyez sur Jouer.",
  "sync.neverPulled": "Vous n'avez pas encore récupéré les peintures des autres",
  "sync.neverPulledWhy": "Tant que ce n'est pas fait, les autres pilotes apparaissent avec des motos par défaut même s'ils ont publié les leurs.",
  "sync.skippedBikes_one": "{{count}} moto n'a pas été publiée — vous en avez plus que ce que nous pouvons stocker.",
  "sync.skippedBikes_other": "{{count}} motos n'ont pas été publiées — vous en avez plus que ce que nous pouvons stocker.",
  "sync.noMatchingProfile": "Ce nom ne correspond à aucun profil MX Bikes sur ce PC, il n'y a donc rien à publier. Vérifiez le dossier des profils dans les Réglages.",
  "sync.guidPendingTitle": "Identifié par votre nom de pilote",
  "sync.keptYours_one": "{{count}} peinture a été laissée intacte",
  "sync.keptYours_other": "{{count}} peintures ont été laissées intactes",
  "sync.keptYoursWhy": "Un autre pilote utilise le même nom de fichier pour une peinture différente. La vôtre a été conservée — l'app n'écrase jamais une livrée qu'elle n'a pas installée. Vous verrez ce pilote dans votre version.",
  "servers.booting": "Démarrage…",
  "servers.bootingStage": "{{stage}}…",
  "servers.bootFailed": "Ce serveur n'a pas pu terminer son installation et s'est éteint. Voici ce qu'il a signalé :",
  "servers.bootingWhy": "Installation du jeu sur la nouvelle machine. Cela prend quelques minutes — l'installeur complet est téléchargé.",
  "servers.shutsDown": "S'éteint",
  "servers.inUse": "En cours d'utilisation",
  "servers.inMinutes_one": "dans {{count}} min",
  "servers.inMinutes_other": "dans {{count}} min",
  "servers.inList": "Dans la liste",
  "servers.destroy": "Éteindre ce serveur",
  "servers.destroyed": "Serveur éteint",
  "servers.runningOfCap": "{{count}} sur {{cap}} actifs",
  "servers.atCap": "{{cap}} serveurs tournent déjà, c'est la limite. Éteignez-en un pour en démarrer un autre.",
  "servers.help": "Partagez vos livrées avec tout le monde sur un serveur, et gérez votre propre serveur dédié.",

  "sync.autoNote":
    "Votre look se publie tout seul — chaque moto, dès que vous le changez dans l'app ou dans le garage du jeu. Celui des autres arrive quand vous appuyez sur Jouer.",

  // ── Chaînes manquées par le premier balayage (JSX multi-lignes) ────────────
  "libraryDetail.noEmbedded": "Aucun détail intégré n'a été trouvé pour cet élément.",
  "modDetail.downloadFromHost": "Télécharger depuis {{host}}",
  "modDetail.openHost": "Ouvrir {{host}}",
  "modDetail.thenAddFile": "Ajoutez ensuite le fichier",
  "modDetail.chooseDownloaded": "Choisir le fichier téléchargé",
  "presets.chooseProfilesFolder": "Choisir le dossier de profils…",
  "presets.viewInRider": "Voir dans Pilote",
  "presets.noModelSwapsHere": "Aucun changement de modèle enregistré pour cette moto —",
  "presets.setUpInLocker": "configurez-les dans le Casier",
  "presets.makeActiveBike": "Faire de celle-ci la moto active",
  "presets.nameClash":
    "Un autre preset s'appelle déjà « {{name}} » — l'enregistrer l'écrasera aussi.",
  "presets.shareWarning":
    "Envoie vers un lien public et temporaire — cela redistribue des fichiers de mods créés par d'autres, alors partagez de façon responsable.",
  "settings.profilesDesc":
    "Les presets lisent vos profils ici — le chemin ci-dessous est celui que l'application utilise actuellement. C'est le dossier {{profiles}} dans votre dossier {{game}}, ou {{documents}} si vous avez déplacé votre dossier de mods. Ne le définissez que si le vôtre est ailleurs.",
  "settings.resetToDefault": "Réinitialiser",
  "settings.gameInstallDesc":
    "Dossier d'installation du jeu (facultatif) — là où {{game}} est installé (contient {{file}}). Définissez-le pour charger le vrai corps du pilote dans l'aperçu 3D.",
  "viewer.stockGearNote":
    "Affiché sur le {{part}} d'origine du jeu. Une déco faite pour un autre modèle peut ne pas s'aligner parfaitement.",
  "viewer.paintNoChange":
    "Aucune texture de cette déco n'est utilisée par les pièces affichées ici, donc l'aperçu ne change pas. Elle peut tout de même peindre les roues ou la chaîne, que cette vue n'affiche pas.",
  "viewer.noPaintPreview": "Pas d'aperçu de la déco ({{err}})",

  // ── Bibliothèque ───────────────────────────────────────────────────────────
  "library.help":
    "Vos mods installés. Vérifiez ce qui est installé et retirez ce dont vous ne voulez plus.",
  "library.rootFolder": "(racine)",
  "library.byAuthor": "par {{author}}",
  "library.locked": "Verrouillé — le contenu ne peut pas être lu",
  "library.searchPlaceholder": "Rechercher parmi les installés…",
  "library.scanning": "Analyse de votre bibliothèque…",
  "library.empty":
    "Aucun mod {{type}} installé — allez dans Parcourir pour en ajouter un.",
  "library.noMatches": "Aucun résultat.",
  "library.quick3d": "Voir en 3D",
  "library.selectNone": "Tout désélectionner",
  "library.move": "Déplacer",
  "library.uninstall": "Désinstaller",
  "library.uninstallAction": "Désinstaller…",
  "library.moveToFolder": "Déplacer vers un dossier…",
  "library.showInExplorer": "Afficher dans l'explorateur",
  "library.moveDialogTitle": "Déplacer vers un dossier",
  "library.moveCount_one": "Déplacer {{count}} élément",
  "library.moveCount_other": "Déplacer {{count}} éléments",
  "library.chooseDestination": "Choisissez un dossier de destination",
  "library.newFolder": "Nouveau dossier…",
  "library.newFolderName": "Nom du nouveau dossier",
  "library.createAndMove": "Créer et déplacer",
  "library.confirmUninstall": "Désinstaller {{name}} ?",
  "library.confirmUninstallBody":
    "L'élément est déplacé vers la Corbeille — vous pouvez le restaurer de là.",
  "library.confirmBulkUninstall_one": "Désinstaller {{count}} élément ?",
  "library.confirmBulkUninstall_other": "Désinstaller {{count}} éléments ?",
  "library.confirmBulkUninstallBody":
    "Chaque élément est déplacé vers la Corbeille — vous pouvez les restaurer de là.",
  "library.uninstallCount": "Désinstaller {{count}}",
  "library.moveFailed": "Impossible de déplacer le mod",
  "library.uninstallFailed": "Impossible de désinstaller",
  "library.openFailed": "Impossible d'ouvrir",
  "library.uninstalledOne": "{{name}} désinstallé",
  "library.movedToBin": "Déplacé vers la Corbeille.",
  "library.someNotRemoved": "Certains éléments n'ont pas pu être retirés.",
  "library.bulkUninstalled_one": "{{count}} élément désinstallé",
  "library.bulkUninstalled_other": "{{count}} éléments désinstallés",
  "library.bulkUninstallPartial": "{{ok}} désinstallés, {{fail}} en échec",
  "library.bulkMovePartial": "{{ok}} déplacés, {{fail}} en échec",
  "library.bulkMoved_one": "{{count}} élément déplacé vers {{folder}}",
  "library.bulkMoved_other": "{{count}} éléments déplacés vers {{folder}}",

  // ── Casier ─────────────────────────────────────────────────────────────────
  "locker.help":
    "Changez le modèle et le son moteur de chaque moto parmi les sets que vous avez installés.",
  "locker.rescan": "Réanalyser",
  "locker.restore": "Restaurer",
  "locker.register": "Enregistrer",
  "locker.scanning": "Analyse des motos…",
  "locker.scanForSwaps": "Chercher des sets",
  "locker.orphanBanner":
    "Il manque à {{bike}} ses fichiers de setup — une version précédente les a déplacés dans un dossier de swap, ce qui empêche totalement la moto de se charger en jeu. {{files}}",
  "locker.looseBanner_one":
    "{{count}} set modèle / son trouvé en vrac dans vos motos — enregistrez-le dans {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} sets modèle / son trouvés en vrac dans vos motos — enregistrez-les dans {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "Aucune moto échangeable pour l'instant.",
  "locker.emptyIntro":
    "Deux conditions doivent être réunies pour qu'un échange soit possible :",
  "locker.unpacked": "extraite",
  "locker.emptyRuleUnpacked":
    "La moto est {{unpacked}} dans {{path}}— un {{pkz}} compressé ne peut pas être échangé. Extrayez-en une depuis la Bibliothèque.",
  "locker.emptyRuleMesh":
    "Chaque modèle alternatif se trouve dans son propre dossier à l'intérieur de cette moto et contient un maillage ({{edf}}). Déposez-le n'importe où dans le dossier de la moto et cliquez sur Chercher ci-dessous — nous proposerons de le ranger dans {{folder}}.",
  "locker.summary": "{{model}} · son « {{sound}} »",
  "locker.modelNamed": "modèle « {{name}} »",
  "locker.noModelSwaps": "aucun changement de modèle",
  "locker.models": "Modèles",
  "locker.sounds": "Sons",
  "locker.onlyOneModel":
    "Un seul modèle — installez-en d'autres pour échanger",
  "locker.onlyStock":
    "Stock uniquement — installez un mod audio pour échanger",
  "locker.noModel": "Aucun modèle",
  "locker.stock": "Stock",
  "locker.activeModel": "Modèle actif",
  "locker.activeSound": "Son actif",
  "locker.switchToNoModel":
    "Passer à aucun modèle — retire les fichiers du modèle actuel",
  "locker.switchToStock":
    "Passer à Stock — retire le mod audio (le son d'origine est joué)",
  "locker.missingModelEdf": "Ce set n'a pas de model.edf",
  "locker.missingSoundFiles": "Il manque engine.scl ou sfx.cfg à ce set",
  "locker.switchTo": "Passer à {{name}}",
  "locker.tiedToModel": "Lié au modèle {{models}}",
  "locker.boundHint":
    "« {{sound}} » est lié au modèle « {{model}} » — il suit ce modèle. Cliquez pour délier.",
  "locker.unboundHint":
    "Liez le son actif « {{sound}} » au modèle « {{model}} » pour qu'y passer amène aussi le son.",
  "locker.tieAction": "Lier « {{sound}} » à « {{model}} »",
  "locker.untieAction": "Délier « {{sound}} » de « {{model}} »",
  "locker.restored": "Fichiers de setup de {{bike}} restaurés.",
  "locker.restoredNote_one":
    "{{count}} fichier remis en place — la moto devrait se charger à nouveau.",
  "locker.restoredNote_other":
    "{{count}} fichiers remis en place — la moto devrait se charger à nouveau.",
  "locker.switchedModel":
    "Modèle de {{bike}} changé pour « {{target}} ».",
  "locker.switchedSound": "Son de {{bike}} changé pour « {{target}} ».",
  "locker.tied": "« {{sound}} » lié au modèle « {{model}} ».",
  "locker.untied": "« {{sound}} » délié du modèle « {{model}} ».",
  "locker.refreshedLive": "Actualisé en direct dans le jeu.",
  "locker.refreshFailed":
    "Actualisation instantanée échouée — resélectionnez votre profil en jeu pour la charger.",
  "locker.reselectProfile":
    "Resélectionnez votre profil dans MX Bikes pour charger l'échange.",
  "locker.loadsNextTime":
    "Sera chargé à la prochaine ouverture du jeu.",
  "locker.modelRefreshing":
    "Actualisation en jeu — si c'est la moto que vous avez sélectionnée, elle change maintenant.",
  "locker.modelFrostmodNotRunning":
    "Lancez FrostMod pour voir les changements de modèle en direct — pour l'instant, resélectionnez la moto en jeu.",
  "locker.modelReselectBike":
    "Modèle changé — resélectionnez la moto dans MX Bikes pour le voir.",
  "locker.modelFrostmodUnreachable":
    "Impossible de joindre FrostMod — resélectionnez la moto en jeu pour la charger.",
  "locker.modelRefreshWindowsOnly":
    "L'actualisation du modèle en direct est réservée à Windows — resélectionnez la moto en jeu.",
  "locker.modelInstantRefreshOff":
    "Resélectionnez la moto dans MX Bikes pour la charger (l'actualisation instantanée est désactivée).",

  // ── Enregistrement des sets en vrac ────────────────────────────────────────
  "swaps.model": "modèle",
  "swaps.modelSets_one": "{{count}} changement de modèle",
  "swaps.modelSets_other": "{{count}} changements de modèle",
  "swaps.soundSets_one": "{{count}} mod audio",
  "swaps.soundSets_other": "{{count}} mods audio",
  "swaps.and": "{{a}} et {{b}}",
  "swaps.noSets": "0 set",
  "swaps.foundTitle": "{{summary}} trouvé(s)",
  "swaps.description":
    "Ces dossiers traînent en vrac dans vos motos. Enregistrez-les pour déplacer chacun dans la bonne bibliothèque — {{modelsFolder}} pour les modèles, {{soundsFolder}} pour les sons — afin qu'ils apparaissent dans le Casier.",
  "swaps.registered_one": "{{count}} set enregistré.",
  "swaps.registered_other": "{{count}} sets enregistrés.",
  "swaps.nothingMoved": "Rien n'a été déplacé.",
  "swaps.skipped_one": "{{count}} ignoré (nom déjà utilisé).",
  "swaps.skipped_other": "{{count}} ignorés (noms déjà utilisés).",
  "swaps.foldersCreated_one":
    "Dossiers de bibliothèque créés pour {{count}} moto.",
  "swaps.foldersCreated_other":
    "Dossiers de bibliothèque créés pour {{count}} motos.",
  "swaps.foldersCreatedDesc":
    "Vos dossiers modèle / son sont restés où ils étaient.",
  "swaps.justCreateFolders": "Créer seulement les dossiers",
  "swaps.registerAndMove": "Enregistrer et déplacer",
  "swaps.fileCount_one": "{{count}} fichier",
  "swaps.fileCount_other": "{{count}} fichiers",

  // ── Installation ───────────────────────────────────────────────────────────
  "install.installed": "{{title}} installé",
  "install.reloadedDesc":
    "Jeu rechargé via FrostMod — c'est actif maintenant.",
  "install.addedDesc": "Ajouté à votre bibliothèque.",
  "install.failed": "Échec de l'installation — {{title}}",
  "install.openModPage": "Ouvrir la page du mod",
  "install.clickToOpen": "Cliquez pour ouvrir la page du mod",

  // ── Catégories (singulier) ─────────────────────────────────────────────────
  "category.track": "Circuit",
  "category.bike": "Moto",
  "category.bikePaint": "Livrée",
  "category.bikeModelSwap": "Changement de modèle",
  "category.sound": "Son",
  "category.helmet": "Casque",
  "category.helmetPaint": "Déco casque",
  "category.goggles": "Masque",
  "category.boots": "Bottes",
  "category.bootPaint": "Déco bottes",
  "category.protection": "Protections",
  "category.protectionPaint": "Déco protections",
  "category.gloves": "Gants",
  "category.outfit": "Tenue / kit",
  "category.misc": "Autre",

  // ── En-têtes de section (pluriel) ──────────────────────────────────────────
  "section.bikePaint": "Livrées",
  "section.bikeModelSwap": "Changements de modèle",
  "section.sound": "Sons",
  "section.helmet": "Casques",
  "section.helmetPaint": "Décos casque",
  "section.boots": "Bottes",
  "section.bootPaint": "Décos bottes",
  "section.protection": "Protections",
  "section.protectionPaint": "Décos protections",
  "section.gloves": "Gants",
  "section.outfit": "Tenue / kit",

  // ── Destinations d'installation ────────────────────────────────────────────
  "dest.bikesRoot": "Motos (racine)",
  "dest.tracksRoot": "Circuits (racine)",
  "dest.bikeFolder": "{{name}} — dossier moto",
  "dest.bikePaints": "{{name}} — décos",
  "dest.helmetsNewModel": "Casques (nouveau modèle)",
  "dest.bootsNewModel": "Bottes (nouveau modèle)",
  "dest.protectionNewModel": "Protections (nouveau modèle)",
  "dest.riderModelsNew": "Modèles de pilote (nouveau modèle)",
  "dest.animationsNewStyle": "Styles de pilotage (nouvelle animation)",
  "dest.helmetPaintsFor": "{{name}} · décos casque",
  "dest.gogglesFor": "{{name}} · masque",
  "dest.bootPaintsFor": "{{name}} · décos bottes",
  "dest.protectionPaintsFor": "{{name}} · décos protections",
  "dest.outfitFor": "{{name}} · tenue / kit",
  "dest.suitPaintsFor": "{{name}} · décos combinaison",
  "dest.glovesFor": "{{name}} · gants",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay en jeu",
  "overlay.enable": "Activer l'overlay en jeu",
  "overlay.enableDesc": "Appuie sur un raccourci pendant que {{game}} tourne pour afficher Presets, Locker et Browse par-dessus le jeu — sans alt-tab. Les presets et les changements de modèle s'appliquent au jeu en cours.",
  "overlay.shortcut": "Raccourci de l'overlay",
  "overlay.shortcutDesc": "Fonctionne même quand le jeu a le focus. Esc ferme l'overlay et rend la main au jeu.",
  "overlay.borderlessTitle": "Lance {{game}} en sans bordure ou en fenêtre",
  "overlay.borderlessNote": "Rien ne peut s'afficher par-dessus un jeu qui garde l'écran en plein écran exclusif — l'overlay compris. Passe {{game}} en Borderless (ou Windowed) dans Options → Video et il s'affiche au-dessus du jeu comme prévu.",
  "overlay.gameRunning": "{{game}} est lancé",
  "overlay.gameNotRunning": "{{game}} n'est pas lancé",
  "overlay.showNow": "Afficher l'overlay maintenant",
  "overlay.showFailed": "Impossible d'ouvrir l'overlay",
  "overlay.hotkeyTaken": "Une autre application utilise ce raccourci",
  "overlay.hotkeyTakenDesc": "La combinaison revient à l'application qui l'a demandée en premier, donc l'overlay ne s'ouvre jamais. Choisis-en une autre ci-dessus — le mute de Discord est le coupable habituel.",
  "overlay.fullscreenNow": "{{game}} est en plein écran exclusif en ce moment",
  "overlay.fullscreenNowDesc": "L'overlay s'ouvre quand même — c'est le jeu qui se dessine par-dessus. Passe en sans bordure ou en fenêtre dans Options → Video.",
  "overlay.notWorking": "Tu l'as pressé et rien ne s'est passé ?",
  "overlay.notWorkingDesc": "Vérifie le raccourci ci-dessus : une autre application a peut-être déjà cette combinaison, et en choisir une libre suffit à régler ça.",
  "overlay.pressKeys": "Appuie sur les touches…",
  "overlay.needModifier": "Ajoute un modificateur",
  "overlay.needModifierDesc": "Maintiens Ctrl, Alt ou Shift pour que le raccourci ne se déclenche pas pendant que tu écris.",
  "overlay.shortcutUpdated": "Raccourci de l'overlay mis à jour",
  "overlay.shortcutRejected": "Impossible d'utiliser ce raccourci",
  "overlay.registerFailed": "Impossible d'enregistrer le raccourci de l'overlay",
  "overlay.toClose": "{{hotkey}} pour fermer",
  "overlay.closeTitle": "Fermer l'overlay (Esc)",
  "overlay.openMain": "Ouvrir l'app complète",
  "overlay.openMainTitle": "Ferme l'overlay et ouvre la fenêtre principale de MXB App",
  "overlay.needsSetup": "Termine d'abord la configuration de MXB App dans sa fenêtre principale — elle doit savoir où se trouve ton dossier {{game}}.",
  "overlay.fullscreenBlocked": "L'overlay ne peut pas s'afficher par-dessus le plein écran exclusif",
  "overlay.fullscreenBlockedDesc": "Passe {{game}} en sans bordure ou en fenêtre dans Options → Video, puis réessaie le raccourci.",

  // Présentation de la version — la fenêtre « nouveautés » affichée une fois après une mise à jour.
  "showcase.eyebrow": "Tout juste mis à jour",
  "showcase.title": "Nouveautés de la {{version}}",
  "showcase.subtitle": "Le gros morceau d'abord. Tout le reste de cette version est dans les notes.",
  "showcase.whileGameRunning": "pendant que MX Bikes tourne",
  "showcase.releaseNotes": "Lire les notes de version",
  "showcase.gotIt": "Compris",
  "showcase.v091.hero.title": "Peins directement sur le gabarit",
  "showcase.v091.hero.body":
    "Le Designer savait placer images et textes sur les planches d'une déco, mais il ne laissait poser aucun pixel à la main — un dégradé sur un ouïe voulait dire partir dans un éditeur d'images et revenir. Il a maintenant sa boîte à outils : pinceau doux avec taille, bord et intensité, gomme, dégradé, remplissage, rectangle, ellipse et ligne. Tout arrive sur la planche et sur le modèle 3D en même temps, pendant que tu fais glisser.",
  "showcase.v091.gradient":
    "Un dégradé qui emmène une couleur vers une autre. Fais glisser pour dire où se fait la transition : avant c'est la première couleur, après la seconde. Linéaire ou radial, et il peut se fondre vers rien plutôt que vers une couleur.",
  "showcase.v091.paintLayer":
    "La peinture va sur son propre calque, donc elle a opacité, fusion et empilement comme le reste — et le gabarit en dessous n'est jamais touché. Masque le calque et tu retrouves le gabarit intact. ⌘Z annule les tracés.",
  "showcase.v091.macos":
    "Jouer et Rejoindre un serveur fonctionnent sur macOS, via la bouteille CrossOver, Whisky ou Wine qui contient le jeu — et l'app trouve seule une installation en bouteille au lieu de te demander le chemin.",
  "showcase.v091.steamos":
    "Sur SteamOS, l'app Linux s'ouvre sur son interface au lieu d'un écran blanc.",
  "showcase.v090.hero.title": "Transforme tes images en une déco que le jeu charge",
  "showcase.v090.hero.body":
    "Un nouvel onglet Décos fabrique des décos à partir de simples fichiers image — TGA, PNG, JPG — et les installe là où le jeu les cherche : une livrée de moto, une déco de casque ou de masque, la tenue ou les gants de ton pilote. Décompresse une déco existante pour obtenir un gabarit vraiment adapté au modèle, retouche-la dans n'importe quel éditeur et remets-la telle quelle. Le studio vérifie tes noms de fichiers face à ceux que le maillage utilise avant l'enregistrement, puis affiche le résultat sur le vrai modèle.",
  "showcase.v090.reshade":
    "Parcours, installe et change de preset ReShade depuis l'app — avec une entrée Aucun pour comparer au rendu d'origine, et un avertissement quand un preset réclame des effets que tu n'as pas.",
  "showcase.v090.bundles":
    "Partage un preset en paquet complet : le code emporte les mods eux-mêmes — déco, casque et masque, tenue, gants, bottes, pneus. Import complet place chaque fichier là où le jeu le lit, si bien qu'une personne au dossier mods vide porte exactement ce que tu as monté.",
  "showcase.v090.purchases":
    "Mes achats se connecte à ton compte mxbikes-shop.com et installe ce que tu as déjà payé, via la même fiche de contrôle qu'un glisser-déposer.",
  "showcase.v090.ridingStyles":
    "Les presets peuvent utiliser un style de pilotage que tu as installé, pas seulement les deux du jeu — et un preset partagé l'emporte avec lui.",
  "showcase.v090.frostmod":
    "Quand FrostMod meurt sur une bibliothèque Windows manquante, l'app la nomme clairement et l'installe pour toi. FrostMod peut aussi être arrêté depuis l'app, quel que soit ce qui l'a lancé.",
  "showcase.v090.updates":
    "Installer par-dessus une copie en cours d'exécution ne bloque plus sur « erreur d'ouverture du fichier en écriture », et un second lancement ramène ta fenêtre au lieu d'ouvrir une deuxième copie.",
  "showcase.v080.hero.title": "MXB App gère aussi GP Bikes",
  "showcase.v080.hero.body":
    "Choisis ton jeu au premier lancement, ou change quand tu veux dans les Réglages : toute l'app suit — Bibliothèque, Gérer, Presets, Jouer, et un onglet Parcourir servi par gpb-mods.com. Les dossiers pilote de GP sont lus comme ceux de GP, pas comme ceux de MX Bikes, et FrostMod y recharge à chaud aussi. Chaque jeu garde ses propres dossiers : ta configuration MX Bikes n'est pas touchée.",
  "showcase.v080.shop":
    "Un onglet Boutique parcourt mxbikes-shop.com et installe ce que tu as acheté, sans quitter l'app.",
  "showcase.v080.dropzone":
    "Dépose n'importe quoi sur la fenêtre. L'app devine ce qu'est chaque fichier, montre où il va et ce qu'il remplacerait, et te laisse reclasser chaque ligne avant l'installation.",
  "showcase.v080.destinations":
    "Les mods atterrissent dans le dossier que le jeu lit vraiment — une déco sur sa moto, une déco de casque sur son casque, une combinaison GP sur ton modèle de pilote.",
  "showcase.v080.protection":
    "L'emplacement protections fonctionne : chaque pièce dessinée droite et entière, et installée là où le jeu la cherche.",
  "showcase.v080.faster":
    "Les vignettes sont mises en cache et dessinées à la taille affichée : Parcourir et la Boutique s'ouvrent bien plus vite.",
  "showcase.v070.hero.title": "Un overlay en jeu, sur un raccourci",
  "showcase.v070.hero.body": "Ouvre Preset, Locker et Browse par-dessus MX Bikes — sans alt-tab. Esc rend la main aussitôt, et un preset choisi ici arrive sur la session que tu es en train de rouler. Joue en sans bordure ou en fenêtre : rien ne peut s'afficher par-dessus le plein écran exclusif.",
  "showcase.v070.hero.action": "Configurer l'overlay",
  "showcase.v070.languages": "MXB App parle six langues — choisis la tienne dans Paramètres → Apparence.",
  "showcase.v070.browse": "Browse trie par les plus populaires, et les cartes affichent les notes en étoiles.",
  "showcase.v070.play": "Un bouton Play dans la barre latérale lance MX Bikes.",
  "showcase.v070.paint": "Les motos portent à nouveau la bonne déco — les Kawasaki KX et Yamaha YZ sont corrigées.",
  "manage.help":
    "MX Bikes charge tous les mods de votre dossier au démarrage. Donnez à un preset la piste sur laquelle il court, cliquez sur Mode course et tout le reste s'écarte — rien n'est supprimé, le contenu part dans un dossier d'attente jusqu'à ce que vous le rameniez.",
  "manage.tabRace": "Presets de course",
  "manage.tabMods": "Mods",
  "manage.disabledCount_one": "{{count}} mod désactivé",
  "manage.disabledCount_other": "{{count}} mods désactivés",
  "manage.restoreAll": "Tout réactiver",
  "manage.restoreTitle": "Tout remettre en place ?",
  "manage.restoreBody":
    "Les {{count}} mods désactivés retournent exactement dans les dossiers d'où ils viennent. MX Bikes les rechargera tous.",
  "manage.restored_one": "{{count}} mod remis en place.",
  "manage.restored_other": "{{count}} mods remis en place.",
  "manage.applyLookTo": "Appliquer le look à",
  "manage.applyLookHelp":
    "Le mode course écrit la déco et l'équipement du preset sur ce profil et cette moto, comme l'onglet Presets. Laissez l'un des deux vide pour ne déplacer que le contenu sans toucher à votre look.",
  "manage.noPresets": "Aucun preset enregistré — créez-en un dans l'onglet Presets.",
  "manage.noContentYet": "Pas encore de contenu de course — ajoutez une piste pour utiliser le mode course",
  "manage.noTrack": "Aucune piste",
  "manage.pinnedCount_one": "{{count}} épinglé",
  "manage.pinnedCount_other": "{{count}} épinglés",
  "manage.editContent": "Modifier le contenu",
  "manage.raceMode": "Mode course",
  "manage.raceTitle": "Courir avec « {{name}} » ?",
  "manage.raceBody":
    "Garde {{keep}} mods et en écarte {{disable}}, pour que MX Bikes ne charge que le contenu de cette course.",
  "manage.raceReEnable_one": "{{count}} mod désactivé dont ce preset a besoin revient.",
  "manage.raceReEnable_other": "{{count}} mods désactivés dont ce preset a besoin reviennent.",
  "manage.raceLook": "Sa déco et son équipement vont sur {{bike}} dans le profil {{profile}}.",
  "manage.raceNoLook": "Contenu seul — choisissez un profil et une moto ci-dessus pour appliquer aussi le look.",
  "manage.raceNoBike":
    "Aucun mod de moto n'est gardé — il ne resterait que les motos d'origine du jeu. Épinglez la moto que vous pilotez dans Toujours garder.",
  "manage.raceGameRunning":
    "MX Bikes est ouvert. Les fichiers qu'il utilise ne peuvent pas être déplacés — fermez d'abord le jeu.",
  "manage.raceUnresolved": "Pas installés, ils resteront donc d'origine : {{slots}}",
  "manage.raceGo": "Préparer la course",
  "manage.raceApplied": "Prêt à courir « {{name}} » — {{count}} mods écartés.",
  "manage.contentSaved": "Contenu de course enregistré pour « {{name}} ».",
  "manage.contentTitle": "Contenu de course de « {{name}} »",
  "manage.contentBody":
    "La déco, l'équipement et le model swap du preset sont trouvés tout seuls. Ceci sert au reste : la piste, les modèles d'équipement à garder en plus, et les packs dont une course a besoin de toute façon.",
  "manage.paneTracks": "Pistes",
  "manage.paneHelmets": "Casques",
  "manage.paneBoots": "Bottes",
  "manage.paneProtection": "Protections",
  "manage.paneKeep": "Toujours garder",
  "manage.paneTracksHint": "La piste (ou les pistes) à laquelle ce preset est destiné.",
  "manage.paneGearHint":
    "Modèles supplémentaires à laisser dans le sélecteur du jeu. L'équipement du preset est gardé automatiquement — cochez ici ce que vous voulez encore pouvoir choisir. Tout ce qui n'est pas coché s'écarte.",
  "manage.paneKeepHint":
    "Les mods qui restent actifs quoi qu'il arrive — le pack OEM, la moto de ce preset, un mod de son.",
  "manage.notInstalled": "pas installé",
  "manage.off": "off",
  "manage.enabledOne": "{{name}} activé.",
  "manage.disabledOne": "{{name}} désactivé.",
  "manage.enabledMany_one": "{{count}} mod activé.",
  "manage.enabledMany_other": "{{count}} mods activés.",
  "manage.disabledMany_one": "{{count}} mod désactivé.",
  "manage.disabledMany_other": "{{count}} mods désactivés.",
  "manage.enableShown": "Activer les affichés ({{count}})",
  "manage.disableShown": "Désactiver les affichés ({{count}})",
  "manage.noMods": "Aucun mod installé pour l'instant.",
  "manage.someFailed_one": "{{count}} mod n'a pas pu être déplacé : {{first}}",
  "manage.someFailed_other": "{{count}} mods n'ont pas pu être déplacés : {{first}}",
  "manage.deleteTitle": "Supprimer {{name}} ?",
  "manage.deleteBody": "Il part à la corbeille, vous pouvez donc encore le récupérer.",
  "manage.deleted": "{{name}} supprimé.",
  "game.label": "Jeu",
  "game.switch": "Changer de jeu",
  "game.switchFailed": "Impossible de changer de jeu",
  "settings.instantRefreshMxOnly": "MX Bikes uniquement — {{game}} ne recharge pas les profils à chaud.",
  "modType.misc": "Divers",
  "modType.miscInline": "extras",
  "browseCat.raceTracks": "Circuits",
  "browseCat.kartTracks": "Circuits de karting",
  "browseCat.others": "Autres",
  "browseCat.riderModels": "Modèles de pilote",
  "browseCat.suitPaints": "Peintures de combinaison",
  "browseCat.helmetModels": "Modèles de casque",
  "browseCat.plugins": "Plugins",
  "browseCat.tools": "Outils",
  "browseCat.menuBackgrounds": "Fonds de menu",
  "category.animation": "Style de pilotage",
  "section.animation": "Styles de pilotage",
  "modDetail.restartHint": "Redémarrez {{game}} pour prendre en compte les nouveaux {{kind}}.",
  "modDetail.protonHint": "Les fichiers Proton Drive sont chiffrés : impossible de les télécharger automatiquement.",
  "setup.whichGame": "Quel jeu configurez-vous ? Vous pourrez ajouter l'autre plus tard.",
  "setup.switchLater": "Vous pouvez changer de jeu à tout moment dans les Paramètres.",
  "setup.chooseDifferentGame": "Choisir un autre jeu",
  // ── Dropzone ───────────────────────────────────────────────────────────────
  "drop.dropHere": "Déposez pour installer",
  "drop.dropHint": "Archives, .pkz, décos, dossiers — tout ce qui concerne {{game}}",
  "drop.scanning": "Analyse en cours…",
  "drop.found_one": "{{count}} élément trouvé",
  "drop.found_other": "{{count}} éléments trouvés",
  "drop.reviewHint": "Vérifiez les destinations, puis installez.",
  "drop.install_one": "Installer {{count}}",
  "drop.install_other": "Installer {{count}}",
  "drop.fileCount_one": "{{count}} fichier",
  "drop.fileCount_other": "{{count}} fichiers",
  "drop.replaces_one": "Remplace {{count}} fichier existant",
  "drop.replaces_other": "Remplace {{count}} fichiers existants",
  "drop.willReplace_one": "{{count}} fichier existant sera remplacé",
  "drop.willReplace_other": "{{count}} fichiers existants seront remplacés",
  "drop.nothingOverwritten": "Rien de ce qui existe ne sera remplacé.",
  "drop.needChoice_one": "{{count}} élément attend encore une destination",
  "drop.needChoice_other": "{{count}} éléments attendent encore une destination",
  "drop.skipped_one": "{{count}} fichier ignoré",
  "drop.skipped_other": "{{count}} fichiers ignorés",
  "drop.pickDestinationFirst": "Choisissez sa destination avant d'installer.",
  "drop.chooseDestination": "Choisir une destination",
  "drop.searchDestinations": "Rechercher motos et équipement…",
  "drop.noDestinations": "Rien d'installé pour l'instant où le mettre.",
  "drop.destAsPackaged": "Tel quel",
  "drop.include": "Inclure cet élément",
  "drop.exclude": "Laisser cet élément de côté",
  "drop.installed_one": "{{count}} élément installé",
  "drop.installed_other": "{{count}} éléments installés",
  "drop.itemFailed": "Impossible d'installer {{name}}",
  "drop.installFailed": "Échec de l'installation",
  "drop.scanFailed": "Impossible de lire ce que vous avez déposé",
  "drop.previewFailed": "Impossible de vérifier cette destination",
  "drop.nothingUsable": "Rien d'installable dans ce dépôt",
  "drop.kind.modsTree": "Dossier mods",
  "drop.kind.track": "Circuit",
  "drop.kind.bike": "Moto",
  "drop.kind.bikePaint": "Déco",
  "drop.kind.soundSet": "Son",
  "drop.kind.riderGear": "Équipement",
  "drop.kind.reshadePreset": "Préréglage ReShade",
  "drop.kind.unknown": "Inconnu",
  "drop.reason.modsTree": "Contient un dossier mods complet",
  "drop.reason.categoryDirs": "Contient des dossiers motos/circuits/pilote",
  "drop.reason.paintsBundle": "Contient un dossier paints",
  "drop.reason.soundMarkers": "engine.scl et sfx.cfg trouvés",
  "drop.reason.trackMarkers": "Fichiers de circuit trouvés",
  "drop.reason.trackPackage": "Circuit empaqueté",
  "drop.reason.bikeConfig": "Configuration de moto trouvée",
  "drop.reason.loosePaint": "Décos isolées — rien n'indique le modèle",
  "drop.reason.gearFolders": "Dossiers d'équipement trouvés",
  "drop.reason.riderTexture": "Peint le corps du pilote — une tenue",
  "drop.reason.gearTexture": "Peint un élément d'équipement",
  "drop.reason.reshadePreset": "Liste des techniques ReShade",
  "drop.reason.unrecognised": "Non reconnu — à vous de le placer",

  // ── Import (le même flux que le dépôt, en le sélectionnant) ────────────────
  "import.action": "Importer",
  "import.staging": "Lecture…",
  "import.pickFiles": "Choisir des fichiers…",
  "import.pickFolder": "Choisir un dossier…",
  "import.modFiles": "Mods et peintures",
  "import.allFiles": "Tous les fichiers",
  "import.pickFailed": "Impossible d'ouvrir le sélecteur de fichiers",
  "import.readFailed": "Impossible de lire ce que vous avez choisi",

  // ── ReShade ────────────────────────────────────────────────────────────────
  "settings.reshade": "ReShade",
  "settings.reshadeDesc": "Préréglages de post-traitement — le rendu de {{game}} à l'écran.",

  // ── Soutiens (Buy Me a Coffee) ─────────────────────────────────────────────
  "settings.supporters": "Soutiens",
  "settings.supportersDesc": "Les personnes qui font vivre MXB App sur Buy Me a Coffee.",
  "supporters.intro":
    "MXB App est gratuite, et le restera. Les cafés ci-dessous paient le temps passé dessus : celles et ceux qui les ont offerts sont la raison pour laquelle il y a une nouvelle version à installer.",
  "supporters.count_one": "{{count}} soutien",
  "supporters.count_other": "{{count}} soutiens",
  "supporters.untiered": "Soutiens",
  "supporters.since": "depuis {{date}}",
  "supporters.loading": "Chargement de la liste…",
  "supporters.refresh": "Actualiser",
  "supporters.become": "Offrez-moi un café",
  "supporters.empty": "Personne n'est encore listé ici",
  "supporters.emptyDesc":
    "La liste se met à jour toute seule : offrez un café et votre nom apparaîtra ici, sans attendre une nouvelle version.",
  "supporters.offline":
    "Impossible de joindre la liste pour l'instant — voici la dernière connue.",
  "supporters.optOut":
    "Les noms sont affichés avec accord. Un message sur Discord ou Buy Me a Coffee et le vôtre est retiré aussitôt.",

  "modType.reshade": "ReShade",
  "modType.reshadeInline": "préréglages ReShade",
  "reshade.needsGameFolder":
    "Indique ton dossier {{game}} ci-dessus et les préréglages ReShade apparaîtront ici.",
  "reshade.intro":
    "ReShade ajoute du post-traitement à {{game}}. C'est un outil gratuit distinct : installe-le une fois, puis choisis un préréglage ici.",
  "reshade.wrongApi":
    "ReShade est installé sous le nom {{dll}}, que {{game}} ne charge jamais — il utilise OpenGL. Relance l'installateur ReShade et choisis OpenGL.",
  "reshade.step1": "Télécharge l'installateur sur reshade.me.",
  "reshade.step2": "Lance-le et sélectionne {{exe}} dans ton dossier {{game}}.",
  "reshade.step3": "Choisis OpenGL quand il le demande — pas DirectX.",
  "reshade.getIt": "Obtenir ReShade",
  "reshade.recheck": "Revérifier",
  "reshade.installed": "Installé",
  "reshade.installedVersion": "Installé · {{version}}",
  "reshade.off": "Désactivé — aucun effet",
  "reshade.delete": "Supprimer le préréglage",
  "reshade.deleted": "{{name}} supprimé",
  "reshade.applied": "{{name}} est maintenant actif",
  "reshade.appliedNextLaunch":
    "{{name}} est défini — il s'appliquera au prochain lancement",
  "reshade.loosePreset": "Dans ton dossier de jeu — pas installé par MXB App",
  "reshade.missingEffects_one": "Nécessite {{list}}, qui n'est pas installé",
  "reshade.missingEffects_other":
    "Nécessite {{count}} effets non installés : {{list}}",
  "reshade.noShaders":
    "Aucun effet ReShade n'est installé, les préréglages ne changeront donc rien. Relance l'installateur ReShade et choisis un pack de shaders.",
  "reshade.noPresets":
    "Aucun préréglage — installes-en depuis Parcourir, ou dépose un .ini ici.",
  "reshade.browseHint": "Plus de préréglages dans Parcourir → ReShade.",
  "reshade.nextLaunchHint":
    "{{game}} est lancé — le changement s'appliquera au prochain démarrage.",
  // ── Paint studio ───────────────────────────────────────────────────────────
  "paints.help":
    "Transforme des .tga ou .png dessinés dans GIMP ou Photoshop en un .pnt que le jeu charge — et décompresse une déco existante pour partir de celle-ci.",
  "paints.unpack": "Décompresser une déco…",
  "paints.toDesigner": "Dessiner dessus…",
  "paints.unpacked": "{{count}} textures extraites — modifiez-les, puis enregistrez.",
  "paints.whereTitle": "Destination",
  "paints.kind.bike": "Déco de moto",
  "paints.kind.helmet": "Casque",
  "paints.kind.goggles": "Masque",
  "paints.kind.profileGoggles": "Masque (pilote)",
  "paints.kind.boots": "Bottes",
  "paints.kind.protection": "Protections",
  "paints.kind.kit": "Tenue du pilote",
  "paints.kind.gloves": "Gants",
  "paints.model": "Pour",
  "paints.profile": "Profil de pilote",
  "paints.noModels": "Rien d'installé à peindre pour l'instant.",
  "paints.destPath": "Installé dans mods/{{rel}}",
  "paints.saveElsewhere": "Enregistrer dans un dossier…",
  "paints.saveTitle": "Nom et enregistrement",
  "paints.namePlaceholder": "Nommez cette déco…",
  "paints.save": "Enregistrer la déco",
  "paints.saved": "Enregistrée dans {{path}}",
  "paints.preview3d": "Aperçu 3D",
  "paints.openFolder": "Ouvrir le dossier",
  "paints.sheetsTitle": "Textures",
  "paints.reload": "Recharger depuis le disque",
  "paints.addImages": "Ajouter des images…",
  "paints.expected": "Les décos ici utilisent :",
  "paints.empty":
    "Ajoutez un .tga ou .png par texture. Ce sont les noms qui comptent, pas les fichiers : une texture nommée « livery » se pose sur la pièce qui demande « livery ». Décompresser une déco existante donne les bons noms.",
  "paints.resized": "Redimensionnée {{from}} → {{to}} — le jeu exige des puissances de deux.",
  "paints.unknownName": "Aucune déco ici n'utilise ce nom : elle pourrait ne pas apparaître sur le modèle.",
  "paints.needSheets": "Ajoutez au moins une image.",
  "paints.needName": "Nommez cette déco.",
  "paints.needTextureNames": "Chaque texture doit avoir un nom.",
  "paints.duplicateName": "Deux textures s'appellent « {{name}} ».",
  "paints.needTarget": "Choisissez la destination de la déco.",
  "paints.replaceTitle": "Remplacer cette déco ?",
  "paints.replaceBody": "{{path}} existe déjà. L'enregistrement la remplace.",
  "paints.replace": "Remplacer",

  // ── Designer (l'éditeur par calques) ──────────────────────────────────────────
  "designer.help":
    "Dessine une déco sur les planches que le jeu lit vraiment, et regarde-la sur le modèle au fur et à mesure. Pars d'une déco installée pour avoir les bons noms de planches, peins dessus au pinceau, au dégradé ou avec des formes, empile images et textes par-dessus, puis enregistre : ce qui sort est un .pnt que le jeu charge, pas un export à convertir.",
  "designer.empty":
    "Rien sur quoi dessiner pour l'instant. Pars d'une déco installée pour ce modèle — tu récupères ses planches et leurs noms — ou ajoute une planche vierge.",
  "designer.startFromPaint": "Partir d'une déco…",
  "designer.blankSheet": "Planche vierge",
  "designer.sheets": "Planches",
  "designer.moveDown": "Descendre",
  "designer.moveUp": "Monter",
  "designer.noSheetsFound":
    "Cette déco n'a produit aucune planche, il n'y a donc rien sur quoi dessiner.",
  "designer.loadedSheets": "{{count}} planche(s) chargée(s) — dessine dessus et enregistre.",
  "designer.sheetName": "Nom de texture",
  "designer.editSheet": "Modifier cette planche",
  "designer.addImage": "Ajouter une image",
  "designer.addText": "Ajouter du texte",
  "designer.newTextValue": "TEXTE",
  "designer.layers": "Calques",
  "designer.showRail": "Afficher planches et calques",
  "designer.hideRail": "Masquer planches et calques",
  "designer.noLayers":
    "Aucun calque — ajoute une image, du texte ou un calque de peinture pour dessiner dessus.",
  "designer.layerCount": "{{count}} calque(s)",
  "designer.layerTitle": "Calque sélectionné",
  "designer.hide": "Masquer",
  "designer.show": "Afficher",
  "designer.raise": "Vers l'avant",
  "designer.lower": "Vers l'arrière",
  "designer.scale": "Taille",
  "designer.rotation": "Rotation",
  "designer.opacity": "Opacité",
  "designer.blend": "Fusion",
  "designer.blend.normal": "Normal",
  "designer.blend.multiply": "Produit",
  "designer.blend.screen": "Superposition claire",
  "designer.blend.overlay": "Incrustation",
  "designer.text": "Texte",
  "designer.font": "Police",
  "designer.size": "Taille du texte",
  "designer.colour": "Couleur",
  "designer.outline": "Contour",
  "designer.needSheets": "Ajoute une planche sur laquelle dessiner.",
  "designer.noModelFound":
    "« {{model}} » n'est pas dans ta bibliothèque, il n'y a donc rien pour l'afficher.",
  "designer.noBikePreview":
    "Cette version ne lit pas la géométrie des motos, une déco n'a donc pas de modèle où se poser. Tout le reste s'enregistre normalement.",
  "designer.noPreviewForGame":
    "L'aperçu 3D est réservé à MX Bikes pour l'instant : les modèles de {{game}} ont besoin de leurs propres liaisons de pièces. Tout le reste fonctionne pareil et la déco s'enregistre normalement.",
  "designer.gearNote":
    "Affiché sur le pilote d'origine — ta propre tenue n'est pas chargée ici.",
  "designer.resetView": "Réinitialiser la vue",

  // ── Designer › les outils de peinture ─────────────────────────────────────────
  "designer.paint": "Peinture",
  "designer.addPaint": "Calque de peinture",
  "designer.paintLayerName": "Peinture",
  "designer.undoStroke": "Annuler le tracé",
  "designer.redoStroke": "Rétablir le tracé",
  "designer.tool.move": "Déplacer",
  "designer.tool.brush": "Pinceau",
  "designer.tool.eraser": "Gomme",
  "designer.tool.gradient": "Dégradé",
  "designer.tool.fill": "Remplissage",
  "designer.tool.rect": "Rectangle",
  "designer.tool.ellipse": "Ellipse",
  "designer.tool.line": "Ligne",
  "designer.moveHint":
    "Fais glisser les calques sur la planche pour les placer. Choisis un outil ci-dessus pour peindre dessus.",
  "designer.colourFrom": "Peindre avec cette couleur",
  "designer.colourTo": "Fondre vers cette couleur",
  "designer.swapColours": "Inverser les deux couleurs",
  "designer.brushSize": "Pinceau",
  "designer.hardness": "Bord",
  "designer.strength": "Intensité",
  "designer.gradient": "Dégradé",
  "designer.gradient.linear": "Linéaire",
  "designer.gradient.radial": "Radial",
  "designer.fadeOut": "Fondu",
  "designer.shape": "Style",
  "designer.shape.fill": "Plein",
  "designer.shape.outline": "Contour",
  "designer.lineWidth": "Épaisseur",
  "designer.paintHint":
    "Fais glisser sur la planche. Maintiens Maj pour rester droit, clic droit glissé pour déplacer la vue.",
  "designer.fillHint": "Clique sur la planche pour remplir tout le calque.",
  "designer.gradientHint":
    "Fais glisser sur la planche pour définir où se fait la transition. Cela remplit tout le calque : ajoute un autre calque de peinture pour garder ce qu'il y a dessous.",
};
