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
  "nav.hub": "MXB Hub",
  "nav.library": "Bibliothèque",
  "nav.downloads": "Téléchargements",
  "nav.locker": "Casier",
  "nav.presets": "Presets",
  "nav.rider": "Pilote",
  "nav.pose": "Posture",
  "nav.designer": "Designer",
  "nav.paints": "Décos",
  "nav.studio": "Studio",
  "nav.servers": "Serveurs",
  "nav.manage": "Mode course",
  "nav.settings": "Réglages",

  "sidebar.installing": "Installation de « {{name}} »",
  "sidebar.installingCount": "Installation de {{count}} mods",
  "sidebar.queued": "+{{count}} en attente",
  "sidebar.expand": "Développer la barre latérale",
  "sidebar.collapse": "Réduire la barre latérale",
  "sidebar.showGroup": "Afficher ce qui se trouve sous {{name}}",
  "sidebar.hideGroup": "Masquer ce qui se trouve sous {{name}}",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Vérification de FrostMod…",
  "frostmod.running": "FrostMod actif",
  "frostmod.notRunning": "FrostMod inactif",
  "frostmod.notInGame": "FrostMod absent du jeu",
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
  "welcome.analyticsNote":
    "MXB App compte anonymement quelles pages sont utilisées. Aucun nom, aucun fichier — désactivable dans les Réglages.",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "manquant",
  "presets.missingHint":
    "Ce mod n'est pas installé — il apparaîtra en Stock dans le jeu",
  "presets.missingMods":
    "Mods manquants : {{mods}}. Installez-les pour voir ces éléments.",
  "presets.help":
    "Enregistrez un look de pilote complet et chargez-le sur une moto à la demande.",
  "presets.profile": "Profil",
  "presets.forgetBike": "Retirer la moto",
  "presets.forgetBikeOne": "Retirer {{name}} de ce profil",
  "presets.forgetBikeQ": "Retirer cette moto ?",
  "presets.forgetBikeBody":
    "« {{name}} » quitte la liste des motos de ce profil, avec le look enregistré pour elle. Rien d’installé n’est supprimé : si vous roulez de nouveau avec cette moto, le jeu la remet aussitôt.",
  "presets.bikeForgotten": "« {{name}} » retirée de ce profil.",
  "presets.forgetFailed": "Impossible de retirer cette moto",
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


  // ── Pose studio ────────────────────────────────────────────────────────────
  "pose.help": "Place le pilote — où sont les mains, l'écartement des jambes, une jambe en avant. L'aperçu seulement ; MX Bikes tire la posture du style de pilotage.",
  "pose.showing": "Affiché",
  "pose.none": "—",
  "pose.bike": "Moto",
  "pose.quick": "Postures rapides",
  "pose.quickHint": "Chacune s'ajoute à la posture, elles se cumulent. Ajuste en dessous.",
  "pose.dragHint": "Fais glisser les points sur le pilote pour bouger un membre — c'est l'articulation au-dessus de celle que tu attrapes qui tourne. Le membre va à la moitié de la vitesse du curseur ; maintiens Maj pour plus de finesse. Les curseurs servent à la torsion et aux valeurs exactes.",
  "pose.reset": "Réinitialiser",
  "pose.group.torso": "Buste et tête",
  "pose.group.arms": "Bras",
  "pose.group.hands": "Mains",
  "pose.group.legs": "Jambes",
  "pose.move.legsWide": "Jambes plus écartées",
  "pose.move.legsNarrow": "Jambes plus serrées",
  "pose.move.leftLegForward": "Jambe gauche en avant",
  "pose.move.elbowsUp": "Coudes hauts",
  "pose.move.leanIn": "Se pencher",
  "pose.move.ride": "Position de pilotage",
  "pose.axis.bend": "Flexion",
  "pose.axis.twist": "Rotation",
  "pose.axis.splay": "Écartement",
  "pose.quickWaiting": "En attente du modèle du pilote : chaque mouvement est un endroit où envoyer une articulation, il lui faut donc le rig pour savoir où elle est.",
  "pose.photo": "Photo",
  "pose.photoHint": "Le cadre net masque les points et les panneaux. La photo est enregistrée au double de la taille du panneau — ouvre l'aperçu en plein écran pour une plus grande.",
  "pose.cleanFrame": "Cadre net",
  "pose.savePhoto": "Enregistrer la photo",
  "pose.photoSaved": "Photo enregistrée",
  "pose.photoFailed": "Impossible d'enregistrer la photo",
  "pose.scene.studio": "Studio",
  "pose.scene.white": "Blanc",
  "pose.scene.sky": "Jour",
  "pose.scene.sunset": "Coucher de soleil",
  "pose.scene.dusk": "Crépuscule",

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
  "rider.unwrapTitle": "Un mod {{area}} a été installé un dossier trop bas",
  "rider.unwrapBody":
    "« {{folder}} » ne contient que {{model}}, et un mod packagé ne se charge que depuis {{area}} lui-même — ni le jeu ni cette app ne le voient. Le remonter ?",
  "rider.unwrapDone_one": "{{count}} mod remonté. Il est listé comme « {{model}} » maintenant.",
  "rider.unwrapDone_other": "{{count}} mods remontés, à commencer par « {{model}} ».",

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
  "runtime.componentVc140X86": "Microsoft Visual C++ 2015–2022 (x86)",
  "runtime.repairing": "Réparation…",
  "runtime.repairDone": "Composants réparés",
  "runtime.repairDoneDesc":
    "Redémarrez MX Bikes s'il est déjà ouvert, puis réessayez.",
  "runtime.repairNothingToDo": "Tout était déjà en place",
  "runtime.repairNothingToDoDesc":
    "Tous les composants Visual C++ sont installés et le dossier du jeu a ce qu'il lui faut. Si le jeu ne démarre toujours pas, envoyez-nous votre journal.",
  "runtime.repairPartial": "Une partie a encore besoin de vous",
  "runtime.repairPartialDesc":
    "N'a pas pu aboutir : {{what}}. Windows demande votre autorisation, ou le téléchargement n'est pas arrivé — vous pouvez l'installer à la main.",
  "runtime.repairNoGameFolder": "Aucun dossier de jeu défini",
  "runtime.repairNoGameFolderDesc":
    "Les composants sont installés, mais sans le dossier d'installation nous ne pouvons pas vérifier le dossier du jeu lui-même. Indiquez-le ci-dessus, puis réparez à nouveau.",
  "runtime.repairFailed": "Impossible de réparer les composants",
  "runtime.strayForeign": "Un fichier de votre dossier de jeu ({{what}}) fait planter MX Bikes.",
  "runtime.strayLocked": "{{what}}, dans votre dossier de jeu, fait planter MX Bikes.",
  "runtime.strayPitch":
    "C'est l'origine de l'erreur « R6034 » au lancement. Le mettre de côté suffit, et rien n'est supprimé.",
  "runtime.strayLockedPitch":
    "C'est l'origine de l'erreur « R6034 » au lancement. Fermez d'abord MX Bikes, puis mettez-le de côté.",
  "runtime.strayFix": "Le mettre de côté",
  "runtime.strayFixHint":
    "Le renomme en msvcr90.dll.disabled pour que Windows cesse de le charger. Rien n'est supprimé.",
  "runtime.strayClearing": "Déplacement…",
  "runtime.strayCleared": "Fichier mis de côté",
  "runtime.strayClearedDesc":
    "Il s'appelle désormais msvcr90.dll.disabled, dans le même dossier. Relancez MX Bikes.",
  "runtime.strayClearFailed": "Impossible de déplacer le fichier",
  "update.checkFailed": "Impossible de vérifier les mises à jour",
  "update.failed": "Échec de la mise à jour",

  // ── Visualiseur 3D ─────────────────────────────────────────────────────────
  "viewer.preview3d": "Aperçu 3D",
  "viewer.expand": "Agrandir",
  "viewer.fullscreen": "Plein écran",
  "viewer.exitFullscreen": "Quitter le plein écran",
  "viewer.paint": "Déco",
  "viewer.tyres": "Pneus",
  "viewer.tyresOwn": "Ceux de la moto",
  "viewer.loadingModel": "Chargement du modèle…",
  "viewer.loadingPaint": "Chargement de la déco…",
  "viewer.loadingRider": "Chargement du pilote…",
  "viewer.riderLoadFailed": "Aperçu obsolète — impossible de le mettre à jour",
  "viewer.both": "Les deux",
  "viewer.onBike": "Sur la moto",
  "viewer.noSeat": "Le fichier de réglages de cette moto ne dit pas où est la selle, le pilote ne peut donc pas s'y asseoir.",
  "viewer.loadingBike": "Chargement de la moto…",
  "viewer.bikeLoadFailed": "Aperçu de la moto obsolète — impossible de le mettre à jour",
  "viewer.dragToRotate": "Glisser pour pivoter",
  "viewer.scrollToZoom": "Molette pour zoomer",
  "viewer.rightDragToPan": "Clic droit glissé pour déplacer",
  "viewer.paintReloaded": "Déco rechargée",
  "viewer.pose": "Position",
  "viewer.poseRear": "Arrière",
  "viewer.poseFront": "Avant",
  "viewer.poseSteer": "Direction",
  "viewer.poseLevel": "Aligner les roues",
  "viewer.poseReset": "Réinitialiser",
  "viewer.place": "Placement",
  "viewer.placeSide": "Côté",
  "viewer.placeUp": "Hauteur",
  "viewer.placeFwd": "Avancer",
  "viewer.placeTurn": "Pivoter",
  "viewer.resizePanel": "Glisser pour redimensionner · double-clic pour réinitialiser",

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
  "browse.queuedDesc": "Il s'installera dès que son tour arrivera.",
  "browse.byAuthor": "par {{author}}",
  "browse.needsBrowser":
    "« {{title}} » doit être téléchargé depuis le navigateur",
  "browse.needsBrowserDesc":
    "{{host}} bloque les téléchargements dans l'application — ouvrez sa page pour terminer.",
  "browse.noDownload": "Aucun téléchargement trouvé pour « {{title}} »",
  "browse.serverOnly": "« {{title}} » ne propose que des fichiers serveur",
  "browse.serverOnlyDesc":
    "Ouvrez le mod pour voir ses téléchargements — une version pour serveur dédié n'est pas installée à votre place.",
  "browse.quickInstallFailed":
    "Impossible d'installer rapidement « {{title}} »",
  "browse.queuedBulk_one": "{{count}} mod en file d'attente",
  "browse.queuedBulk_other": "{{count}} mods en file d'attente",
  "browse.queuedBulkDesc": "Ils s'installeront l'un après l'autre.",

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

  // ── MXB Hub (shop.mxb-hub.com) ──
  "hub.help":
    "Parcourez la place de marché MXB Hub sur shop.mxb-hub.com et installez ce que vous possédez déjà. L'achat se fait toujours sur le site de la boutique ; connectez-vous dans Mes achats pour installer vos commandes depuis ici.",
  "hub.count_one": "{{count}} article",
  "hub.count_other": "{{count}} articles",
  "hub.searchPlaceholder": "Rechercher sur MXB Hub…",
  "hub.myDownloads": "Mes achats",
  "hub.signInTitle": "Se connecter à MXB Hub",
  "hub.signInBody":
    "Connectez-vous à shop.mxb-hub.com pour voir et installer tout ce que vous possédez, mods gratuits compris. Nous ouvrons le vrai site : votre mot de passe ne touche jamais cette application.",
  "hub.signIn": "Se connecter",
  "hub.signedIn": "Connecté à MXB Hub",
  "hub.sessionFailed": "Impossible de récupérer votre session MXB Hub",
  "hub.empty": "Aucun téléchargement trouvé sur votre compte MXB Hub.",
  "hubSort.newest": "Plus récents",
  "hubSort.popular": "Plus populaires",
  "hubSort.priceAsc": "Prix : croissant",
  "hubSort.priceDesc": "Prix : décroissant",
  "hubSort.nameAsc": "Nom (A–Z)",
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
  "installDialog.recommendedBadge": "Recommandé",
  "installDialog.matchedBadge": "Votre moto",
  "installDialog.browserBadge": "Navigateur",
  "installDialog.serverBadge": "Serveur",
  "installDialog.serverBuildNote": "Version serveur dédié — pas pour jouer",
  "installDialog.serverFiles_one": "1 fichier pour serveur dédié",
  "installDialog.serverFiles_other": "{{count}} fichiers pour serveur dédié",
  "installDialog.serverOnlyNotice":
    "Ici, chaque téléchargement est une version pour serveur dédié. N'en installez une que si vous hébergez un serveur — elle n'ajoute rien à piloter.",
  "installDialog.moreMirrors_one": "1 autre miroir",
  "installDialog.moreMirrors_other": "{{count}} autres miroirs",
  "installDialog.perBikeHint":
    "Chaque téléchargement correspond à une moto différente — sélectionné automatiquement selon votre choix. Choisissez le pack « all bikes » pour toutes les motos d'un coup.",

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
  "modDetail.loadFailed": "Impossible de charger ce mod",
  "modDetail.copied": "Copié",
  "modDetail.copy": "Copier",
  "modDetail.addToLibrary": "Ajouter à la bibliothèque",
  "modDetail.host": "Hébergeur",
  "modDetail.installsTo": "Installe dans",
  "modDetail.noDownloadLink": "Aucun lien de téléchargement trouvé sur cette page — ouvrez-la sur {{site}}.",
  "modDetail.serverOnlyNotice":
    "Cette page ne propose que des fichiers pour serveur dédié. Ils s'installent, mais il n'y a rien à piloter en jeu.",
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
  "modDetail.chooseWhatToInstall": "Choisissez quoi installer",
  "modDetail.resolving": "Résolution du téléchargement…",
  "modDetail.finishInBrowser": "Terminez dans votre navigateur",
  "modDetail.viewOnSite": "Voir sur {{site}}",

  // ── Réglages ───────────────────────────────────────────────────────────────
  "settings.help":
    "Configurez votre dossier de jeu, les mises à jour et les préférences de l'application.",
  "settings.groupSetup": "Configuration",
  "settings.groupApp": "App",
  "settings.groupAdvanced": "Avancé",
  "settings.groupAbout": "À propos",
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
  "settings.analytics": "Partager des statistiques d'usage anonymes",
  "settings.analyticsDesc":
    "Envoie un identifiant aléatoire, la version de l'app et le nombre d'utilisations de chaque page et fonction, pour savoir quoi construire. Jamais votre nom, vos fichiers ni votre adresse.",
  "settings.runInBackground": "Continuer en arrière-plan",
  "settings.runInBackgroundDesc":
    "Fermer la fenêtre place MXB App dans la barre d'état pour que FrostMod reste connecté. Quittez depuis l'icône de la barre.",
  "settings.launchAtStartup": "Lancer au démarrage",
  "settings.launchAtStartupDesc":
    "Démarrer MXB App automatiquement à votre connexion.",
  "settings.instantRefresh": "Actualisation instantanée des presets",
  "settings.instantRefreshDesc":
    "Quand vous appliquez un preset pendant que {{game}} tourne, actualise le look en jeu instantanément — sans redémarrage ni resélection de profil. Si ce n'est pas possible, il vous sera demandé de resélectionner votre profil.",
  "settings.paintSync": "Synchro des peintures",
  "settings.paintSyncDesc":
    "Partage les peintures que vous portez avec les autres pilotes de votre serveur, et installe les leurs, pour que la grille ressemble à ce que chacun a voulu. Fonctionne tout seul sur n'importe quel serveur — rien à configurer, ni pour vous ni pour l'hébergeur.",
  "settings.instantRefreshWindowsOnly":
    "Actualiser le look en jeu sans redémarrer suppose d'intervenir dans le jeu en cours, ce que seule la version Windows peut faire — il vous sera demandé de resélectionner votre profil à la place.",
  "settings.autoRunFrostmod": "Lancer FrostMod automatiquement",
  "settings.autoRunFrostmodDesc":
    "Démarrer FrostMod en arrière-plan à chaque ouverture de MXB App.",
  "settings.rcamKeys":
    "Touches de la caméra de replay",
  "settings.rcamKeysDesc":
    "Le jeu lit la même touche au même moment : choisis des touches que tes commandes de caméra n'utilisent pas — les touches de fonction et le pavé numérique sont souvent libres.",
  "settings.rcamReset":
    "Réinitialiser",
  "settings.rcamPressKey":
    "Appuie sur une touche",
  "settings.rcamUnbound":
    "Non attribuée",
  "settings.rcamCaptureHint":
    "Appuie sur la touche voulue. Échap annule, Retour arrière efface l'attribution.",
  "settings.rcamAppliesHint":
    "Prise en compte à la prochaine ouverture de l'éditeur en jeu — sans redémarrer.",
  "settings.rcamDuplicate":
    "Deux actions partagent une touche.",
  "settings.rcamUnsupportedKey":
    "FrostMod ne peut pas utiliser cette touche.",
  "settings.rcamSaveFailed":
    "Impossible d'enregistrer les touches",
  "settings.rcamActionSetkey":
    "Placer un point",
  "settings.rcamActionDelete":
    "Supprimer le point",
  "settings.rcamActionClear":
    "Effacer le trajet",
  "settings.rcamActionPlay":
    "Lecture / arrêt",
  "settings.rcamActionPrev":
    "Point précédent",
  "settings.rcamActionNext":
    "Point suivant",
  "settings.rcamActionSave":
    "Enregistrer le trajet",
  "settings.rcamActionClean":
    "Masquer la surcouche",
  "settings.rcamActionLoad":
    "Charger le trajet",
  "settings.rcamActionEase":
    "Interpolation",
  "settings.rcamActionTarget":
    "Viser un pilote",
  "settings.rcamActionNudgeback":
    "Decaler en arriere",
  "settings.rcamActionNudgefwd":
    "Decaler en avant",
  "settings.rcamActionUndo":
    "Annuler",
  "settings.rcamActionRig":
    "Support camera",
  "settings.rcamActionRetime":
    "Repartir par distance",
  "settings.rcamActionPreview":
    "Afficher le trajet",
  "settings.rcamActionCurve":
    "Style de courbe",
  "settings.rcamActionAnchor":
    "Axe du trajet",
  "settings.rcamActionAutofov":
    "FOV automatique",
  "settings.rcamPaths":
    "Trajets de camera replay",
  "settings.rcamPathsDesc":
    "Les neuf emplacements ou FrostMod enregistre les trajets. Ouvrez-en un pour voir ou tombent ses cles, changer sa facon de voler ou le transmettre.",
  "settings.rcamPathsEmpty":
    "Rien d'enregistre pour l'instant. Posez des cles en jeu avec l'editeur de camera, puis enregistrez.",
  "settings.rcamUnavailable":
    "La camera replay ne fonctionne pas sur cette version du jeu : {{reason}}",
  "settings.rcamNotCalibrated":
    "La visee n'est pas encore calibree. Ouvrez l'editeur en jeu et faites pivoter la camera une fois.",
  "settings.rcamRefresh":
    "Actualiser",
  "settings.rcamSlot":
    "Emplacement {{n}}",
  "settings.rcamSlotEmpty":
    "Vide",
  "settings.rcamKeyCount_one":
    "{{count}} cle",
  "settings.rcamKeyCount_other":
    "{{count}} cles",
  "settings.rcamShotCount_one":
    "{{count}} plan",
  "settings.rcamShotCount_other":
    "{{count}} plans",
  "settings.rcamTimelineHint":
    "Faites glisser une cle pour la deplacer. Les temps s'alignent sur la grille de 30 ms et une cle ne peut pas depasser sa voisine.",
  "settings.rcamKeyAt":
    "Cle a {{time}}",
  "settings.rcamNoKeySelected":
    "Choisissez une cle sur la timeline pour changer ce qu'elle fait.",
  "settings.rcamAim":
    "Viser",
  "settings.rcamAimNone":
    "personne",
  "settings.rcamEaseSmooth":
    "Fluide",
  "settings.rcamEaseHold":
    "Pause",
  "settings.rcamEaseCut":
    "Coupe",
  "settings.rcamCurve":
    "Courbe",
  "settings.rcamCurveCentripetal":
    "Centripete",
  "settings.rcamCurveUniform":
    "Uniforme",
  "settings.rcamAxis":
    "Axe",
  "settings.rcamAxisClock":
    "Horloge du replay",
  "settings.rcamAxisTrack":
    "Tour du pilote",
  "settings.rcamAxisHint":
    "Cale sur le tour d'un pilote, le trajet fonctionne sur n'importe quel tour, pilote ou replay. Chaque cle a besoin d'un pilote vise et d'une position sur le tour.",
  "settings.rcamRig":
    "Support",
  "settings.rcamRigLocked":
    "Fixe",
  "settings.rcamRigHandheld":
    "A l'epaule",
  "settings.rcamRigDrone":
    "Drone",
  "settings.rcamRigCrane":
    "Grue",
  "settings.rcamRigAmount":
    "Intensite",
  "settings.rcamAutoFov":
    "Garder la taille du sujet",
  "settings.rcamAutoFovHint":
    "Le FOV suit le pilote vise pour qu'il garde la meme taille a l'ecran.",
  "settings.rcamSave":
    "Enregistrer",
  "settings.rcamRevert":
    "Retablir",
  "settings.rcamRetime":
    "Repartir par distance",
  "settings.rcamExport":
    "Exporter",
  "settings.rcamImport":
    "Importer",
  "settings.rcamDelete":
    "Supprimer",
  "settings.rcamSaved":
    "Trajet enregistre",
  "settings.rcamWriteFailed":
    "Impossible d'enregistrer le trajet",
  "settings.rcamReadFailed":
    "Impossible de lire le trajet",
  "settings.rcamRetimeDone":
    "Cles reparties. Enregistrez pour conserver.",
  "settings.rcamRetimeFailed":
    "Impossible de repartir les cles",
  "settings.rcamDeleted":
    "Emplacement {{n}} supprime",
  "settings.rcamDeleteFailed":
    "Impossible de supprimer le trajet",
  "settings.rcamImported":
    "Importe dans l'emplacement {{n}}",
  "settings.rcamImportFailed":
    "Impossible d'importer ce trajet",
  "settings.rcamExported":
    "Trajet exporte",
  "settings.rcamExportFailed":
    "Impossible d'exporter le trajet",
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
  "settings.frostmodStrayMsvcr90":
    "Un fichier de votre dossier de jeu fait planter MX Bikes avec « R6034 » — mettez-le de côté pour régler ça.",
  "settings.frostmodRuntimeMissing":
    "Il manque à Windows un composant Visual C++ dont FrostMod a besoin — installez-le pour faire disparaître l'erreur « dll was not found ».",
  "settings.repairRuntimes": "Réparer les composants",
  "settings.repairRuntimesHint":
    "Installe tous les composants Visual C++ qui manquent à ce PC, 32 et 64 bits, et retire ce qu'une ancienne version de cette app a laissé dans le dossier du jeu. Utile même si rien ne semble anormal ci-dessus.",
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
  "sync.notStartedTitle": "Pas encore démarrée",
  "sync.notStartedWhy":
    "La synchro des peintures se met en place toute seule à votre première course en ligne — rien à remplir. Lancez MX Bikes, rejoignez un serveur, et le reste s'affichera ici.",
  "sync.oversized_one": "{{count}} peinture est trop lourde à partager, les autres pilotes ne la verront pas.",
  "sync.oversized_other": "{{count}} peintures sont trop lourdes à partager, les autres pilotes ne les verront pas.",
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
    "Aucune texture de cette déco n'est utilisée par les pièces affichées ici, donc l'aperçu ne change pas. Elle peut tout de même peindre la chaîne, que cette vue n'affiche pas.",
  "viewer.noPaintPreview": "Pas d'aperçu de la déco ({{err}})",

  // ── Bibliothèque ───────────────────────────────────────────────────────────
  "library.help":
    "Vos mods installés. Vérifiez ce qui est installé et retirez ce dont vous ne voulez plus.",
  "library.rootFolder": "(racine)",
  "library.byAuthor": "par {{author}}",
  "library.locked": "Verrouillé — le contenu ne peut pas être lu",
  "library.searchPlaceholder": "Rechercher parmi les installés…",
  "library.sortFolder": "Par dossier",
  "library.sortRecent": "Ajoutés récemment",
  "library.showRemoved": "Supprimés",
  "library.showRemovedHint":
    "Afficher les mods qu'a contenus ce dossier, y compris ceux supprimés hors de l'app",
  "library.goneOn": "Supprimé le {{date}}",
  "library.goneNote": "conservés pour que tu les retrouves",
  "library.parkedHint": "Désactivé dans Gérer — toujours sur le disque",
  "library.parkedNote": "réactive-les dans Gérer",
  "library.nothingRemoved":
    "Rien ne manque encore. Désormais, tout ce que tu supprimes est retenu ici.",
  "library.reinstall": "Télécharger à nouveau",
  "library.copyName": "Copier le nom",
  "library.copiedName": "Nom copié",
  "library.forget": "Oublier",
  "library.forgetFailed": "Impossible d'oublier ça",
  "library.restore": "Restaurer",
  "library.restored": "Remis en place",
  "library.restoreFailed": "Impossible de restaurer",
  "library.findAgain": "Le retrouver",
  "library.findAgainFor": "Recherche de « {{name}} » dans toutes les sources.",
  "library.findAgainNone": "Rien à ce nom.",
  "library.findAgainFailed": "Recherche impossible ici.",
  "library.scanning": "Analyse de votre bibliothèque…",
  "library.empty":
    "Aucun mod {{type}} installé — allez dans Parcourir pour en ajouter un.",
  "library.noMatches": "Aucun résultat.",
  "library.quick3d": "Voir en 3D",
  "swapActions.menu": "Déplacer ou supprimer ce modèle",
  "swapActions.move": "Déplacer vers une autre moto…",
  "swapActions.delete": "Supprimer le modèle…",
  "swapActions.activeFirst": "C'est le modèle actif — passez d'abord la moto sur un autre",
  "swapActions.stockHasNoFiles": "Stock n'est pas un set de modèle : il n'y a rien à déplacer ni à supprimer",
  "swapActions.moveTitle": "Déplacer {{name}} vers une autre moto",
  "swapActions.moveBlurb": "Les fichiers du modèle partent. La moto garde tout le reste.",
  "swapActions.pickBike": "Choisissez une moto…",
  "swapActions.liveriesTitle": "Emporter ses décos ?",
  "swapActions.liveriesBlurb": "Une déco est dessinée pour le layout d'une moto et convient rarement à une autre. Ce que vous laissez reste sur cette moto.",
  "swapActions.moveConfirm": "Déplacer",
  "swapActions.moved": "{{name}} déplacé vers {{bike}}",
  "swapActions.deleteTitle": "Supprimer {{name}} ?",
  "swapActions.deleteBlurb_one": "Son {{count}} fichier part à la Corbeille. Les décos restent sur la moto.",
  "swapActions.deleteBlurb_other": "Ses {{count}} fichiers partent à la Corbeille. Les décos restent sur la moto.",
  "swapActions.deleteConfirm": "Supprimer",
  "swapActions.deleted": "{{name}} envoyé à la Corbeille",
  "library.models_one": "{{count}} modèle",
  "library.models_other": "{{count}} modèles",
  "library.modelsHint": "Model swaps installés pour cette moto — changez-en dans le Locker",
  "library.modelIncomplete": "Incomplet",
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

  // ── Partage des fichiers installés (n'importe quel circuit ou peinture) ────
  "share.share": "Partager",
  "share.action": "Partager…",
  "share.title": "Partager ces fichiers",
  "share.hint":
    "On les empaquette, on les envoie, et tu récupères un seul code à coller où tu veux. Celui qui le colle retrouve les fichiers dans les mêmes dossiers.",
  "share.hintDone": "Envoie ce code : il installe tout ce qui est listé au-dessus.",
  "share.nothingToShare":
    "Rien à partager ici : seuls les fichiers de ton dossier mods peuvent entrer dans un code.",
  "share.skipped_one": "1 élément écarté ({{reason}}).",
  "share.skipped_other": "{{count}} éléments écartés ({{reason}}).",
  "share.createCode_one": "Partager 1 fichier ({{size}})",
  "share.createCode_other": "Partager {{count}} fichiers ({{size}})",
  "share.copyCode": "Copier le code",
  "share.copied": "Code de partage copié.",
  "share.uploaded": "Envoyé : copie le code ci-dessous.",
  "share.uploadedCopied": "Envoyé : le code est dans le presse-papiers.",
  "share.importAction": "Coller un code…",
  "share.importTitle": "Importer des fichiers partagés",
  "share.importBody":
    "Colle le code qu'on t'a envoyé. Les fichiers s'installent là où l'expéditeur les avait.",
  "share.downloadNotice": "Télécharge {{size}} depuis {{host}}.",
  "share.install": "Télécharger et installer",
  "share.installed_one": "1 fichier installé.",
  "share.installed_other": "{{count}} fichiers installés.",
  "share.phasePacking": "Préparation des fichiers…",
  "share.phaseUploading": "Envoi…",
  "share.phaseDownloading": "Téléchargement…",
  "share.phaseInstalling": "Installation…",

  // ── Casier ─────────────────────────────────────────────────────────────────
  "locker.help":
    "Changez le modèle et le son moteur de chaque moto parmi les sets que vous avez installés.",
  "locker.rescan": "Réanalyser",
  "locker.restore": "Restaurer",
  "locker.hideOrphan": "Masquer cet avertissement",
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
  "locker.stockModel": "Modèle d’origine",
  "locker.activeModel": "Modèle actif",
  "locker.activeSound": "Son actif",
  "locker.switchToNoModel":
    "Passer à aucun modèle — retire les fichiers du modèle actuel",
  "locker.switchToStockModel":
    "Retire le modèle actuel pour laisser celui du jeu reprendre la main — il est archivé, pas supprimé",
  "locker.switchToStock":
    "Passer à Stock — retire le mod audio (le son d'origine est joué)",
  "locker.missingModelEdf": "Ce set n'a pas de model.edf",
  "locker.missingSoundFiles": "Il manque engine.scl ou sfx.cfg à ce set",
  "locker.switchTo": "Passer à {{name}}",
  "locker.preview3d": "Voir {{name}} en 3D — rien n’est changé",
  "locker.view3d": "Voir 3D",
  "locker.paints": "Livrées",
  "locker.assignPaints": "Choisis les livr\u00e9es qui appartiennent \u00e0 {{name}}",
  "locker.paintsClaimed_one": "{{count}} livr\u00e9e attribu\u00e9e \u00e0 ce mod\u00e8le",
  "locker.paintsClaimed_other": "{{count}} livr\u00e9es attribu\u00e9es \u00e0 ce mod\u00e8le",
  "locker.paintsTitle": "Livr\u00e9es de \u00ab\u00a0{{model}}\u00a0\u00bb",
  "locker.paintsBlurb":
    "Coche les livr\u00e9es dessin\u00e9es pour ce mod\u00e8le. Ce sont les seules propos\u00e9es tant qu\u2019il est actif, et celles qui appartiennent \u00e0 un autre mod\u00e8le sont sorties du dossier paints de la moto, si bien que {{game}} cesse aussi de les lister. Une livr\u00e9e coch\u00e9e par aucun mod\u00e8le reste disponible avec tous.",
  "locker.paintsFilter": "Rechercher des livr\u00e9es\u2026",
  "locker.paintsSelectAll": "Tout s\u00e9lectionner",
  "locker.paintsClearAll": "Tout effacer",
  "locker.paintsLoading": "Lecture des livr\u00e9es\u2026",
  "locker.paintsNone": "Cette moto n\u2019a pas encore de livr\u00e9e \u2014 installes-en une et elle appara\u00eetra ici.",
  "locker.paintsNoMatch": "Aucune livr\u00e9e ne correspond.",
  "locker.paintsAlsoOn": "\u00c9galement attribu\u00e9e \u00e0 {{models}}",
  "locker.paintsSaved_one": "{{count}} livr\u00e9e attribu\u00e9e \u00e0 \u00ab\u00a0{{model}}\u00a0\u00bb.",
  "locker.paintsSaved_other": "{{count}} livr\u00e9es attribu\u00e9es \u00e0 \u00ab\u00a0{{model}}\u00a0\u00bb.",
  "locker.paintsStuck_one":
    "{{count}} fichier de livr\u00e9e n\u2019a pas pu \u00eatre d\u00e9plac\u00e9 \u2014 ferme {{game}} et relance le scan, sinon il reste visible en jeu.",
  "locker.paintsStuck_other":
    "{{count}} fichiers de livr\u00e9e n\u2019ont pas pu \u00eatre d\u00e9plac\u00e9s \u2014 ferme {{game}} et relance le scan, sinon ils restent visibles en jeu.",
  "locker.paintsReselect": "Res\u00e9lectionne ton profil dans {{game}} pour voir la nouvelle liste.",
  "locker.paintsNextLaunch": "Le jeu affichera la nouvelle liste \u00e0 sa prochaine ouverture.",
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
  "locker.modelFrostmodNotRunning":
    "FrostMod n'est pas lancé — dans MX Bikes, changez de catégorie de moto puis revenez pour voir le modèle.",
  "locker.modelSwitchCategory":
    "Modèle changé — dans MX Bikes, changez de catégorie de moto puis revenez pour le voir.",
  "locker.modelFrostmodUnreachable":
    "Impossible de joindre FrostMod — en jeu, changez de catégorie de moto puis revenez pour charger le modèle.",
  "locker.modelRefreshWindowsOnly":
    "Modèle changé — en jeu, changez de catégorie de moto puis revenez pour le voir.",
  "locker.modelInstantRefreshOff":
    "Dans MX Bikes, changez de catégorie de moto puis revenez pour charger le modèle.",

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
  "install.cancelled": "{{title}} annulé",

  "downloads.title": "Téléchargements",
  "downloads.open": "Afficher la file de téléchargement",
  "downloads.preparing": "Préparation…",
  "downloads.waiting": "En attente",
  "downloads.cancel": "Annuler ce téléchargement",
  "downloads.remove": "Retirer de la file",
  "downloads.cancelling": "Annulation…",
  "downloads.stageResolving": "Recherche du fichier…",
  "downloads.stageDownloading": "Téléchargement",
  "downloads.stageExtracting": "Extraction",
  "downloads.stagePlacing": "Installation",
  "downloads.stageReview": "En attente de vous",

  // ── Téléchargements (historique) ───────────────────────────────────────────
  "downloads.help":
    "Tout ce que vous avez téléchargé, du plus récent au plus ancien — échecs compris. Filtrez par statut, ou cherchez un mod dont le nom vous échappe.",
  "downloads.filterAll": "Tous",
  "downloads.filterFailed": "Échecs",
  "downloads.searchPlaceholder": "Rechercher dans les téléchargements…",
  "downloads.clearAction": "Vider",
  "downloads.clearTitle": "Vider l'historique des téléchargements ?",
  "downloads.clearBody":
    "Cela n'oublie que la liste. Rien de ce que vous avez installé n'est supprimé.",
  "downloads.empty":
    "Rien de téléchargé pour l'instant — allez dans Parcourir pour ajouter quelque chose.",
  "downloads.noMatches": "Aucun résultat.",
  "downloads.today": "Aujourd'hui",
  "downloads.yesterday": "Hier",
  "downloads.sourceSite": "Téléchargement",
  "downloads.sourceShop": "Boutique",
  "downloads.sourceHub": "MXB Hub",
  "downloads.sourceFile": "Fichier importé",
  "downloads.showInLibrary": "Voir dans la bibliothèque",
  "downloads.openModPage": "Ouvrir la page du mod",
  "downloads.forget": "Retirer de la liste",
  "downloads.rowActions": "Plus",
  "downloads.failedBadge_one": "{{count}} téléchargement échoué",
  "downloads.failedBadge_other": "{{count}} téléchargements échoués",

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
  "section.removed": "Plus installés",
  "section.parked": "Mis de côté par Gérer",
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
  // Voice chat — devices and levels.
  "voice.section": "Chat vocal",
  "voice.enable": "Activer le chat vocal",
  "voice.microphone": "Microphone",
  "voice.output": "Sortie",
  "voice.systemDefault": "Périphérique par défaut",
  "voice.testMic": "Tester le micro",
  "voice.stopTest": "Arrêter",
  "voice.speakNow": "Dis quelque chose — la barre doit bouger.",
  "voice.testOutput": "Jouer un son test",
  "voice.testOutputDesc": "Vérifie que tu entendras les autres dans le bon casque.",
  "voice.micGain": "Gain du microphone",
  "voice.volume": "Volume",
  "voice.micMode": "Mode de la touche",
  "voice.modePush": "Maintenir",
  "voice.modeToggle": "Bascule",
  "voice.micKey": "Touche micro",
  "voice.micOpen": "Micro ouvert",
  "voice.toggleDesc": "Appuie une fois pour ouvrir le micro, encore une fois pour le fermer. Rien ne le referme tout seul — surveille l'indicateur.",
  "voice.ptt": "Push-to-talk",
  "voice.pttDesc": "Maintiens la touche pour parler, relâche pour arrêter. Fonctionne quand le jeu a le focus.",
  "voice.pttUpdated": "Touche push-to-talk mise à jour",
  "voice.micFailed": "Impossible d'ouvrir le microphone",
  "voice.outputFailed": "Impossible de jouer le son test",
  "voice.registerFailed": "Réglages vocaux enregistrés, mais la touche push-to-talk n'a pas été enregistrée",
  "voice.deviceGone": "Ce périphérique n'est pas connecté",
  "voice.noDevices": "Aucun périphérique audio trouvé",
  "voice.notConnected": "Pas encore connecté à qui que ce soit",
  "voice.notConnectedDesc": "La voix démarre toute seule quand tu rejoins un serveur : rien à configurer, rien à télécharger et rien à faire tourner côté serveur. Tous ceux qui y sont avec l'app apparaissent ici.",
  "voice.inRoom": "En vocal sur {{server}}",
  "voice.stopped": "Vocal arrêté",
  "voice.unnamedRider": "Pilote",
  "voice.connecting": "connexion…",
  "voice.mute": "Couper",
  "voice.unmute": "Réactiver",

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
  "showcase.supporters.title_one": "Rendu possible par {{count}} soutien",
  "showcase.supporters.title_other": "Rendu possible par {{count}} soutiens",
  "showcase.supporters.more": "+{{count}} autres",
  "showcase.v0124.hero.title":
    "Toute la grille dans la peinture qu'elle a vraiment choisie",
  "showcase.v0124.hero.body":
    "MX Bikes ne transmet aucun contenu personnalisé : un lobby d'inconnus a toujours été un lobby de livrées par défaut. MXB App partage désormais ce que vous portez avec les pilotes autour de vous et installe les leurs — sur n'importe quel serveur, sans rien à faire côté hébergeur et sans code à saisir.",
  "showcase.v0124.anyserver":
    "N'importe quel serveur, pas seulement les nôtres. L'app lit le serveur depuis le jeu en cours, donc un lobby public fonctionne comme un lobby privé.",
  "showcase.v0124.nosetup":
    "Rien à configurer. Pas de code d'invitation, pas d'inscription, rien à installer côté serveur — lancez le jeu et c'est parti.",
  "showcase.v0124.everyone":
    "Vous verrez tout pilote qui a aussi MXB App. Plus votre lobby en compte, plus la grille est correcte.",
  "showcase.v0124.settings":
    "Paramètres → Synchro des peintures montre ce qui est parti, ce qui est arrivé, et toute peinture qu'elle a refusé d'écraser. L'interrupteur est dans Général.",
  "showcase.v0124.shape":
    "Les sauts se façonnent en tirant leur contour. Attrapez le haut d'un tabletop pour sa hauteur, l'extrémité pour sa longueur, l'autre côté d'un double pour son écart — ou dessinez-en un point par point.",
  "showcase.v0124.stats":
    "MXB App compte désormais anonymement quelles pages et fonctions sont utilisées — un identifiant aléatoire et rien sur vous. Paramètres → Général le désactive.",
  "showcase.v0122.hero.title":
    "La caméra replay peut suivre le pilote",
  "showcase.v0122.hero.body":
    "Visez un pilote avec une clé et la caméra replay de FrostMod le cadre image par image, au lieu des angles que vous avez posés. Coupez entre les plans, tenez-en un, et voyez tout le trajet dans le monde pendant que vous le construisez.",
  "showcase.v0122.aim":
    "Appuyez sur T sur une clé pour viser un pilote. La caméra le suit, et avec deux pilotes sur un même segment elle glisse doucement de l'un à l'autre.",
  "showcase.v0122.shots":
    "E décide de ce que fait une clé : la traverser, s'y poser, ou couper net vers le plan suivant. Un trajet peut être un montage entier.",
  "showcase.v0122.preview":
    "V dessine le trajet dans le monde pendant que vous le faites, et U annule la dernière modification — jusqu'à vingt-quatre en arrière.",
  "showcase.v0122.paths":
    "Paramètres → FrostMod liste chaque trajet enregistré : où tombent ses clés, comment il vole, et comment le transmettre.",
  "showcase.v0121.hero.title":
    "Tes touches de caméra replay, ton clavier",
  "showcase.v0121.hero.body":
    "La caméra replay de FrostMod lit le clavier directement, et le jeu aussi, au même instant. Si S recule ta caméra, appuyer sur S pour enregistrer un trajet la faisait reculer aussi. Chaque touche de l'éditeur peut désormais aller là où le jeu ne va pas.",
  "showcase.v0121.rebind":
    "Réattribue n'importe quelle touche de l'éditeur dans Paramètres → FrostMod : clique une action, appuie sur la touche.",
  "showcase.v0121.clash":
    "Choisis une touche que tes commandes de caméra n'utilisent pas. Les touches de fonction et le pavé numérique sont souvent libres ; un modificateur n'y change rien, le jeu voit toujours la touche en dessous.",
  "showcase.v0121.clean":
    "F7 masque tout ce que FrostMod dessine — panneau, radar, contours — pour un enregistrement propre.",
  "showcase.v0113.hero.title":
    "Chaque piste s\u2019ouvre comme le lieu qu\u2019elle est",
  "showcase.v0113.hero.body":
    "Une piste s\u2019ouvrait auparavant comme un sol nu \u2014 les orni\u00e8res, les virages relev\u00e9s et les faces de saut justes, mais rien dessus, si bien qu\u2019un supercross et un circuit national se ressemblaient. La visionneuse dessine d\u00e9sormais ce qu\u2019une piste pose sur elle-m\u00eame\u00a0: les tentes et les auvents, les bottes de paille et les murs de pneus, les lignes de banderoles et les barri\u00e8res, les remorques du paddock et le paysage au-del\u00e0 de son propre carr\u00e9.",
  "showcase.v0113.sky":
    "Une piste se tient sous son propre ciel, \u00e9clair\u00e9e et voil\u00e9e comme son propre fichier d\u2019ambiance le demande.",
  "showcase.v0113.ground":
    "Le sol garde son d\u00e9tail de pr\u00e8s \u2014 la terre de la piste elle-m\u00eame, avec un vrai relief quand elle fournit une carte de normales.",
  "showcase.v0113.pick":
    "Cliquez sur ce qui se dresse sur la piste et l\u2019objet s\u2019allume seul, nomm\u00e9 et mesur\u00e9.",
  "showcase.v0113.speed":
    "Cela arrive par \u00e9tapes \u2014 le terrain d\u2019abord, puis la forme du d\u00e9cor, puis ses couleurs \u2014 et rouvrir la m\u00eame piste \u00e9vite enti\u00e8rement l\u2019archive.",
  "showcase.v0112.hero.title":
    "Un pack de motos s'installe comme les motos qu'il contient",
  "showcase.v0112.hero.body":
    "Le pack OEM, c'est 54 machines dans une archive de 3,8 Go, et il arrivait jusqu'ici sous la forme d'une seule ligne intitul\u00e9e \u00ab\u00a0Dossier mods\u00a0\u00bb\u00a0: tout ou rien. Il est maintenant list\u00e9 tel qu'il est fait\u00a0: chaque moto avec son vrai nom et sa cat\u00e9gorie, et sa case \u00e0 cocher. Prends les quatre que tu pilotes et laisse les cinquante autres.",
  "showcase.v0112.fullscreen":
    "L'aper\u00e7u 3D du Designer s'ouvre en plein \u00e9cran\u00a0: un bouton remplit la fen\u00eatre avec le mod\u00e8le que tu es en train de peindre.",
  "showcase.v0112.review":
    "Un t\u00e9l\u00e9chargement qui contient plusieurs mods s'arr\u00eate et te montre ce qu'il y a dedans avant que quoi que ce soit n'atteigne ton dossier mods.",
  "showcase.v0112.paint":
    "Un paint qui propose un fichier par machine installe d\u00e9sormais celui de la moto choisie, et non le premier de la page.",
  "showcase.v0112.speed":
    "La fiche de v\u00e9rification s'ouvre en une seconde environ avec les motos OEM install\u00e9es, l\u00e0 o\u00f9 elle prenait pr\u00e8s de vingt.",
  "showcase.v0112.window":
    "Une fen\u00eatre noire au d\u00e9marrage ne peut plus te bloquer\u00a0: elle reste cach\u00e9e tant qu'elle n'a rien dessin\u00e9, et il y a toujours de quoi la fermer.",
  "showcase.v0111.hero.title":
    "Les modèles protégés s'ouvrent en 3D",
  "showcase.v0111.hero.body":
    "Un modèle acheté chez un créateur livre son maillage scellé, et la visionneuse ne pouvait pas le lire : « Voir en 3D » annonçait un modèle sans maillage lisible, alors qu'il fonctionne parfaitement en jeu. Il s'ouvre désormais comme n'importe quelle moto.",
  "showcase.v0111.messages":
    "Si une moto refuse toujours de s'ouvrir, l'app indique la vraie panne au lieu d'accuser la synchronisation cloud à chaque fois.",
  "showcase.v0110.hero.title":
    "Attrapez le pilote et positionnez-le",
  "showcase.v0110.hero.body":
    "Attrapez les articulations du pilote dans l'aperçu 3D et déplacez-le : mains, coudes, hanches, pieds. Les poses rapides se cumulent, les curseurs affinent, et Position de pilotage l'assoit sur la moto. Aperçu seulement : le jeu n'est pas touché.",
  "showcase.v0110.designer":
    "Miroir d'un calque à travers la moto, sélection multiple, magnétisme au glisser, retournement et positions saisies au chiffre.",
  "showcase.v0110.wheels":
    "Les motos s'affichent avec leurs roues, et vous choisissez les pneus sur lesquels elles reposent.",
  "showcase.v0110.speed":
    "Les circuits s'affichent sept fois plus vite, les motos s'ouvrent en 127 ms au lieu de 201, et les mods s'installent deux à deux.",
  "showcase.v0110.swaps":
    "Déplacez un jeu de modèles vers une autre moto ou supprimez-le, et voyez n'importe quel swap en 3D depuis la Bibliothèque.",
  "showcase.v0102.hero.title":
    "Des décos qui appartiennent au modèle qui les porte",
  "showcase.v0102.hero.body":
    "MX Bikes ne donne à une moto qu'un seul dossier paints et ignore tout des changements de modèle : un mesh Yami sur une KTM proposait donc aussi toutes les décos KTM. Chaque modèle du Locker a désormais un bouton palette — cochez les décos dessinées pour lui et ce seront les seules proposées, y compris dans le sélecteur de peintures de MX Bikes.",
  "showcase.v0102.packs":
    "Les décos livrées dans un pack de modèle étaient installées mais invisibles. Ouvrir le sélecteur de ce modèle les lui attribue, et c'est précisément ce qui les rend utilisables.",
  "showcase.v0102.presets":
    "La liste des décos dans Presets ne propose plus que celles qui conviennent au modèle choisi par le preset.",
  "showcase.v0102.vcredist":
    "Sur un Windows fraîchement réinstallé, l'app se fermait dès son lancement, sans fenêtre ni journal. L'installateur met maintenant le runtime Visual C++ de Microsoft en place avant d'écrire l'app.",
  "showcase.v0102.msvcr90":
    "Un msvcr90.dll resté sur place que l'app ne supprime pas d'elle-même n'est plus un plantage silencieux : elle nomme le fichier et propose de le désactiver en un clic.",
  "showcase.v0102.paintsync":
    "La synchro des peintures envoyait la déco de la mauvaise moto quand deux motos partageaient un nom de peinture — et les peintures de casque, masque, bottes et protections n'étaient jamais partagées.",
  "showcase.v0101.hero.title":
    "Ta bibliothèque se souvient de ce que tu as supprimé",
  "showcase.v0101.hero.body":
    "Supprimer un circuit l'effaçait complètement. L'app garde désormais le nom, l'auteur, le lieu et une image — pour que celui dont tu ne retrouves plus le nom des mois après reste retrouvable.",
  "showcase.v0101.restore":
    "Restaurer remet en place un mod supprimé par l'app, et « Le retrouver » cherche sur mxb-mods et la boutique avec le nom conservé.",
  "showcase.v0101.paints":
    "Une peinture enregistrée apparaît maintenant dans le jeu en cours — sans alt-tab, sans resélectionner ton profil.",
  "showcase.v0101.r6034":
    "Un plantage causé par cette app est corrigé : la copie de msvcr90.dll qu'elle déposait tuait MX Bikes avec R6034. Elle la retire désormais.",
  "showcase.v0101.logs":
    "Partager les journaux crée la même archive que Enregistrer et te rend un lien, au lieu d'un fichier à téléverser.",
  "showcase.v0101.bikes":
    "Les motos que tu ne pilotes plus peuvent être retirées du sélecteur des préréglages.",
  "showcase.v0100.hero.title": "Le Designer prépare ses planches tout seul",
  "showcase.v0100.hero.body":
    "Il crée maintenant les planches qu'un modèle demande, glisse en dessous les plastiques de la moto elle-même pour décalquer et ouvre un modèle en une seconde environ au lieu de près de vingt.",
  "showcase.v0100.location":
    "Survole la planche et elle te dit ce qu'il y a sous le curseur : la pièce, le côté de la moto où elle se trouve, et si c'est une face que tu verras ou un dessous que tu ne verras pas.",
  "showcase.v0100.downloads":
    "La page Téléchargements liste ce que tu as récupéré : par jour, le plus récent en haut, avec l'endroit où chaque fichier a atterri et le miroir d'où il vient.",
  "showcase.v0100.terrain":
    "Un circuit s'ouvre maintenant en 3D directement depuis la bibliothèque, ses sauts et ses ornières dessinés à partir du propre champ de hauteurs du jeu.",
  "showcase.v0100.sharing":
    "Maintenant tout ce qui se trouve dans ta Bibliothèque peut devenir un code que tu donnes à quelqu'un, et il repart dans les mêmes dossiers que chez toi.",
  "showcase.v0100.linux":
    "Sur Linux, FrostMod tourne maintenant dans le même prefix Proton que celui sous lequel tourne déjà le jeu.",
  "showcase.v092.hero.title": "Regarde le terrain d'un circuit en 3D",
  "showcase.v092.hero.body":
    "Les circuits étaient la seule chose que la bibliothèque ne savait pas montrer : un nom, une image et une taille. Le visualiseur lit maintenant le champ de hauteurs d'un circuit et dessine le sol lui-même, donc les sauts, les ornières et la forme d'un virage sont là à regarder avant même de le charger. Il s'ouvre depuis un circuit dans la bibliothèque, à côté de Voir en 3D.",
  "showcase.v092.surfaces":
    "Un circuit est dessiné avec ses propres surfaces. Là où le circuit dit ce qui est quoi, l'herbe, l'accotement, le revêtement dur et la terre de la trajectoire prennent chacun la couleur du matériau qu'il nomme — un tracé de ferme ressort donc en terre et un circuit sur herbe ressort vert.",
  "showcase.v092.relief":
    "Le terrain est éclairé par ses propres creux et projette de vraies ombres : une ornière se lit comme une ornière et une table de saut comme une table, quel que soit son sens.",
  "showcase.v092.accuracy":
    "Les circuits sont dessinés comme le jeu les tient : dans le bon sens plutôt qu'en miroir, sans le mur de onze mètres autour de ceux qui passent sous leur niveau de référence, et avec environ quatre fois plus de détail au sol.",
  "showcase.v092.voice":
    "Réglages du chat vocal : choisis le micro par lequel on t'entend et le casque d'où sortent les autres, avec un vumètre en direct et une tonalité de test. Rien ne transmet encore — c'est la moitié « périphériques », et la page le dit.",
  "showcase.v092.pushToTalk":
    "Une touche push-to-talk qui fonctionne pendant que le jeu a le focus, attribuée par le même chemin que le raccourci de l'overlay.",
  "showcase.v091.hero.title": "Peins directement sur le gabarit",
  "showcase.v091.hero.body":
    "Le Designer savait placer images et textes sur les planches d'une déco, mais il ne laissait poser aucun pixel à la main — un dégradé sur un ouïe voulait dire partir dans un éditeur d'images et revenir. Il a maintenant sa boîte à outils : pinceau doux avec taille, bord et intensité, gomme, dégradé, remplissage, rectangle, ellipse et ligne. Tout arrive sur la planche et sur le modèle 3D en même temps, pendant que tu fais glisser.",
  "showcase.v091.gradient":
    "Un dégradé qui emmène une couleur vers une autre. Fais glisser pour dire où se fait la transition : avant c'est la première couleur, après la seconde. Linéaire ou radial, et il peut se fondre vers rien plutôt que vers une couleur.",
  "showcase.v091.paintLayer":
    "La peinture va sur son propre calque, donc elle a opacité, fusion et empilement comme le reste — et le gabarit en dessous n'est jamais touché. Masque le calque et tu retrouves le gabarit intact. ⌘Z annule les tracés.",
  "showcase.v091.ghost":
    "Dessine par-dessus un fantôme de la moto. Une planche peut afficher en transparence dessous la peinture dont tu es parti, pour la décalquer — sortie de la planche, donc pas enregistrée dans la tienne — et une carte UV des carrosseries du modèle, chaque pièce dans sa couleur, pour voir sur quel panneau tu peins.",
  "showcase.v091.parts":
    "Pose une photo sur un seul panneau. Choisis une pièce de carrosserie : le calque s'y ajuste et se découpe sur son contour, donc une image prise sur internet couvre l'ouïe et s'arrête à la jointure. Survoler la planche affiche le nom de la pièce.",
  "showcase.v091.resize":
    "Les calques se redimensionnent en tirant leurs coins, pas seulement au curseur.",
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
  "drop.selectAll": "Tout sélectionner",
  "drop.selectNone": "Tout désélectionner",
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
  "drop.kind.tyres": "Pneus",
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
  "drop.reason.packLayout": "Rangé ici par le pack",
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

  // ── Journaux ───────────────────────────────────────────────────────────────
  "settings.logs": "Journaux",
  "logs.desc":
    "Les fichiers à envoyer quand quelque chose ne va pas. MXB App, FrostMod et {{game}} ont chacun les leurs — ouvre le dossier qu'il te faut, enregistre le tout dans un zip, ou partage-le sous forme de lien à coller dans un rapport.",
  "logs.appLogs": "MXB App",
  "logs.appLogsDesc": "Ce que l'app elle-même a enregistré",
  "logs.frostmodLogsDesc": "Ce que le loader a écrit dans son propre dossier",
  "logs.gameLogsDesc": "Le journal du jeu, à côté de ses fichiers",
  "logs.open": "Ouvrir le dossier",
  "logs.save": "Enregistrer les journaux…",
  "logs.saving": "Enregistrement…",
  "logs.refresh": "Actualiser",
  "logs.loading": "Recherche…",
  "logs.empty": "Aucun fichier de journal ici pour l'instant.",
  "logs.folderMissing":
    "Ce dossier n'existe pas — rien n'y a encore écrit de journal.",
  "logs.summary_one": "{{count}} fichier · {{size}} · le plus récent {{when}}",
  "logs.summary_other": "{{count}} fichiers · {{size}} · le plus récent {{when}}",
  "logs.saved": "Journaux enregistrés",
  "logs.savedDesc_one": "{{count}} fichier de journal, {{size}}",
  "logs.savedDesc_other": "{{count}} fichiers de journal, {{size}}",
  "logs.saveFailed": "Impossible d'enregistrer les journaux",
  "logs.share": "Partager les journaux",
  "logs.sharePacking": "Préparation…",
  "logs.sharing": "Envoi…",
  "logs.shared": "Journaux envoyés",
  "logs.sharedCopied": "{{size}} — le lien est dans ton presse-papiers.",
  "logs.sharedDesc": "{{size}} — le lien est ci-dessous.",
  "logs.sharedSummary_one": "{{count}} fichier de journal, {{size}} envoyés.",
  "logs.sharedSummary_other": "{{count}} fichiers de journal, {{size}} envoyés.",
  "logs.shareFailed": "Impossible de partager les journaux",
  "logs.copyLink": "Copier le lien",
  "logs.linkCopiedShort": "Copié",
  "logs.linkCopied": "Lien copié",
  "logs.shareWarning":
    "Le zip est déposé sur un hébergeur public — n'importe qui avec le lien peut le télécharger, alors ne le donne qu'à la personne qui l'a demandé.",
  "logs.privacy":
    "Les journaux contiennent des chemins de dossiers et ce que faisait l'app — jamais tes mots de passe ni tes cookies de session, et aucun fichier de réglages n'est inclus.",

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
    "ReShade se trouve dans ton dossier {{game}} — indique-le dans Dossier de jeu, ou pointe directement dessus ici.",
  "reshade.folder": "Recherche dans ton dossier {{game}} :",
  "reshade.customFolder": "Recherche dans le dossier que tu as choisi :",
  "reshade.browse": "Choisir un dossier…",
  "reshade.pickFolder": "Choisis le dossier où ReShade est installé",
  "reshade.folderMissing": "Le dossier que tu as choisi n'existe plus.",
  "reshade.resetFolder": "Revenir au dossier {{game}}",
  "reshade.folderSet": "ReShade trouvé",
  "reshade.notThere": "Pas de ReShade dans ce dossier",
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
  "paints.expected": "Planches utilisées ici :",
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
    "Dessine une déco sur les planches que le jeu lit vraiment, et regarde-la sur le modèle au fur et à mesure. Choisis un modèle et ses planches sont déjà là, sous les noms qu'il utilise — ou pars d'une déco installée ou d'un fichier Photoshop pour amener ton travail avec toi. Peins au pinceau, au dégradé ou avec des formes, empile images et textes par-dessus, puis enregistre : ce qui sort est un .pnt que le jeu charge, pas un export à convertir.",
  "designer.empty":
    "Rien sur quoi dessiner pour l'instant. Pars d'une déco installée pour ce modèle, ouvre un fichier Photoshop pour en garder les calques, ou ajoute une planche vierge.",
  "designer.startFromPaint": "Partir d'une déco…",
  "designer.startFromPsd": "Partir d'un PSD…",
  "designer.exportPsd": "Exporter en PSD",
  "designer.exportPsdHint": "Écrit chaque planche en .psd avec ses calques, dans un dossier de votre choix.",
  "designer.exportedPsd_one": "1 planche écrite dans {{dir}}",
  "designer.exportedPsd_other": "{{count}} planches écrites dans {{dir}}",
  "designer.sheetsSwitched": "Passé aux planches utilisées par {{dest}}.",
  "designer.switchSheetsTitle": "Ces planches sont celles d'un autre modèle",
  "designer.switchSheetsBody": "{{dest}} utilise {{names}}. Changer remplace toutes les planches ouvertes.",
  "designer.switchSheets": "Changer de planches",
  "designer.blankSheet": "Planche vierge",
  "designer.addSheet": "Ajouter une planche",
  "designer.nothingToSave": "Toutes les planches sont vides : dessinez quelque chose avant d'enregistrer.",
  "designer.blankSheetsSkipped_one": "1 planche vide a été écartée : une planche vide effacerait la texture du modèle.",
  "designer.blankSheetsSkipped_other": "{{count}} planches vides ont été écartées : une planche vide effacerait la texture du modèle.",
  "designer.createExpected_one": "Créer 1 planche",
  "designer.createExpected_other": "Créer {{count}} planches",
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
  "designer.part": "Pièce",
  "designer.wholeSheet": "Toute la planche",
  "designer.fitToPart": "Ajuster à la pièce",
  "designer.fitToPartHint":
    "Place et redimensionne ce calque pour couvrir la pièce choisie. Il la couvre au lieu d'y tenir, donc pas de vide — découpe-le pour retirer le débordement.",
  "designer.fitNotForPaint": "Un calque de peinture est la planche : il n'y a rien à déplacer ni à redimensionner.",
  "designer.clipped": "Découpé",
  "designer.clippedHint": "Ce calque est rogné sur la pièce : rien ne dépasse la jointure.",
  "designer.flank.left": "côté gauche",
  "designer.flank.right": "côté droit",
  "designer.flank.both": "les deux côtés",
  "designer.flankWashHint":
    "Le chaud, c'est le côté gauche de la moto ; le froid, le côté droit. Les deux côtés sont souvent dépliés en deux copies presque identiques du même panneau — c'est la seule chose sur la texture qui les distingue.",
  "designer.flankSharedHint":
    "Les deux flancs sont dépliés sur cette même zone : ce que vous dessinez ici apparaît de chaque côté de la moto, en miroir, et pas là où vous l'attendriez de l'autre côté.",
  "designer.focusHint": "Double-cliquez sur une pièce pour remplir la vue avec elle.",
  "designer.partOver": "{{part}} sur {{over}}",
  "designer.face.under": "face intérieure",
  "designer.face.both": "extérieur + intérieur",
  "designer.faceHint.under":
    "Cette zone est la face intérieure de la pièce : ce que vous y peignez regarde le sol et ne se voit jamais de l'extérieur.",
  "designer.faceHint.both":
    "La face extérieure de la pièce et sa face intérieure partagent cette zone : ce que vous dessinez ici se retrouve sur les deux.",
  // ── Designer › la sélection, et ce qu'on peut en faire ────────────────────────
  "designer.layersSelected": "{{count}} calques sélectionnés",
  "designer.position": "Position",
  "designer.duplicate": "Dupliquer",
  "designer.copy": "Copier",
  "designer.paste": "Coller",
  "designer.copyName": "{{name}} copie",
  "designer.copied_one": "1 calque copié.",
  "designer.copied_other": "{{count}} calques copiés.",
  "designer.pasteWrongSize":
    "Ça vient d'une planche d'une autre taille, et un calque de peinture *est* la planche — il n'y a rien ici qui puisse aller.",
  "designer.pasteDropped_one":
    "1 calque de peinture a été laissé de côté — un calque de peinture est la planche, et celle-ci n'a pas la même taille.",
  "designer.pasteDropped_other":
    "{{count}} calques de peinture ont été laissés de côté — un calque de peinture est la planche, et celle-ci n'a pas la même taille.",
  "designer.group": "Grouper",
  "designer.ungroup": "Dégrouper",
  "designer.groupRow": "Ensemble",
  "designer.groupOf": "Groupe de {{count}}",
  "designer.groupHint":
    "Les déplacer d'un bloc. Cliquer sur l'un d'eux prend tout le groupe — maintiens Alt pour n'en prendre qu'un.",
  "designer.flip": "Retourner",
  "designer.flipX": "Retourner de gauche à droite",
  "designer.flipY": "Retourner de haut en bas",

  // ── Designer › miroir vers l'autre flanc ──────────────────────────────────────
  "designer.mirror": "Miroir de l'autre côté",
  "designer.mirrorName": "{{name}} miroir",
  "designer.mirrorHint":
    "Place une copie de ce calque là où il tombe de l'autre côté de la moto. Calculé à partir du modèle plutôt qu'en retournant la planche, donc il arrive sur la bonne pièce — et il suit ce calque tant que tu ne le détaches pas.",
  "designer.mirroredFrom": "Miroir de « {{name}} ».",
  "designer.mirroredShort": "Miroir",
  "designer.mirroredOrphan": "Ceci est le miroir d'un calque qui n'existe plus.",
  "designer.unlink": "Détacher",
  "designer.unlinkHint":
    "Arrête de suivre, et garde ce qu'il y a. Ça devient un calque ordinaire que tu peux modifier seul.",
  "designer.selectSource": "Sélectionner l'original",
  "designer.mirrorPaused":
    "Aucun modèle chargé : ceci reste là où il a été placé la dernière fois au lieu de suivre.",
  "designer.mirrorRough":
    "L'autre côté n'est pas déplié en miroir de celui-ci, donc le placement est approchant plutôt qu'exact.",
  "designer.mirrorWhy.no-model":
    "Charge d'abord la moto dans l'aperçu — sans le modèle, il n'y a pas d'autre côté à trouver.",
  "designer.mirrorWhy.shared":
    "Les deux flancs sont dépliés au même endroit : c'est donc déjà sur les deux côtés de la moto. Une deuxième copie tomberait sur la première.",
  "designer.mirrorWhy.centre":
    "Ceci est sur l'axe de la moto, qui est son propre miroir — il n'y a pas d'autre côté où l'envoyer.",
  "designer.mirrorWhy.asymmetric":
    "Le modèle n'a rien au miroir de cet endroit, donc il n'y a pas d'autre côté où le poser.",

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
  "designer.noModelFound":
    "« {{model}} » n'est pas dans ta bibliothèque, il n'y a donc rien pour l'afficher.",
  "designer.noBikePreview":
    "Cette version ne lit pas la géométrie des motos, une déco n'a donc pas de modèle où se poser. Tout le reste s'enregistre normalement.",
  "designer.noPreviewForGame":
    "L'aperçu 3D est réservé à MX Bikes pour l'instant : les modèles de {{game}} ont besoin de leurs propres liaisons de pièces. Tout le reste fonctionne pareil et la déco s'enregistre normalement.",
  "designer.gearNote":
    "Affiché sur le pilote d'origine — ta propre tenue n'est pas chargée ici.",
  "designer.gearOnly": "Pièce seule",
  "designer.gearOnlyHint": "Afficher seulement la pièce que tu peins, sans le pilote",
  "designer.reference": "Référence",
  "designer.traceTemplate": "Modèle",
  "designer.traceHint":
    "Sors de la planche la peinture dont tu es parti et affiche-la en transparence dessous, pour la décalquer. Elle cesse de faire partie de ce que tu enregistres.",
  "designer.noTemplate": "Cette planche n'a aucun modèle à décalquer : elle est partie vierge.",
  "designer.stockTexture": "Texture d'origine",
  "designer.stockHint":
    "Affiche sous ta planche la texture livrée avec le modèle : les plastiques de la moto elle-même, avant qu'une peinture ne les remplace. Rien n'en est enregistré.",
  "designer.noStock":
    "Seules les motos savent dire quelles textures leur appartiennent. Un casque porte la peinture avec laquelle il est arrivé, et ce n'est pas un aspect d'origine à décalquer.",
  "designer.stockNoMatch":
    "Ce modèle n'embarque aucune texture à lui nommée « {{name}} », il n'y a donc rien de la moto à montrer sous cette planche.",
  "designer.stockAsBase": "Origine en base",
  "designer.stockAsBaseHint":
    "Peint la texture de la moto elle-même dans cette planche à pleine intensité — l'aspect d'origine comme point de départ, à un numéro d'être une peinture. Contrairement à la référence ci-dessus, celle-ci fait partie de ce que tu enregistres.",
  "designer.stockAsBaseDone":
    "« {{name}} » porte maintenant la texture de la moto elle-même — pose ton numéro dessus et enregistre.",
  "designer.stockReadFailed": "Impossible de lire la texture « {{name}} » du modèle.",
  "designer.uvMap": "Carte UV",
  "designer.uvHint":
    "Montre où tombent sur cette planche les carrosseries du modèle, chacune dans sa couleur.",
  "designer.noGeometry": "Charge un modèle dans l'aperçu pour voir sa disposition UV.",
  "designer.uvNoMatch":
    "Rien sur le modèle n'utilise une texture nommée « {{name}} », il n'y a donc aucune disposition UV à montrer.",
  "designer.ghostBuried":
    "La référence est sous la planche, et le modèle de cette planche est opaque : active Modèle pour l'en sortir et voir au travers.",
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
    "Fais glisser les calques sur la planche pour les placer : ils s'aimantent aux coutures et entre eux — maintiens Alt pour placer librement. Maj+clic ajoute à la sélection, un glissé sur le vide fait un lasso, et le clic droit a le reste. Choisis un outil ci-dessus pour peindre dessus.",
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

  // The track terrain viewer.
  "trackViewer.open": "Voir le terrain",
  "trackViewer.title": "Aperçu du circuit",
  "trackViewer.loading": "Lecture du terrain…",
  "trackViewer.refining": "Affinage…",
  "trackViewer.grid": "Grille",
  "trackViewer.surface": "Surface",
  "trackViewer.surfaceMasks": "From the track's surface data",
  "trackViewer.relief": "Dénivelé",
  "trackViewer.objects": "Objets",
  "trackViewer.painting": "Peinture…",
  "trackViewer.scenery": "Décors",
  "trackViewer.sceneryTris": "{{count}} triangles",
  "trackViewer.fixtures": "Éléments repérés",
  "trackViewer.pieces": "Éléments distincts",
  "trackViewer.selected": "Élément choisi",
  "trackViewer.place": "Placer un modèle",
  "trackViewer.placePick": "Choisis un modèle, puis clique au sol.",
  "trackViewer.placeHint": "Clique au sol pour le poser.",
  "trackViewer.placedCount": "{{count}} posés",
  "trackViewer.saveProps": "Enregistrer le .scr",
  "trackViewer.noTerrain": "Aucun terrain à afficher",
  "trackViewer.noTerrainHint":
    "Les données d'altitude de ce circuit ne sont pas dans un format que la visionneuse sait encore lire.",
  "trackViewer.inferredNote":
    "Le fichier d'altitudes de ce circuit n'a pas de format documenté ; sa forme a donc été déduite des données. À lire comme une approximation fidèle, pas comme une mesure exacte.",
  "trackViewer.assumedScaleNote":
    "Ce circuit n'indique pas l'écart entre ses points d'altitude : le relief est réel, mais sa pente est approximative.",
  "trackViewer.whyDetails": "Pourquoi ?",
  "trackViewer.copyDetails": "Copier les détails",
  "trackViewer.copied": "Copié",
  /* ── Protect ─────────────────────────────────────────────────────────── */
  "nav.protect": "Protéger",
  "protect.help": "Verrouille les fichiers que tu as créés sur les GUID de ceux qui ont le droit de les charger. Un fichier verrouillé ne s'ouvre que sur l'installation dont il porte le GUID : un acheteur, une copie. Choisis les fichiers une fois, colle les GUID, et tu obtiens un dossier par acheteur. Tes originaux sont seulement lus.",
  "protect.ownTitle": "Ton GUID",
  "protect.ownDesc": "L'identité sous laquelle MX Bikes te connaît. Elle n'est dans aucun fichier — Steam la fournit à la connexion — donc l'app la lit dans le jeu en cours d'exécution.",
  "protect.ownUnknown": "Pas encore connu",
  "protect.readOwn": "Lire depuis le jeu",
  "protect.copy": "Copier",
  "protect.copied": "Copié",
  "protect.useMine": "Ajouter à la liste",
  "protect.ownFound": "Ton GUID est {{guid}}",
  "protect.ownNotFound": "Impossible de lire ton GUID",
  "protect.ownNotFoundWhy": "Lance MX Bikes, connecte-toi à Steam, puis réessaie.",
  "protect.ownFailed": "La lecture de ton GUID a échoué",
  "protect.filesTitle": "Fichiers à verrouiller",
  "protect.filesDesc": "Ajoute des fichiers, ou un dossier pour prendre tout ce qu'il contient. Un .pkz est verrouillé comme archive ; tout le reste est verrouillé fichier par fichier.",
  "protect.addFiles": "Ajouter des fichiers",
  "protect.addFolder": "Ajouter un dossier",
  "protect.clear": "Vider",
  "protect.summary": "{{files}} fichiers · {{size}}",
  "protect.summarySkipped": "{{count}} ignorés",
  "protect.planFailed": "Impossible de lire cette sélection",
  "protect.skipJunk": "Ignoré",
  "protect.skipEmpty": "Vide",
  "protect.skipProtected": "Déjà protégé",
  "protect.guidsTitle": "Verrouiller pour",
  "protect.guidsDesc": "Un GUID par ligne — un dossier par GUID en sortie. Chaque copie a sa propre clé, deux acheteurs ne peuvent donc pas voir qu'ils détiennent le même fichier.",
  "protect.guidsPlaceholder": "FF0110000108D7CFE3",
  "protect.guidBad": "Ce n'est pas un GUID — 18 caractères hexadécimaux",
  "protect.outTitle": "Où les mettre",
  "protect.outDesc": "Un dossier par GUID est créé ici, avec les fichiers aux mêmes chemins qu'avant. Tes originaux ne sont pas touchés.",
  "protect.outPlaceholder": "Choisir un dossier",
  "protect.browse": "Parcourir",
  "protect.lock": "Verrouiller {{files}} fichiers pour {{guids}} GUID",
  "protect.done": "{{files}} fichiers verrouillés pour {{guids}} GUID",
  "protect.failed": "Le verrouillage a échoué",
  "protect.showFolder": "Afficher le dossier",

  // ── Track Studio ───────────────────────────────────────────────────────────
  "nav.track": "Circuit",
  "track.help": "Décris un circuit et Claude écrit le tour — les virages, les lignes droites et chaque saut. L'app le construit, le mesure face à de vrais circuits publiés et redemande ce qui ne colle pas. Prévisualise-le en 3D, modifie n'importe quel saut, puis exporte le dossier que compile le TerrainEd de MX Bikes.",
  "track.briefPlaceholder": "Un national sableux avec une longue section rythmique et deux gros step-ups",
  "track.generate": "Générer",
  "track.generating": "Rédaction…",
  "track.generatingHint": "Trace un tour qui se referme, le construit et le mesure. Une ou deux minutes, plus si le premier essai doit être corrigé.",
  "track.generated": "« {{name}} » écrit",
  "track.generateFailed": "Impossible d'écrire un circuit",
  "track.empty": "Décris le circuit voulu. Claude écrit le tour et chaque saut ; l'app le construit et vérifie qu'il mesure comme un vrai.",
  "track.lap": "Tour",
  "track.width": "Largeur",
  "track.ground": "Terrain",
  "track.features": "Éléments",
  "track.corners": "Virages",
  "track.measured": "Mesuré",
  "track.lips": "Sauts",
  "track.steepest": "Plus raide",
  "track.relief": "Relief",
  "track.budget": "Hauteur utilisée",
  "track.problems": "À corriger",
  "track.preview": "Aperçu en 3D",
  "track.building": "Construction…",
  "track.install": "Installer",
  "track.export": "Exporter la source",
  "track.installed": "Aperçu installé",
  "track.installFailed": "Installation impossible",
  "track.buildFailed": "Construction impossible",
  "track.exported": "{{count}} fichiers écrits",
  "track.exportFailed": "Export impossible",
  "track.previewOnly": "L'aperçu n'est que le terrain — le jeu l'affichera et échouera à la charger. Lance TerrainEd sur le dossier exporté pour un circuit jouable.",
  "track.at": "À",
  "track.kind": "Type",
  "track.height": "Hauteur",
  "track.length": "Longueur",
  "track.kind.tabletop": "Tabletop",
  "track.kind.double": "Double",
  "track.kind.roller": "Roller",
  "track.kind.whoops": "Whoops",
  "track.kind.stepUp": "Step-up",
  "track.kind.berm": "Berme",
  "track.straight": "Ligne droite",
  "track.turnLeft": "Virage à gauche",
  "track.turnRight": "Virage à droite",
  "track.radius": "de rayon",
  "track.gap": "d'écart",
  "track.deep": "de profondeur",
  "track.over": "sur",
  "track.kind.rut": "Ornière",
  "track.sequenceHint": "Tu peux décrire le tour dans l'ordre — « une longue ligne droite, une épingle à gauche, puis un double et une section rythmique » — et il sera construit ainsi.",
  "track.compile": "Compiler avec TerrainEd",
  "track.compiling": "Compilation…",
  "track.compiled": "Compilé",
  "track.compileFailed": "Compilation impossible",
  "track.buildStepFailed": "L'étape « {{step}} » a échoué",
  "track.pointAtTools": "Indiquer les outils…",
  "track.toolsNotFound": "Aucun terrained.exe dans ce dossier",
  "track.stillNeeded": "Il manque encore un .rdf de TrackEd (grille, stands, caméras) et un gate.edf copié d'un autre circuit.",
  "track.base": "Circuit de base",
  "track.baseLoaded": "« {{name}} » chargé",
  "track.name": "Nom du circuit",
  "track.author": "Auteur",
  "track.previewHint": "Appuie sur Aperçu pour le construire et le voir ici.",
  "track.location": "Lieu",
  "track.blank": "Vierge",
  "track.closeLap": "Fermer le tour",
  "track.lapClosed": "Le tour se referme",
  "track.closeFailed": "Fermeture impossible",
  "track.notes": "Différent d'un circuit publié",
  "track.replaceTitle": "Remplacer ce circuit ?",
  "track.replaceBody": "Tu as fait des modifications qui n'ont été ni exportées ni installées. Charger un autre circuit les supprime.",
  "track.replaceConfirm": "Supprimer et charger",
  "track.across": "de large",
  "track.hills": "collines",
  "track.surface": "sol",
  "track.soil": "Terre",
  "track.sand": "Sable",
  "track.grass": "Herbe",
  "track.smoothing": "fondu",
  "track.live": "Direct",
  "track.rebuild": "Reconstruire l'aperçu",
  "track.wholeLap": "tour entier",
  "track.kind.custom": "Forme",
  "track.mode.height": "Hauteur",
  "track.mode.shape": "Forme",
};
