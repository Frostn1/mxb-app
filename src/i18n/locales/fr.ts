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
  "nav.settings": "Réglages",

  "sidebar.installing": "Installation de « {{name}} »",
  "sidebar.queued": "+{{count}} en attente",

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
  "frostmod.installedToast": "FrostMod {{version}} installé",
  "frostmod.installedToastDesc":
    "Il rechargera le jeu à chaud dès que vous ajouterez des mods.",
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
  "setup.tagline":
    "Parcourez mxb-mods, installez en un clic, et laissez FrostMod recharger le jeu pour vous.",
  "setup.modsFolder": "Dossier MX Bikes",
  "setup.autoDetect":
    "MXB App détectera automatiquement votre dossier {{hint}}. Vous pouvez aussi le choisir vous-même.",
  "setup.chooseManually": "Choisir le dossier manuellement…",
  "setup.chooseDifferent": "Choisir un autre dossier…",
  "setup.gameInstall": "Installation de MX Bikes",
  "setup.detecting": "Recherche de votre installation de MX Bikes…",
  "setup.found": "Trouvée",
  "setup.detectedAutomatically": "Détectée automatiquement",
  "setup.installNotFound":
    "Impossible de trouver automatiquement votre installation de MX Bikes — elle alimente l'aperçu 3D du pilote. Choisissez-la manuellement, ou définissez-la plus tard dans les Réglages.",
  "setup.chooseInstallManually":
    "Choisir le dossier d'installation manuellement…",
  "setup.startBrowsing": "Commencer à parcourir les mods",
  "setup.detectAndStart": "Détecter et commencer",
  "setup.pickModsFolder": "Sélectionnez votre dossier MX Bikes",
  "setup.pickInstallFolder":
    "Sélectionnez votre dossier d'installation de MX Bikes",

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

  // ── Visite guidée ──────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Faites un tour rapide",
  "tour.welcomeTour.body":
    "Quelques secondes pour voir où se trouve chaque chose. Vous pouvez passer à tout moment.",
  "tour.browse.title": "Parcourir les mods",
  "tour.browse.body":
    "Cherchez sur mxb-mods.com directement ici et installez n'importe quel circuit, moto ou déco en un seul clic.",
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
  "update.checkFailed": "Impossible de vérifier les mises à jour",
  "update.failed": "Échec de la mise à jour",

  // ── Visualiseur 3D ─────────────────────────────────────────────────────────
  "viewer.preview3d": "Aperçu 3D",
  "viewer.expand": "Agrandir",
  "viewer.paint": "Déco",
  "viewer.loadingModel": "Chargement du modèle…",
  "viewer.loadingPaint": "Chargement de la déco…",
  "viewer.loadingRider": "Chargement du pilote…",
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
  "browse.sortedByNewest": "Triés du plus récent",
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

  // ── Boutique ───────────────────────────────────────────────────────────────
  "shop.myDownloads": "Mes téléchargements",
  "shop.signInTitle": "Connectez-vous à MX Bikes Shop",
  "shop.signInBody":
    "Connectez-vous à mxbikes-shop.com pour voir et installer les circuits que vous avez achetés. Nous ouvrons le vrai site — votre mot de passe ne passe jamais par cette application.",
  "shop.signIn": "Se connecter",
  "shop.logOut": "Se déconnecter",
  "shop.signedIn": "Connecté à MX Bikes Shop",
  "shop.sessionFailed": "Impossible de récupérer votre session MX Bikes Shop",
  "shop.queuedDesc": "Installation dans votre dossier circuits.",
  "shop.loadFailed":
    "Impossible de charger vos téléchargements : {{error}}",
  "shop.empty": "Aucun téléchargement acheté trouvé sur votre compte.",

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
  "modDetail.noDownloadLink":
    "Aucun lien de téléchargement trouvé sur cette page — ouvrez-la sur mxb-mods.com.",
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
  "modDetail.viewOnSite": "Voir sur mxb-mods.com",

  // ── Réglages ───────────────────────────────────────────────────────────────
  "settings.help":
    "Configurez votre dossier de jeu, les mises à jour et les préférences de l'application.",
  "settings.gameFolder": "Dossier de jeu",
  "settings.general": "Général",
  "settings.appearance": "Apparence",
  "settings.frostmod": "FrostMod",
  "settings.about": "À propos et mises à jour",
  "settings.modsFolderDesc":
    "Là où les mods sont installés. Le modifier relance l'analyse de votre bibliothèque.",
  "settings.insideModsFolder": "Dans votre dossier MX Bikes",
  "settings.notSet": "Non défini",
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
    "Quand vous appliquez un preset pendant que MX Bikes tourne, actualise le look en jeu instantanément — sans redémarrage ni resélection de profil. Si ce n'est pas possible, il vous sera demandé de resélectionner votre profil.",
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
  "settings.setFolderFailed": "Impossible de définir le dossier",
  "settings.reDetected": "Dossier MX Bikes détecté à nouveau",
  "settings.detectFolderFailed": "Impossible de détecter le dossier",
  "settings.pickInstallFolder":
    "Sélectionnez votre dossier d'installation de MX Bikes (contient rider.pkz)",
  "settings.installSet": "Installation du jeu définie",
  "settings.installSetDesc":
    "L'aperçu 3D du pilote peut désormais charger le vrai modèle du corps.",
  "settings.setInstallFailed":
    "Impossible de définir le dossier d'installation",
  "settings.installNotFound": "Impossible de trouver MX Bikes",
  "settings.installNotFoundDesc":
    "Aucune installation Steam détectée — définissez le dossier manuellement.",
  "settings.installFound": "Installation de MX Bikes trouvée",
  "settings.detectInstallFailed":
    "Impossible de détecter le dossier d'installation",
  "settings.pickProfilesFolder":
    "Sélectionnez votre dossier de profils MX Bikes",
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
  "game.running": "MX Bikes en cours",
  "game.launch": "Lancer MX Bikes",
  "game.alreadyRunning": "MX Bikes est déjà en cours d'exécution",
  "game.launching": "Lancement de MX Bikes…",
  "game.launchFailed": "Impossible de lancer MX Bikes",

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
    "Les presets lisent vos profils ici — le chemin ci-dessous est celui que l'application utilise actuellement. C'est le dossier {{profiles}} dans votre dossier MX Bikes, ou {{documents}} si vous avez déplacé votre dossier de mods. Ne le définissez que si le vôtre est ailleurs.",
  "settings.resetToDefault": "Réinitialiser",
  "settings.gameInstallDesc":
    "Dossier d'installation du jeu (facultatif) — là où MX Bikes est installé (contient {{file}}). Définissez-le pour charger le vrai corps du pilote dans l'aperçu 3D.",
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
  "library.quick3d": "Aperçu 3D rapide",
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
  "dest.helmetPaintsFor": "{{name}} · décos casque",
  "dest.gogglesFor": "{{name}} · masque",
  "dest.bootPaintsFor": "{{name}} · décos bottes",
  "dest.protectionPaintsFor": "{{name}} · décos protections",
  "dest.outfitFor": "{{name}} · tenue / kit",
  "dest.glovesFor": "{{name}} · gants",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay en jeu",
  "overlay.enable": "Activer l'overlay en jeu",
  "overlay.enableDesc": "Appuie sur un raccourci pendant que MX Bikes tourne pour afficher Presets, Locker et Browse par-dessus le jeu — sans alt-tab. Les presets et les changements de modèle s'appliquent au jeu en cours.",
  "overlay.shortcut": "Raccourci de l'overlay",
  "overlay.shortcutDesc": "Fonctionne même quand le jeu a le focus. Esc ferme l'overlay et rend la main au jeu.",
  "overlay.borderlessNote": "Passe MX Bikes en sans bordure ou en fenêtre dans Options → Video. Rien ne peut s'afficher par-dessus un jeu en plein écran exclusif — l'overlay compris.",
  "overlay.pressKeys": "Appuie sur les touches…",
  "overlay.needModifier": "Ajoute un modificateur",
  "overlay.needModifierDesc": "Maintiens Ctrl, Alt ou Shift pour que le raccourci ne se déclenche pas pendant que tu écris.",
  "overlay.shortcutUpdated": "Raccourci de l'overlay mis à jour",
  "overlay.shortcutRejected": "Impossible d'utiliser ce raccourci",
  "overlay.registerFailed": "Impossible d'enregistrer le raccourci de l'overlay",
  "overlay.toClose": "{{hotkey}} pour fermer",
  "overlay.closeTitle": "Fermer l'overlay (Esc)",
  "overlay.needsSetup": "Termine d'abord la configuration de MXB App dans sa fenêtre principale — elle doit savoir où se trouve ton dossier MX Bikes.",
  "overlay.fullscreenBlocked": "L'overlay ne peut pas s'afficher par-dessus le plein écran exclusif",
  "overlay.fullscreenBlockedDesc": "Passe MX Bikes en sans bordure ou en fenêtre dans Options → Video, puis réessaie le raccourci.",
};
