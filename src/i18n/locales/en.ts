/**
 * English — the source of truth.
 *
 * Every other locale is typed as `Record<keyof typeof en, string>`, so adding a
 * key here makes `tsc` fail until all five translations supply it. Keys are
 * flat and namespaced by screen.
 *
 * Plural families are `<key>_one` / `<key>_other` and are looked up by passing
 * `{ count }`; `Intl.PluralRules` picks the form, so languages that don't split
 * one/other the way English does still land correctly.
 */
export const en = {
  // ── Generic, reused everywhere ─────────────────────────────────────────────
  "common.cancel": "Cancel",
  "common.back": "Back",
  "common.next": "Next",
  "common.skip": "Skip",
  "common.close": "Close",
  "common.save": "Save",
  "common.delete": "Delete",
  "common.rename": "Rename",
  "common.retry": "Retry",
  "common.tryAgain": "Try again",
  "common.loading": "Loading…",
  "common.installed": "Installed",
  "common.select": "Select",
  "common.deselect": "Deselect",
  "common.selectAll": "Select all",
  "common.clear": "Clear",
  "common.done": "Done",
  "common.apply": "Apply",
  "common.remove": "Remove",
  "common.open": "Open",
  "common.refresh": "Refresh",
  "common.dismiss": "Dismiss",
  "common.later": "Later",
  "common.active": "Active",

  // ── Window controls ────────────────────────────────────────────────────────
  "window.minimize": "Minimize",
  "window.maximize": "Maximize",
  "window.close": "Close",

  // ── Sidebar navigation ─────────────────────────────────────────────────────
  "nav.browse": "Browse",
  "nav.shop": "Shop",
  "nav.library": "Library",
  "nav.locker": "Locker",
  "nav.presets": "Presets",
  "nav.rider": "Rider",
  "nav.settings": "Settings",

  "sidebar.installing": "Installing “{{name}}”",
  "sidebar.queued": "+{{count}} queued",

  // ── FrostMod status + actions ──────────────────────────────────────────────
  "frostmod.checking": "Checking FrostMod…",
  "frostmod.running": "FrostMod running",
  "frostmod.notRunning": "FrostMod not running",
  "frostmod.reloadGame": "Reload game",
  "frostmod.start": "Start FrostMod",
  "frostmod.reloadedGame": "FrostMod reloaded the game.",
  "frostmod.notRunningToast": "FrostMod isn't running.",
  "frostmod.started": "FrostMod started",
  "frostmod.alreadyRunning": "FrostMod is already running",
  "frostmod.startFailed": "Couldn't start FrostMod",
  "frostmod.installedToast": "FrostMod {{version}} installed",
  "frostmod.installedToastDesc":
    "It'll live-reload the game when you add mods.",
  "frostmod.installFailed": "Couldn't install FrostMod",
  // Folder-watcher notifications.
  "frostmod.newModsAdded": "New mods added",
  "frostmod.modsAdded_one": "New mod added",
  "frostmod.modsAdded_other": "{{count}} mods added",
  "frostmod.askedReload": "Asked FrostMod to reload the game.",
  "frostmod.andMore_one": "{{names}} and {{count}} more",
  "frostmod.andMore_other": "{{names}} and {{count}} more",
  "frostmod.watchDesc": "{{names}} — asked FrostMod to reload the game.",

  // ── First-run setup ────────────────────────────────────────────────────────
  "setup.title": "Welcome to MXB App",
  "setup.tagline":
    "Browse mxb-mods, install with one click, and let FrostMod reload the game for you.",
  "setup.modsFolder": "MX Bikes folder",
  "setup.autoDetect":
    "MXB App will auto-detect your {{hint}} folder. You can also pick it yourself.",
  "setup.chooseManually": "Choose the folder manually…",
  "setup.chooseDifferent": "Choose a different folder…",
  "setup.gameInstall": "MX Bikes install",
  "setup.detecting": "Looking for your MX Bikes install…",
  "setup.found": "Found",
  "setup.detectedAutomatically": "Detected automatically",
  "setup.installNotFound":
    "Couldn't find your MX Bikes install automatically — it powers the 3D rider preview. Pick it manually, or set it later in Settings.",
  "setup.chooseInstallManually": "Choose the install folder manually…",
  "setup.startBrowsing": "Start browsing mods",
  "setup.detectAndStart": "Detect & start browsing",
  "setup.pickModsFolder": "Select your MX Bikes folder",
  "setup.pickInstallFolder": "Select your MX Bikes install folder",

  // ── Welcome slideshow ──────────────────────────────────────────────────────
  "welcome.intro.title": "Welcome to MXB App",
  "welcome.intro.body":
    "Your mod manager for MX Bikes. Keep your tracks, bikes and paints organized in one place — no more zip files scattered across your desktop. We'll show you around in a few seconds.",
  "welcome.getStarted": "Get started",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "missing",
  "presets.missingHint": "This mod isn't installed — shows as stock in-game",
  "presets.missingMods":
    "Missing mods: {{mods}}. Install them for those parts to show.",
  "presets.help":
    "Save a full rider look and load it onto a bike on command.",
  "presets.profile": "Profile",
  "presets.namePlaceholder": "Preset name…",
  "presets.savePreset": "Save preset",
  "presets.saveChanges": "Save changes",
  "presets.saveChangesQ": "Save changes?",
  "presets.replaceQ": "Replace preset?",
  "presets.replace": "Replace",
  "presets.loadCopy": "Load a copy into the builder",
  "presets.viewOnRider": "View on rider",
  "presets.editNameOrOptions": "Edit name or options",
  "presets.share": "Share",
  "presets.nameFirst": "Give the preset a name first.",
  "presets.pickProfileAndBike": "Pick a profile and bike to apply to.",
  "presets.updated": "Updated preset “{{name}}”.",
  "presets.renamed": "Renamed to “{{name}}” and saved changes.",
  "presets.saved": "Saved preset “{{name}}”.",
  "presets.editing": "Editing “{{name}}” — change anything, then Save changes.",
  // Whole sentences, not "Applied X — {fragment}": the split reads as English grammar.
  "presets.appliedRefreshed":
    "Applied “{{label}}” to {{bike}} — refreshed live in-game.",
  "presets.appliedRefreshFailed":
    "Applied “{{label}}” to {{bike}} — saved, but instant refresh failed, so reselect your profile in-game to load it.",
  "presets.appliedGameRunning":
    "Applied “{{label}}” to {{bike}} — saved. Reselect your profile in MX Bikes (Profile menu) to load the new look.",
  "presets.appliedNextTime":
    "Applied “{{label}}” to {{bike}} — saved. It loads next time the game opens.",
  // Share / import.
  "presets.phaseBundling": "Packaging assets…",
  "presets.phaseUploading": "Uploading bundle…",
  "presets.phaseDownloading": "Downloading bundle…",
  "presets.phaseInstalling": "Installing assets…",
  "presets.bundleUploaded":
    "Full bundle uploaded — the code now includes the assets.",
  "presets.shareHintFull":
    "This code includes a downloadable asset bundle — the recipient picks Full import and gets everything, even with no mods installed.",
  "presets.shareHintConfig":
    "Send this code to anyone. They import it under Presets → Import. They'll need the same mods installed for every part to show.",
  "presets.generatingCode": "Generating code…",
  "presets.nothingToBundle":
    "No installed assets to bundle — this look is all stock/fonts.",
  "presets.createFullBundle": "Create full bundle",
  "presets.copiedFull": "Copied full-bundle code.",
  "presets.copiedShare": "Copied share code.",
  "presets.copyFailed": "Couldn't copy — select the code and copy manually.",
  "presets.copyFullCode": "Copy full code",
  "presets.copyCode": "Copy code",
  "presets.importTitle": "Import preset",
  "presets.importBody": "Paste a share code someone sent you.",
  "presets.configOnly": "Config only",
  "presets.import": "Import",
  "presets.fullImport": "Full import",
  "presets.editingBanner":
    "Editing {{name}} — change the name or any slot, then {{save}}.",
  "presets.bundleNotice":
    "Includes a full asset bundle (~{{size}} from {{host}}). Use {{fullImport}} to download and install everything — no mods needed first.",

  // ── Preset / rider loadout slots ───────────────────────────────────────────
  "slot.paint": "Bike livery",
  "slot.modelSwap": "Model swap",
  "slot.bikeFont": "Number font",
  "slot.tyres": "Tyres",
  "slot.rider": "Rider profile",
  "slot.suitPaint": "Kit / suit",
  "slot.suitFont": "Suit font",
  "slot.glovesPaint": "Gloves",
  "slot.ridingStyle": "Riding style",
  "slot.helmet": "Helmet",
  "slot.helmetPaint": "Helmet paint",
  "slot.gogglesPaint": "Goggles",
  "slot.boots": "Boots",
  "slot.bootsPaint": "Boot paint",
  "slot.protection": "Protection",
  "slot.protectionPaint": "Protection paint",
  "slotGroup.bike": "Bike",
  "slotGroup.rider": "Rider",
  "slotGroup.head": "Head",
  "slotGroup.body": "Body",

  // ── Rider studio ───────────────────────────────────────────────────────────
  "rider.help":
    "Dress the player model — helmet, goggles, outfit and boots together.",
  "rider.namePlaceholder": "Name this rider…",
  "rider.nameFirst": "Name this rider look first.",
  "rider.showOnModel": "Show on model",

  // ── Guided tour ────────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Take a quick tour",
  "tour.welcomeTour.body":
    "A few seconds to see where everything lives. You can skip at any point.",
  "tour.browse.title": "Browse mods",
  "tour.browse.body":
    "Search mxb-mods.com right here and install any track, bike or paint with a single click.",
  "tour.library.title": "Your library",
  "tour.library.body":
    "Everything you've installed, in one place — update or remove mods without ever touching a zip file.",
  "tour.locker.title": "The locker",
  "tour.locker.body":
    "Swap bike models in and out. MXB App registers the pieces so the game picks them up.",
  "tour.presets.title": "Presets",
  "tour.presets.body":
    "Save gear and paint loadouts, then apply a full look in one click — even while you're riding.",
  "tour.rider.title": "Rider studio",
  "tour.rider.body":
    "Preview your gear and paints on the 3D rider before you take them out on track.",
  "tour.frostmod.title": "FrostMod, live",
  "tour.frostmod.body":
    "This shows FrostMod's status. It live-reloads MX Bikes after an install, so new content shows up without restarting the game.",
  "tour.settings.title": "Settings",
  "tour.settings.body":
    "Set your game folder, background behaviour and FrostMod options here. You can replay this tour from here too.",
  "tour.done.title": "You're all set",
  "tour.done.body": "That's the tour. Head to Browse and install your first mod.",

  // ── Error boundary ─────────────────────────────────────────────────────────
  "error.previewFailed": "Preview failed to render",
  "error.somethingWentWrong": "Something went wrong",
  "error.unexpected": "An unexpected error occurred.",
  "error.reloadApp": "Reload app",

  // ── Updater banner ─────────────────────────────────────────────────────────
  "update.available": "{{version}} is available.",
  "update.downloading": "Downloading…",
  "update.downloadingPct": "Downloading… {{pct}}%",
  "update.pitch": "Update to get the latest features and fixes.",
  "update.updating": "Updating…",
  "update.updateAndRestart": "Update & restart",
  "update.dismiss": "Dismiss update notification",
  "update.onLatest": "You're on the latest version",
  "update.checkFailed": "Couldn't check for updates",
  "update.failed": "Update failed",

  // ── 3D viewer ──────────────────────────────────────────────────────────────
  "viewer.preview3d": "3D Preview",
  "viewer.expand": "Expand",
  "viewer.paint": "Paint",
  "viewer.loadingModel": "Loading model…",
  "viewer.loadingPaint": "Loading paint…",
  "viewer.loadingRider": "Loading rider…",
  "viewer.dragToRotate": "Drag to rotate",
  "viewer.scrollToZoom": "Scroll to zoom",
  "viewer.rightDragToPan": "Right-drag to pan",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Search…",
  "combobox.use": "Use “{{value}}”",

  // ── Mod types (Browse / Library segmented control) ─────────────────────────
  "modType.tracks": "Tracks",
  "modType.bikes": "Bikes",
  "modType.rider": "Rider",
  // The same nouns as they read inside a sentence ("Search tracks…"). English
  // lowercases them; languages that capitalize their nouns keep doing so.
  "modType.tracksInline": "tracks",
  "modType.bikesInline": "bikes",
  "modType.riderInline": "rider gear",

  // ── Browse category filters ────────────────────────────────────────────────
  "browseCat.all": "All",
  "browseCat.beginner": "Beginner",
  "browseCat.intermediate": "Intermediate",
  "browseCat.pro": "Pro",
  "browseCat.assets": "Assets",
  "browseCat.newBikes": "New Bikes",
  "browseCat.liveries": "Liveries",
  "browseCat.sounds": "Sounds",
  "browseCat.riderKit": "Rider Kit",
  "browseCat.helmets": "Helmets",
  "browseCat.helmetPaints": "Helmet Paints",
  "browseCat.gloves": "Gloves",
  "browseCat.boots": "Boots",
  "browseCat.bootPaints": "Boot Paints",
  "browseCat.protection": "Protection",
  "browseCat.protectionPaints": "Protection Paints",

  // ── Browse ─────────────────────────────────────────────────────────────────
  "browse.help":
    "Discover and install mods from the online catalog — search, filter by type, and open a mod to download it into the game.",
  "browse.searchPlaceholder": "Search {{type}}…",
  "browse.sortedByNewest": "Sorted by newest",
  "browse.loadFailed": "Couldn't load mods",
  "browse.empty": "No {{type}} found.",
  "browse.loadMore": "Load more",
  "browse.selectedCount": "{{count}} selected",
  "browse.queuing": "Queuing…",
  "browse.quickInstallCount": "Quick install {{count}}",
  "browse.quickInstall": "Quick install",
  "browse.quickReinstall": "Quick reinstall",
  "browse.openDetails": "Open details",
  "browse.reinstallOne": "Reinstall “{{title}}”?",
  "browse.reinstallMany": "Reinstall mods you already have?",
  "browse.reinstallOneBody":
    "This mod is already in your library. Reinstalling downloads it again and overwrites the installed files.",
  "browse.reinstallManyBody":
    "{{installed}} of the {{total}} selected are already installed. Continuing reinstalls and overwrites them.",
  "browse.reinstall": "Reinstall",
  "browse.reinstallAll": "Reinstall all",
  "browse.queued": "Queued “{{title}}”",
  "browse.queuedDesc": "Installing to {{folder}}.",
  "browse.rootFolder": "root",
  "browse.needsBrowser": "“{{title}}” needs a browser download",
  "browse.needsBrowserDesc":
    "{{host}} blocks in-app downloads — open its page to finish.",
  "browse.noDownload": "No download found for “{{title}}”",
  "browse.quickInstallFailed": "Couldn't quick-install “{{title}}”",
  "browse.queuedBulk_one": "Queued {{count}} mod",
  "browse.queuedBulk_other": "Queued {{count}} mods",
  "browse.queuedBulkDesc": "They'll install one after another.",
  "browse.queuedBulkSkipped_one": "{{count}} skipped — browser-only host.",
  "browse.queuedBulkSkipped_other": "{{count}} skipped — browser-only host.",
  "browse.bulkFailed": "Couldn't quick-install the selection",
  "browse.bulkFailedDesc_one": "It needs a browser download.",
  "browse.bulkFailedDesc_other": "All {{count}} need a browser download.",

  // ── Shop (MX Bikes Shop — purchased downloads) ─────────────────────────────
  "shop.myDownloads": "My Downloads",
  "shop.signInTitle": "Sign in to MX Bikes Shop",
  "shop.signInBody":
    "Log in to mxbikes-shop.com to see and install the tracks you've purchased. We open the real site — your password never touches this app.",
  "shop.signIn": "Sign in",
  "shop.logOut": "Log out",
  "shop.signedIn": "Signed in to MX Bikes Shop",
  "shop.sessionFailed": "Couldn't capture your MX Bikes Shop session",
  "shop.queuedDesc": "Installing to your tracks folder.",
  "shop.loadFailed": "Couldn't load your downloads: {{error}}",
  "shop.empty": "No purchased downloads found on your account yet.",

  // ── Install dialog (destination + mirror picker) ───────────────────────────
  "installDialog.installTo": "Install to",
  "installDialog.installToFolder": "Install to {{folder}}",
  "installDialog.change": "Change",
  "installDialog.searchBikes": "Search bikes…",
  "installDialog.searchFolders": "Search folders…",
  "installDialog.probably": "Probably",
  "installDialog.allFolders": "All folders",
  "installDialog.noFolderMatch": "No folder matches — create it below.",
  "installDialog.rememberedFor": "Remembered for {{type}}",
  "installDialog.downloadFrom": "Download from",
  "installDialog.downloadPerBike": "Download (per bike)",
  "installDialog.opensInBrowser":
    "Opens in browser — MXB App finishes the install",
  "installDialog.matchedBike": "Matched to your bike",
  "installDialog.differentBike": "Different bike / pack",
  "installDialog.directFastest": "Direct · fastest",
  "installDialog.direct": "Direct",
  "installDialog.perBikeHint":
    "Each download is a different bike — auto-selected to match your pick. Choose the “all bikes” pack for every bike at once.",
  "installDialog.mirrorsHint":
    "All mirrors contain the same file. If one fails, try the next.",

  // ── Library detail panel ───────────────────────────────────────────────────
  "libraryDetail.author": "Author",
  "libraryDetail.length": "Length",
  "libraryDetail.altitude": "Altitude",
  "libraryDetail.location": "Location",
  "libraryDetail.type": "Type",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Belongs to",
  "libraryDetail.format": "Format",
  "libraryDetail.extractedFolder": "Extracted folder",
  "libraryDetail.paintFile": "Paint file",
  "libraryDetail.packagedPkz": "Packaged .pkz",
  "libraryDetail.size": "Size",
  "libraryDetail.folder": "Folder",
  "libraryDetail.lockedWord": "locked",
  "libraryDetail.lockedWithMeta":
    "This track is {{locked}} by its creator. Its name, details and preview are shown here, but the files stay sealed — it can't be unpacked or previewed in 3D.",
  "libraryDetail.lockedNoMeta":
    "This track is {{locked}}, so its name, length and preview can't be read from the file — only its filename and size.",

  // ── Mod detail page ────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Resolve",
  "modDetail.stageDownload": "Download",
  "modDetail.stageExtract": "Extract",
  "modDetail.stagePlace": "Place",
  "modDetail.stageReload": "Reload",
  "modDetail.modFiles": "Mod files",
  "modDetail.copied": "Copied",
  "modDetail.copy": "Copy",
  "modDetail.addToLibrary": "Add to Library",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Installs to",
  "modDetail.noDownloadLink":
    "No download link was found on this page — open it on mxb-mods.com.",
  "modDetail.frostmodHint":
    "FrostMod will hot-reload the {{kind}} list when this finishes.",
  "modDetail.kindRider": "rider",
  "modDetail.kindBike": "bike",
  "modDetail.kindTrack": "track",
  "modDetail.details": "Details",
  "modDetail.format": "Format",
  "modDetail.mirrors": "Mirrors",
  "modDetail.type": "Type",
  "modDetail.addedToLibrary": "Added to your library",
  "modDetail.extracting": "Extracting…",
  "modDetail.addingToLibrary": "Adding to library…",
  "modDetail.resolving": "Resolving download…",
  "modDetail.finishInBrowser": "Finish in your browser",
  "modDetail.viewOnSite": "View on mxb-mods.com",

  // ── Settings ───────────────────────────────────────────────────────────────
  "settings.help": "Configure your game folder, updates, and app preferences.",
  "settings.gameFolder": "Game folder",
  "settings.general": "General",
  "settings.appearance": "Appearance",
  "settings.frostmod": "FrostMod",
  "settings.about": "About & updates",
  "settings.modsFolderDesc":
    "Where mods are installed. Changing it re-scans your library.",
  "settings.insideModsFolder": "Inside your MX Bikes folder",
  "settings.notSet": "Not set",
  "settings.change": "Change…",
  "settings.set": "Set…",
  "settings.theme": "Theme",
  "settings.themeLight": "Light",
  "settings.themeDark": "Dark",
  "settings.themeSystem": "System",
  "settings.language": "Language",
  "settings.languageSystem": "System",
  "settings.runInBackground": "Keep running in the background",
  "settings.runInBackgroundDesc":
    "Closing the window hides MXB App to the tray so FrostMod stays connected. Quit from the tray icon.",
  "settings.launchAtStartup": "Launch at startup",
  "settings.launchAtStartupDesc":
    "Start MXB App automatically when you log in.",
  "settings.instantRefresh": "Instant preset refresh",
  "settings.instantRefreshDesc":
    "When you apply a preset while MX Bikes is running, refresh the look in-game instantly — no restart or profile reselect. If it can't, you'll be told to reselect your profile.",
  "settings.instantRefreshWindowsOnly":
    "Refreshing the look in-game without a restart needs FrostMod, which is Windows-only — you'll be told to reselect your profile instead.",
  "settings.autoRunFrostmod": "Run FrostMod automatically",
  "settings.autoRunFrostmodDesc":
    "Start FrostMod in the background whenever MXB App opens.",
  "settings.watchModsReload": "Auto-reload on folder changes",
  "settings.watchModsReloadDesc":
    "Reload the game automatically when tracks or bikes are added to your mods folder — even downloaded manually outside MXB App.",
  "settings.checking": "Checking…",
  "settings.runningConnected": "Running · game connected",
  "settings.notRunning": "Not running",
  "settings.frostmodInstalled": "Installed{{suffix}}",
  "settings.notInstalled": "Not installed",
  "settings.checkingGitHub": "Checking GitHub for the latest release…",
  "settings.updateCheckFailed":
    "Couldn't check for updates — offline or GitHub unavailable.",
  "settings.latestVersion": "Latest: {{version}}",
  "settings.checkNewer": "Check for a newer FrostMod",
  "settings.working": "Working…",
  "settings.installFrostmod": "Install FrostMod",
  "settings.updateTo": "Update to {{version}}",
  "settings.reinstallLatest": "Reinstall latest",
  "settings.upToDate": "Up to date",
  "settings.madeWith": "Made with",
  // Toasts.
  "settings.updateFailed": "Couldn't update setting",
  "settings.startupUpdateFailed": "Couldn't update startup setting",
  "settings.folderUpdated": "Game folder updated",
  "settings.folderUpdatedDesc": "Your library will re-scan.",
  "settings.setFolderFailed": "Couldn't set folder",
  "settings.reDetected": "Re-detected your MX Bikes folder",
  "settings.detectFolderFailed": "Couldn't detect folder",
  "settings.pickInstallFolder":
    "Select your MX Bikes install folder (contains rider.pkz)",
  "settings.installSet": "Game install set",
  "settings.installSetDesc":
    "The 3D rider preview can now load the real body model.",
  "settings.setInstallFailed": "Couldn't set install folder",
  "settings.installNotFound": "Couldn't find MX Bikes",
  "settings.installNotFoundDesc":
    "No Steam install detected — set the folder manually.",
  "settings.installFound": "Found your MX Bikes install",
  "settings.detectInstallFailed": "Couldn't detect install folder",
  "settings.pickProfilesFolder": "Select your MX Bikes profiles folder",
  "settings.profilesSet": "Profiles folder set",
  "settings.profilesFound_one": "Found {{count}} profile.",
  "settings.profilesFound_other": "Found {{count}} profiles.",
  "settings.noProfilesThere": "No profiles found there",
  "settings.noProfilesThereDesc":
    "Saved anyway, but preset creation needs a folder that holds your profile.ini folders.",
  "settings.setProfilesFailed": "Couldn't set profiles folder",
  "settings.profilesReverted": "Reverted to the default profiles folder",
  "settings.resetProfilesFailed": "Couldn't reset profiles folder",
  "settings.frostmodNotRunningHint":
    "FrostMod isn't running — start it to hot-reload mods.",
  "settings.reloadUnavailable": "Reload isn't available on this platform.",

  // ── Launching the game ─────────────────────────────────────────────────────
  "game.play": "Play",
  "game.starting": "Starting…",
  "game.running": "MX Bikes running",
  "game.launch": "Launch MX Bikes",
  "game.alreadyRunning": "MX Bikes is already running",
  "game.launching": "Launching MX Bikes…",
  "game.launchFailed": "Couldn't launch MX Bikes",

  // ── Strings the multi-line JSX sweep initially missed ──────────────────────
  "libraryDetail.noEmbedded": "No embedded details were found for this item.",
  "modDetail.downloadFromHost": "Download from {{host}}",
  "modDetail.openHost": "Open {{host}}",
  "modDetail.thenAddFile": "Then add the file",
  "modDetail.chooseDownloaded": "Choose the downloaded file",
  "presets.chooseProfilesFolder": "Choose profiles folder…",
  "presets.viewInRider": "View in Rider",
  "presets.noModelSwapsHere": "No model swaps registered for this bike —",
  "presets.setUpInLocker": "set them up in the Locker",
  "presets.makeActiveBike": "Make this the active bike",
  "presets.nameClash":
    "Another preset is already named “{{name}}” — saving will overwrite it too.",
  "presets.shareWarning":
    "Uploads to a public, temporary link — it redistributes mod files made by others, so share responsibly.",
  "settings.profilesDesc":
    "Presets read your profiles from here — the path below is where the app is looking right now. It's the {{profiles}} folder inside your MX Bikes folder, or your {{documents}} one if you moved your mods folder. Set it only if yours is somewhere else.",
  "settings.resetToDefault": "Reset to default",
  "settings.gameInstallDesc":
    "Game install folder (optional) — where MX Bikes is installed (holds {{file}}). Set it to load the real rider body in the 3D preview.",
  "viewer.stockGearNote":
    "Shown on the game's stock {{part}}. A paint made for a different model may not line up perfectly.",
  "viewer.paintNoChange":
    "None of this paint's textures are used by the parts shown here, so the preview doesn't change. It may still paint the wheels or chain, which this view doesn't render.",
  "viewer.noPaintPreview": "No paint preview ({{err}})",

  // ── Library (installed mods) ───────────────────────────────────────────────
  "library.help":
    "Your installed mods. Review what's installed and remove ones you no longer want.",
  "library.rootFolder": "(root)",
  "library.byAuthor": "by {{author}}",
  "library.locked": "Locked — contents can't be read",
  "library.searchPlaceholder": "Search installed…",
  "library.scanning": "Scanning your library…",
  "library.empty": "No {{type}} installed yet — head to Browse and add one.",
  "library.noMatches": "No matches.",
  "library.quick3d": "Quick 3D view",
  "library.selectNone": "Select none",
  "library.move": "Move",
  "library.uninstall": "Uninstall",
  "library.uninstallAction": "Uninstall…",
  "library.moveToFolder": "Move to folder…",
  "library.showInExplorer": "Show in Explorer",
  "library.moveDialogTitle": "Move to folder",
  "library.moveCount_one": "Move {{count}} item",
  "library.moveCount_other": "Move {{count}} items",
  "library.chooseDestination": "Choose a destination folder",
  "library.newFolder": "New folder…",
  "library.newFolderName": "New folder name",
  "library.createAndMove": "Create & move",
  "library.confirmUninstall": "Uninstall {{name}}?",
  "library.confirmUninstallBody":
    "The item is moved to the Recycle Bin — you can restore it from there.",
  "library.confirmBulkUninstall_one": "Uninstall {{count}} item?",
  "library.confirmBulkUninstall_other": "Uninstall {{count}} items?",
  "library.confirmBulkUninstallBody":
    "Each item is moved to the Recycle Bin — you can restore them from there.",
  "library.uninstallCount": "Uninstall {{count}}",
  // Toasts.
  "library.moveFailed": "Couldn't move mod",
  "library.uninstallFailed": "Couldn't uninstall",
  "library.openFailed": "Couldn't open",
  "library.uninstalledOne": "{{name}} uninstalled",
  "library.movedToBin": "Moved to the Recycle Bin.",
  "library.someNotRemoved": "Some items couldn't be removed.",
  "library.bulkUninstalled_one": "{{count}} item uninstalled",
  "library.bulkUninstalled_other": "{{count}} items uninstalled",
  "library.bulkUninstallPartial": "Uninstalled {{ok}}, {{fail}} failed",
  "library.bulkMovePartial": "Moved {{ok}}, {{fail}} failed",
  "library.bulkMoved_one": "Moved {{count}} item to {{folder}}",
  "library.bulkMoved_other": "Moved {{count}} items to {{folder}}",

  // ── Locker (per-bike model + sound swaps) ──────────────────────────────────
  "locker.help":
    "Swap each bike's model and engine sound between the sets you've installed.",
  "locker.rescan": "Rescan",
  "locker.restore": "Restore",
  "locker.register": "Register",
  "locker.scanning": "Scanning bikes…",
  "locker.scanForSwaps": "Scan for swaps",
  "locker.orphanBanner":
    "{{bike}} is missing its setup files — an earlier version moved them into a swap folder, which stops the bike loading in-game at all. {{files}}",
  "locker.looseBanner_one":
    "{{count}} model / sound set found loose in your bikes — register it into {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} model / sound sets found loose in your bikes — register them into {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "No swappable bikes yet.",
  "locker.emptyIntro": "Two things have to be true before a swap can happen:",
  "locker.unpacked": "unpacked",
  "locker.emptyRuleUnpacked":
    "The bike is {{unpacked}} into {{path}}— a packed {{pkz}} can't be swapped. Unpack one from the Library.",
  "locker.emptyRuleMesh":
    "Each alternate model sits in its own folder inside that bike, holding a mesh ({{edf}}). Drop it anywhere in the bike folder and hit Scan below — we'll offer to file it under {{folder}}.",
  "locker.summary": "{{model}} · sound “{{sound}}”",
  "locker.modelNamed": "model “{{name}}”",
  "locker.noModelSwaps": "no model swaps",
  "locker.models": "Models",
  "locker.sounds": "Sounds",
  "locker.onlyOneModel": "Only one model — install more to swap",
  "locker.onlyStock": "Only Stock — install a sound mod to swap",
  "locker.noModel": "No model",
  "locker.stock": "Stock",
  "locker.activeModel": "Active model",
  "locker.activeSound": "Active sound",
  "locker.switchToNoModel": "Switch to no model — removes the current model files",
  "locker.switchToStock":
    "Switch to Stock — removes the sound mod (built-in sound plays)",
  "locker.missingModelEdf": "This set has no model.edf",
  "locker.missingSoundFiles": "This set is missing engine.scl or sfx.cfg",
  "locker.switchTo": "Switch to {{name}}",
  "locker.tiedToModel": "Tied to model {{models}}",
  "locker.boundHint":
    "“{{sound}}” is tied to model “{{model}}” — it travels with that model. Click to untie.",
  "locker.unboundHint":
    "Tie the active sound “{{sound}}” to model “{{model}}” so switching to it pulls the sound in.",
  "locker.tieAction": "Tie “{{sound}}” to “{{model}}”",
  "locker.untieAction": "Untie “{{sound}}” from “{{model}}”",
  // Toasts.
  "locker.restored": "Restored {{bike}}'s setup files.",
  "locker.restoredNote_one":
    "{{count}} file put back — the bike should load again.",
  "locker.restoredNote_other":
    "{{count}} files put back — the bike should load again.",
  "locker.switchedModel": "Switched {{bike}} model to “{{target}}”.",
  "locker.switchedSound": "Switched {{bike}} sound to “{{target}}”.",
  "locker.tied": "Tied “{{sound}}” to model “{{model}}”.",
  "locker.untied": "Untied “{{sound}}” from model “{{model}}”.",
  "locker.refreshedLive": "Refreshed live in-game.",
  "locker.refreshFailed":
    "Instant refresh failed — reselect your profile in-game to load it.",
  "locker.reselectProfile": "Reselect your profile in MX Bikes to load the swap.",
  "locker.loadsNextTime": "Loads next time the game opens.",
  "locker.modelRefreshing":
    "Refreshing in-game — if it's your selected bike, it changes now.",
  "locker.modelFrostmodNotRunning":
    "Run FrostMod to see model swaps live — for now, reselect the bike in-game.",
  "locker.modelFrostmodUnreachable":
    "Couldn't reach FrostMod — reselect the bike in-game to load it.",
  "locker.modelRefreshWindowsOnly":
    "Live model refresh is Windows-only — reselect the bike in-game.",
  "locker.modelInstantRefreshOff":
    "Reselect the bike in MX Bikes to load it (instant refresh is off).",

  // ── Loose model/sound swap registration ────────────────────────────────────
  "swaps.model": "model",
  "swaps.modelSets_one": "{{count}} model swap",
  "swaps.modelSets_other": "{{count}} model swaps",
  "swaps.soundSets_one": "{{count}} sound mod",
  "swaps.soundSets_other": "{{count}} sound mods",
  // " and " is English prose, not punctuation — each language joins its own way.
  "swaps.and": "{{a}} and {{b}}",
  "swaps.noSets": "0 sets",
  "swaps.foundTitle": "Found {{summary}}",
  "swaps.description":
    "These folders are sitting loose inside your bikes. Register them to move each into the right library — {{modelsFolder}} for models, {{soundsFolder}} for sounds — so they show up in the Locker.",
  "swaps.registered_one": "Registered {{count}} set.",
  "swaps.registered_other": "Registered {{count}} sets.",
  "swaps.nothingMoved": "Nothing was moved.",
  "swaps.skipped_one": "{{count}} skipped (name already in use).",
  "swaps.skipped_other": "{{count}} skipped (names already in use).",
  "swaps.foldersCreated_one": "Created the library folders for {{count}} bike.",
  "swaps.foldersCreated_other": "Created the library folders for {{count}} bikes.",
  "swaps.foldersCreatedDesc":
    "Your model / sound folders were left where they are.",
  "swaps.justCreateFolders": "Just create folders",
  "swaps.registerAndMove": "Register & move",
  "swaps.fileCount_one": "{{count}} file",
  "swaps.fileCount_other": "{{count}} files",

  // ── Install pipeline (toasts + failure card) ───────────────────────────────
  "install.installed": "{{title}} installed",
  "install.reloadedDesc": "Game reloaded via FrostMod — it's live now.",
  "install.addedDesc": "Added to your library.",
  "install.failed": "Install failed — {{title}}",
  "install.openModPage": "Open the mod's page",
  "install.clickToOpen": "Click to open the mod's page",

  // ── Library categories (singular — item "Type" label) ──────────────────────
  "category.track": "Track",
  "category.bike": "Bike",
  "category.bikePaint": "Livery",
  "category.bikeModelSwap": "Model swap",
  "category.sound": "Sound",
  "category.helmet": "Helmet",
  "category.helmetPaint": "Helmet paint",
  "category.goggles": "Goggles",
  "category.boots": "Boots",
  "category.bootPaint": "Boot paint",
  "category.protection": "Protection",
  "category.protectionPaint": "Protection paint",
  "category.gloves": "Gloves",
  "category.outfit": "Outfit / kit",
  "category.misc": "Other",

  // ── Library/Rider section headers (plural) ─────────────────────────────────
  "section.bikePaint": "Liveries",
  "section.bikeModelSwap": "Model swaps",
  "section.sound": "Sounds",
  "section.helmet": "Helmets",
  "section.helmetPaint": "Helmet paints",
  "section.boots": "Boots",
  "section.bootPaint": "Boot paints",
  "section.protection": "Protection",
  "section.protectionPaint": "Protection paints",
  "section.gloves": "Gloves",
  "section.outfit": "Outfit / kit",

  // ── Install destinations ───────────────────────────────────────────────────
  "dest.bikesRoot": "Bikes (root)",
  "dest.tracksRoot": "Tracks (root)",
  "dest.bikeFolder": "{{name}} — bike folder",
  "dest.bikePaints": "{{name}} — paints",
  "dest.helmetsNewModel": "Helmets (new model)",
  "dest.bootsNewModel": "Boots (new model)",
  "dest.protectionNewModel": "Protection (new model)",
  "dest.helmetPaintsFor": "{{name}} · helmet paints",
  "dest.gogglesFor": "{{name}} · goggles",
  "dest.bootPaintsFor": "{{name}} · boot paints",
  "dest.protectionPaintsFor": "{{name}} · protection paints",
  "dest.outfitFor": "{{name}} · outfit / kit",
  "dest.glovesFor": "{{name}} · gloves",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "In-game overlay",
  "overlay.enable": "Enable the in-game overlay",
  "overlay.enableDesc": "Press a shortcut while MX Bikes is running to bring up Presets, Locker and Browse over the game — no alt-tab. Presets and model swaps apply to the running game.",
  "overlay.shortcut": "Overlay shortcut",
  "overlay.shortcutDesc": "Works while the game has focus. Esc closes the overlay and hands control back.",
  "overlay.borderlessNote": "Set MX Bikes to borderless or windowed in Options → Video. Nothing can be drawn over a game running in exclusive fullscreen — the overlay included.",
  "overlay.pressKeys": "Press keys…",
  "overlay.needModifier": "Add a modifier",
  "overlay.needModifierDesc": "Hold Ctrl, Alt or Shift so the shortcut can't fire while you type.",
  "overlay.shortcutUpdated": "Overlay shortcut updated",
  "overlay.shortcutRejected": "Couldn't use that shortcut",
  "overlay.registerFailed": "Overlay hotkey couldn't be registered",
  "overlay.toClose": "{{hotkey}} to close",
  "overlay.closeTitle": "Close overlay (Esc)",
  "overlay.needsSetup": "Finish setting up MXB App in its main window first — it needs to know where your MX Bikes folder is.",
  "overlay.fullscreenBlocked": "The overlay can't show over exclusive fullscreen",
  "overlay.fullscreenBlockedDesc": "Set MX Bikes to borderless or windowed in Options → Video, then try the shortcut again.",
} as const;
