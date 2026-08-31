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
  "nav.hub": "MXB Hub",
  "nav.library": "Biblioteca",
  "nav.downloads": "Descargas",
  "nav.locker": "Taquilla",
  "nav.presets": "Presets",
  "nav.rider": "Piloto",
  "nav.pose": "Postura",
  "nav.designer": "Designer",
  "nav.paints": "Pinturas",
  "nav.studio": "Studio",
  "nav.servers": "Servidores",
  "nav.manage": "Gestionar",
  "nav.settings": "Ajustes",

  "sidebar.installing": "Instalando «{{name}}»",
  "sidebar.installingCount": "Instalando {{count}} mods",
  "sidebar.queued": "+{{count}} en cola",
  "sidebar.expand": "Expandir la barra lateral",
  "sidebar.collapse": "Contraer la barra lateral",
  "sidebar.showGroup": "Mostrar lo que hay dentro de {{name}}",
  "sidebar.hideGroup": "Ocultar lo que hay dentro de {{name}}",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Comprobando FrostMod…",
  "frostmod.running": "FrostMod activo",
  "frostmod.notRunning": "FrostMod inactivo",
  "frostmod.notInGame": "FrostMod no está en el juego",
  "frostmod.reloadGame": "Recargar el juego",
  "frostmod.start": "Iniciar FrostMod",
  "frostmod.reloadedGame": "FrostMod recargó el juego.",
  "frostmod.notRunningToast": "FrostMod no está en ejecución.",
  "frostmod.started": "FrostMod iniciado",
  "frostmod.alreadyRunning": "FrostMod ya está en ejecución",
  "frostmod.startFailed": "No se pudo iniciar FrostMod",
  "frostmod.stop": "Detener FrostMod",
  "frostmod.stopped": "FrostMod detenido",
  "frostmod.stopFailed": "No se pudo detener FrostMod",
  "frostmod.stopFailedDesc":
    "Sigue en ejecución: puede que lo haya iniciado otro usuario o que se ejecute con permisos de administrador.",
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
    "No se pudo encontrar automáticamente tu instalación de {{game}} — es lo que alimenta la vista previa 3D del piloto. Elígela manualmente, o configúrala más tarde en Ajustes.",
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
  "presets.forgetBike": "Quitar moto",
  "presets.forgetBikeOne": "Quitar {{name}} de este perfil",
  "presets.forgetBikeQ": "¿Quitar esta moto?",
  "presets.forgetBikeBody":
    "“{{name}}” desaparece de la lista de motos de este perfil, junto con el aspecto guardado para ella. No se borra nada instalado: si vuelves a rodar con esa moto, el juego la añade otra vez.",
  "presets.bikeForgotten": "“{{name}}” quitada de este perfil.",
  "presets.forgetFailed": "No se pudo quitar esa moto",
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


  // ── Pose studio ────────────────────────────────────────────────────────────
  "pose.help": "Coloca al piloto — dónde van las manos, cuánto se abren las piernas, una pierna adelante. Solo la vista previa; MX Bikes toma la postura del estilo de pilotaje.",
  "pose.showing": "Mostrando",
  "pose.none": "—",
  "pose.bike": "Moto",
  "pose.quick": "Posturas rápidas",
  "pose.quickHint": "Cada una se suma a la postura, así que se acumulan. Ajusta abajo.",
  "pose.dragHint": "Arrastra los puntos del piloto para mover un miembro: gira la articulación que está por encima de la que agarras. El miembro se mueve a la mitad de la velocidad del cursor; mantén Mayús para más precisión. Los deslizadores son para el giro y los valores exactos.",
  "pose.reset": "Restablecer",
  "pose.group.torso": "Torso y cabeza",
  "pose.group.arms": "Brazos",
  "pose.group.hands": "Manos",
  "pose.group.legs": "Piernas",
  "pose.move.legsWide": "Piernas más abiertas",
  "pose.move.legsNarrow": "Piernas más juntas",
  "pose.move.leftLegForward": "Pierna izquierda adelante",
  "pose.move.elbowsUp": "Codos arriba",
  "pose.move.leanIn": "Inclinarse",
  "pose.move.ride": "Posición de pilotaje",
  "pose.axis.bend": "Flexión",
  "pose.axis.twist": "Giro",
  "pose.axis.splay": "Apertura",
  "pose.quickWaiting": "Esperando al modelo del piloto: cada movimiento es un sitio al que mandar una articulación, así que necesita el rig para saber dónde está.",
  "pose.photo": "Foto",
  "pose.photoHint": "El encuadre limpio oculta los puntos y los paneles. La foto se guarda al doble del tamaño del panel: abre la vista previa a pantalla completa para una mayor.",
  "pose.cleanFrame": "Encuadre limpio",
  "pose.savePhoto": "Guardar foto",
  "pose.photoSaved": "Foto guardada",
  "pose.photoFailed": "No se pudo guardar la foto",
  "pose.scene.studio": "Estudio",
  "pose.scene.white": "Blanco",
  "pose.scene.sky": "Día",
  "pose.scene.sunset": "Atardecer",
  "pose.scene.dusk": "Anochecer",

  // ── Estudio del piloto ─────────────────────────────────────────────────────
  "rider.help":
    "Viste al modelo del piloto — casco, gafas, equipación y botas a la vez.",
  "rider.namePlaceholder": "Pon nombre a este piloto…",
  "rider.nameFirst": "Primero ponle nombre a este look.",
  "rider.showOnModel": "Mostrar en el modelo",
  "rider.repairTitle": "Un mod de {{area}} se instaló suelto",
  "rider.repairBody":
    "Sus archivos están directamente en {{area}} en vez de en una carpeta, así que ni el juego ni esta app pueden cargarlo. ¿Recogerlos en “{{model}}”?",
  "rider.repairAction": "Reparar",
  "rider.repairDone_one": "Se recogió {{count}} archivo en “{{model}}”.",
  "rider.repairDone_other": "Se recogieron {{count}} archivos en “{{model}}”.",
  "rider.repairNothing": "No queda nada por recoger.",
  "rider.unwrapTitle": "Un mod de {{area}} se instaló una carpeta más abajo de la cuenta",
  "rider.unwrapBody":
    "“{{folder}}” no contiene más que {{model}}, y un mod empaquetado solo carga desde {{area}} en sí — así que ni el juego ni esta app lo ven. ¿Subirlo?",
  "rider.unwrapDone_one": "Se subió {{count}} mod. Ahora aparece como “{{model}}”.",
  "rider.unwrapDone_other": "Se subieron {{count}} mods, empezando por “{{model}}”.",

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
  "tour.servers.title": "Verte bien en línea",
  "tour.servers.body": "MX Bikes nunca envía pinturas entre jugadores, así que todos aparecen con la equipación por defecto salvo que ya tengas su archivo exacto. Regístrate aquí y la app publica tu look y descarga el de los demás — y desde la misma página puedes arrancar un servidor dedicado.",
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

  // ── Falta el runtime de Visual C++ ─────────────────────────────────────────
  "runtime.componentVc90": "Microsoft Visual C++ 2008 (x64)",
  "runtime.componentVc140": "Microsoft Visual C++ 2015–2022 (x64)",
  "runtime.bannerGame":
    "MX Bikes necesita {{what}} antes de que FrostMod pueda entrar en el juego.",
  "runtime.bannerFrostmod": "FrostMod necesita {{what}} para funcionar.",
  "runtime.pitch":
    "Sin esto Windows muestra el error «dll was not found». Se arregla en segundos.",
  "runtime.fixIt": "Instalarlo",
  "runtime.installing": "Instalando…",
  "runtime.dismiss": "Descartar este aviso",
  "runtime.installed": "Componente instalado",
  "runtime.installedDesc":
    "FrostMod ya debería llegar al juego. Reinicia MX Bikes si lo tienes abierto.",
  "runtime.cancelled": "No se instaló nada",
  "runtime.cancelledDesc":
    "Windows necesita tu permiso para instalarlo. Abriendo la descarga de Microsoft.",
  "runtime.installFailed": "No se pudo instalar el componente",
  "runtime.downloadManually": "Descargarlo tú mismo",
  "runtime.componentVc140X86": "Microsoft Visual C++ 2015–2022 (x86)",
  "runtime.repairing": "Reparando…",
  "runtime.repairDone": "Componentes reparados",
  "runtime.repairDoneDesc":
    "Reinicia MX Bikes si ya está abierto y vuelve a intentarlo.",
  "runtime.repairNothingToDo": "Ya estaba todo en su sitio",
  "runtime.repairNothingToDoDesc":
    "Todos los componentes de Visual C++ están instalados y la carpeta del juego tiene lo que necesita. Si aun así no arranca, mándanos tu registro.",
  "runtime.repairPartial": "Una parte todavía te necesita",
  "runtime.repairPartialDesc":
    "No se pudo terminar: {{what}}. Windows necesita tu permiso, o la descarga no llegó — puedes instalarlo a mano.",
  "runtime.repairNoGameFolder": "No hay carpeta del juego",
  "runtime.repairNoGameFolderDesc":
    "Los componentes están instalados, pero sin la carpeta de instalación no podemos revisar la carpeta del juego. Indícala arriba y repara otra vez.",
  "runtime.repairFailed": "No se pudieron reparar los componentes",
  "runtime.strayForeign": "Un archivo de tu carpeta del juego ({{what}}) hace que MX Bikes falle.",
  "runtime.strayLocked": "{{what}}, en tu carpeta del juego, hace que MX Bikes falle.",
  "runtime.strayPitch":
    "Es lo que provoca el error \"R6034\" al arrancar. Apartarlo lo soluciona, y no se borra nada.",
  "runtime.strayLockedPitch":
    "Es lo que provoca el error \"R6034\" al arrancar. Cierra MX Bikes primero y luego apártalo.",
  "runtime.strayFix": "Apartarlo",
  "runtime.strayFixHint":
    "Lo renombra a msvcr90.dll.disabled para que Windows deje de cargarlo. No se borra nada.",
  "runtime.strayClearing": "Moviendo…",
  "runtime.strayCleared": "Apartado del camino",
  "runtime.strayClearedDesc":
    "Ahora se llama msvcr90.dll.disabled, en la misma carpeta. Vuelve a abrir MX Bikes.",
  "runtime.strayClearFailed": "No se pudo mover el archivo",
  "update.checkFailed": "No se pudieron comprobar las actualizaciones",
  "update.failed": "La actualización falló",

  // ── Visor 3D ───────────────────────────────────────────────────────────────
  "viewer.preview3d": "Vista previa 3D",
  "viewer.expand": "Ampliar",
  "viewer.fullscreen": "Pantalla completa",
  "viewer.exitFullscreen": "Salir de pantalla completa",
  "viewer.paint": "Gráficos",
  "viewer.tyres": "Neumáticos",
  "viewer.tyresOwn": "Los de la moto",
  "viewer.loadingModel": "Cargando modelo…",
  "viewer.loadingPaint": "Cargando gráficos…",
  "viewer.loadingRider": "Cargando piloto…",
  "viewer.riderLoadFailed": "La vista previa está desactualizada: no se pudo actualizar",
  "viewer.both": "Ambos",
  "viewer.onBike": "En la moto",
  "viewer.noSeat": "El archivo de reglaje de esta moto no dice dónde está el asiento, así que el piloto no puede sentarse en ella.",
  "viewer.loadingBike": "Cargando moto…",
  "viewer.bikeLoadFailed": "La vista previa de la moto está desactualizada: no se pudo actualizar",
  "viewer.dragToRotate": "Arrastra para rotar",
  "viewer.scrollToZoom": "Desplaza para hacer zoom",
  "viewer.rightDragToPan": "Arrastra con el botón derecho para mover",
  "viewer.paintReloaded": "Pintura recargada",
  "viewer.pose": "Postura",
  "viewer.poseRear": "Trasera",
  "viewer.poseFront": "Delantera",
  "viewer.poseSteer": "Dirección",
  "viewer.poseLevel": "Nivelar ruedas",
  "viewer.poseReset": "Restablecer",
  "viewer.place": "Colocación",
  "viewer.placeSide": "Lateral",
  "viewer.placeUp": "Altura",
  "viewer.placeFwd": "Adelante",
  "viewer.placeTurn": "Girar",
  "viewer.resizePanel": "Arrastra para ajustar · doble clic para restablecer",

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
  "browse.queuedDesc": "Se instalará en cuanto le llegue el turno.",
  "browse.byAuthor": "por {{author}}",
  "browse.needsBrowser": "«{{title}}» requiere descarga desde el navegador",
  "browse.needsBrowserDesc":
    "{{host}} bloquea las descargas dentro de la app — abre su página para terminar.",
  "browse.noDownload": "No se encontró descarga para «{{title}}»",
  "browse.serverOnly": "«{{title}}» solo ofrece archivos de servidor",
  "browse.serverOnlyDesc":
    "Abre el mod para ver sus descargas: una compilación para servidor dedicado no se instala por ti.",
  "browse.quickInstallFailed":
    "No se pudo instalar rápido «{{title}}»",
  "browse.queuedBulk_one": "{{count}} mod en cola",
  "browse.queuedBulk_other": "{{count}} mods en cola",
  "browse.queuedBulkDesc": "Se instalarán uno tras otro.",

  // ── Tienda (MX Bikes Shop — descargas compradas) ───────────────────────────
  "shop.help":
    "Explora el catálogo de mxbikes-shop.com e instala lo que ya has comprado. La compra sigue haciéndose en el sitio de la tienda; inicia sesión en Mis compras para instalar tus pedidos desde aquí.",
  "shopTab.catalog": "Catálogo",
  "shopTab.purchases": "Mis compras",
  "shop.myDownloads": "Mis compras",
  "shop.signInTitle": "Inicia sesión en MX Bikes Shop",
  "shop.signInBody":
    "Inicia sesión en mxbikes-shop.com para ver e instalar todo lo que has comprado. Abrimos el sitio real — tu contraseña nunca pasa por esta aplicación.",
  "shop.signIn": "Iniciar sesión",
  "shop.logOut": "Cerrar sesión",
  "shop.signedIn": "Sesión iniciada en MX Bikes Shop",
  "shop.sessionFailed": "No se pudo capturar tu sesión de MX Bikes Shop",
  "shop.loadFailed": "No se pudieron cargar tus compras: {{error}}",
  "shop.empty": "Aún no hay descargas compradas en tu cuenta.",

  // ── MXB Hub (shop.mxb-hub.com) ──
  "hub.help":
    "Explora el mercado MXB Hub en shop.mxb-hub.com e instala lo que ya tienes. La compra sigue haciéndose en el sitio de la tienda; inicia sesión en Mis compras para instalar tus pedidos desde aquí.",
  "hub.count_one": "{{count}} artículo",
  "hub.count_other": "{{count}} artículos",
  "hub.searchPlaceholder": "Buscar en MXB Hub…",
  "hub.myDownloads": "Mis compras",
  "hub.signInTitle": "Iniciar sesión en MXB Hub",
  "hub.signInBody":
    "Inicia sesión en shop.mxb-hub.com para ver e instalar todo lo que tienes, incluidos los mods gratuitos. Abrimos el sitio real: tu contraseña nunca pasa por esta aplicación.",
  "hub.signIn": "Iniciar sesión",
  "hub.signedIn": "Sesión iniciada en MXB Hub",
  "hub.sessionFailed": "No se pudo capturar tu sesión de MXB Hub",
  "hub.empty": "Aún no hay descargas en tu cuenta de MXB Hub.",
  "hubSort.newest": "Más recientes",
  "hubSort.popular": "Más populares",
  "hubSort.priceAsc": "Precio: de menor a mayor",
  "hubSort.priceDesc": "Precio: de mayor a menor",
  "hubSort.nameAsc": "Nombre (A–Z)",
  "purchases.count_one": "{{count}} compra",
  "purchases.count_other": "{{count}} compras",
  "purchases.fileCount_one": "{{count}} archivo",
  "purchases.fileCount_other": "{{count}} archivos",
  "purchases.install": "Instalar",
  "purchases.reinstall": "Reinstalar",
  "purchases.installed": "Instalado",
  "purchases.downloading": "Descargando…",
  "purchases.downloadFailed": "No se pudo descargar {{title}}",
  "purchases.searchPlaceholder": "Buscar en tus compras…",
  "purchases.otherCategory": "Otros",
  "purchases.notInstalledOnly": "Sin instalar",
  "purchases.noMatches": "Ninguna de tus compras coincide con eso.",
  "purchases.viewDetails": "Ver detalles",
  "purchaseSort.recentlyPurchased": "Compradas recientemente",
  "purchaseSort.nameAsc": "Nombre (A–Z)",
  "purchaseSort.notInstalled": "Sin instalar primero",
  // ── Catálogo de MX Bikes Shop (solo explorar; la compra es en la tienda) ───
  "shopCatalog.searchPlaceholder": "Buscar en la tienda…",
  "shopCatalog.allCategories": "Todo",
  "shopCatalog.onSaleOnly": "En oferta",
  "shopCatalog.loadMore": "Cargar más",
  "shopCatalog.loadFailed": "No se pudo cargar el catálogo de la tienda",
  "shopCatalog.empty": "Nada en la tienda coincide con eso.",
  "shopCatalog.viewDetails": "Ver detalles",
  "shopCatalog.openOnStore": "Abrir en mxbikes-shop.com",
  "shopCatalog.buyOnStore": "Comprar en mxbikes-shop.com",
  "shopCatalog.buyNote": "Se abre en tu navegador. La compra y la descarga se hacen en la tienda.",
  "shopCatalog.noProductLink": "Este artículo no tiene una página de producto que podamos abrir.",
  "shopCatalog.noScreenshots": "Sin capturas",
  "shopCatalog.about": "Sobre este artículo",
  "shopCatalog.author": "Creador",
  "shopCatalog.category": "Categoría",
  "shopCatalog.updated": "Actualizado",
  "shopCatalog.priceUnknown": "Precio no indicado",
  "shopCatalog.free": "Gratis",
  "shopCatalog.refresh": "Actualizar",
  "shopCatalog.refreshing": "Actualizando…",
  "shopCatalog.stale": "Precios comprobados por última vez {{when}}.",
  "shopCatalog.staleHard":
    "Estos precios se comprobaron por última vez {{when}} y pueden estar desactualizados. Actualiza antes de fiarte de ellos.",
  "shopCatalog.saleEndsDays_one": "La oferta termina en 1 día",
  "shopCatalog.saleEndsDays_other": "La oferta termina en {{count}} días",
  "shopCatalog.saleEndsHours_one": "La oferta termina en 1 hora",
  "shopCatalog.saleEndsHours_other": "La oferta termina en {{count}} horas",
  "shopCatalog.saleEndsSoon": "La oferta termina pronto",
  "shopCatalog.agoJustNow": "ahora mismo",
  "shopCatalog.agoUnknown": "hace un tiempo",
  "shopCatalog.agoMinutes_one": "hace 1 minuto",
  "shopCatalog.agoMinutes_other": "hace {{count}} minutos",
  "shopCatalog.agoHours_one": "hace 1 hora",
  "shopCatalog.agoHours_other": "hace {{count}} horas",
  "shopCatalog.agoDays_one": "hace 1 día",
  "shopCatalog.agoDays_other": "hace {{count}} días",
  "shopSort.newest": "Más recientes",
  "shopSort.recentlyUpdated": "Actualizados recientemente",
  "shopSort.priceAsc": "Precio: de menor a mayor",
  "shopSort.priceDesc": "Precio: de mayor a menor",
  "shopSort.onSale": "Ofertas primero",
  "shopSort.nameAsc": "Nombre (A–Z)",

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
  "installDialog.recommendedBadge": "Recomendado",
  "installDialog.matchedBadge": "Tu moto",
  "installDialog.browserBadge": "Navegador",
  "installDialog.serverBadge": "Servidor",
  "installDialog.serverBuildNote": "Compilación para servidor dedicado — no sirve para jugar",
  "installDialog.serverFiles_one": "1 archivo para servidor dedicado",
  "installDialog.serverFiles_other": "{{count}} archivos para servidor dedicado",
  "installDialog.serverOnlyNotice":
    "Todas las descargas de aquí son compilaciones para servidor dedicado. Instala una solo si gestionas un servidor: no añade nada para rodar.",
  "installDialog.moreMirrors_one": "1 espejo más",
  "installDialog.moreMirrors_other": "{{count}} espejos más",
  "installDialog.perBikeHint":
    "Cada descarga es una moto distinta — se selecciona automáticamente según tu elección. Elige el pack «all bikes» para todas las motos de una vez.",

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
  "modDetail.loadFailed": "No se pudo cargar este mod",
  "modDetail.copied": "Copiado",
  "modDetail.copy": "Copiar",
  "modDetail.addToLibrary": "Añadir a la biblioteca",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Se instala en",
  "modDetail.noDownloadLink": "No se encontró ningún enlace de descarga en esta página — ábrela en {{site}}.",
  "modDetail.serverOnlyNotice":
    "Esta página solo ofrece archivos para servidor dedicado. Se instalan bien, pero en el juego no hay nada que rodar.",
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
  "modDetail.chooseWhatToInstall": "Elige qué instalar",
  "modDetail.resolving": "Resolviendo la descarga…",
  "modDetail.finishInBrowser": "Termina en tu navegador",
  "modDetail.viewOnSite": "Ver en {{site}}",

  // ── Ajustes ────────────────────────────────────────────────────────────────
  "settings.help":
    "Configura tu carpeta de juego, las actualizaciones y las preferencias de la aplicación.",
  "settings.groupSetup": "Configuración",
  "settings.groupApp": "App",
  "settings.groupAdvanced": "Avanzado",
  "settings.groupAbout": "Acerca de",
  "settings.gameFolder": "Carpeta del juego",
  "settings.general": "General",
  "settings.appearance": "Apariencia",
  "settings.frostmod": "FrostMod",
  "settings.about": "Acerca de y actualizaciones",
  "settings.whatsNew": "Novedades",
  "settings.modsFolderDesc":
    "Donde se instalan los mods. Elige la carpeta que contiene las carpetas mods y profiles \u2014 la de encima de mods, no la carpeta mods en sí. Cambiarla vuelve a analizar tu biblioteca.",
  "settings.insideModsFolder": "Dentro de tu carpeta de {{game}}",
  "settings.notSet": "Sin definir",
  "settings.selectFolderFor": "Selecciona una carpeta para {{game}}",
  "settings.gameDesc":
    "Qué juego está gestionando MXB App. Tus carpetas, tu biblioteca y tus presets pertenecen al juego que elijas aquí.",
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
    "Cuando aplicas un preset con {{game}} en marcha, actualiza el look en el juego al instante — sin reiniciar ni volver a seleccionar el perfil. Si no puede, se te pedirá que vuelvas a seleccionar tu perfil.",
  "settings.instantRefreshWindowsOnly":
    "Actualizar el look en el juego sin reiniciar implica entrar en el juego en marcha, y eso solo puede hacerlo la versión de Windows — en su lugar se te pedirá que vuelvas a seleccionar tu perfil.",
  "settings.autoRunFrostmod": "Ejecutar FrostMod automáticamente",
  "settings.autoRunFrostmodDesc":
    "Inicia FrostMod en segundo plano cada vez que abres MXB App.",
  "settings.rcamKeys":
    "Teclas de la cámara de repetición",
  "settings.rcamKeysDesc":
    "El juego lee la misma tecla en el mismo momento, así que elige teclas que tus controles de cámara no usen — las teclas de función y el teclado numérico suelen estar libres.",
  "settings.rcamReset":
    "Restablecer",
  "settings.rcamPressKey":
    "Pulsa una tecla",
  "settings.rcamUnbound":
    "Sin asignar",
  "settings.rcamCaptureHint":
    "Pulsa la tecla que quieras. Esc cancela, Retroceso borra la asignación.",
  "settings.rcamAppliesHint":
    "Se aplica la próxima vez que abras el editor en el juego — sin reiniciar.",
  "settings.rcamDuplicate":
    "Dos acciones comparten una tecla.",
  "settings.rcamUnsupportedKey":
    "FrostMod no puede usar esa tecla.",
  "settings.rcamSaveFailed":
    "No se pudieron guardar las teclas",
  "settings.rcamActionSetkey":
    "Fijar punto",
  "settings.rcamActionDelete":
    "Borrar punto",
  "settings.rcamActionClear":
    "Vaciar trayecto",
  "settings.rcamActionPlay":
    "Reproducir / parar",
  "settings.rcamActionPrev":
    "Punto anterior",
  "settings.rcamActionNext":
    "Punto siguiente",
  "settings.rcamActionSave":
    "Guardar trayecto",
  "settings.rcamActionClean":
    "Ocultar superposición",
  "settings.rcamActionLoad":
    "Cargar trayecto",
  "settings.rcamActionEase":
    "Interpolacion",
  "settings.rcamActionTarget":
    "Apuntar al piloto",
  "settings.rcamActionNudgeback":
    "Mover atras",
  "settings.rcamActionNudgefwd":
    "Mover adelante",
  "settings.rcamActionUndo":
    "Deshacer",
  "settings.rcamActionRig":
    "Soporte de camara",
  "settings.rcamActionRetime":
    "Redistribuir por distancia",
  "settings.rcamActionPreview":
    "Mostrar la ruta",
  "settings.rcamActionCurve":
    "Estilo de curva",
  "settings.rcamActionAnchor":
    "Eje de la ruta",
  "settings.rcamActionAutofov":
    "FOV automatico",
  "settings.rcamPaths":
    "Rutas de camara de repeticion",
  "settings.rcamPathsDesc":
    "Las nueve ranuras donde FrostMod guarda las rutas. Abre una para ver donde caen sus claves, cambiar como vuela o compartirla.",
  "settings.rcamPathsEmpty":
    "Aun no hay nada guardado. Pon claves en el juego con el editor de camara y pulsa guardar.",
  "settings.rcamUnavailable":
    "La camara de repeticion no funciona en esta version del juego: {{reason}}",
  "settings.rcamNotCalibrated":
    "La punteria aun no esta calibrada. Abre el editor en el juego y gira la camara una vez.",
  "settings.rcamRefresh":
    "Actualizar",
  "settings.rcamSlot":
    "Ranura {{n}}",
  "settings.rcamSlotEmpty":
    "Vacia",
  "settings.rcamKeyCount_one":
    "{{count}} clave",
  "settings.rcamKeyCount_other":
    "{{count}} claves",
  "settings.rcamShotCount_one":
    "{{count}} plano",
  "settings.rcamShotCount_other":
    "{{count}} planos",
  "settings.rcamTimelineHint":
    "Arrastra una clave para moverla. Los tiempos se ajustan a la rejilla de 30 ms y una clave no puede pasar a su vecina.",
  "settings.rcamKeyAt":
    "Clave en {{time}}",
  "settings.rcamNoKeySelected":
    "Elige una clave en la linea de tiempo para cambiar lo que hace.",
  "settings.rcamAim":
    "Apuntar a",
  "settings.rcamAimNone":
    "nadie",
  "settings.rcamEaseSmooth":
    "Suave",
  "settings.rcamEaseHold":
    "Pausa",
  "settings.rcamEaseCut":
    "Corte",
  "settings.rcamCurve":
    "Curva",
  "settings.rcamCurveCentripetal":
    "Centripeta",
  "settings.rcamCurveUniform":
    "Uniforme",
  "settings.rcamAxis":
    "Eje",
  "settings.rcamAxisClock":
    "Reloj de la repeticion",
  "settings.rcamAxisTrack":
    "Vuelta del piloto",
  "settings.rcamAxisHint":
    "Atada a la vuelta de un piloto, la ruta sirve en cualquier vuelta, piloto y repeticion. Cada clave necesita un piloto apuntado y una posicion en la vuelta.",
  "settings.rcamRig":
    "Soporte",
  "settings.rcamRigLocked":
    "Fijo",
  "settings.rcamRigHandheld":
    "En mano",
  "settings.rcamRigDrone":
    "Dron",
  "settings.rcamRigCrane":
    "Grua",
  "settings.rcamRigAmount":
    "Intensidad",
  "settings.rcamAutoFov":
    "Mantener el tamano del sujeto",
  "settings.rcamAutoFovHint":
    "El FOV sigue al piloto apuntado para que se vea siempre del mismo tamano.",
  "settings.rcamSave":
    "Guardar",
  "settings.rcamRevert":
    "Revertir",
  "settings.rcamRetime":
    "Redistribuir por distancia",
  "settings.rcamExport":
    "Exportar",
  "settings.rcamImport":
    "Importar",
  "settings.rcamDelete":
    "Eliminar",
  "settings.rcamSaved":
    "Ruta guardada",
  "settings.rcamWriteFailed":
    "No se pudo guardar la ruta",
  "settings.rcamReadFailed":
    "No se pudo leer la ruta",
  "settings.rcamRetimeDone":
    "Claves redistribuidas. Guarda para conservarlo.",
  "settings.rcamRetimeFailed":
    "No se pudieron redistribuir las claves",
  "settings.rcamDeleted":
    "Ranura {{n}} eliminada",
  "settings.rcamDeleteFailed":
    "No se pudo eliminar la ruta",
  "settings.rcamImported":
    "Importada en la ranura {{n}}",
  "settings.rcamImportFailed":
    "No se pudo importar esa ruta",
  "settings.rcamExported":
    "Ruta exportada",
  "settings.rcamExportFailed":
    "No se pudo exportar la ruta",
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
  "settings.frostmodStrayMsvcr90":
    "Un archivo de tu carpeta del juego hace que MX Bikes falle con \"R6034\" — apártalo para solucionarlo.",
  "settings.frostmodRuntimeMissing":
    "A Windows le falta un componente de Visual C++ que FrostMod necesita — instálalo para quitar el error «dll was not found».",
  "settings.repairRuntimes": "Reparar componentes",
  "settings.repairRuntimesHint":
    "Instala todos los componentes de Visual C++ que le falten a este PC, 32 y 64 bits, y retira lo que una versión anterior de esta app dejó en la carpeta del juego. Vale la pena aunque arriba no se vea nada mal.",
  "settings.frostmodNeedsRepair":
    "Los archivos instalados no coinciden con esta versión — reinstalar lo arregla.",
  "settings.frostmodRepair": "Reparar instalación",
  "settings.frostmodUnsupportedForGame":
    "Esta versión de FrostMod no es segura en {{game}} — actualízala para usar FrostMod aquí.",
  "settings.frostmodUpdateRequired": "Actualización necesaria",
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
  "settings.folderUsedParent":
    "Esa era la carpeta mods \u2014 se usó la carpeta superior: {{folder}}",
  "settings.setFolderFailed": "No se pudo definir la carpeta",
  "settings.reDetected": "Carpeta de {{game}} detectada de nuevo",
  "settings.detectFolderFailed": "No se pudo detectar la carpeta",
  "settings.pickInstallFolder":
    "Selecciona tu carpeta de instalación de {{game}} (contiene rider.pkz)",
  "settings.installSet": "Instalación del juego definida",
  "settings.installSetDesc":
    "La vista previa 3D del piloto ya puede cargar el modelo real del cuerpo.",
  "settings.setInstallFailed":
    "No se pudo definir la carpeta de instalación",
  "settings.installNotFound": "No se pudo encontrar {{game}}",
  "settings.installNotFoundDesc":
    "No se detectó ninguna instalación de Steam — define la carpeta manualmente.",
  "settings.installFound": "Instalación de {{game}} encontrada",
  "settings.detectInstallFailed":
    "No se pudo detectar la carpeta de instalación",
  "settings.wineRunnerDesc":
    "{{game}} es un juego de Windows, así que en un Mac se ejecuta dentro de una bottle de CrossOver, Whisky o Wine. Es lo que usa Jugar para arrancarlo.",
  "settings.wineRunnerNone": "No se encontró ningún runner de Wine",
  "settings.pickWineRunner": "Selecciona un binario de Wine (p. ej. el wine de CrossOver)",
  "settings.wineRunnerFailed": "No se pudo definir el runner de Wine",
  "settings.wineBottlesFound_one":
    "Se encontró {{count}} bottle donde buscar tu instalación.",
  "settings.wineBottlesFound_other":
    "Se encontraron {{count}} bottles donde buscar tu instalación.",
  "settings.wineBottlesNone":
    "No se encontraron bottles — instala primero {{game}} en CrossOver, Whisky o Wine.",
  "settings.pickProfilesFolder":
    "Selecciona tu carpeta de perfiles de {{game}}",
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
  "game.running": "{{game}} en ejecución",
  "game.launch": "Iniciar {{game}}",
  "game.alreadyRunning": "{{game}} ya está en ejecución",
  "game.launching": "Iniciando {{game}}…",
  "game.launchFailed": "No se pudo iniciar {{game}}",
  "join.title": "Unirse a un servidor",
  "join.desc":
    "Introduce la dirección de un servidor para iniciar {{game}} conectado directamente a él.",
  "join.address": "Dirección del servidor",
  "join.action": "Unirse",
  "join.joining": "Conectando…",
  "join.launching": "Conectando a {{address}}…",
  "join.alreadyRunning":
    "Cierra {{game}} primero — un juego ya iniciado no se puede enviar a un servidor.",
  "join.failed": "No se pudo unir a ese servidor",
  "join.manual": "Unirse a un servidor que no está en la lista",
  "join.noServers": "Todavía no hay servidores en la lista — escribe una dirección que te hayan dado.",

  "servers.title": "Servidores",
  "servers.subtitle":
    "Gestiona los servidores dedicados que tengas. Cada uno necesita el agente de MXB instalado.",
  "servers.empty": "Aún no hay servidores. Añade uno para gestionarlo desde aquí.",
  "servers.add": "Añadir un servidor",
  "servers.remove": "Quitar este servidor",
  "servers.namePlaceholder": "Nombre del servidor",
  "servers.tokenPlaceholder": "Token del agente",
  "servers.track": "Pista",
  "servers.slots": "Plazas",
  "servers.uptime": "Tiempo activo",
  "servers.restarts": "Reinicios",
  "servers.stopped": "Parado",
  "servers.start": "Iniciar",
  "servers.stop": "Parar",
  "servers.restart": "Reiniciar",
  "servers.setTrack": "Cambiar pista",
  "servers.trackPlaceholder": "ID de la pista",
  "servers.actionDone": "Hecho",
  "servers.actionFailed": "No funcionó",
  "servers.trackChanged": "Pista cambiada a {{track}} — el servidor se reinició.",
  "servers.saveFailed": "No se pudo guardar tu lista de servidores",
  "servers.trackLoading": "Leyendo las pistas…",
  "servers.trackEmpty": "No hay pistas en ese host",
  "servers.nameOptional": "Nombre del servidor (opcional — leído del host)",
  "servers.probing": "Comprobando ese agente…",
  "servers.probeFailed": "No se pudo contactar con ese agente",
  "servers.probed": "Se encontró {{name}}",
  "servers.pairingWhere":
    "Ejecuta mxb-agent en la máquina que aloja tu servidor. Imprime esta línea cada vez que arranca — cópiala entera.",
  "servers.manualEntry": "No tengo código de emparejamiento — introducir los datos a mano",
  "servers.publish": "Añadir a la lista de servidores",
  "servers.unpublish": "Quitar de la lista",
  "servers.listed": "En la lista pública de servidores — cualquiera puede encontrarlo y unirse.",
  "servers.notListed": "Todavía no está en la lista pública de servidores.",
  "servers.published": "Añadido — ahora otros jugadores pueden encontrarlo",
  "servers.publishedUnreachable":
    "Guardado, pero no pudimos alcanzarlo desde internet, así que aún no aparece en la lista. Comprueba que el agente esté en marcha y su puerto abierto.",
  "servers.publishFailed": "No se pudo cambiar la lista de servidores",
  "servers.unpublished": "Quitado de la lista de servidores",
  "servers.createTitle": "Crear un servidor",
  "servers.createDesc":
    "Lanza un servidor dedicado en la nube sin tener una máquina. Se apaga solo cuando nadie ha rodado en él durante un rato, así que no acumula gastos por la noche.",
  "servers.create": "Crear",
  "servers.creating": "Creándolo — tarda unos minutos en estar listo",
  "servers.createFailed": "No se pudo crear ese servidor",
  "servers.runningCount_one": "{{count}} activo",
  "servers.runningCount_other": "{{count}} activos",
  "servers.pairingPlaceholder": "Pega el código de emparejamiento",
  "servers.pairingHint":
    "El agente imprime esta línea al arrancar. Pégala aquí y la dirección y el token se rellenan solos — o introdúcelos a mano abajo.",

  "settings.experimental": "Experimental",
  "settings.experimentalServers": "Servidores y sincronización de pinturas",
  "settings.experimentalServersDesc":
    "Sin terminar. Añade la pestaña Servidores, te deja gestionar servidores dedicados y sincroniza las pinturas para que todos en un servidor se vean bien.",
  "settings.experimentalForced":
    "Activado en esta sesión por MXB_EXPERIMENTAL — el ajuste no hace nada hasta que lo quites.",
  "settings.betaBadge": "Beta",

  "sync.title": "Sincronización de pinturas",
  "sync.desc":
    "MX Bikes nunca envía las pinturas, así que los demás pilotos se ven con las de serie si no tienes ya su archivo exacto. Publica la tuya y descarga las de los demás.",
  "sync.enroll": "Registrarse",
  "sync.enrolled": "Registrado como {{name}}",
  "sync.enrollFailed": "No se pudo registrar",
  "sync.codePlaceholder": "Código de invitación",
  "sync.riderNamePlaceholder": "Nombre de piloto en el juego",
  "sync.riderNameHint":
    "Tiene que coincidir exactamente con tu nombre de piloto en MX Bikes — así saben las apps de los demás qué pinturas son tuyas.",
  "sync.ridingAs": "Publicando como {{name}}",
  "sync.pull": "Sincronizar pinturas",
  "sync.setGuid": "Guardar GUID",
  "sync.guidPlaceholder": "Tu GUID de MX Bikes",
  "sync.guidHint":
    "Tu GUID de MX Bikes (opcional). Te identifica aunque cambies de nombre de piloto, y el servidor lo registra cada vez que te conectas.",
  "sync.guidSaved": "GUID guardado",
  "sync.pulled": "Instaladas {{installed}} de {{riders}} pilotos ({{had}} ya estaban)",
  "sync.pullFailed": "No se pudieron sincronizar las pinturas",
  "sync.rejected": "Se omitieron {{count}} con un destino no seguro",
  "sync.pickProfile": "Corres como",
  "sync.pickProfileHint":
    "Tus perfiles de MX Bikes, tal como los encontró la app. Elegir uno es lo que indica a las apps de los demás jugadores qué pinturas son tuyas.",
  "sync.noProfiles":
    "No se encontraron perfiles de MX Bikes, así que escribe tu nombre de piloto exactamente como aparece en el juego.",
  "sync.guidClaimed": "Identificado por el GUID {{guid}}",
  "sync.guidPending":
    "Tu GUID se detecta solo la primera vez que uno de tus servidores te ve conectarte. Hasta entonces te identifica tu nombre de piloto.",
  "sync.guidManual": "Introducirlo manualmente",
  "sync.whereCode":
    "Por ahora el paint sync es solo por invitación. Los códigos se reparten en el Discord — pídelo allí y pega arriba el que te den.",
  "sync.getCode": "Pregunta en el Discord",
  "sync.sidebarOk": "Sincronizado · {{count}} pilotos",
  "sync.sidebarUnpublished": "Tu look no está publicado",
  "sync.agoJustNow": "ahora mismo",
  "sync.agoMinutes_one": "hace {{count}} minuto",
  "sync.agoMinutes_other": "hace {{count}} minutos",
  "sync.agoHours_one": "hace {{count}} hora",
  "sync.agoHours_other": "hace {{count}} horas",
  "sync.agoDays_one": "hace {{count}} día",
  "sync.agoDays_other": "hace {{count}} días",
  "sync.publishing": "Enviando tu look…",
  "sync.pulling": "Descargando las pinturas de los demás…",
  "sync.publishNow": "Publicar ahora",
  "sync.published": "Publicadas {{paints}} pinturas en {{bikes}} motos",
  "sync.publishFailed": "No se pudieron publicar tus pinturas",
  "sync.publishedState": "Tu look está publicado — {{bikes}} motos, {{paints}} pinturas",
  "sync.lastPublished": "Enviado {{ago}}. Se vuelve a enviar solo cada vez que cambias algo.",
  "sync.neverPublished": "Tu look aún no se ha publicado",
  "sync.neverPublishedWhy": "Hasta que lo esté, los demás en el servidor te ven con moto y equipación por defecto.",
  "sync.pulledState": "Tienes las pinturas de {{count}} pilotos",
  "sync.lastPulled": "Última comprobación {{ago}}. Se repite sola cuando pulsas Jugar.",
  "sync.neverPulled": "Aún no has descargado las pinturas de los demás",
  "sync.neverPulledWhy": "Hasta que lo hagas, los otros pilotos aparecen con motos por defecto aunque hayan publicado las suyas.",
  "sync.oversized_one": "{{count}} pintura es demasiado grande para compartirla, así que los demás pilotos no la verán.",
  "sync.oversized_other": "{{count}} pinturas son demasiado grandes para compartirlas, así que los demás pilotos no las verán.",
  "sync.skippedBikes_one": "{{count}} moto no se publicó — tienes más de las que podemos guardar.",
  "sync.skippedBikes_other": "{{count}} motos no se publicaron — tienes más de las que podemos guardar.",
  "sync.noMatchingProfile": "Este nombre no coincide con ningún perfil de MX Bikes en este PC, así que no hay nada que publicar. Revisa la carpeta de perfiles en Ajustes.",
  "sync.guidPendingTitle": "Identificado por tu nombre de piloto",
  "sync.keptYours_one": "{{count}} pintura se dejó intacta",
  "sync.keptYours_other": "{{count}} pinturas se dejaron intactas",
  "sync.keptYoursWhy": "Otro piloto usa el mismo nombre de archivo para una pintura distinta. La tuya se conservó — la app nunca sobrescribe una librea que no instaló. Verás a ese piloto con tu versión.",
  "servers.booting": "Arrancando…",
  "servers.bootingStage": "{{stage}}…",
  "servers.bootFailed": "Este servidor no pudo terminar de configurarse y se apagó. Esto es lo que informó:",
  "servers.bootingWhy": "Instalando el juego en la máquina nueva. Tarda unos minutos — descarga el instalador completo.",
  "servers.shutsDown": "Se apaga",
  "servers.inUse": "En uso",
  "servers.inMinutes_one": "en {{count}} min",
  "servers.inMinutes_other": "en {{count}} min",
  "servers.inList": "En la lista",
  "servers.destroy": "Apagar este servidor",
  "servers.destroyed": "Servidor apagado",
  "servers.runningOfCap": "{{count}} de {{cap}} activos",
  "servers.atCap": "Ya hay {{cap}} servidores activos, que es el límite. Apaga uno para arrancar otro.",
  "servers.help": "Comparte tus libreas con todos en un servidor y gestiona un servidor dedicado propio.",

  "sync.autoNote":
    "Tu look se publica solo — cada moto, cada vez que lo cambias en la app o en el garaje del juego. El de los demás llega cuando pulsas Jugar.",

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
    "Los presets leen tus perfiles de aquí — la ruta de abajo es donde está mirando la app ahora mismo. Es la carpeta {{profiles}} dentro de tu carpeta de {{game}}, o {{documents}} si moviste tu carpeta de mods. Defínela solo si la tuya está en otro sitio.",
  "settings.resetToDefault": "Restablecer",
  "settings.gameInstallDesc":
    "Carpeta de instalación del juego (opcional) — donde está instalado {{game}} (contiene {{file}}). Defínela para cargar el cuerpo real del piloto en la vista previa 3D.",
  "viewer.stockGearNote":
    "Mostrado sobre el {{part}} de serie del juego. Unos gráficos hechos para otro modelo pueden no encajar del todo.",
  "viewer.paintNoChange":
    "Ninguna de las texturas de estos gráficos la usan las piezas que se muestran aquí, así que la vista previa no cambia. Aun así puede pintar la cadena, que esta vista no representa.",
  "viewer.noPaintPreview": "Sin vista previa de los gráficos ({{err}})",

  // ── Biblioteca ─────────────────────────────────────────────────────────────
  "library.help":
    "Tus mods instalados. Revisa lo que tienes instalado y quita lo que ya no quieras.",
  "library.rootFolder": "(raíz)",
  "library.byAuthor": "de {{author}}",
  "library.locked": "Bloqueado — no se puede leer el contenido",
  "library.searchPlaceholder": "Buscar entre los instalados…",
  "library.sortFolder": "Por carpeta",
  "library.sortRecent": "Añadidas recientemente",
  "library.showRemoved": "Eliminados",
  "library.showRemovedHint":
    "Muestra los mods que tuvo esta carpeta, incluidos los borrados fuera de la app",
  "library.goneOn": "Eliminado el {{date}}",
  "library.goneNote": "guardados para que puedas encontrarlos otra vez",
  "library.parkedHint": "Desactivado en Gestionar — sigue en el disco",
  "library.parkedNote": "vuelve a activarlos en Gestionar",
  "library.nothingRemoved":
    "Aún no falta nada. A partir de ahora se recordará todo lo que borres.",
  "library.reinstall": "Descargar de nuevo",
  "library.copyName": "Copiar nombre",
  "library.copiedName": "Nombre copiado",
  "library.forget": "Olvidar esto",
  "library.forgetFailed": "No se pudo olvidar",
  "library.restore": "Restaurar",
  "library.restored": "Restaurado",
  "library.restoreFailed": "No se pudo restaurar",
  "library.findAgain": "Encontrarlo otra vez",
  "library.findAgainFor": "Buscando “{{name}}” en todas las fuentes.",
  "library.findAgainNone": "Nada con ese nombre.",
  "library.findAgainFailed": "No se pudo buscar aquí.",
  "library.scanning": "Analizando tu biblioteca…",
  "library.empty":
    "Aún no hay {{type}} instaladas — ve a Explorar y añade alguna.",
  "library.noMatches": "Sin resultados.",
  "library.quick3d": "Ver en 3D",
  "swapActions.menu": "Mover o eliminar este modelo",
  "swapActions.move": "Mover a otra moto…",
  "swapActions.delete": "Eliminar modelo…",
  "swapActions.activeFirst": "Es el modelo activo: cambia la moto a otro modelo primero",
  "swapActions.stockHasNoFiles": "Stock no es un set de modelo: no hay nada que mover ni eliminar",
  "swapActions.moveTitle": "Mover {{name}} a otra moto",
  "swapActions.moveBlurb": "Los archivos del modelo se mueven. La moto conserva todo lo demás.",
  "swapActions.pickBike": "Elige una moto…",
  "swapActions.liveriesTitle": "¿Llevar sus decoraciones?",
  "swapActions.liveriesBlurb": "Una decoración se dibuja para el layout de una moto, así que rara vez encaja en otra. Lo que dejes se queda en esta moto.",
  "swapActions.moveConfirm": "Mover",
  "swapActions.moved": "{{name}} movido a {{bike}}",
  "swapActions.deleteTitle": "¿Eliminar {{name}}?",
  "swapActions.deleteBlurb_one": "Su {{count}} archivo va a la Papelera. Las decoraciones se quedan en la moto.",
  "swapActions.deleteBlurb_other": "Sus {{count}} archivos van a la Papelera. Las decoraciones se quedan en la moto.",
  "swapActions.deleteConfirm": "Eliminar",
  "swapActions.deleted": "{{name}} enviado a la Papelera",
  "library.models_one": "{{count}} modelo",
  "library.models_other": "{{count}} modelos",
  "library.modelsHint": "Model swaps instalados para esta moto: cámbialos en el Locker",
  "library.modelIncomplete": "Incompleto",
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

  // ── Compartir archivos instalados (cualquier pista o pintura) ──────────────
  "share.share": "Compartir",
  "share.action": "Compartir…",
  "share.title": "Comparte estos archivos",
  "share.hint":
    "Los empaqueta, los sube y te da un único código para pegar donde quieras. Quien lo pegue recibe los archivos en las mismas carpetas.",
  "share.hintDone": "Envía este código: instala todo lo que aparece arriba.",
  "share.nothingToShare":
    "Aquí no hay nada que compartir: en un código solo caben archivos de tu carpeta mods.",
  "share.skipped_one": "1 elemento excluido ({{reason}}).",
  "share.skipped_other": "{{count}} elementos excluidos ({{reason}}).",
  "share.createCode_one": "Compartir 1 archivo ({{size}})",
  "share.createCode_other": "Compartir {{count}} archivos ({{size}})",
  "share.copyCode": "Copiar código",
  "share.copied": "Código de compartir copiado.",
  "share.uploaded": "Subido: copia el código de abajo.",
  "share.uploadedCopied": "Subido: el código está en el portapapeles.",
  "share.importAction": "Pegar un código…",
  "share.importTitle": "Importar archivos compartidos",
  "share.importBody":
    "Pega el código que te han enviado. Los archivos se instalan donde los tenía quien los compartió.",
  "share.downloadNotice": "Descarga {{size}} desde {{host}}.",
  "share.install": "Descargar e instalar",
  "share.installed_one": "1 archivo instalado.",
  "share.installed_other": "{{count}} archivos instalados.",
  "share.phasePacking": "Empaquetando archivos…",
  "share.phaseUploading": "Subiendo…",
  "share.phaseDownloading": "Descargando…",
  "share.phaseInstalling": "Instalando…",

  // ── Taquilla ───────────────────────────────────────────────────────────────
  "locker.help":
    "Cambia el modelo y el sonido del motor de cada moto entre los sets que tengas instalados.",
  "locker.rescan": "Volver a analizar",
  "locker.restore": "Restaurar",
  "locker.hideOrphan": "Ocultar este aviso",
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
  "locker.stockModel": "Predeterminado del juego",
  "locker.activeModel": "Modelo activo",
  "locker.activeSound": "Sonido activo",
  "locker.switchToNoModel":
    "Cambiar a sin modelo — quita los archivos del modelo actual",
  "locker.switchToStockModel":
    "Quita el modelo actual para que tome el relevo el del juego — se archiva, no se elimina",
  "locker.switchToStock":
    "Cambiar a Stock — quita el mod de sonido (suena el original)",
  "locker.missingModelEdf": "Este set no tiene model.edf",
  "locker.missingSoundFiles": "A este set le falta engine.scl o sfx.cfg",
  "locker.switchTo": "Cambiar a {{name}}",
  "locker.preview3d": "Ver {{name}} en 3D — no se cambia nada",
  "locker.view3d": "Ver 3D",
  "locker.paints": "Pinturas",
  "locker.assignPaints": "Elige qu\u00e9 decoraciones pertenecen a {{name}}",
  "locker.paintsClaimed_one": "{{count}} decoraci\u00f3n asignada a este modelo",
  "locker.paintsClaimed_other": "{{count}} decoraciones asignadas a este modelo",
  "locker.paintsTitle": "Decoraciones de \u201c{{model}}\u201d",
  "locker.paintsBlurb":
    "Marca las decoraciones dise\u00f1adas para este modelo. Ser\u00e1n las \u00fanicas disponibles mientras est\u00e9 activo, y las que pertenecen a otro modelo se sacan de la carpeta paints de la moto, as\u00ed que {{game}} tampoco las lista. Una decoraci\u00f3n sin marcar en ning\u00fan modelo sigue disponible con todos.",
  "locker.paintsFilter": "Buscar decoraciones\u2026",
  "locker.paintsSelectAll": "Seleccionar todo",
  "locker.paintsClearAll": "Borrar todo",
  "locker.paintsLoading": "Leyendo decoraciones\u2026",
  "locker.paintsNone": "Esta moto todav\u00eda no tiene decoraciones \u2014 instala una y aparecer\u00e1 aqu\u00ed.",
  "locker.paintsNoMatch": "Ninguna decoraci\u00f3n coincide.",
  "locker.paintsAlsoOn": "Tambi\u00e9n asignada a {{models}}",
  "locker.paintsSaved_one": "{{count}} decoraci\u00f3n asignada a \u201c{{model}}\u201d.",
  "locker.paintsSaved_other": "{{count}} decoraciones asignadas a \u201c{{model}}\u201d.",
  "locker.paintsStuck_one":
    "No se pudo mover {{count}} archivo de decoraci\u00f3n \u2014 cierra {{game}} y vuelve a escanear, o seguir\u00e1 visible en el juego.",
  "locker.paintsStuck_other":
    "No se pudieron mover {{count}} archivos de decoraci\u00f3n \u2014 cierra {{game}} y vuelve a escanear, o seguir\u00e1n visibles en el juego.",
  "locker.paintsReselect": "Vuelve a seleccionar tu perfil en {{game}} para ver la nueva lista.",
  "locker.paintsNextLaunch": "El juego mostrar\u00e1 la nueva lista la pr\u00f3xima vez que se abra.",
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
  "locker.modelFrostmodNotRunning":
    "FrostMod no está en ejecución — en MX Bikes cambia de categoría de moto y vuelve para ver el modelo.",
  "locker.modelSwitchCategory":
    "Modelo cambiado — en MX Bikes cambia de categoría de moto y vuelve para verlo.",
  "locker.modelFrostmodUnreachable":
    "No se pudo contactar con FrostMod — en el juego cambia de categoría de moto y vuelve para cargar el modelo.",
  "locker.modelRefreshWindowsOnly":
    "Modelo cambiado — en el juego cambia de categoría de moto y vuelve para verlo.",
  "locker.modelInstantRefreshOff":
    "En MX Bikes cambia de categoría de moto y vuelve para cargar el modelo.",

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
  "install.cancelled": "{{title}} cancelado",

  "downloads.title": "Descargas",
  "downloads.open": "Mostrar la cola de descargas",
  "downloads.preparing": "Preparando…",
  "downloads.waiting": "En espera",
  "downloads.cancel": "Cancelar esta descarga",
  "downloads.remove": "Quitar de la cola",
  "downloads.cancelling": "Cancelando…",
  "downloads.stageResolving": "Buscando el archivo…",
  "downloads.stageDownloading": "Descargando",
  "downloads.stageExtracting": "Extrayendo",
  "downloads.stagePlacing": "Instalando",
  "downloads.stageReview": "Esperándote",

  // ── Descargas (historial) ──────────────────────────────────────────────────
  "downloads.help":
    "Todo lo que has descargado, lo más reciente primero — incluidas las que fallaron. Filtra por estado o busca una mod cuyo nombre no recuerdes del todo.",
  "downloads.filterAll": "Todas",
  "downloads.filterFailed": "Fallidas",
  "downloads.searchPlaceholder": "Buscar descargas…",
  "downloads.clearAction": "Vaciar",
  "downloads.clearTitle": "¿Vaciar el historial de descargas?",
  "downloads.clearBody":
    "Esto solo olvida la lista. No se elimina nada de lo que has instalado.",
  "downloads.empty": "Aún no has descargado nada — ve a Explorar y añade algo.",
  "downloads.noMatches": "Sin resultados.",
  "downloads.today": "Hoy",
  "downloads.yesterday": "Ayer",
  "downloads.sourceSite": "Descarga",
  "downloads.sourceShop": "Tienda",
  "downloads.sourceHub": "MXB Hub",
  "downloads.sourceFile": "Archivo importado",
  "downloads.showInLibrary": "Ver en la biblioteca",
  "downloads.openModPage": "Abrir la página del mod",
  "downloads.forget": "Quitar de la lista",
  "downloads.rowActions": "Más",
  "downloads.failedBadge_one": "{{count}} descarga fallida",
  "downloads.failedBadge_other": "{{count}} descargas fallidas",

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
  "section.removed": "Ya no instalados",
  "section.parked": "Apartados por Gestionar",
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
  "dest.riderModelsNew": "Modelos de piloto (modelo nuevo)",
  "dest.animationsNewStyle": "Estilos de pilotaje (animación nueva)",
  "dest.helmetPaintsFor": "{{name}} · gráficos de casco",
  "dest.gogglesFor": "{{name}} · gafas",
  "dest.bootPaintsFor": "{{name}} · gráficos de botas",
  "dest.protectionPaintsFor": "{{name}} · gráficos de protecciones",
  "dest.outfitFor": "{{name}} · equipación / kit",
  "dest.suitPaintsFor": "{{name}} · gráficos de mono",
  "dest.glovesFor": "{{name}} · guantes",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay en el juego",
  "overlay.enable": "Activar el overlay en el juego",
  "overlay.enableDesc": "Pulsa un atajo mientras {{game}} está abierto para mostrar Presets, Locker y Browse sobre el juego — sin alt-tab. Los presets y los cambios de modelo se aplican al juego en marcha.",
  "overlay.shortcut": "Atajo del overlay",
  "overlay.shortcutDesc": "Funciona aunque el juego tenga el foco. Esc cierra el overlay y devuelve el control.",
  "overlay.borderlessTitle": "Juega a {{game}} sin bordes o en ventana",
  "overlay.borderlessNote": "Nada se puede dibujar sobre un juego que retiene la pantalla en modo exclusivo — el overlay incluido. Pon {{game}} en Borderless (o Windowed) desde Options → Video y aparecerá sobre el juego como esperas.",
  "overlay.gameRunning": "{{game}} está abierto",
  "overlay.gameNotRunning": "{{game}} no está abierto",
  "overlay.showNow": "Mostrar el overlay ahora",
  "overlay.showFailed": "No se pudo abrir el overlay",
  "overlay.hotkeyTaken": "Otra aplicación está usando este atajo",
  "overlay.hotkeyTakenDesc": "La combinación se la queda la primera aplicación que la pide, así que el overlay nunca se abre. Elige otra arriba — el silenciar de Discord suele ser el culpable.",
  "overlay.fullscreenNow": "{{game}} está ahora en pantalla completa exclusiva",
  "overlay.fullscreenNowDesc": "El overlay sí se abre — es el juego el que se dibuja encima. Cambia a sin bordes o en ventana desde Options → Video.",
  "overlay.notWorking": "¿Lo pulsaste y no pasó nada?",
  "overlay.notWorkingDesc": "Revisa el atajo de arriba: puede que otra aplicación ya tenga esa combinación, y elegir una libre es lo que lo arregla.",
  // Voice chat — devices and levels.
  "voice.section": "Chat de voz",
  "voice.enable": "Activar el chat de voz",
  "voice.microphone": "Micrófono",
  "voice.output": "Salida",
  "voice.systemDefault": "Predeterminado del sistema",
  "voice.testMic": "Probar micro",
  "voice.stopTest": "Parar",
  "voice.speakNow": "Di algo — la barra debería moverse.",
  "voice.testOutput": "Reproducir tono de prueba",
  "voice.testOutputDesc": "Comprueba que oirás a los demás en los auriculares correctos.",
  "voice.micGain": "Ganancia del micrófono",
  "voice.volume": "Volumen",
  "voice.micMode": "Modo de la tecla",
  "voice.modePush": "Mantener",
  "voice.modeToggle": "Alternar",
  "voice.micKey": "Tecla del micro",
  "voice.micOpen": "Micro abierto",
  "voice.toggleDesc": "Pulsa una vez para abrir el micro y otra vez para cerrarlo. Nada lo cierra solo — vigila el indicador.",
  "voice.ptt": "Pulsar para hablar",
  "voice.pttDesc": "Mantén la tecla para hablar y suéltala para parar. Funciona mientras el juego tiene el foco.",
  "voice.pttUpdated": "Tecla de pulsar para hablar actualizada",
  "voice.micFailed": "No se pudo abrir el micrófono",
  "voice.outputFailed": "No se pudo reproducir el tono de prueba",
  "voice.registerFailed": "Ajustes de voz guardados, pero la tecla de pulsar para hablar no se registró",
  "voice.deviceGone": "Ese dispositivo no está conectado",
  "voice.noDevices": "No se encontraron dispositivos de audio",
  "voice.notConnected": "Aún no conectado con nadie",
  "voice.notConnectedDesc": "La voz se activa sola al entrar en un servidor: no hay nada que configurar, nada que descargar y nada que el servidor tenga que ejecutar. Cualquiera que esté ahí con la app aparece aquí.",
  "voice.inRoom": "En voz en {{server}}",
  "voice.stopped": "Voz detenida",
  "voice.unnamedRider": "Piloto",
  "voice.connecting": "conectando…",
  "voice.mute": "Silenciar",
  "voice.unmute": "Reactivar",

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
  "overlay.needsSetup": "Termina de configurar MXB App en su ventana principal — necesita saber dónde está tu carpeta de {{game}}.",
  "overlay.fullscreenBlocked": "El overlay no puede mostrarse sobre la pantalla completa exclusiva",
  "overlay.fullscreenBlockedDesc": "Pon {{game}} en modo sin bordes o en ventana desde Options → Video y vuelve a pulsar el atajo.",

  // Presentación de la versión — la ventana de novedades que aparece una vez tras actualizar.
  "showcase.eyebrow": "Recién actualizado",
  "showcase.title": "Novedades de la {{version}}",
  "showcase.subtitle": "Primero lo grande. Todo lo demás de esta versión está en las notas.",
  "showcase.whileGameRunning": "mientras MX Bikes está abierto",
  "showcase.releaseNotes": "Leer las notas de la versión",
  "showcase.gotIt": "Entendido",
  "showcase.supporters.title_one": "Posible gracias a {{count}} mecenas",
  "showcase.supporters.title_other": "Posible gracias a {{count}} mecenas",
  "showcase.supporters.more": "+{{count}} más",
  "showcase.v0122.hero.title":
    "La cámara de repetición puede seguir al piloto",
  "showcase.v0122.hero.body":
    "Apunta una clave a un piloto y la cámara de repetición de FrostMod lo enfoca fotograma a fotograma, en vez de los ángulos que volaste. Corta entre planos, mantente en uno y mira todo el recorrido en el mundo mientras lo construyes.",
  "showcase.v0122.aim":
    "Pulsa T sobre una clave para apuntarla a un piloto. La cámara lo sigue, y con dos pilotos en un mismo tramo pasa suavemente de uno a otro.",
  "showcase.v0122.shots":
    "E decide qué hace una clave: pasar por ella, detenerse en ella o cortar en seco al siguiente plano. Un recorrido puede ser un montaje entero.",
  "showcase.v0122.preview":
    "V dibuja el recorrido en el mundo mientras lo haces, y U deshace el último cambio — hasta veinticuatro atrás.",
  "showcase.v0122.paths":
    "Ajustes → FrostMod lista cada recorrido guardado: dónde caen sus claves, cómo vuela y cómo pasárselo a alguien.",
  "showcase.v0121.hero.title":
    "Tus teclas de la cámara de repetición, tu teclado",
  "showcase.v0121.hero.body":
    "La cámara de repetición de FrostMod lee el teclado directamente, y el juego hace lo mismo en el mismo instante. Si S mueve tu cámara hacia atrás, pulsar S para guardar un trayecto también movía la cámara. Ahora cada tecla del editor puede ir a donde el juego no llega.",
  "showcase.v0121.rebind":
    "Reasigna cualquier tecla del editor en Ajustes → FrostMod: pulsa una acción y luego la tecla.",
  "showcase.v0121.clash":
    "Elige una tecla que tus controles de cámara no usen. Las teclas de función y el teclado numérico suelen estar libres; un modificador no sirve, porque el juego sigue viendo la tecla de debajo.",
  "showcase.v0121.clean":
    "F7 oculta todo lo que dibuja FrostMod — panel, radar, contornos — para grabar limpio.",
  "showcase.v0113.hero.title":
    "Cada pista se abre como el lugar que es",
  "showcase.v0113.hero.body":
    "Antes una pista se abr\u00eda como terreno desnudo: los surcos, peraltes y caras de salto correctos, pero nada encima, as\u00ed que un supercross y un circuito nacional se ve\u00edan casi igual. El visor ahora dibuja lo que la pista pone sobre s\u00ed misma: las carpas y toldos, las pacas de paja y muros de neum\u00e1ticos, las l\u00edneas de banderolas y el vallado, los remolques del paddock y el paisaje m\u00e1s all\u00e1 de su propio cuadrado.",
  "showcase.v0113.sky":
    "Una pista se sit\u00faa bajo su propio cielo, iluminada y con la bruma que indica su propio archivo de ambiente.",
  "showcase.v0113.ground":
    "El suelo conserva su detalle de cerca: la tierra de la propia pista, con relieve real cuando incluye un mapa de normales para ella.",
  "showcase.v0113.pick":
    "Haz clic en cualquier cosa que est\u00e9 sobre la pista y se ilumina por s\u00ed sola, con su nombre y su tama\u00f1o.",
  "showcase.v0113.speed":
    "Llega por etapas \u2014 primero el terreno, luego la forma de los objetos, luego sus colores \u2014 y volver a abrir la misma pista se salta el archivo por completo.",
  "showcase.v0112.hero.title":
    "Un pack de motos se instala como las motos que lleva dentro",
  "showcase.v0112.hero.body":
    "El pack OEM son 54 m\u00e1quinas en un archivo de 3,8 GB, y hasta ahora llegaba como una sola fila que dec\u00eda \u00abCarpeta de mods\u00bb: todo o nada. Ahora aparece tal como est\u00e1 hecho: cada moto con su nombre y su clase reales, con su casilla. Coge las cuatro con las que corres y deja las otras cincuenta.",
  "showcase.v0112.fullscreen":
    "La vista 3D del Dise\u00f1ador se abre a pantalla completa: un bot\u00f3n llena la ventana con el modelo que est\u00e1s pintando.",
  "showcase.v0112.review":
    "Una descarga que resulta contener varios mods se detiene y te ense\u00f1a qu\u00e9 lleva dentro antes de que nada llegue a tu carpeta de mods.",
  "showcase.v0112.paint":
    "Un paint que ofrece un archivo por m\u00e1quina instala ahora el de la moto que elegiste, no el primero de la p\u00e1gina.",
  "showcase.v0112.speed":
    "La hoja de revisi\u00f3n se abre en aproximadamente un segundo con las motos OEM instaladas, cuando antes tardaba casi veinte.",
  "showcase.v0112.window":
    "Una ventana en negro al arrancar ya no puede atraparte: no aparece hasta haber dibujado algo y siempre tiene una forma de cerrarse.",
  "showcase.v0111.hero.title":
    "Los cambios de modelo protegidos se abren en 3D",
  "showcase.v0111.hero.body":
    "Un modelo comprado a un creador trae su malla sellada y el visor no podía leerla: al pulsar Ver en 3D decía que el modelo no tenía ninguna malla legible, aunque funciona perfectamente en el juego. Ahora se abre como cualquier otra moto.",
  "showcase.v0111.messages":
    "Si una moto sigue sin abrirse, la app dice qué ha fallado en realidad en vez de culpar siempre a la sincronización en la nube.",
  "showcase.v0110.hero.title":
    "Agarra al piloto y colócalo",
  "showcase.v0110.hero.body":
    "Agarra las articulaciones del piloto en la vista 3D y muévelo: manos, codos, caderas, pies. Los movimientos rápidos se acumulan, los deslizadores afinan y Posición de pilotaje lo sienta en la moto. Solo vista previa: no se toca el juego.",
  "showcase.v0110.designer":
    "Refleja una capa a través de la moto, selecciona varias a la vez, ajusta al arrastrar, voltea y escribe posiciones exactas.",
  "showcase.v0110.wheels":
    "Las motos se muestran con sus ruedas, y tú eliges sobre qué neumáticos se apoyan.",
  "showcase.v0110.speed":
    "Los circuitos se dibujan siete veces más rápido, las motos abren en 127 ms en vez de 201, y los mods se instalan de dos en dos.",
  "showcase.v0110.swaps":
    "Mueve un set de modelo a otra moto o bórralo, y mira cualquier swap en 3D desde la Biblioteca.",
  "showcase.v0102.hero.title":
    "Decoraciones que pertenecen al modelo que las lleva",
  "showcase.v0102.hero.body":
    "MX Bikes da a cada moto una sola carpeta paints y no sabe nada de los cambios de modelo, así que una malla Yami sobre una KTM ofrecía también todas las decoraciones de KTM. Cada modelo del Locker tiene ahora un botón de paleta: marca las decoraciones dibujadas para él y serán las únicas que ofrezca, también en el selector de pinturas del propio MX Bikes.",
  "showcase.v0102.packs":
    "Las decoraciones que venían dentro de un pack de modelo estaban instaladas pero invisibles. Al abrir el selector de ese modelo pasan a ser suyas, que es justo lo que las hace funcionar.",
  "showcase.v0102.presets":
    "El desplegable de decoraciones de Presets solo ofrece las que encajan con el modelo que elige el preset.",
  "showcase.v0102.vcredist":
    "En un Windows recién restaurado la app se cerraba nada más abrirla, sin ventana y sin registro. El instalador ahora coloca el runtime de Visual C++ de Microsoft antes de escribir la app.",
  "showcase.v0102.msvcr90":
    "Un msvcr90.dll suelto que la app no borra por su cuenta ya no es un fallo silencioso: nombra el archivo y ofrece desactivarlo con una pulsación.",
  "showcase.v0102.paintsync":
    "La sincronización de pinturas enviaba la decoración de la moto equivocada cuando dos motos compartían nombre de pintura, y las pinturas de casco, gafas, botas y protecciones no se compartían nunca.",
  "showcase.v0101.hero.title":
    "Tu biblioteca recuerda lo que borraste",
  "showcase.v0101.hero.body":
    "Antes, borrar un circuito lo borraba de la memoria de la app. Ahora guarda el nombre, el autor, dónde estaba y una imagen — así el que no sabes nombrar meses después sigue ahí para encontrarlo.",
  "showcase.v0101.restore":
    "Restaurar devuelve a su sitio un mod que borró la app, y “Encontrarlo otra vez” busca en mxb-mods y en la tienda con el nombre guardado.",
  "showcase.v0101.paints":
    "Una pintura guardada en disco ya aparece en el juego en marcha: sin alt-tab y sin volver a elegir tu perfil.",
  "showcase.v0101.r6034":
    "Arreglado un fallo que causaba esta app: la copia de msvcr90.dll que dejaba mataba MX Bikes con R6034. Ahora la retira.",
  "showcase.v0101.logs":
    "Compartir registros empaqueta lo mismo que Guardar registros y te da un enlace, en vez de un archivo que subir.",
  "showcase.v0101.bikes":
    "Las motos que ya no usas se pueden quitar del selector de Ajustes rápidos.",
  "showcase.v0100.hero.title": "El Designer prepara sus propias hojas",
  "showcase.v0100.hero.body":
    "Ahora crea las hojas que pide un modelo, coloca debajo los plásticos de la propia moto para calcar y abre un modelo en cerca de un segundo en vez de casi veinte.",
  "showcase.v0100.location":
    "Pasa por encima de la hoja y te dice qué hay bajo el cursor: la pieza, el lado de la moto en el que está, y si es una cara que vas a ver o una parte de abajo que no.",
  "showcase.v0100.downloads":
    "La página de Descargas lista lo que te has bajado: por día, lo más nuevo arriba, con dónde acabó cada archivo y de qué mirror vino.",
  "showcase.v0100.terrain":
    "Un circuito se abre ahora en 3D directamente desde la biblioteca, con sus saltos y roderas dibujados a partir del propio mapa de alturas del juego.",
  "showcase.v0100.sharing":
    "Ahora cualquier cosa de tu Biblioteca puede convertirse en un código que le pasas a alguien, y vuelve a las mismas carpetas en las que lo tienes.",
  "showcase.v0100.linux":
    "En Linux, FrostMod ahora corre en el mismo prefix de Proton bajo el que ya corre el juego.",
  "showcase.v092.hero.title": "Mira el terreno de un circuito en 3D",
  "showcase.v092.hero.body":
    "Los circuitos eran lo único que la biblioteca no sabía enseñarte: un nombre, una imagen y un tamaño. El visor ahora lee el mapa de alturas de un circuito y dibuja el propio terreno, así que los saltos, las roderas y la forma de una curva están ahí para mirarlos antes incluso de cargarlo. Se abre desde un circuito en la biblioteca, junto a Ver en 3D.",
  "showcase.v092.surfaces":
    "Un circuito se dibuja con sus propias superficies. Donde el circuito dice cuál es cuál, la hierba, el arcén, el firme duro y la tierra de la trazada toman cada uno el color del material que nombra — así un campo de labranza sale como la tierra que es y un circuito de hierba sale verde.",
  "showcase.v092.relief":
    "El terreno se ilumina con sus propios huecos y proyecta sombras de verdad, así que una rodera se lee como rodera y una rampa como rampa, vaya en la dirección que vaya.",
  "showcase.v092.accuracy":
    "Los circuitos se dibujan como los guarda el juego: en el sentido correcto en vez de reflejados, sin el muro de once metros alrededor de los que quedan por debajo de su cota, y con unas cuatro veces más detalle sobre el terreno.",
  "showcase.v092.voice":
    "Ajustes de chat de voz: elige el micrófono por el que se te oye y los auriculares por los que sale todo el mundo, con medidor de entrada en vivo y tono de prueba. Todavía no se transmite nada — esta es la mitad de los dispositivos, y la página lo dice.",
  "showcase.v092.pushToTalk":
    "Una tecla de pulsar para hablar que funciona mientras el juego tiene el foco, asignada por la misma vía que el atajo del overlay.",
  "showcase.v091.hero.title": "Pinta directamente sobre la plantilla",
  "showcase.v091.hero.body":
    "El Designer sabía colocar imágenes y texto sobre las hojas de una pintura, pero no dejaba poner ni un píxel a mano — un degradado en un lateral obligaba a salir a un editor de imágenes y volver. Ahora tiene su caja de herramientas: pincel suave con tamaño, borde e intensidad, borrador, degradado, relleno, y rectángulo, elipse y línea. Todo aparece en la hoja y en el modelo 3D a la vez, mientras arrastras.",
  "showcase.v091.gradient":
    "Un degradado que lleva un color hasta otro. Arrastra para decir dónde ocurre la transición: antes está el primer color, después el segundo. Lineal o radial, y puede desvanecerse hasta nada en lugar de hasta un color.",
  "showcase.v091.paintLayer":
    "Lo que pintas va en su propia capa, así que tiene opacidad, fusión y orden como todo lo demás — y la plantilla de debajo no se toca nunca. Oculta la capa y tienes la plantilla limpia otra vez. ⌘Z deshace trazos.",
  "showcase.v091.ghost":
    "Dibuja sobre un fantasma de la moto. Una hoja puede mostrar tenue por debajo la pintura de la que partiste para calcarla — sacada de la hoja, así que no se guarda en la tuya — y un mapa UV del carenado del modelo, cada pieza con su color, para ver sobre qué panel estás pintando.",
  "showcase.v091.parts":
    "Pon una foto en un solo panel. Elige una pieza del carenado y la capa se ajusta a ella y se recorta a su contorno, así una imagen de internet cubre el spoiler y se detiene en la junta. Al pasar por la hoja se muestra el nombre de la pieza.",
  "showcase.v091.resize":
    "Las capas se redimensionan arrastrando sus esquinas, no solo con el deslizador.",
  "showcase.v091.macos":
    "Jugar y Unirse a un servidor funcionan en macOS, a través de la botella CrossOver, Whisky o Wine que contenga el juego — y la app encuentra sola una instalación embotellada en vez de pedirte la ruta.",
  "showcase.v091.steamos":
    "En SteamOS la app de Linux abre en su interfaz en lugar de una pantalla en blanco.",
  "showcase.v090.hero.title": "Convierte tus imágenes en un diseño que el juego carga",
  "showcase.v090.hero.body":
    "Una nueva pestaña Diseños crea diseños a partir de imágenes corrientes — TGA, PNG, JPG — y los instala donde el juego los busca: una decoración de moto, un diseño de casco o de gafas, el kit o los guantes de tu piloto. Descomprime un diseño que ya tengas para conseguir una plantilla que encaje de verdad con el modelo, edítala en cualquier editor y devuélvela tal cual. El estudio comprueba tus nombres de archivo frente a los que usa la malla antes de guardar, y luego muestra el resultado sobre el modelo real.",
  "showcase.v090.reshade":
    "Explora, instala y cambia presets de ReShade desde la app — con una entrada Desactivado para comparar con el aspecto original, y un aviso cuando a un preset le faltan efectos.",
  "showcase.v090.bundles":
    "Comparte un preset como paquete completo y el código lleva los propios mods: decoración, casco y gafas, equipación, guantes, botas y neumáticos. Importación completa deja cada archivo donde el juego lo lee, así que alguien con la carpeta de mods vacía acaba llevando exactamente lo que creaste.",
  "showcase.v090.purchases":
    "Mis compras entra en tu cuenta de mxbikes-shop.com e instala lo que ya has pagado, con la misma hoja de revisión que usa el arrastrar y soltar.",
  "showcase.v090.ridingStyles":
    "Los presets pueden usar un estilo de pilotaje que hayas instalado, no solo los dos del juego — y un preset compartido se lo lleva consigo.",
  "showcase.v090.frostmod":
    "Cuando FrostMod muere por una biblioteca de Windows que falta, la app la nombra en lenguaje claro y la instala por ti. FrostMod también se puede parar desde la app, lo haya arrancado quien lo haya arrancado.",
  "showcase.v090.updates":
    "Instalar sobre una copia en ejecución ya no se detiene en «error al abrir el archivo para escritura», y abrir la app dos veces recupera la ventana que tenías en lugar de crear una segunda copia.",
  "showcase.v080.hero.title": "MXB App también maneja GP Bikes",
  "showcase.v080.hero.body":
    "Elige tu juego en el primer arranque, o cámbialo cuando quieras en Ajustes: toda la app lo sigue — Biblioteca, Gestionar, Presets, Jugar y una pestaña Explorar servida por gpb-mods.com. Las carpetas de piloto de GP se leen como las de GP, no como las de MX Bikes, y FrostMod también recarga en caliente allí. Cada juego guarda sus propias carpetas, así que tu configuración de MX Bikes queda intacta.",
  "showcase.v080.shop":
    "Una pestaña Tienda explora mxbikes-shop.com e instala lo que has comprado, sin salir de la app.",
  "showcase.v080.dropzone":
    "Arrastra lo que sea a la ventana. Deduce qué es cada archivo, muestra dónde va y qué reemplazaría, y te deja recolocar cualquier fila antes de instalar.",
  "showcase.v080.destinations":
    "Los mods aterrizan en la carpeta que el juego lee de verdad — una decoración en su moto, un gráfico de casco en su casco, un mono de GP en tu modelo de piloto.",
  "showcase.v080.protection":
    "La ranura de protecciones funciona: cada pieza dibujada derecha y entera, e instalada donde el juego la busca.",
  "showcase.v080.faster":
    "Las miniaturas se cachean y se dibujan al tamaño en que se muestran, así que Explorar y la Tienda abren mucho más rápido.",
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
  "setup.whichGame": "¿Qué juego vas a configurar? Puedes añadir el otro más tarde.",
  "setup.switchLater": "Puedes cambiar de juego cuando quieras en Ajustes.",
  "setup.chooseDifferentGame": "Elegir otro juego",
  // ── Dropzone ───────────────────────────────────────────────────────────────
  "drop.dropHere": "Suelta para instalar",
  "drop.dropHint": "Archivos, .pkz, gráficos, carpetas — cualquier cosa de {{game}}",
  "drop.scanning": "Viendo qué es esto…",
  "drop.found_one": "{{count}} elemento encontrado",
  "drop.found_other": "{{count}} elementos encontrados",
  "drop.selectAll": "Seleccionar todo",
  "drop.selectNone": "No seleccionar nada",
  "drop.reviewHint": "Revisa los destinos y luego instala.",
  "drop.install_one": "Instalar {{count}}",
  "drop.install_other": "Instalar {{count}}",
  "drop.fileCount_one": "{{count}} archivo",
  "drop.fileCount_other": "{{count}} archivos",
  "drop.replaces_one": "Reemplaza {{count}} archivo existente",
  "drop.replaces_other": "Reemplaza {{count}} archivos existentes",
  "drop.willReplace_one": "Se reemplazará {{count}} archivo existente",
  "drop.willReplace_other": "Se reemplazarán {{count}} archivos existentes",
  "drop.nothingOverwritten": "No se reemplazará nada de lo que ya tienes.",
  "drop.needChoice_one": "{{count}} elemento aún necesita un destino",
  "drop.needChoice_other": "{{count}} elementos aún necesitan un destino",
  "drop.skipped_one": "{{count}} archivo omitido",
  "drop.skipped_other": "{{count}} archivos omitidos",
  "drop.pickDestinationFirst": "Elige dónde va antes de instalar.",
  "drop.chooseDestination": "Elegir destino",
  "drop.searchDestinations": "Buscar motos y equipación…",
  "drop.noDestinations": "Todavía no hay nada instalado donde ponerlo.",
  "drop.destAsPackaged": "Tal como viene",
  "drop.include": "Incluir este elemento",
  "drop.exclude": "Dejar fuera este elemento",
  "drop.installed_one": "{{count}} elemento instalado",
  "drop.installed_other": "{{count}} elementos instalados",
  "drop.itemFailed": "No se pudo instalar {{name}}",
  "drop.installFailed": "Falló la instalación",
  "drop.scanFailed": "No se pudo leer lo que soltaste",
  "drop.previewFailed": "No se pudo comprobar ese destino",
  "drop.nothingUsable": "No hay nada instalable ahí",
  "drop.kind.modsTree": "Carpeta mods",
  "drop.kind.track": "Pista",
  "drop.kind.bike": "Moto",
  "drop.kind.bikePaint": "Gráficos",
  "drop.kind.soundSet": "Sonido",
  "drop.kind.riderGear": "Equipación",
  "drop.kind.tyres": "Neumáticos",
  "drop.kind.reshadePreset": "Ajuste de ReShade",
  "drop.kind.unknown": "Desconocido",
  "drop.reason.modsTree": "Contiene una carpeta mods completa",
  "drop.reason.categoryDirs": "Contiene carpetas de motos/pistas/piloto",
  "drop.reason.paintsBundle": "Contiene una carpeta paints",
  "drop.reason.soundMarkers": "Encontrados engine.scl y sfx.cfg",
  "drop.reason.trackMarkers": "Encontrados archivos de pista",
  "drop.reason.trackPackage": "Pista empaquetada",
  "drop.reason.bikeConfig": "Encontrada una configuración de moto",
  "drop.reason.loosePaint": "Gráficos sueltos — nada indica de qué modelo son",
  "drop.reason.gearFolders": "Encontradas carpetas de equipación",
  "drop.reason.riderTexture": "Pinta el cuerpo del piloto — una equipación",
  "drop.reason.gearTexture": "Pinta una pieza de equipación",
  "drop.reason.reshadePreset": "Enumera técnicas de ReShade",
  "drop.reason.packLayout": "El paquete lo coloca aquí",
  "drop.reason.unrecognised": "No reconocido — tendrás que colocarlo tú",

  // ── Import (el mismo flujo que soltar, pero eligiendo) ─────────────────────
  "import.action": "Importar",
  "import.staging": "Leyendo…",
  "import.pickFiles": "Elegir archivos…",
  "import.pickFolder": "Elegir una carpeta…",
  "import.modFiles": "Mods y pinturas",
  "import.allFiles": "Todos los archivos",
  "import.pickFailed": "No se pudo abrir el selector de archivos",
  "import.readFailed": "No se pudo leer lo que elegiste",

  // ── ReShade ────────────────────────────────────────────────────────────────
  "settings.reshade": "ReShade",
  "settings.reshadeDesc": "Ajustes de posprocesado — cómo se ve {{game}} en pantalla.",

  // ── Registros ──────────────────────────────────────────────────────────────
  "settings.logs": "Registros",
  "logs.desc":
    "Los archivos que hay que enviar cuando algo falla. MXB App, FrostMod y {{game}} guardan los suyos por separado — abre la carpeta que necesites, guárdalos todos en un zip, o compártelos como un enlace para pegar en un informe.",
  "logs.appLogs": "MXB App",
  "logs.appLogsDesc": "Lo que registró la propia app",
  "logs.frostmodLogsDesc": "Lo que el cargador escribió en su propia carpeta",
  "logs.gameLogsDesc": "El registro del juego, junto a sus archivos",
  "logs.open": "Abrir carpeta",
  "logs.save": "Guardar registros…",
  "logs.saving": "Guardando…",
  "logs.refresh": "Actualizar",
  "logs.loading": "Buscando…",
  "logs.empty": "Aquí todavía no hay archivos de registro.",
  "logs.folderMissing":
    "Esa carpeta no está ahí — nada ha escrito aún un registro en ella.",
  "logs.summary_one": "{{count}} archivo · {{size}} · el más reciente {{when}}",
  "logs.summary_other": "{{count}} archivos · {{size}} · el más reciente {{when}}",
  "logs.saved": "Registros guardados",
  "logs.savedDesc_one": "{{count}} archivo de registro, {{size}}",
  "logs.savedDesc_other": "{{count}} archivos de registro, {{size}}",
  "logs.saveFailed": "No se pudieron guardar los registros",
  "logs.share": "Compartir registros",
  "logs.sharePacking": "Empaquetando…",
  "logs.sharing": "Subiendo…",
  "logs.shared": "Registros subidos",
  "logs.sharedCopied": "{{size}} — el enlace está en tu portapapeles.",
  "logs.sharedDesc": "{{size}} — el enlace está abajo.",
  "logs.sharedSummary_one": "{{count}} archivo de registro, {{size}} subidos.",
  "logs.sharedSummary_other": "{{count}} archivos de registro, {{size}} subidos.",
  "logs.shareFailed": "No se pudieron compartir los registros",
  "logs.copyLink": "Copiar enlace",
  "logs.linkCopiedShort": "Copiado",
  "logs.linkCopied": "Enlace copiado",
  "logs.shareWarning":
    "El zip queda en un alojamiento público — cualquiera con el enlace puede descargarlo, así que dáselo solo a quien te lo pidió.",
  "logs.privacy":
    "Los registros contienen rutas de carpetas y lo que estaba haciendo la app — nunca tus contraseñas ni las cookies de sesión, y no se incluye ningún archivo de ajustes.",

  // ── Mecenas (Buy Me a Coffee) ──────────────────────────────────────────────
  "settings.supporters": "Mecenas",
  "settings.supportersDesc": "Quienes mantienen MXB App en Buy Me a Coffee.",
  "supporters.intro":
    "MXB App es gratis, y así seguirá. Los cafés de abajo son los que pagan el tiempo que hay detrás: quienes los invitaron son la razón de que haya una versión nueva que instalar.",
  "supporters.count_one": "{{count}} mecenas",
  "supporters.count_other": "{{count}} mecenas",
  "supporters.untiered": "Mecenas",
  "supporters.since": "desde {{date}}",
  "supporters.loading": "Cargando la lista…",
  "supporters.refresh": "Actualizar",
  "supporters.become": "Invítame a un café",
  "supporters.empty": "Todavía no hay nadie en la lista",
  "supporters.emptyDesc":
    "La lista se actualiza sola: invita a un café y tu nombre aparecerá aquí sin esperar a una versión nueva.",
  "supporters.offline":
    "No se pudo consultar la lista ahora mismo — esta es la última que vimos.",
  "supporters.optOut":
    "Los nombres se muestran con permiso. Escribe por Discord o por Buy Me a Coffee y el tuyo se quita al momento.",

  "modType.reshade": "ReShade",
  "modType.reshadeInline": "ajustes de ReShade",
  "reshade.needsGameFolder":
    "ReShade está en tu carpeta de {{game}} — configúrala en Carpeta del juego, o apunta directamente aquí.",
  "reshade.folder": "Buscando en tu carpeta de {{game}}:",
  "reshade.customFolder": "Buscando en la carpeta que elegiste:",
  "reshade.browse": "Elegir carpeta…",
  "reshade.pickFolder": "Elige la carpeta donde está instalado ReShade",
  "reshade.folderMissing": "La carpeta que elegiste ya no está.",
  "reshade.resetFolder": "Volver a la carpeta de {{game}}",
  "reshade.folderSet": "ReShade encontrado",
  "reshade.notThere": "No hay ReShade en esa carpeta",
  "reshade.intro":
    "ReShade añade posprocesado a {{game}}. Es una herramienta gratuita aparte: instálala una vez y luego elige un ajuste aquí.",
  "reshade.wrongApi":
    "ReShade está instalado como {{dll}}, que {{game}} nunca carga — usa OpenGL. Vuelve a ejecutar el instalador de ReShade y elige OpenGL.",
  "reshade.step1": "Descarga el instalador desde reshade.me.",
  "reshade.step2": "Ejecútalo y elige {{exe}} en tu carpeta de {{game}}.",
  "reshade.step3": "Elige OpenGL cuando lo pregunte — no DirectX.",
  "reshade.getIt": "Obtener ReShade",
  "reshade.recheck": "Volver a comprobar",
  "reshade.installed": "Instalado",
  "reshade.installedVersion": "Instalado · {{version}}",
  "reshade.off": "Desactivado — sin efectos",
  "reshade.delete": "Eliminar ajuste",
  "reshade.deleted": "{{name}} eliminado",
  "reshade.applied": "{{name}} ya está activo",
  "reshade.appliedNextLaunch": "{{name}} está listo — se aplica al próximo inicio",
  "reshade.loosePreset": "En tu carpeta del juego — no lo instaló MXB App",
  "reshade.missingEffects_one": "Necesita {{list}}, que no está instalado",
  "reshade.missingEffects_other":
    "Necesita {{count}} efectos que no están instalados: {{list}}",
  "reshade.noShaders":
    "No hay efectos de ReShade instalados, así que los ajustes no cambiarán nada. Vuelve a ejecutar el instalador de ReShade y elige un paquete de shaders.",
  "reshade.noPresets":
    "Aún no hay ajustes — instala alguno desde Explorar, o suelta un .ini aquí.",
  "reshade.browseHint": "Más ajustes en Explorar → ReShade.",
  "reshade.nextLaunchHint":
    "{{game}} está en ejecución — el cambio se aplica al próximo inicio.",
  // ── Paint studio ───────────────────────────────────────────────────────────
  "paints.help":
    "Convierte archivos .tga o .png hechos en GIMP o Photoshop en un .pnt que el juego carga — y desempaqueta una pintura existente para partir de ella.",
  "paints.unpack": "Desempaquetar una pintura…",
  "paints.toDesigner": "Dibujar sobre estas…",
  "paints.unpacked": "Extraídas {{count}} texturas — edítalas y luego guarda.",
  "paints.whereTitle": "Dónde va",
  "paints.kind.bike": "Decoración de moto",
  "paints.kind.helmet": "Casco",
  "paints.kind.goggles": "Gafas",
  "paints.kind.boots": "Botas",
  "paints.kind.protection": "Protecciones",
  "paints.kind.kit": "Equipación",
  "paints.kind.gloves": "Guantes",
  "paints.model": "Para",
  "paints.profile": "Perfil de piloto",
  "paints.noModels": "Todavía no hay nada instalado que pintar.",
  "paints.destPath": "Se instala en mods/{{rel}}",
  "paints.saveElsewhere": "Guardar en una carpeta…",
  "paints.saveTitle": "Nombre y guardado",
  "paints.namePlaceholder": "Nombra esta pintura…",
  "paints.save": "Guardar pintura",
  "paints.saved": "Guardada en {{path}}",
  "paints.preview3d": "Ver en 3D",
  "paints.openFolder": "Abrir carpeta",
  "paints.sheetsTitle": "Texturas",
  "paints.reload": "Recargar del disco",
  "paints.addImages": "Añadir imágenes…",
  "paints.expected": "Hojas usadas aquí:",
  "paints.empty":
    "Añade un .tga o .png por cada textura. Importan los nombres, no los archivos: una textura llamada “livery” va a la pieza que pide “livery”. Desempaquetar una pintura existente te da los nombres correctos.",
  "paints.resized": "Redimensionada {{from}} → {{to}} — el juego necesita potencias de dos.",
  "paints.unknownName": "Ninguna pintura de aquí usa este nombre: puede que no aparezca en el modelo.",
  "paints.needSheets": "Añade al menos una imagen.",
  "paints.needName": "Nombra esta pintura.",
  "paints.needTextureNames": "Cada textura necesita un nombre.",
  "paints.duplicateName": "Dos texturas se llaman “{{name}}”.",
  "paints.needTarget": "Elige dónde va la pintura.",
  "paints.replaceTitle": "¿Reemplazar esta pintura?",
  "paints.replaceBody": "{{path}} ya existe. Al guardar se reemplaza.",
  "paints.replace": "Reemplazar",

  // ── Designer (el editor por capas) ────────────────────────────────────────────
  "designer.help":
    "Dibuja una pintura sobre las hojas que el juego lee de verdad y mírala en el modelo mientras trabajas. Elige un modelo y sus hojas ya están ahí, con los nombres que él usa — o empieza desde una pintura instalada o un archivo de Photoshop para traerte el trabajo. Pinta con pincel, degradado o formas, apila imágenes y texto encima y guarda: lo que sale es un .pnt que el juego carga, no una exportación que convertir.",
  "designer.empty":
    "Todavía no hay nada sobre lo que dibujar. Empieza desde una pintura instalada para este modelo, abre un archivo de Photoshop para conservar sus capas, o añade una hoja en blanco.",
  "designer.startFromPaint": "Empezar desde una pintura…",
  "designer.startFromPsd": "Empezar desde un PSD…",
  "designer.exportPsd": "Exportar PSD",
  "designer.exportPsdHint": "Guarda cada hoja como un .psd con capas, en una carpeta que elijas.",
  "designer.exportedPsd_one": "1 hoja guardada en {{dir}}",
  "designer.exportedPsd_other": "{{count}} hojas guardadas en {{dir}}",
  "designer.sheetsSwitched": "Cambiado a las hojas que usa {{dest}}.",
  "designer.switchSheetsTitle": "Estas hojas son de otro modelo",
  "designer.switchSheetsBody": "{{dest}} usa {{names}}. Cambiar sustituye todas las hojas abiertas.",
  "designer.switchSheets": "Cambiar hojas",
  "designer.blankSheet": "Hoja en blanco",
  "designer.addSheet": "Añadir una hoja",
  "designer.nothingToSave": "Todas las hojas están vacías: dibuja algo antes de guardar.",
  "designer.blankSheetsSkipped_one": "Se dejó fuera 1 hoja vacía: una hoja vacía borraría la textura del modelo.",
  "designer.blankSheetsSkipped_other": "Se dejaron fuera {{count}} hojas vacías: una hoja vacía borraría la textura del modelo.",
  "designer.createExpected_one": "Crear 1 hoja",
  "designer.createExpected_other": "Crear {{count}} hojas",
  "designer.sheets": "Hojas",
  "designer.moveDown": "Bajar",
  "designer.moveUp": "Subir",
  "designer.noSheetsFound":
    "Esa pintura no produjo ninguna hoja, así que no hay nada sobre lo que dibujar.",
  "designer.loadedSheets": "Se cargaron {{count}} hoja(s) — dibuja encima y guarda.",
  "designer.sheetName": "Nombre de textura",
  "designer.editSheet": "Editar esta hoja",
  "designer.addImage": "Añadir imagen",
  "designer.addText": "Añadir texto",
  "designer.newTextValue": "TEXTO",
  "designer.layers": "Capas",
  "designer.showRail": "Mostrar hojas y capas",
  "designer.hideRail": "Ocultar hojas y capas",
  "designer.noLayers":
    "Aún no hay capas — añade una imagen, texto o una capa de pintura sobre la que dibujar.",
  "designer.layerCount": "{{count}} capa(s)",
  "designer.layerTitle": "Capa seleccionada",
  "designer.hide": "Ocultar",
  "designer.show": "Mostrar",
  "designer.raise": "Traer al frente",
  "designer.lower": "Enviar atrás",
  "designer.scale": "Tamaño",
  "designer.rotation": "Rotación",
  "designer.part": "Pieza",
  "designer.wholeSheet": "Toda la hoja",
  "designer.fitToPart": "Ajustar a la pieza",
  "designer.fitToPartHint":
    "Coloca y escala esta capa para cubrir la pieza elegida. La cubre en vez de caber dentro, así no quedan huecos: recórtala para quitar lo que sobresale.",
  "designer.fitNotForPaint": "Una capa de pintura es la hoja, así que no hay nada que mover ni escalar.",
  "designer.clipped": "Recortada",
  "designer.clippedHint": "Esta capa está recortada a la pieza: nada se sale por la junta.",
  "designer.flank.left": "lado izquierdo",
  "designer.flank.right": "lado derecho",
  "designer.flank.both": "ambos lados",
  "designer.flankWashHint":
    "El cálido es el lado izquierdo de la moto; el frío, el derecho. Los dos lados suelen desplegarse como dos copias casi idénticas del mismo panel, así que esto es lo único en la textura que los distingue.",
  "designer.flankSharedHint":
    "Los dos flancos se despliegan sobre esta misma zona, así que lo que dibujes aquí aparece en ambos lados de la moto: reflejado, y no donde lo esperarías en el otro.",
  "designer.focusHint": "Haz doble clic en una pieza para llenar la vista con ella.",
  "designer.partOver": "{{part}} sobre {{over}}",
  "designer.face.under": "cara interior",
  "designer.face.both": "exterior + interior",
  "designer.faceHint.under":
    "Esta zona es la cara interior de la pieza: lo que pintes aquí mira al suelo y nunca se ve desde fuera.",
  "designer.faceHint.both":
    "La cara exterior de la pieza y su cara interior comparten esta zona, así que lo que dibujes aquí cae en las dos.",
  // ── Designer › la selección, y qué se puede hacer con ella ────────────────────
  "designer.layersSelected": "{{count}} capas seleccionadas",
  "designer.position": "Posición",
  "designer.duplicate": "Duplicar",
  "designer.copy": "Copiar",
  "designer.paste": "Pegar",
  "designer.copyName": "{{name}} copia",
  "designer.copied_one": "1 capa copiada.",
  "designer.copied_other": "{{count}} capas copiadas.",
  "designer.pasteWrongSize":
    "Eso se copió de una hoja de otro tamaño, y una capa de pintura *es* la hoja: aquí no hay nada que encaje.",
  "designer.pasteDropped_one":
    "Se dejó fuera 1 capa de pintura: una capa de pintura es la hoja, y esta es de otro tamaño.",
  "designer.pasteDropped_other":
    "Se dejaron fuera {{count}} capas de pintura: una capa de pintura es la hoja, y esta es de otro tamaño.",
  "designer.group": "Agrupar",
  "designer.ungroup": "Desagrupar",
  "designer.groupRow": "Juntas",
  "designer.groupOf": "Grupo de {{count}}",
  "designer.groupHint":
    "Muévelas como una. Al hacer clic en cualquiera se toma el grupo entero; mantén Alt para sacar una sola capa.",
  "designer.flip": "Voltear",
  "designer.flipX": "Voltear de izquierda a derecha",
  "designer.flipY": "Voltear de arriba abajo",

  // ── Designer › reflejar al otro flanco ────────────────────────────────────────
  "designer.mirror": "Reflejar al otro lado",
  "designer.mirrorName": "{{name}} reflejada",
  "designer.mirrorHint":
    "Pone una copia de esta capa donde cae en el otro lado de la moto. Se calcula con el modelo en vez de volteando la hoja, así que llega a la pieza que le toca, y sigue a esta capa hasta que la desvincules.",
  "designer.mirroredFrom": "Reflejo de «{{name}}».",
  "designer.mirroredShort": "Reflejada",
  "designer.mirroredOrphan": "Esto se reflejó de una capa que ya no está.",
  "designer.unlink": "Desvincular",
  "designer.unlinkHint":
    "Deja de seguir y conserva lo que hay. Pasa a ser una capa normal que puedes editar por su cuenta.",
  "designer.selectSource": "Seleccionar la original",
  "designer.mirrorPaused":
    "No hay modelo cargado, así que esto se queda donde se colocó la última vez en lugar de seguir.",
  "designer.mirrorRough":
    "El otro lado no está desplegado como reflejo de este, así que la colocación es aproximada, no exacta.",
  "designer.mirrorWhy.no-model":
    "Carga primero la moto en la vista previa: sin el modelo no hay otro lado que encontrar.",
  "designer.mirrorWhy.shared":
    "Los dos flancos están desplegados en este mismo sitio, así que esto ya está en ambos lados de la moto. Una segunda copia caería sobre la primera.",
  "designer.mirrorWhy.centre":
    "Esto está en el eje de la moto, que es su propio reflejo: no hay otro lado al que mandarlo.",
  "designer.mirrorWhy.asymmetric":
    "El modelo no tiene nada en el reflejo de este punto, así que no hay otro lado donde ponerlo.",

  "designer.opacity": "Opacidad",
  "designer.blend": "Fusión",
  "designer.blend.normal": "Normal",
  "designer.blend.multiply": "Multiplicar",
  "designer.blend.screen": "Trama",
  "designer.blend.overlay": "Superponer",
  "designer.text": "Texto",
  "designer.font": "Fuente",
  "designer.size": "Tamaño del texto",
  "designer.colour": "Color",
  "designer.outline": "Contorno",
  "designer.noModelFound":
    "“{{model}}” no está en tu biblioteca, así que no hay nada donde mostrarla.",
  "designer.noBikePreview":
    "Esta versión no puede leer la geometría de las motos, así que una pintura no tiene modelo donde ir. Todo lo demás se guarda con normalidad.",
  "designer.noPreviewForGame":
    "La vista 3D es solo para MX Bikes por ahora: los modelos de {{game}} necesitan sus propias asignaciones de piezas. Todo lo demás funciona igual y la pintura se guarda con normalidad.",
  "designer.gearNote":
    "Se muestra sobre el piloto de serie — tu propio equipo no está cargado aquí.",
  "designer.gearOnly": "Solo la pieza",
  "designer.gearOnlyHint": "Muestra solo la pieza que estás pintando, sin el piloto",
  "designer.reference": "Referencia",
  "designer.traceTemplate": "Plantilla",
  "designer.traceHint":
    "Saca de la hoja la pintura de la que partiste y muéstrala tenue por debajo, para calcarla. Deja de formar parte de lo que guardas.",
  "designer.noTemplate": "Esta hoja no tiene plantilla que calcar: nació en blanco.",
  "designer.stockTexture": "Textura de fábrica",
  "designer.stockHint":
    "Muestra bajo tu hoja la textura con la que viene el modelo: los plásticos propios de la moto, antes de que ninguna pintura los reemplazara. Nada de ella se guarda.",
  "designer.noStock":
    "Solo las motos pueden decir qué texturas son suyas. Un casco lleva la pintura con la que vino, y eso no es un aspecto de fábrica que calcar.",
  "designer.stockNoMatch":
    "Este modelo no trae ninguna textura propia llamada “{{name}}”, así que no hay nada de la moto que mostrar bajo esta hoja.",
  "designer.stockAsBase": "Fábrica como base",
  "designer.stockAsBaseHint":
    "Pinta la textura propia de la moto en esta hoja a plena intensidad — el aspecto de fábrica como punto de partida, a un número de ser una pintura. A diferencia de la referencia de arriba, esta sí forma parte de lo que guardas.",
  "designer.stockAsBaseDone":
    "“{{name}}” ya lleva la textura propia de la moto — ponle tu número y guarda.",
  "designer.stockReadFailed": "No se pudo leer la textura “{{name}}” del modelo.",
  "designer.uvMap": "Mapa UV",
  "designer.uvHint":
    "Muestra dónde cae en esta hoja cada pieza del carenado del modelo, cada una con su color.",
  "designer.noGeometry": "Carga un modelo en la vista previa para ver su distribución UV.",
  "designer.uvNoMatch":
    "Nada del modelo usa una textura llamada “{{name}}”, así que no hay distribución UV que mostrar.",
  "designer.ghostBuried":
    "La referencia va debajo de la hoja, y la plantilla de esta hoja es opaca: activa Plantilla para sacarla y poder ver a través.",
  "designer.resetView": "Restablecer vista",

  // ── Designer › las herramientas de pintura ────────────────────────────────────
  "designer.paint": "Pintura",
  "designer.addPaint": "Capa de pintura",
  "designer.paintLayerName": "Pintura",
  "designer.undoStroke": "Deshacer trazo",
  "designer.redoStroke": "Rehacer trazo",
  "designer.tool.move": "Mover",
  "designer.tool.brush": "Pincel",
  "designer.tool.eraser": "Borrador",
  "designer.tool.gradient": "Degradado",
  "designer.tool.fill": "Relleno",
  "designer.tool.rect": "Rectángulo",
  "designer.tool.ellipse": "Elipse",
  "designer.tool.line": "Línea",
  "designer.moveHint":
    "Arrastra las capas sobre la hoja para colocarlas: se enganchan a las costuras y entre sí; mantén Alt para colocarlas libremente. Mayús+clic añade a la selección, arrastrar sobre el vacío hace un lazo, y el clic derecho tiene el resto. Elige una herramienta arriba para pintar sobre ella.",
  "designer.colourFrom": "Pinta con este",
  "designer.colourTo": "Funde hacia este",
  "designer.swapColours": "Intercambiar los dos colores",
  "designer.brushSize": "Pincel",
  "designer.hardness": "Borde",
  "designer.strength": "Intensidad",
  "designer.gradient": "Degradado",
  "designer.gradient.linear": "Lineal",
  "designer.gradient.radial": "Radial",
  "designer.fadeOut": "Desvanecer",
  "designer.shape": "Estilo",
  "designer.shape.fill": "Relleno",
  "designer.shape.outline": "Contorno",
  "designer.lineWidth": "Grosor",
  "designer.paintHint":
    "Arrastra sobre la hoja. Mantén Shift para que salga recto, arrastra con el botón derecho para mover la vista.",
  "designer.fillHint": "Haz clic en la hoja para inundar toda la capa.",
  "designer.gradientHint":
    "Arrastra sobre la hoja para marcar dónde ocurre la transición. Rellena toda esta capa: añade otra capa de pintura para conservar lo que hay debajo.",

  // The track terrain viewer.
  "trackViewer.open": "Ver el terreno",
  "trackViewer.title": "Vista previa del circuito",
  "trackViewer.loading": "Leyendo el terreno…",
  "trackViewer.refining": "Afinando…",
  "trackViewer.grid": "Cuadrícula",
  "trackViewer.surface": "Surface",
  "trackViewer.surfaceMasks": "From the track's surface data",
  "trackViewer.relief": "Desnivel",
  "trackViewer.objects": "Objetos",
  "trackViewer.painting": "Pintando…",
  "trackViewer.scenery": "Escenografía",
  "trackViewer.sceneryTris": "{{count}} triángulos",
  "trackViewer.fixtures": "Elementos marcados",
  "trackViewer.pieces": "Piezas separables",
  "trackViewer.selected": "Pieza seleccionada",
  "trackViewer.place": "Colocar un modelo",
  "trackViewer.placePick": "Elige un modelo y haz clic en el suelo.",
  "trackViewer.placeHint": "Haz clic en el suelo para colocarlo.",
  "trackViewer.placedCount": "{{count}} colocados",
  "trackViewer.saveProps": "Guardar .scr",
  "trackViewer.noTerrain": "No hay terreno que mostrar",
  "trackViewer.noTerrainHint":
    "Los datos de altura de este circuito no están en un formato que el visor sepa leer todavía.",
  "trackViewer.inferredNote":
    "El archivo de alturas de este circuito no tiene un formato documentado, así que su forma se dedujo de los datos. Tómalo como una lectura aproximada, no exacta.",
  "trackViewer.assumedScaleNote":
    "Este circuito no indica la separación entre sus puntos de altura: el relieve es real, pero su pendiente es aproximada.",
  "trackViewer.whyDetails": "¿Por qué?",
  "trackViewer.copyDetails": "Copiar detalles",
  "trackViewer.copied": "Copiado",
  /* ── Protect ─────────────────────────────────────────────────────────── */
  "nav.protect": "Proteger",
  "protect.help": "Bloquea los archivos que has creado a los GUID de quienes pueden cargarlos. Un archivo bloqueado solo se abre en la instalación cuyo GUID lleva, así que un comprador es una copia: elige los archivos una vez, pega los GUID y obtienes una carpeta por comprador. Tus originales solo se leen.",
  "protect.ownTitle": "Tu GUID",
  "protect.ownDesc": "La identidad con la que MX Bikes te reconoce. No está en ningún archivo — la entrega Steam al iniciar sesión — así que la app la lee del juego en marcha.",
  "protect.ownUnknown": "Aún desconocido",
  "protect.readOwn": "Leer del juego",
  "protect.copy": "Copiar",
  "protect.copied": "Copiado",
  "protect.useMine": "Añadir a la lista",
  "protect.ownFound": "Tu GUID es {{guid}}",
  "protect.ownNotFound": "No se pudo leer tu GUID",
  "protect.ownNotFoundWhy": "Abre MX Bikes e inicia sesión en Steam, y vuelve a intentarlo.",
  "protect.ownFailed": "Falló la lectura de tu GUID",
  "protect.filesTitle": "Archivos a bloquear",
  "protect.filesDesc": "Añade archivos sueltos, o una carpeta para tomar todo lo que hay dentro. Un .pkz se bloquea como archivo comprimido; lo demás se bloquea por separado.",
  "protect.addFiles": "Añadir archivos",
  "protect.addFolder": "Añadir carpeta",
  "protect.clear": "Vaciar",
  "protect.summary": "{{files}} archivos · {{size}}",
  "protect.summarySkipped": "{{count}} omitidos",
  "protect.planFailed": "No se pudo leer esa selección",
  "protect.skipJunk": "Omitido",
  "protect.skipEmpty": "Vacío",
  "protect.skipProtected": "Ya protegido",
  "protect.guidsTitle": "Bloquear para",
  "protect.guidsDesc": "Un GUID por línea — sale una carpeta por GUID. Cada copia lleva su propia clave, así que dos compradores no pueden saber que tienen el mismo archivo.",
  "protect.guidsPlaceholder": "FF0110000108D7CFE3",
  "protect.guidBad": "No es un GUID — 18 caracteres hexadecimales",
  "protect.outTitle": "Dónde ponerlos",
  "protect.outDesc": "Aquí se crea una carpeta por GUID, con los archivos en las mismas rutas que ya tenían. Tus originales no se tocan.",
  "protect.outPlaceholder": "Elige una carpeta",
  "protect.browse": "Examinar",
  "protect.lock": "Bloquear {{files}} archivos para {{guids}} GUID",
  "protect.done": "Bloqueados {{files}} archivos para {{guids}} GUID",
  "protect.failed": "El bloqueo falló",
  "protect.showFolder": "Mostrar carpeta",

  // ── Track Studio ───────────────────────────────────────────────────────────
  "nav.track": "Pista",
  "track.help": "Describe una pista y Claude escribe la vuelta — las curvas, las rectas y cada salto. La app la construye, la mide contra pistas publicadas reales y vuelve a preguntar por lo que no encaja. Previsualízala en 3D, edita cualquier salto y exporta la carpeta que compila el TerrainEd de MX Bikes.",
  "track.briefPlaceholder": "Un nacional arenoso con una sección de ritmo larga y dos step-ups grandes",
  "track.generate": "Generar",
  "track.generating": "Escribiendo…",
  "track.generatingHint": "Trazando una vuelta que cierre, construyéndola y midiéndola. Uno o dos minutos, más si el primer intento necesita arreglo.",
  "track.generated": "Se escribió «{{name}}»",
  "track.generateFailed": "No se pudo escribir una pista",
  "track.empty": "Describe la pista que quieres. Claude escribe la vuelta y cada salto; la app la construye y comprueba que mide como una real.",
  "track.lap": "Vuelta",
  "track.width": "Ancho",
  "track.ground": "Terreno",
  "track.features": "Elementos",
  "track.corners": "Curvas",
  "track.measured": "Medido",
  "track.lips": "Saltos",
  "track.steepest": "Más inclinado",
  "track.relief": "Relieve",
  "track.budget": "Altura usada",
  "track.problems": "Por corregir",
  "track.preview": "Ver en 3D",
  "track.building": "Construyendo…",
  "track.install": "Instalar",
  "track.export": "Exportar fuente",
  "track.installed": "Vista previa instalada",
  "track.installFailed": "No se pudo instalar",
  "track.buildFailed": "No se pudo construir",
  "track.exported": "Se escribieron {{count}} archivos",
  "track.exportFailed": "No se pudo exportar",
  "track.previewOnly": "La vista previa es solo terreno — el juego la listará y fallará al cargarla. Ejecuta TerrainEd sobre la carpeta exportada para obtener una pista jugable.",
  "track.at": "En",
  "track.kind": "Tipo",
  "track.height": "Altura",
  "track.length": "Longitud",
  "track.kind.tabletop": "Tabletop",
  "track.kind.double": "Doble",
  "track.kind.roller": "Roller",
  "track.kind.whoops": "Whoops",
  "track.kind.stepUp": "Step-up",
  "track.kind.berm": "Peralte",
  "track.straight": "Recta",
  "track.turnLeft": "Curva a izquierda",
  "track.turnRight": "Curva a derecha",
  "track.radius": "de radio",
  "track.gap": "de hueco",
  "track.deep": "de profundidad",
  "track.over": "sobre",
  "track.kind.rut": "Rodada",
  "track.sequenceHint": "Puedes describir la vuelta en orden — «una recta larga, una horquilla a izquierda, luego un doble y una sección de ritmo» — y se construirá así.",
  "track.compile": "Compilar con TerrainEd",
  "track.compiling": "Compilando…",
  "track.compiled": "Compilado",
  "track.compileFailed": "No se pudo compilar",
  "track.buildStepFailed": "Falló el paso «{{step}}»",
  "track.pointAtTools": "Indicar las herramientas…",
  "track.toolsNotFound": "No hay terrained.exe en esa carpeta",
  "track.stillNeeded": "Aún necesita un .rdf de TrackEd (parrilla, boxes, cámaras) y un gate.edf copiado de otra pista.",
  "track.base": "Pista base",
  "track.baseLoaded": "Se cargó «{{name}}»",
  "track.name": "Nombre de la pista",
  "track.author": "Autor",
  "track.previewHint": "Pulsa Previsualizar para construirla y verla aquí.",
  "track.location": "Ubicación",
  "track.blank": "En blanco",
  "track.closeLap": "Cerrar la vuelta",
  "track.lapClosed": "La vuelta ya cierra",
  "track.closeFailed": "No se pudo cerrar",
  "track.notes": "Distinto de una pista publicada",
  "track.replaceTitle": "¿Reemplazar esta pista?",
  "track.replaceBody": "Has hecho cambios que no se han exportado ni instalado. Cargar otra pista los descarta.",
  "track.replaceConfirm": "Descartar y cargar",
  "track.across": "de ancho",
  "track.hills": "colinas",
  "track.surface": "suelo",
  "track.soil": "Tierra",
  "track.sand": "Arena",
  "track.grass": "Hierba",
};
