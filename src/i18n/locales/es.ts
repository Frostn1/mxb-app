import type { Translation } from "..";

/**
 * Spanish (neutral — avoids vos/vosotros so it reads naturally in both Spain and
 * Latin America; uses "tú" throughout).
 *
 * Community terminology rather than dictionary equivalents: `mod`, `setup`,
 * `preset` and `Stock` stay as loanwords, while gear is translated — `casco`,
 * `botas`, `gafas` for goggles, `librea` for a bike paint.
 *
 * Product names (MXB App, FrostMod, MX Bikes) are never translated.
 */
export const es: Translation = {
  // ── Genérico ───────────────────────────────────────────────────────────────
  "common.cancel": "Cancelar",
  "common.back": "Atrás",
  "common.next": "Siguiente",
  "common.skip": "Omitir",
  "common.close": "Cerrar",
  "common.save": "Guardar",
  "common.delete": "Eliminar",
  "common.rename": "Renombrar",
  "common.retry": "Reintentar",
  "common.tryAgain": "Reintentar",
  "common.loading": "Cargando…",
  "common.installed": "Instalado",
  "common.select": "Seleccionar",
  "common.deselect": "Deseleccionar",
  "common.selectAll": "Seleccionar todo",
  "common.clear": "Limpiar",
  "common.done": "Hecho",
  "common.apply": "Aplicar",
  "common.remove": "Quitar",
  "common.open": "Abrir",
  "common.refresh": "Actualizar",
  "common.dismiss": "Descartar",
  "common.later": "Más tarde",
  "common.active": "Activo",

  // ── Controles de ventana ───────────────────────────────────────────────────
  "window.minimize": "Minimizar",
  "window.maximize": "Maximizar",
  "window.close": "Cerrar",

  // ── Navegación ─────────────────────────────────────────────────────────────
  "nav.browse": "Explorar",
  "nav.shop": "Tienda",
  "nav.library": "Biblioteca",
  "nav.locker": "Taquilla",
  "nav.presets": "Presets",
  "nav.rider": "Piloto",
  "nav.manage": "Gestionar",
  "nav.settings": "Ajustes",

  "sidebar.installing": "Instalando «{{name}}»",
  "sidebar.queued": "+{{count}} en cola",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Comprobando FrostMod…",
  "frostmod.running": "FrostMod activo",
  "frostmod.notRunning": "FrostMod inactivo",
  "frostmod.reloadGame": "Recargar el juego",
  "frostmod.start": "Iniciar FrostMod",
  "frostmod.reloadedGame": "FrostMod recargó el juego.",
  "frostmod.notRunningToast": "FrostMod no está en ejecución.",
  "frostmod.started": "FrostMod iniciado",
  "frostmod.alreadyRunning": "FrostMod ya está en ejecución",
  "frostmod.startFailed": "No se pudo iniciar FrostMod",
  "frostmod.installedToast": "FrostMod {{version}} instalado",
  "frostmod.installedToastDesc":
    "Recargará el juego en caliente cuando añadas mods.",
  "frostmod.installedToastRestart":
    "Reinicia MX Bikes para usarla — el juego abierto sigue con el FrostMod anterior.",
  "frostmod.installFailed": "No se pudo instalar FrostMod",
  "frostmod.newModsAdded": "Nuevos mods añadidos",
  "frostmod.modsAdded_one": "Nuevo mod añadido",
  "frostmod.modsAdded_other": "{{count}} mods añadidos",
  "frostmod.askedReload": "Se pidió a FrostMod que recargara el juego.",
  "frostmod.andMore_one": "{{names}} y {{count}} más",
  "frostmod.andMore_other": "{{names}} y {{count}} más",
  "frostmod.watchDesc":
    "{{names}} — se pidió a FrostMod que recargara el juego.",

  // ── Configuración inicial ──────────────────────────────────────────────────
  "setup.title": "Bienvenido a MXB App",
  "setup.tagline": "Explora mods, instálalos con un clic y vuelve a la moto enseguida.",
  "setup.modsFolder": "Carpeta de {{game}}",
  "setup.autoDetect":
    "MXB App detectará automáticamente tu carpeta {{hint}}. También puedes elegirla tú.",
  "setup.chooseManually": "Elegir la carpeta manualmente…",
  "setup.chooseDifferent": "Elegir otra carpeta…",
  "setup.gameInstall": "Instalación de {{game}}",
  "setup.detecting": "Buscando tu instalación de {{game}}…",
  "setup.found": "Encontrada",
  "setup.detectedAutomatically": "Detectada automáticamente",
  "setup.installNotFound":
    "No se pudo encontrar automáticamente tu instalación de MX Bikes — es lo que alimenta la vista previa 3D del piloto. Elígela manualmente, o configúrala más tarde en Ajustes.",
  "setup.chooseInstallManually":
    "Elegir la carpeta de instalación manualmente…",
  "setup.startBrowsing": "Empezar a explorar mods",
  "setup.detectAndStart": "Detectar y empezar",
  "setup.pickModsFolder": "Selecciona tu carpeta de {{game}}",
  "setup.pickInstallFolder": "Selecciona la carpeta de instalación de {{game}}",

  // ── Bienvenida ─────────────────────────────────────────────────────────────
  "welcome.intro.title": "Bienvenido a MXB App",
  "welcome.intro.body":
    "Tu gestor de mods para MX Bikes. Mantén pistas, motos y gráficos organizados en un solo sitio — se acabaron los archivos zip repartidos por el escritorio. Te enseñamos todo en unos segundos.",
  "welcome.getStarted": "Empezar",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "falta",
  "presets.missingHint":
    "Este mod no está instalado — se verá como Stock en el juego",
  "presets.missingMods":
    "Mods que faltan: {{mods}}. Instálalos para ver esas piezas.",
  "presets.help":
    "Guarda un look completo de piloto y cárgalo en una moto cuando quieras.",
  "presets.profile": "Perfil",
  "presets.namePlaceholder": "Nombre del preset…",
  "presets.savePreset": "Guardar preset",
  "presets.saveChanges": "Guardar cambios",
  "presets.saveChangesQ": "¿Guardar los cambios?",
  "presets.replaceQ": "¿Reemplazar el preset?",
  "presets.replace": "Reemplazar",
  "presets.loadCopy": "Cargar una copia en el editor",
  "presets.viewOnRider": "Ver en el piloto",
  "presets.editNameOrOptions": "Editar nombre u opciones",
  "presets.share": "Compartir",
  "presets.nameFirst": "Primero dale un nombre al preset.",
  "presets.pickProfileAndBike":
    "Elige un perfil y una moto a los que aplicarlo.",
  "presets.updated": "Preset «{{name}}» actualizado.",
  "presets.renamed": "Renombrado a «{{name}}» y cambios guardados.",
  "presets.saved": "Preset «{{name}}» guardado.",
  "presets.editing":
    "Editando «{{name}}» — cambia lo que quieras y guarda los cambios.",
  "presets.appliedRefreshed":
    "«{{label}}» aplicado a {{bike}} — actualizado en directo en el juego.",
  "presets.appliedRefreshFailed":
    "«{{label}}» aplicado a {{bike}} — guardado, pero falló la actualización instantánea: vuelve a seleccionar tu perfil en el juego para cargarlo.",
  "presets.appliedGameRunning":
    "«{{label}}» aplicado a {{bike}} — guardado. Vuelve a seleccionar tu perfil en MX Bikes (menú Perfil) para cargar el nuevo look.",
  "presets.appliedNextTime":
    "«{{label}}» aplicado a {{bike}} — guardado. Se cargará la próxima vez que abras el juego.",
  "presets.appliedReselectBike":
    "«{{label}}» aplicado a {{bike}} — las decoraciones ya están activas; vuelve a seleccionar la moto en MX Bikes para ver el modelo.",
  "presets.phaseBundling": "Empaquetando los archivos…",
  "presets.phaseUploading": "Subiendo el paquete…",
  "presets.phaseDownloading": "Descargando el paquete…",
  "presets.phaseInstalling": "Instalando los archivos…",
  "presets.bundleUploaded":
    "Paquete completo subido — el código ya incluye los archivos.",
  "presets.shareHintFull":
    "Este código incluye un paquete descargable — quien lo reciba elige Importación completa y lo obtiene todo, incluso sin mods instalados.",
  "presets.shareHintConfig":
    "Envía este código a quien quieras. Lo importa desde Presets → Importar. Necesitará los mismos mods instalados para que se vea cada pieza.",
  "presets.generatingCode": "Generando el código…",
  "presets.nothingToBundle":
    "No hay archivos instalados que empaquetar — este look es todo Stock/fuentes.",
  "presets.createFullBundle": "Crear paquete completo",
  "presets.copiedFull": "Código con paquete completo copiado.",
  "presets.copiedShare": "Código para compartir copiado.",
  "presets.copyFailed":
    "No se pudo copiar — selecciona el código y cópialo a mano.",
  "presets.copyFullCode": "Copiar código completo",
  "presets.copyCode": "Copiar código",
  "presets.importTitle": "Importar preset",
  "presets.importBody": "Pega un código que te hayan enviado.",
  "presets.configOnly": "Solo configuración",
  "presets.import": "Importar",
  "presets.fullImport": "Importación completa",
  "presets.editingBanner":
    "Editando {{name}} — cambia el nombre o cualquier ranura y luego {{save}}.",
  "presets.bundleNotice":
    "Incluye un paquete completo (~{{size}} desde {{host}}). Usa {{fullImport}} para descargar e instalar todo — no hace falta tener mods antes.",

  // ── Ranuras de preset ──────────────────────────────────────────────────────
  "slot.paint": "Librea de la moto",
  "slot.modelSwap": "Cambio de modelo",
  "slot.bikeFont": "Fuente de dorsales",
  "slot.tyres": "Neumáticos",
  "slot.rider": "Perfil de piloto",
  "slot.suitPaint": "Equipación / kit",
  "slot.suitFont": "Fuente de la equipación",
  "slot.glovesPaint": "Guantes",
  "slot.ridingStyle": "Estilo de pilotaje",
  "slot.helmet": "Casco",
  "slot.helmetPaint": "Gráficos del casco",
  "slot.gogglesPaint": "Gafas",
  "slot.boots": "Botas",
  "slot.bootsPaint": "Gráficos de las botas",
  "slot.protection": "Protecciones",
  "slot.protectionPaint": "Gráficos de las protecciones",
  "slotGroup.bike": "Moto",
  "slotGroup.rider": "Piloto",
  "slotGroup.head": "Cabeza",
  "slotGroup.body": "Cuerpo",

  // ── Estudio del piloto ─────────────────────────────────────────────────────
  "rider.help":
    "Viste al modelo del piloto — casco, gafas, equipación y botas a la vez.",
  "rider.namePlaceholder": "Pon nombre a este piloto…",
  "rider.nameFirst": "Primero ponle nombre a este look.",
  "rider.showOnModel": "Mostrar en el modelo",

  // ── Visita guiada ──────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Haz un recorrido rápido",
  "tour.welcomeTour.body":
    "Unos segundos para ver dónde está cada cosa. Puedes omitirlo cuando quieras.",
  "tour.browse.title": "Explorar mods",
  "tour.browse.body": "Busca en {{site}} desde aquí e instala cualquier circuito, moto o diseño con un solo clic.",
  "tour.library.title": "Tu biblioteca",
  "tour.library.body":
    "Todo lo que has instalado, en un solo sitio — actualiza o elimina mods sin tocar nunca un archivo zip.",
  "tour.locker.title": "La taquilla",
  "tour.locker.body":
    "Cambia los modelos de las motos a tu gusto. MXB App registra las piezas para que el juego las reconozca.",
  "tour.presets.title": "Presets",
  "tour.presets.body":
    "Guarda combinaciones de equipación y gráficos, y aplica un look completo con un clic — incluso mientras estás rodando.",
  "tour.rider.title": "Estudio del piloto",
  "tour.rider.body":
    "Previsualiza tu equipación y tus gráficos sobre el piloto 3D antes de llevarlos a la pista.",
  "tour.frostmod.title": "FrostMod, en directo",
  "tour.frostmod.body":
    "Aquí ves el estado de FrostMod. Recarga MX Bikes tras una instalación, así el contenido nuevo aparece sin reiniciar el juego.",
  "tour.settings.title": "Ajustes",
  "tour.settings.body":
    "Aquí configuras tu carpeta de juego, el comportamiento en segundo plano y las opciones de FrostMod. También puedes repetir esta visita desde aquí.",
  "tour.done.title": "Todo listo",
  "tour.done.body":
    "Fin del recorrido. Ve a Explorar e instala tu primer mod.",

  // ── Errores ────────────────────────────────────────────────────────────────
  "error.previewFailed": "No se pudo mostrar la vista previa",
  "error.somethingWentWrong": "Algo salió mal",
  "error.unexpected": "Se produjo un error inesperado.",
  "error.reloadApp": "Recargar la aplicación",

  // ── Actualizaciones ────────────────────────────────────────────────────────
  "update.available": "{{version}} ya está disponible.",
  "update.downloading": "Descargando…",
  "update.downloadingPct": "Descargando… {{pct}} %",
  "update.pitch":
    "Actualiza para tener las últimas funciones y correcciones.",
  "update.updating": "Actualizando…",
  "update.updateAndRestart": "Actualizar y reiniciar",
  "update.dismiss": "Descartar la notificación de actualización",
  "update.onLatest": "Ya tienes la última versión",
  "update.checkFailed": "No se pudieron comprobar las actualizaciones",
  "update.failed": "La actualización falló",

  // ── Visor 3D ───────────────────────────────────────────────────────────────
  "viewer.preview3d": "Vista previa 3D",
  "viewer.expand": "Ampliar",
  "viewer.paint": "Gráficos",
  "viewer.loadingModel": "Cargando modelo…",
  "viewer.loadingPaint": "Cargando gráficos…",
  "viewer.loadingRider": "Cargando piloto…",
  "viewer.riderLoadFailed": "La vista previa está desactualizada: no se pudo actualizar",
  "viewer.dragToRotate": "Arrastra para rotar",
  "viewer.scrollToZoom": "Desplaza para hacer zoom",
  "viewer.rightDragToPan": "Arrastra con el botón derecho para mover",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Buscar…",
  "combobox.use": "Usar «{{value}}»",

  // ── Tipos de mod ───────────────────────────────────────────────────────────
  "modType.tracks": "Pistas",
  "modType.bikes": "Motos",
  "modType.rider": "Piloto",
  "modType.tracksInline": "pistas",
  "modType.bikesInline": "motos",
  "modType.riderInline": "equipación de piloto",

  // ── Filtros de categoría ───────────────────────────────────────────────────
  "browseCat.all": "Todo",
  "browseCat.beginner": "Principiante",
  "browseCat.intermediate": "Intermedio",
  "browseCat.pro": "Pro",
  "browseCat.assets": "Recursos",
  "browseCat.newBikes": "Motos nuevas",
  "browseCat.liveries": "Libreas",
  "browseCat.sounds": "Sonidos",
  "browseCat.riderKit": "Kit de piloto",
  "browseCat.helmets": "Cascos",
  "browseCat.helmetPaints": "Gráficos de casco",
  "browseCat.gloves": "Guantes",
  "browseCat.boots": "Botas",
  "browseCat.bootPaints": "Gráficos de botas",
  "browseCat.protection": "Protecciones",
  "browseCat.protectionPaints": "Gráficos de protecciones",

  // ── Explorar ───────────────────────────────────────────────────────────────
  "browse.help":
    "Descubre e instala mods del catálogo en línea — busca, filtra por tipo y abre un mod para descargarlo al juego.",
  "browse.searchPlaceholder": "Buscar {{type}}…",
  "browseSort.newest": "Más recientes",
  "browseSort.oldest": "Más antiguos",
  "browseSort.popularAll": "Más populares",
  "browseSort.popularMonth": "Populares este mes",
  "browseSort.popularWeek": "Populares esta semana",
  "browse.loadFailed": "No se pudieron cargar los mods",
  "browse.empty": "No se encontraron {{type}}.",
  "browse.loadMore": "Cargar más",
  "browse.selectedCount": "{{count}} seleccionados",
  "browse.queuing": "Añadiendo a la cola…",
  "browse.quickInstallCount": "Instalar rápido {{count}}",
  "browse.quickInstall": "Instalación rápida",
  "browse.quickReinstall": "Reinstalación rápida",
  "browse.openDetails": "Abrir detalles",
  "browse.reinstallOne": "¿Reinstalar «{{title}}»?",
  "browse.reinstallMany": "¿Reinstalar los mods que ya tienes?",
  "browse.reinstallOneBody":
    "Este mod ya está en tu biblioteca. Al reinstalarlo se descarga de nuevo y se sobrescriben los archivos instalados.",
  "browse.reinstallManyBody":
    "{{installed}} de los {{total}} seleccionados ya están instalados. Si continúas se reinstalan y se sobrescriben.",
  "browse.reinstall": "Reinstalar",
  "browse.reinstallAll": "Reinstalar todo",
  "browse.queued": "«{{title}}» en cola",
  "browse.queuedDesc": "Instalando en {{folder}}.",
  "browse.rootFolder": "raíz",
  "browse.needsBrowser": "«{{title}}» requiere descarga desde el navegador",
  "browse.needsBrowserDesc":
    "{{host}} bloquea las descargas dentro de la app — abre su página para terminar.",
  "browse.noDownload": "No se encontró descarga para «{{title}}»",
  "browse.quickInstallFailed":
    "No se pudo instalar rápido «{{title}}»",
  "browse.queuedBulk_one": "{{count}} mod en cola",
  "browse.queuedBulk_other": "{{count}} mods en cola",
  "browse.queuedBulkDesc": "Se instalarán uno tras otro.",
  "browse.queuedBulkSkipped_one":
    "{{count}} omitido — host solo de navegador.",
  "browse.queuedBulkSkipped_other":
    "{{count}} omitidos — host solo de navegador.",
  "browse.bulkFailed": "No se pudo instalar rápido la selección",
  "browse.bulkFailedDesc_one":
    "Requiere descarga desde el navegador.",
  "browse.bulkFailedDesc_other":
    "Los {{count}} requieren descarga desde el navegador.",

  // ── Tienda ─────────────────────────────────────────────────────────────────
  "shop.myDownloads": "Mis descargas",
  "shop.signInTitle": "Inicia sesión en MX Bikes Shop",
  "shop.signInBody":
    "Inicia sesión en mxbikes-shop.com para ver e instalar las pistas que has comprado. Abrimos el sitio real — tu contraseña nunca pasa por esta aplicación.",
  "shop.signIn": "Iniciar sesión",
  "shop.logOut": "Cerrar sesión",
  "shop.signedIn": "Sesión iniciada en MX Bikes Shop",
  "shop.sessionFailed":
    "No se pudo capturar tu sesión de MX Bikes Shop",
  "shop.queuedDesc": "Instalando en tu carpeta de pistas.",
  "shop.loadFailed": "No se pudieron cargar tus descargas: {{error}}",
  "shop.empty": "Aún no hay descargas compradas en tu cuenta.",

  // ── Diálogo de instalación ─────────────────────────────────────────────────
  "installDialog.installTo": "Instalar en",
  "installDialog.installToFolder": "Instalar en {{folder}}",
  "installDialog.change": "Cambiar",
  "installDialog.searchBikes": "Buscar motos…",
  "installDialog.searchFolders": "Buscar carpetas…",
  "installDialog.probably": "Probablemente",
  "installDialog.allFolders": "Todas las carpetas",
  "installDialog.noFolderMatch":
    "Ninguna carpeta coincide — créala abajo.",
  "installDialog.rememberedFor": "Recordado para {{type}}",
  "installDialog.downloadFrom": "Descargar desde",
  "installDialog.downloadPerBike": "Descarga (por moto)",
  "installDialog.opensInBrowser":
    "Se abre en el navegador — MXB App termina la instalación",
  "installDialog.matchedBike": "Coincide con tu moto",
  "installDialog.differentBike": "Moto / pack distinto",
  "installDialog.directFastest": "Directo · el más rápido",
  "installDialog.direct": "Directo",
  "installDialog.perBikeHint":
    "Cada descarga es una moto distinta — se selecciona automáticamente según tu elección. Elige el pack «all bikes» para todas las motos de una vez.",
  "installDialog.mirrorsHint":
    "Todos los espejos contienen el mismo archivo. Si uno falla, prueba el siguiente.",

  // ── Detalles de biblioteca ─────────────────────────────────────────────────
  "libraryDetail.author": "Autor",
  "libraryDetail.length": "Longitud",
  "libraryDetail.altitude": "Altitud",
  "libraryDetail.location": "Ubicación",
  "libraryDetail.type": "Tipo",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Pertenece a",
  "libraryDetail.format": "Formato",
  "libraryDetail.extractedFolder": "Carpeta extraída",
  "libraryDetail.paintFile": "Archivo de gráficos",
  "libraryDetail.packagedPkz": "Paquete .pkz",
  "libraryDetail.size": "Tamaño",
  "libraryDetail.folder": "Carpeta",
  "libraryDetail.lockedWord": "bloqueada",
  "libraryDetail.lockedWithMeta":
    "Esta pista está {{locked}} por su creador. Su nombre, detalles y vista previa se muestran aquí, pero los archivos siguen sellados — no se puede extraer ni ver en 3D.",
  "libraryDetail.lockedNoMeta":
    "Esta pista está {{locked}}, así que su nombre, longitud y vista previa no se pueden leer del archivo — solo su nombre de archivo y su tamaño.",

  // ── Página del mod ─────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Resolver",
  "modDetail.stageDownload": "Descargar",
  "modDetail.stageExtract": "Extraer",
  "modDetail.stagePlace": "Colocar",
  "modDetail.stageReload": "Recargar",
  "modDetail.modFiles": "Archivos de mod",
  "modDetail.copied": "Copiado",
  "modDetail.copy": "Copiar",
  "modDetail.addToLibrary": "Añadir a la biblioteca",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Se instala en",
  "modDetail.noDownloadLink": "No se encontró ningún enlace de descarga en esta página — ábrela en {{site}}.",
  "modDetail.frostmodHint":
    "FrostMod recargará la lista de {{kind}} cuando esto termine.",
  "modDetail.kindRider": "piloto",
  "modDetail.kindBike": "motos",
  "modDetail.kindTrack": "pistas",
  "modDetail.details": "Detalles",
  "modDetail.format": "Formato",
  "modDetail.mirrors": "Espejos",
  "modDetail.type": "Tipo",
  "modDetail.addedToLibrary": "Añadido a tu biblioteca",
  "modDetail.extracting": "Extrayendo…",
  "modDetail.addingToLibrary": "Añadiendo a la biblioteca…",
  "modDetail.resolving": "Resolviendo la descarga…",
  "modDetail.finishInBrowser": "Termina en tu navegador",
  "modDetail.viewOnSite": "Ver en {{site}}",

  // ── Ajustes ────────────────────────────────────────────────────────────────
  "settings.help":
    "Configura tu carpeta de juego, las actualizaciones y las preferencias de la aplicación.",
  "settings.gameFolder": "Carpeta del juego",
  "settings.general": "General",
  "settings.appearance": "Apariencia",
  "settings.frostmod": "FrostMod",
  "settings.about": "Acerca de y actualizaciones",
  "settings.whatsNew": "Novedades",
  "settings.modsFolderDesc":
    "Donde se instalan los mods. Cambiarla vuelve a analizar tu biblioteca.",
  "settings.insideModsFolder": "Dentro de tu carpeta de MX Bikes",
  "settings.notSet": "Sin definir",
  "settings.change": "Cambiar…",
  "settings.set": "Definir…",
  "settings.theme": "Tema",
  "settings.themeLight": "Claro",
  "settings.themeDark": "Oscuro",
  "settings.themeSystem": "Sistema",
  "settings.language": "Idioma",
  "settings.languageSystem": "Sistema",
  "settings.runInBackground": "Seguir en segundo plano",
  "settings.runInBackgroundDesc":
    "Cerrar la ventana deja MXB App en la bandeja del sistema para que FrostMod siga conectado. Sal desde el icono de la bandeja.",
  "settings.launchAtStartup": "Iniciar al arrancar",
  "settings.launchAtStartupDesc":
    "Inicia MXB App automáticamente al iniciar sesión.",
  "settings.instantRefresh": "Actualización instantánea de presets",
  "settings.instantRefreshDesc":
    "Cuando aplicas un preset con MX Bikes en marcha, actualiza el look en el juego al instante — sin reiniciar ni volver a seleccionar el perfil. Si no puede, se te pedirá que vuelvas a seleccionar tu perfil.",
  "settings.instantRefreshWindowsOnly":
    "Actualizar el look en el juego sin reiniciar necesita FrostMod, que es solo para Windows — en su lugar se te pedirá que vuelvas a seleccionar tu perfil.",
  "settings.autoRunFrostmod": "Ejecutar FrostMod automáticamente",
  "settings.autoRunFrostmodDesc":
    "Inicia FrostMod en segundo plano cada vez que abres MXB App.",
  "settings.watchModsReload": "Recarga automática al cambiar la carpeta",
  "settings.watchModsReloadDesc":
    "Recarga el juego automáticamente cuando se añaden pistas o motos a tu carpeta de mods — incluso descargadas manualmente fuera de MXB App.",
  "settings.checking": "Comprobando…",
  "settings.runningConnected": "En ejecución · juego conectado",
  "settings.notRunning": "Inactivo",
  "settings.frostmodInstalled": "Instalado{{suffix}}",
  "settings.notInstalled": "No instalado",
  "settings.checkingGitHub":
    "Comprobando la última versión en GitHub…",
  "settings.updateCheckFailed":
    "No se pudieron comprobar las actualizaciones — sin conexión o GitHub no disponible.",
  "settings.latestVersion": "Última: {{version}}",
  "settings.frostmodNeedsRepair":
    "Los archivos instalados no coinciden con esta versión — reinstalar lo arregla.",
  "settings.frostmodRepair": "Reparar instalación",
  "settings.checkNewer": "Buscar una versión más reciente de FrostMod",
  "settings.working": "Trabajando…",
  "settings.installFrostmod": "Instalar FrostMod",
  "settings.updateTo": "Actualizar a {{version}}",
  "settings.reinstallLatest": "Reinstalar la última",
  "settings.upToDate": "Actualizado",
  "settings.madeWith": "Hecho con",
  "settings.updateFailed": "No se pudo actualizar el ajuste",
  "settings.startupUpdateFailed":
    "No se pudo actualizar el inicio automático",
  "settings.folderUpdated": "Carpeta del juego actualizada",
  "settings.folderUpdatedDesc": "Tu biblioteca se volverá a analizar.",
  "settings.setFolderFailed": "No se pudo definir la carpeta",
  "settings.reDetected": "Carpeta de MX Bikes detectada de nuevo",
  "settings.detectFolderFailed": "No se pudo detectar la carpeta",
  "settings.pickInstallFolder":
    "Selecciona tu carpeta de instalación de MX Bikes (contiene rider.pkz)",
  "settings.installSet": "Instalación del juego definida",
  "settings.installSetDesc":
    "La vista previa 3D del piloto ya puede cargar el modelo real del cuerpo.",
  "settings.setInstallFailed":
    "No se pudo definir la carpeta de instalación",
  "settings.installNotFound": "No se pudo encontrar MX Bikes",
  "settings.installNotFoundDesc":
    "No se detectó ninguna instalación de Steam — define la carpeta manualmente.",
  "settings.installFound": "Instalación de MX Bikes encontrada",
  "settings.detectInstallFailed":
    "No se pudo detectar la carpeta de instalación",
  "settings.pickProfilesFolder":
    "Selecciona tu carpeta de perfiles de MX Bikes",
  "settings.profilesSet": "Carpeta de perfiles definida",
  "settings.profilesFound_one": "Se encontró {{count}} perfil.",
  "settings.profilesFound_other": "Se encontraron {{count}} perfiles.",
  "settings.noProfilesThere": "No se encontraron perfiles ahí",
  "settings.noProfilesThereDesc":
    "Se guardó igualmente, pero crear presets necesita una carpeta que contenga tus carpetas de profile.ini.",
  "settings.setProfilesFailed":
    "No se pudo definir la carpeta de perfiles",
  "settings.profilesReverted":
    "Se restauró la carpeta de perfiles predeterminada",
  "settings.resetProfilesFailed":
    "No se pudo restablecer la carpeta de perfiles",
  "settings.frostmodNotRunningHint":
    "FrostMod no está en ejecución — inícialo para recargar mods en caliente.",
  "settings.reloadUnavailable":
    "La recarga no está disponible en esta plataforma.",

  // ── Inicio del juego ───────────────────────────────────────────────────────
  "game.play": "Jugar",
  "game.starting": "Iniciando…",
  "game.running": "MX Bikes en ejecución",
  "game.launch": "Iniciar MX Bikes",
  "game.alreadyRunning": "MX Bikes ya está en ejecución",
  "game.launching": "Iniciando MX Bikes…",
  "game.launchFailed": "No se pudo iniciar MX Bikes",

  // ── Cadenas que el primer barrido no vio (JSX multilínea) ─────────────────
  "libraryDetail.noEmbedded": "No se encontraron detalles incrustados para este elemento.",
  "modDetail.downloadFromHost": "Descargar desde {{host}}",
  "modDetail.openHost": "Abrir {{host}}",
  "modDetail.thenAddFile": "Después añade el archivo",
  "modDetail.chooseDownloaded": "Elige el archivo descargado",
  "presets.chooseProfilesFolder": "Elegir carpeta de perfiles…",
  "presets.viewInRider": "Ver en Piloto",
  "presets.noModelSwapsHere": "No hay cambios de modelo registrados para esta moto —",
  "presets.setUpInLocker": "configúralos en la Taquilla",
  "presets.makeActiveBike": "Hacer que esta sea la moto activa",
  "presets.nameClash":
    "Ya hay otro preset llamado «{{name}}» — al guardar también lo sobrescribirás.",
  "presets.shareWarning":
    "Se sube a un enlace público y temporal — redistribuye archivos de mods hechos por otros, así que comparte con responsabilidad.",
  "settings.profilesDesc":
    "Los presets leen tus perfiles de aquí — la ruta de abajo es donde está mirando la app ahora mismo. Es la carpeta {{profiles}} dentro de tu carpeta de MX Bikes, o {{documents}} si moviste tu carpeta de mods. Defínela solo si la tuya está en otro sitio.",
  "settings.resetToDefault": "Restablecer",
  "settings.gameInstallDesc":
    "Carpeta de instalación del juego (opcional) — donde está instalado MX Bikes (contiene {{file}}). Defínela para cargar el cuerpo real del piloto en la vista previa 3D.",
  "viewer.stockGearNote":
    "Mostrado sobre el {{part}} de serie del juego. Unos gráficos hechos para otro modelo pueden no encajar del todo.",
  "viewer.paintNoChange":
    "Ninguna de las texturas de estos gráficos la usan las piezas que se muestran aquí, así que la vista previa no cambia. Aun así puede pintar las ruedas o la cadena, que esta vista no representa.",
  "viewer.noPaintPreview": "Sin vista previa de los gráficos ({{err}})",

  // ── Biblioteca ─────────────────────────────────────────────────────────────
  "library.help":
    "Tus mods instalados. Revisa lo que tienes instalado y quita lo que ya no quieras.",
  "library.rootFolder": "(raíz)",
  "library.byAuthor": "de {{author}}",
  "library.locked": "Bloqueado — no se puede leer el contenido",
  "library.searchPlaceholder": "Buscar entre los instalados…",
  "library.scanning": "Analizando tu biblioteca…",
  "library.empty":
    "Aún no hay {{type}} instaladas — ve a Explorar y añade alguna.",
  "library.noMatches": "Sin resultados.",
  "library.quick3d": "Vista 3D rápida",
  "library.selectNone": "No seleccionar nada",
  "library.move": "Mover",
  "library.uninstall": "Desinstalar",
  "library.uninstallAction": "Desinstalar…",
  "library.moveToFolder": "Mover a una carpeta…",
  "library.showInExplorer": "Mostrar en el explorador",
  "library.moveDialogTitle": "Mover a una carpeta",
  "library.moveCount_one": "Mover {{count}} elemento",
  "library.moveCount_other": "Mover {{count}} elementos",
  "library.chooseDestination": "Elige una carpeta de destino",
  "library.newFolder": "Nueva carpeta…",
  "library.newFolderName": "Nombre de la nueva carpeta",
  "library.createAndMove": "Crear y mover",
  "library.confirmUninstall": "¿Desinstalar {{name}}?",
  "library.confirmUninstallBody":
    "El elemento se mueve a la Papelera de reciclaje — puedes restaurarlo desde ahí.",
  "library.confirmBulkUninstall_one": "¿Desinstalar {{count}} elemento?",
  "library.confirmBulkUninstall_other":
    "¿Desinstalar {{count}} elementos?",
  "library.confirmBulkUninstallBody":
    "Cada elemento se mueve a la Papelera de reciclaje — puedes restaurarlos desde ahí.",
  "library.uninstallCount": "Desinstalar {{count}}",
  "library.moveFailed": "No se pudo mover el mod",
  "library.uninstallFailed": "No se pudo desinstalar",
  "library.openFailed": "No se pudo abrir",
  "library.uninstalledOne": "{{name}} desinstalado",
  "library.movedToBin": "Movido a la Papelera de reciclaje.",
  "library.someNotRemoved": "Algunos elementos no se pudieron quitar.",
  "library.bulkUninstalled_one": "{{count}} elemento desinstalado",
  "library.bulkUninstalled_other": "{{count}} elementos desinstalados",
  "library.bulkUninstallPartial":
    "{{ok}} desinstalados, {{fail}} fallidos",
  "library.bulkMovePartial": "{{ok}} movidos, {{fail}} fallidos",
  "library.bulkMoved_one": "{{count}} elemento movido a {{folder}}",
  "library.bulkMoved_other": "{{count}} elementos movidos a {{folder}}",

  // ── Taquilla ───────────────────────────────────────────────────────────────
  "locker.help":
    "Cambia el modelo y el sonido del motor de cada moto entre los sets que tengas instalados.",
  "locker.rescan": "Volver a analizar",
  "locker.restore": "Restaurar",
  "locker.register": "Registrar",
  "locker.scanning": "Analizando motos…",
  "locker.scanForSwaps": "Buscar sets",
  "locker.orphanBanner":
    "A {{bike}} le faltan sus archivos de setup — una versión anterior los movió a una carpeta de swap, y eso impide por completo que la moto cargue en el juego. {{files}}",
  "locker.looseBanner_one":
    "{{count}} set de modelo / sonido encontrado suelto entre tus motos — regístralo en {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} sets de modelo / sonido encontrados sueltos entre tus motos — regístralos en {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "Todavía no hay motos intercambiables.",
  "locker.emptyIntro":
    "Se tienen que cumplir dos condiciones para poder hacer un cambio:",
  "locker.unpacked": "extraída",
  "locker.emptyRuleUnpacked":
    "La moto está {{unpacked}} en {{path}}— un {{pkz}} comprimido no se puede intercambiar. Extrae una desde la Biblioteca.",
  "locker.emptyRuleMesh":
    "Cada modelo alternativo va en su propia carpeta dentro de esa moto y contiene una malla ({{edf}}). Ponla en cualquier sitio dentro de la carpeta de la moto y pulsa Buscar abajo — te ofreceremos archivarla en {{folder}}.",
  "locker.summary": "{{model}} · sonido «{{sound}}»",
  "locker.modelNamed": "modelo «{{name}}»",
  "locker.noModelSwaps": "sin cambios de modelo",
  "locker.models": "Modelos",
  "locker.sounds": "Sonidos",
  "locker.onlyOneModel":
    "Solo un modelo — instala más para poder cambiar",
  "locker.onlyStock":
    "Solo Stock — instala un mod de sonido para poder cambiar",
  "locker.noModel": "Sin modelo",
  "locker.stock": "Stock",
  "locker.activeModel": "Modelo activo",
  "locker.activeSound": "Sonido activo",
  "locker.switchToNoModel":
    "Cambiar a sin modelo — quita los archivos del modelo actual",
  "locker.switchToStock":
    "Cambiar a Stock — quita el mod de sonido (suena el original)",
  "locker.missingModelEdf": "Este set no tiene model.edf",
  "locker.missingSoundFiles": "A este set le falta engine.scl o sfx.cfg",
  "locker.switchTo": "Cambiar a {{name}}",
  "locker.tiedToModel": "Vinculado al modelo {{models}}",
  "locker.boundHint":
    "«{{sound}}» está vinculado al modelo «{{model}}» — viaja con ese modelo. Haz clic para desvincular.",
  "locker.unboundHint":
    "Vincula el sonido activo «{{sound}}» al modelo «{{model}}» para que al cambiar a él se traiga también el sonido.",
  "locker.tieAction": "Vincular «{{sound}}» a «{{model}}»",
  "locker.untieAction": "Desvincular «{{sound}}» de «{{model}}»",
  "locker.restored": "Archivos de setup de {{bike}} restaurados.",
  "locker.restoredNote_one":
    "{{count}} archivo devuelto a su sitio — la moto debería cargar de nuevo.",
  "locker.restoredNote_other":
    "{{count}} archivos devueltos a su sitio — la moto debería cargar de nuevo.",
  "locker.switchedModel":
    "Modelo de {{bike}} cambiado a «{{target}}».",
  "locker.switchedSound": "Sonido de {{bike}} cambiado a «{{target}}».",
  "locker.tied": "«{{sound}}» vinculado al modelo «{{model}}».",
  "locker.untied": "«{{sound}}» desvinculado del modelo «{{model}}».",
  "locker.refreshedLive": "Actualizado en directo en el juego.",
  "locker.refreshFailed":
    "Falló la actualización instantánea — vuelve a seleccionar tu perfil en el juego para cargarla.",
  "locker.reselectProfile":
    "Vuelve a seleccionar tu perfil en MX Bikes para cargar el cambio.",
  "locker.loadsNextTime":
    "Se cargará la próxima vez que abras el juego.",
  "locker.modelRefreshing":
    "Actualizando en el juego — si es la moto que tienes seleccionada, cambia ahora.",
  "locker.modelFrostmodNotRunning":
    "Ejecuta FrostMod para ver los cambios de modelo en directo — por ahora, vuelve a seleccionar la moto en el juego.",
  "locker.modelReselectBike":
    "Modelo cambiado — vuelve a seleccionar la moto en MX Bikes para verlo.",
  "locker.modelFrostmodUnreachable":
    "No se pudo contactar con FrostMod — vuelve a seleccionar la moto en el juego para cargarla.",
  "locker.modelRefreshWindowsOnly":
    "La actualización del modelo en directo es solo para Windows — vuelve a seleccionar la moto en el juego.",
  "locker.modelInstantRefreshOff":
    "Vuelve a seleccionar la moto en MX Bikes para cargarla (la actualización instantánea está desactivada).",

  // ── Registro de sets sueltos ───────────────────────────────────────────────
  "swaps.model": "modelo",
  "swaps.modelSets_one": "{{count}} cambio de modelo",
  "swaps.modelSets_other": "{{count}} cambios de modelo",
  "swaps.soundSets_one": "{{count}} mod de sonido",
  "swaps.soundSets_other": "{{count}} mods de sonido",
  "swaps.and": "{{a}} y {{b}}",
  "swaps.noSets": "0 sets",
  "swaps.foundTitle": "Se encontraron {{summary}}",
  "swaps.description":
    "Estas carpetas están sueltas dentro de tus motos. Regístralas para mover cada una a la biblioteca correcta — {{modelsFolder}} para modelos, {{soundsFolder}} para sonidos — y que aparezcan en la Taquilla.",
  "swaps.registered_one": "{{count}} set registrado.",
  "swaps.registered_other": "{{count}} sets registrados.",
  "swaps.nothingMoved": "No se movió nada.",
  "swaps.skipped_one": "{{count}} omitido (nombre ya en uso).",
  "swaps.skipped_other": "{{count}} omitidos (nombres ya en uso).",
  "swaps.foldersCreated_one":
    "Se crearon las carpetas de biblioteca para {{count}} moto.",
  "swaps.foldersCreated_other":
    "Se crearon las carpetas de biblioteca para {{count}} motos.",
  "swaps.foldersCreatedDesc":
    "Tus carpetas de modelo / sonido se quedaron donde estaban.",
  "swaps.justCreateFolders": "Solo crear las carpetas",
  "swaps.registerAndMove": "Registrar y mover",
  "swaps.fileCount_one": "{{count}} archivo",
  "swaps.fileCount_other": "{{count}} archivos",

  // ── Instalación ────────────────────────────────────────────────────────────
  "install.installed": "{{title}} instalado",
  "install.reloadedDesc":
    "Juego recargado con FrostMod — ya está activo.",
  "install.addedDesc": "Añadido a tu biblioteca.",
  "install.failed": "Fallo en la instalación — {{title}}",
  "install.openModPage": "Abrir la página del mod",
  "install.clickToOpen": "Haz clic para abrir la página del mod",

  // ── Categorías (singular) ──────────────────────────────────────────────────
  "category.track": "Pista",
  "category.bike": "Moto",
  "category.bikePaint": "Librea",
  "category.bikeModelSwap": "Cambio de modelo",
  "category.sound": "Sonido",
  "category.helmet": "Casco",
  "category.helmetPaint": "Gráficos del casco",
  "category.goggles": "Gafas",
  "category.boots": "Botas",
  "category.bootPaint": "Gráficos de las botas",
  "category.protection": "Protecciones",
  "category.protectionPaint": "Gráficos de las protecciones",
  "category.gloves": "Guantes",
  "category.outfit": "Equipación / kit",
  "category.misc": "Otros",

  // ── Encabezados de sección (plural) ────────────────────────────────────────
  "section.bikePaint": "Libreas",
  "section.bikeModelSwap": "Cambios de modelo",
  "section.sound": "Sonidos",
  "section.helmet": "Cascos",
  "section.helmetPaint": "Gráficos de casco",
  "section.boots": "Botas",
  "section.bootPaint": "Gráficos de botas",
  "section.protection": "Protecciones",
  "section.protectionPaint": "Gráficos de protecciones",
  "section.gloves": "Guantes",
  "section.outfit": "Equipación / kit",

  // ── Destinos de instalación ────────────────────────────────────────────────
  "dest.bikesRoot": "Motos (raíz)",
  "dest.tracksRoot": "Pistas (raíz)",
  "dest.bikeFolder": "{{name}} — carpeta de la moto",
  "dest.bikePaints": "{{name}} — gráficos",
  "dest.helmetsNewModel": "Cascos (modelo nuevo)",
  "dest.bootsNewModel": "Botas (modelo nuevo)",
  "dest.protectionNewModel": "Protecciones (modelo nuevo)",
  "dest.helmetPaintsFor": "{{name}} · gráficos de casco",
  "dest.gogglesFor": "{{name}} · gafas",
  "dest.bootPaintsFor": "{{name}} · gráficos de botas",
  "dest.protectionPaintsFor": "{{name}} · gráficos de protecciones",
  "dest.outfitFor": "{{name}} · equipación / kit",
  "dest.glovesFor": "{{name}} · guantes",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay en el juego",
  "overlay.enable": "Activar el overlay en el juego",
  "overlay.enableDesc": "Pulsa un atajo mientras MX Bikes está abierto para mostrar Presets, Locker y Browse sobre el juego — sin alt-tab. Los presets y los cambios de modelo se aplican al juego en marcha.",
  "overlay.shortcut": "Atajo del overlay",
  "overlay.shortcutDesc": "Funciona aunque el juego tenga el foco. Esc cierra el overlay y devuelve el control.",
  "overlay.borderlessTitle": "Juega a MX Bikes sin bordes o en ventana",
  "overlay.borderlessNote": "Nada se puede dibujar sobre un juego que retiene la pantalla en modo exclusivo — el overlay incluido. Pon MX Bikes en Borderless (o Windowed) desde Options → Video y aparecerá sobre el juego como esperas.",
  "overlay.gameRunning": "MX Bikes está abierto",
  "overlay.gameNotRunning": "MX Bikes no está abierto",
  "overlay.showNow": "Mostrar el overlay ahora",
  "overlay.showFailed": "No se pudo abrir el overlay",
  "overlay.hotkeyTaken": "Otra aplicación está usando este atajo",
  "overlay.hotkeyTakenDesc": "La combinación se la queda la primera aplicación que la pide, así que el overlay nunca se abre. Elige otra arriba — el silenciar de Discord suele ser el culpable.",
  "overlay.fullscreenNow": "MX Bikes está ahora en pantalla completa exclusiva",
  "overlay.fullscreenNowDesc": "El overlay sí se abre — es el juego el que se dibuja encima. Cambia a sin bordes o en ventana desde Options → Video.",
  "overlay.notWorking": "¿Lo pulsaste y no pasó nada?",
  "overlay.notWorkingDesc": "Revisa el atajo de arriba: puede que otra aplicación ya tenga esa combinación, y elegir una libre es lo que lo arregla.",
  "overlay.pressKeys": "Pulsa las teclas…",
  "overlay.needModifier": "Añade un modificador",
  "overlay.needModifierDesc": "Mantén Ctrl, Alt o Shift para que el atajo no salte mientras escribes.",
  "overlay.shortcutUpdated": "Atajo del overlay actualizado",
  "overlay.shortcutRejected": "No se pudo usar ese atajo",
  "overlay.registerFailed": "No se pudo registrar el atajo del overlay",
  "overlay.toClose": "{{hotkey}} para cerrar",
  "overlay.closeTitle": "Cerrar overlay (Esc)",
  "overlay.openMain": "Abrir la app completa",
  "overlay.openMainTitle": "Cierra el overlay y abre la ventana principal de MXB App",
  "overlay.needsSetup": "Termina de configurar MXB App en su ventana principal — necesita saber dónde está tu carpeta de MX Bikes.",
  "overlay.fullscreenBlocked": "El overlay no puede mostrarse sobre la pantalla completa exclusiva",
  "overlay.fullscreenBlockedDesc": "Pon MX Bikes en modo sin bordes o en ventana desde Options → Video y vuelve a pulsar el atajo.",

  // Presentación de la versión — la ventana de novedades que aparece una vez tras actualizar.
  "showcase.eyebrow": "Recién actualizado",
  "showcase.title": "Novedades de la {{version}}",
  "showcase.subtitle": "Primero lo grande. Todo lo demás de esta versión está en las notas.",
  "showcase.whileGameRunning": "mientras MX Bikes está abierto",
  "showcase.releaseNotes": "Leer las notas de la versión",
  "showcase.gotIt": "Entendido",
  "showcase.v070.hero.title": "Un overlay en el juego, con un atajo",
  "showcase.v070.hero.body": "Abre Preset, Locker y Browse sobre MX Bikes — sin alt-tab. Esc devuelve el control al momento, y un preset elegido aquí cae en la sesión que ya estás rodando. Juega sin bordes o en ventana: sobre la pantalla completa exclusiva no se puede dibujar nada.",
  "showcase.v070.hero.action": "Configurar el overlay",
  "showcase.v070.languages": "MXB App habla seis idiomas — elige el tuyo en Ajustes → Apariencia.",
  "showcase.v070.browse": "Browse ordena por más populares y las tarjetas muestran la puntuación en estrellas.",
  "showcase.v070.play": "Un botón Play en la barra lateral abre MX Bikes.",
  "showcase.v070.paint": "Las motos vuelven a llevar su pintura correcta — Kawasaki KX y Yamaha YZ arregladas.",
  "manage.help":
    "MX Bikes carga todos los mods de tu carpeta al arrancar. Dale a un preset la pista en la que corre, pulsa Modo carrera y todo lo demás se aparta — no se borra nada, solo se mueve a una carpeta de espera hasta que lo traigas de vuelta.",
  "manage.tabRace": "Presets de carrera",
  "manage.tabMods": "Mods",
  "manage.disabledCount_one": "{{count}} mod desactivado",
  "manage.disabledCount_other": "{{count}} mods desactivados",
  "manage.restoreAll": "Activar todo",
  "manage.restoreTitle": "¿Devolver todos los mods?",
  "manage.restoreBody":
    "Los {{count}} mods desactivados vuelven exactamente a las carpetas de las que salieron. MX Bikes volverá a cargarlos todos.",
  "manage.restored_one": "Recuperado {{count}} mod.",
  "manage.restored_other": "Recuperados {{count}} mods.",
  "manage.applyLookTo": "Aplicar el aspecto a",
  "manage.applyLookHelp":
    "El modo carrera escribe la pintura y el equipo del preset en este perfil y esta moto, igual que la pestaña Presets. Deja alguno vacío para mover solo el contenido sin tocar tu aspecto.",
  "manage.noPresets": "Aún no hay presets guardados — crea uno en la pestaña Presets.",
  "manage.noContentYet": "Sin contenido de carrera — añade una pista para usar el modo carrera",
  "manage.noTrack": "Sin pista",
  "manage.pinnedCount_one": "{{count}} fijado",
  "manage.pinnedCount_other": "{{count}} fijados",
  "manage.editContent": "Editar contenido",
  "manage.raceMode": "Modo carrera",
  "manage.raceTitle": "¿Correr con «{{name}}»?",
  "manage.raceBody":
    "Mantiene {{keep}} mods y aparta {{disable}}, así MX Bikes carga solo el contenido de esta carrera.",
  "manage.raceReEnable_one": "Vuelve {{count}} mod desactivado que este preset necesita.",
  "manage.raceReEnable_other": "Vuelven {{count}} mods desactivados que este preset necesita.",
  "manage.raceLook": "Su pintura y equipo van a {{bike}} en el perfil {{profile}}.",
  "manage.raceNoLook": "Solo contenido — elige arriba perfil y moto para aplicar también el aspecto.",
  "manage.raceNoBike":
    "No se mantiene ningún mod de moto — te quedarías con las motos de serie del juego. Fija la moto que usas en Mantener siempre.",
  "manage.raceGameRunning":
    "MX Bikes está abierto. Los archivos que tiene en uso no se pueden mover — cierra el juego primero.",
  "manage.raceUnresolved": "No están instalados, así que saldrán de serie: {{slots}}",
  "manage.raceGo": "Preparar la carrera",
  "manage.raceApplied": "Listo para correr «{{name}}» — {{count}} mods apartados.",
  "manage.contentSaved": "Contenido de carrera guardado para «{{name}}».",
  "manage.contentTitle": "Contenido de carrera de «{{name}}»",
  "manage.contentBody":
    "La pintura, el equipo y el cambio de modelo del preset se encuentran solos. Esto es para el resto: la pista, los modelos de equipo de repuesto que quieras conservar y los packs que una carrera necesita igualmente.",
  "manage.paneTracks": "Pistas",
  "manage.paneHelmets": "Cascos",
  "manage.paneBoots": "Botas",
  "manage.paneProtection": "Protecciones",
  "manage.paneKeep": "Mantener siempre",
  "manage.paneTracksHint": "La pista (o pistas) para las que es este preset.",
  "manage.paneGearHint":
    "Modelos extra que quedan en el selector del juego. El equipo del propio preset se mantiene solo: marca aquí lo demás que quieras seguir teniendo a mano. Todo lo que quede sin marcar se aparta.",
  "manage.paneKeepHint":
    "Mods que siguen activos pase lo que pase — el pack OEM, la moto de este preset, un mod de sonido.",
  "manage.notInstalled": "no instalado",
  "manage.off": "off",
  "manage.enabledOne": "{{name}} activado.",
  "manage.disabledOne": "{{name}} desactivado.",
  "manage.enabledMany_one": "Activado {{count}} mod.",
  "manage.enabledMany_other": "Activados {{count}} mods.",
  "manage.disabledMany_one": "Desactivado {{count}} mod.",
  "manage.disabledMany_other": "Desactivados {{count}} mods.",
  "manage.enableShown": "Activar los visibles ({{count}})",
  "manage.disableShown": "Desactivar los visibles ({{count}})",
  "manage.noMods": "Todavía no hay mods instalados.",
  "manage.someFailed_one": "No se pudo mover {{count}} mod: {{first}}",
  "manage.someFailed_other": "No se pudieron mover {{count}} mods: {{first}}",
  "manage.deleteTitle": "¿Eliminar {{name}}?",
  "manage.deleteBody": "Va a la papelera, así que todavía puedes recuperarlo desde ahí.",
  "manage.deleted": "{{name}} eliminado.",
  "game.label": "Juego",
  "game.switch": "Cambiar de juego",
  "game.switchFailed": "No se pudo cambiar de juego",
  "settings.instantRefreshMxOnly": "Solo MX Bikes — {{game}} no recarga perfiles en caliente.",
  "modType.misc": "Varios",
  "modType.miscInline": "extras",
  "browseCat.raceTracks": "Circuitos",
  "browseCat.kartTracks": "Circuitos de karts",
  "browseCat.others": "Otros",
  "browseCat.riderModels": "Modelos de piloto",
  "browseCat.suitPaints": "Diseños de mono",
  "browseCat.helmetModels": "Modelos de casco",
  "browseCat.plugins": "Complementos",
  "browseCat.tools": "Herramientas",
  "browseCat.menuBackgrounds": "Fondos de menú",
  "category.animation": "Estilo de pilotaje",
  "section.animation": "Estilos de pilotaje",
  "modDetail.restartHint": "Reinicia {{game}} para que detecte {{kind}} nuevo.",
  "modDetail.protonHint": "Los archivos de Proton Drive están cifrados, así que no se pueden descargar automáticamente.",
};
