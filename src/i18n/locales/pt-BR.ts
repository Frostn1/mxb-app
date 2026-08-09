import type { Translation } from "..";

/**
 * Brazilian Portuguese (informal "você" — this is a modding tool for a game
 * community).
 *
 * Community terminology rather than dictionary equivalents: `mod`, `setup`,
 * `preset` and `Stock` stay as loanwords, while gear is translated — `capacete`,
 * `botas`, `óculos` for goggles, `pintura` for a paint/livery.
 *
 * This is the only Portuguese we ship, so a plain `pt` OS tag resolves here
 * (see `resolveSystemLocale`).
 *
 * Product names (MXB App, FrostMod, MX Bikes) are never translated.
 */
export const ptBR: Translation = {
  // ── Genérico ───────────────────────────────────────────────────────────────
  "common.cancel": "Cancelar",
  "common.back": "Voltar",
  "common.next": "Avançar",
  "common.skip": "Pular",
  "common.close": "Fechar",
  "common.save": "Salvar",
  "common.delete": "Excluir",
  "common.rename": "Renomear",
  "common.retry": "Tentar de novo",
  "common.tryAgain": "Tentar de novo",
  "common.loading": "Carregando…",
  "common.installed": "Instalado",
  "common.select": "Selecionar",
  "common.deselect": "Desmarcar",
  "common.selectAll": "Selecionar tudo",
  "common.clear": "Limpar",
  "common.done": "Pronto",
  "common.apply": "Aplicar",
  "common.remove": "Remover",
  "common.open": "Abrir",
  "common.refresh": "Atualizar",
  "common.dismiss": "Dispensar",
  "common.later": "Mais tarde",
  "common.active": "Ativo",

  // ── Controles da janela ────────────────────────────────────────────────────
  "window.minimize": "Minimizar",
  "window.maximize": "Maximizar",
  "window.close": "Fechar",

  // ── Navegação ──────────────────────────────────────────────────────────────
  "nav.browse": "Explorar",
  "nav.shop": "Loja",
  "nav.library": "Biblioteca",
  "nav.locker": "Armário",
  "nav.presets": "Presets",
  "nav.rider": "Piloto",
  "nav.paints": "Pinturas",
  "nav.servers": "Servidores",
  "nav.manage": "Gerenciar",
  "nav.settings": "Configurações",

  "sidebar.installing": "Instalando “{{name}}”",
  "sidebar.queued": "+{{count}} na fila",

  // ── FrostMod ───────────────────────────────────────────────────────────────
  "frostmod.checking": "Verificando o FrostMod…",
  "frostmod.running": "FrostMod ativo",
  "frostmod.notRunning": "FrostMod inativo",
  "frostmod.reloadGame": "Recarregar o jogo",
  "frostmod.start": "Iniciar o FrostMod",
  "frostmod.reloadedGame": "O FrostMod recarregou o jogo.",
  "frostmod.notRunningToast": "O FrostMod não está em execução.",
  "frostmod.started": "FrostMod iniciado",
  "frostmod.alreadyRunning": "O FrostMod já está em execução",
  "frostmod.startFailed": "Não foi possível iniciar o FrostMod",
  "frostmod.stop": "Parar o FrostMod",
  "frostmod.stopped": "FrostMod parado",
  "frostmod.stopFailed": "Não foi possível parar o FrostMod",
  "frostmod.stopFailedDesc":
    "Ele ainda está em execução — pode ter sido iniciado por outro usuário ou com privilégios de administrador.",
  "frostmod.installedToast": "FrostMod {{version}} instalado",
  "frostmod.installedToastDesc":
    "Ele recarrega o jogo na hora quando você adiciona mods.",
  "frostmod.installedToastRestart":
    "Reinicie o MX Bikes para valer — o jogo aberto ainda está com o FrostMod antigo.",
  "frostmod.installFailed": "Não foi possível instalar o FrostMod",
  "frostmod.newModsAdded": "Novos mods adicionados",
  "frostmod.modsAdded_one": "Novo mod adicionado",
  "frostmod.modsAdded_other": "{{count}} mods adicionados",
  "frostmod.askedReload": "Pedimos ao FrostMod para recarregar o jogo.",
  "frostmod.andMore_one": "{{names}} e mais {{count}}",
  "frostmod.andMore_other": "{{names}} e mais {{count}}",
  "frostmod.watchDesc":
    "{{names}} — pedimos ao FrostMod para recarregar o jogo.",

  // ── Configuração inicial ───────────────────────────────────────────────────
  "setup.title": "Bem-vindo ao MXB App",
  "setup.tagline": "Explore mods, instale com um clique e volte logo para a moto.",
  "setup.modsFolder": "Pasta do {{game}}",
  "setup.autoDetect":
    "O MXB App vai detectar sua pasta {{hint}} automaticamente. Você também pode escolher você mesmo.",
  "setup.chooseManually": "Escolher a pasta manualmente…",
  "setup.chooseDifferent": "Escolher outra pasta…",
  "setup.gameInstall": "Instalação do {{game}}",
  "setup.detecting": "Procurando sua instalação do {{game}}…",
  "setup.found": "Encontrada",
  "setup.detectedAutomatically": "Detectada automaticamente",
  "setup.installNotFound":
    "Não deu pra encontrar sua instalação do {{game}} automaticamente — é ela que alimenta a prévia 3D do piloto. Escolha manualmente, ou defina depois nas Configurações.",
  "setup.chooseInstallManually":
    "Escolher a pasta de instalação manualmente…",
  "setup.startBrowsing": "Começar a explorar mods",
  "setup.detectAndStart": "Detectar e começar",
  "setup.pickModsFolder": "Selecione sua pasta do {{game}}",
  "setup.pickInstallFolder": "Selecione a pasta de instalação do {{game}}",

  // ── Boas-vindas ────────────────────────────────────────────────────────────
  "welcome.intro.title": "Bem-vindo ao MXB App",
  "welcome.intro.body":
    "Seu gerenciador de mods do MX Bikes. Mantenha pistas, motos e pinturas organizadas em um só lugar — chega de arquivos zip espalhados pela área de trabalho. Em alguns segundos a gente te mostra tudo.",
  "welcome.getStarted": "Começar",

  // ── Presets ────────────────────────────────────────────────────────────────
  "presets.missing": "faltando",
  "presets.missingHint":
    "Este mod não está instalado — aparece como Stock no jogo",
  "presets.missingMods":
    "Mods faltando: {{mods}}. Instale-os para essas partes aparecerem.",
  "presets.help":
    "Salve um visual completo do piloto e carregue numa moto quando quiser.",
  "presets.profile": "Perfil",
  "presets.namePlaceholder": "Nome do preset…",
  "presets.savePreset": "Salvar preset",
  "presets.saveChanges": "Salvar alterações",
  "presets.saveChangesQ": "Salvar as alterações?",
  "presets.replaceQ": "Substituir o preset?",
  "presets.replace": "Substituir",
  "presets.loadCopy": "Carregar uma cópia no editor",
  "presets.viewOnRider": "Ver no piloto",
  "presets.editNameOrOptions": "Editar nome ou opções",
  "presets.share": "Compartilhar",
  "presets.nameFirst": "Primeiro dê um nome ao preset.",
  "presets.pickProfileAndBike": "Escolha um perfil e uma moto para aplicar.",
  "presets.updated": "Preset “{{name}}” atualizado.",
  "presets.renamed": "Renomeado para “{{name}}” e alterações salvas.",
  "presets.saved": "Preset “{{name}}” salvo.",
  "presets.editing":
    "Editando “{{name}}” — mude o que quiser e depois salve as alterações.",
  "presets.appliedRefreshed":
    "“{{label}}” aplicado em {{bike}} — atualizado na hora dentro do jogo.",
  "presets.appliedRefreshFailed":
    "“{{label}}” aplicado em {{bike}} — salvo, mas a atualização instantânea falhou: selecione seu perfil de novo no jogo para carregar.",
  "presets.appliedGameRunning":
    "“{{label}}” aplicado em {{bike}} — salvo. Selecione seu perfil de novo no MX Bikes (menu Profile) para carregar o novo visual.",
  "presets.appliedNextTime":
    "“{{label}}” aplicado em {{bike}} — salvo. Carrega na próxima vez que o jogo abrir.",
  "presets.appliedReselectBike":
    "“{{label}}” aplicado em {{bike}} — as pinturas já estão valendo; selecione a moto de novo no MX Bikes para ver o modelo.",
  "presets.phaseBundling": "Empacotando os arquivos…",
  "presets.phaseUploading": "Enviando o pacote…",
  "presets.phaseDownloading": "Baixando o pacote…",
  "presets.phaseInstalling": "Instalando os arquivos…",
  "presets.bundleUploaded":
    "Pacote completo enviado — o código agora inclui os arquivos.",
  "presets.shareHintFull":
    "Este código inclui um pacote para download — quem receber escolhe Importação completa e recebe tudo, mesmo sem nenhum mod instalado.",
  "presets.shareHintConfig":
    "Mande este código para quem quiser. A importação é em Presets → Importar. A pessoa vai precisar dos mesmos mods instalados para tudo aparecer.",
  "presets.generatingCode": "Gerando o código…",
  "presets.nothingToBundle":
    "Nenhum arquivo instalado para empacotar — este visual é todo Stock/fontes.",
  "presets.createFullBundle": "Criar pacote completo",
  "presets.copiedFull": "Código com pacote completo copiado.",
  "presets.copiedShare": "Código de compartilhamento copiado.",
  "presets.copyFailed":
    "Não deu pra copiar — selecione o código e copie na mão.",
  "presets.copyFullCode": "Copiar código completo",
  "presets.copyCode": "Copiar código",
  "presets.importTitle": "Importar preset",
  "presets.importBody": "Cole um código que te mandaram.",
  "presets.configOnly": "Só a configuração",
  "presets.import": "Importar",
  "presets.fullImport": "Importação completa",
  "presets.editingBanner":
    "Editando {{name}} — mude o nome ou qualquer slot e depois {{save}}.",
  "presets.bundleNotice":
    "Inclui um pacote completo (~{{size}} de {{host}}). Use {{fullImport}} para baixar e instalar tudo — não precisa ter mods antes.",

  // ── Slots dos presets ──────────────────────────────────────────────────────
  "slot.paint": "Pintura da moto",
  "slot.modelSwap": "Troca de modelo",
  "slot.bikeFont": "Fonte dos números",
  "slot.tyres": "Pneus",
  "slot.rider": "Perfil do piloto",
  "slot.suitPaint": "Uniforme / kit",
  "slot.suitFont": "Fonte do uniforme",
  "slot.glovesPaint": "Luvas",
  "slot.ridingStyle": "Estilo de pilotagem",
  "slot.helmet": "Capacete",
  "slot.helmetPaint": "Pintura do capacete",
  "slot.gogglesPaint": "Óculos",
  "slot.boots": "Botas",
  "slot.bootsPaint": "Pintura das botas",
  "slot.protection": "Proteções",
  "slot.protectionPaint": "Pintura das proteções",
  "slotGroup.bike": "Moto",
  "slotGroup.rider": "Piloto",
  "slotGroup.head": "Cabeça",
  "slotGroup.body": "Corpo",

  // ── Estúdio do piloto ──────────────────────────────────────────────────────
  "rider.help":
    "Vista o modelo do piloto — capacete, óculos, uniforme e botas de uma vez.",
  "rider.namePlaceholder": "Dê um nome a este piloto…",
  "rider.nameFirst": "Primeiro dê um nome a este visual.",
  "rider.showOnModel": "Mostrar no modelo",
  "rider.repairTitle": "Um mod de {{area}} foi instalado solto",
  "rider.repairBody":
    "Os arquivos dele estão direto em {{area}} em vez de numa pasta, então nem o jogo nem este app conseguem carregá-lo. Juntar tudo em “{{model}}”?",
  "rider.repairAction": "Reparar",
  "rider.repairDone_one": "{{count}} arquivo juntado em “{{model}}”.",
  "rider.repairDone_other": "{{count}} arquivos juntados em “{{model}}”.",
  "rider.repairNothing": "Não sobrou nada para juntar.",

  // ── Tour guiado ────────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Faça um tour rápido",
  "tour.welcomeTour.body":
    "Alguns segundos para ver onde fica cada coisa. Você pode pular quando quiser.",
  "tour.browse.title": "Explorar mods",
  "tour.browse.body": "Pesquise no {{site}} aqui mesmo e instale qualquer pista, moto ou pintura com um clique.",
  "tour.library.title": "Sua biblioteca",
  "tour.library.body":
    "Tudo que você instalou, em um só lugar — atualize ou remova mods sem nunca mexer num arquivo zip.",
  "tour.locker.title": "O armário",
  "tour.locker.body":
    "Troque os modelos das motos à vontade. O MXB App registra as peças para o jogo reconhecer.",
  "tour.presets.title": "Presets",
  "tour.presets.body":
    "Salve combinações de equipamento e pinturas e aplique um visual completo com um clique — até enquanto você está pilotando.",
  "tour.rider.title": "Estúdio do piloto",
  "tour.rider.body":
    "Veja a prévia do seu equipamento e das pinturas no piloto 3D antes de levar pra pista.",
  "tour.frostmod.title": "FrostMod, ao vivo",
  "tour.frostmod.body":
    "Aqui você vê o status do FrostMod. Ele recarrega o MX Bikes depois de uma instalação, então o conteúdo novo aparece sem reiniciar o jogo.",
  "tour.settings.title": "Configurações",
  "tour.settings.body":
    "Aqui você define sua pasta do jogo, o comportamento em segundo plano e as opções do FrostMod. Também dá pra rever este tour por aqui.",
  "tour.done.title": "Tudo pronto",
  "tour.done.body":
    "O tour acabou. Vá em Explorar e instale seu primeiro mod.",

  // ── Erros ──────────────────────────────────────────────────────────────────
  "error.previewFailed": "Não foi possível exibir a prévia",
  "error.somethingWentWrong": "Algo deu errado",
  "error.unexpected": "Ocorreu um erro inesperado.",
  "error.reloadApp": "Recarregar o app",

  // ── Atualizações ───────────────────────────────────────────────────────────
  "update.available": "{{version}} está disponível.",
  "update.downloading": "Baixando…",
  "update.downloadingPct": "Baixando… {{pct}}%",
  "update.pitch":
    "Atualize para receber os recursos e as correções mais recentes.",
  "update.updating": "Atualizando…",
  "update.updateAndRestart": "Atualizar e reiniciar",
  "update.dismiss": "Dispensar o aviso de atualização",
  "update.onLatest": "Você já está na versão mais recente",

  // ── Runtime do Visual C++ ausente ──────────────────────────────────────────
  "runtime.componentVc90": "Microsoft Visual C++ 2008 (x64)",
  "runtime.componentVc140": "Microsoft Visual C++ 2015–2022 (x64)",
  "runtime.bannerGame":
    "O MX Bikes precisa do {{what}} antes que o FrostMod consiga entrar nele.",
  "runtime.bannerFrostmod": "O FrostMod precisa do {{what}} para funcionar.",
  "runtime.pitch":
    "Sem isso o Windows mostra o erro \"dll was not found\". Leva segundos para resolver.",
  "runtime.fixIt": "Instalar",
  "runtime.installing": "Instalando…",
  "runtime.dismiss": "Dispensar este aviso",
  "runtime.installed": "Componente instalado",
  "runtime.installedDesc":
    "O FrostMod já deve alcançar o jogo. Reinicie o MX Bikes se ele estiver aberto.",
  "runtime.cancelled": "Nada foi instalado",
  "runtime.cancelledDesc":
    "O Windows precisa da sua permissão. Abrindo o download da Microsoft no lugar.",
  "runtime.installFailed": "Não foi possível instalar o componente",
  "runtime.downloadManually": "Baixar por conta própria",
  "update.checkFailed": "Não foi possível verificar as atualizações",
  "update.failed": "A atualização falhou",

  // ── Visualizador 3D ────────────────────────────────────────────────────────
  "viewer.preview3d": "Prévia 3D",
  "viewer.expand": "Expandir",
  "viewer.paint": "Pintura",
  "viewer.loadingModel": "Carregando o modelo…",
  "viewer.loadingPaint": "Carregando a pintura…",
  "viewer.loadingRider": "Carregando o piloto…",
  "viewer.riderLoadFailed": "A prévia está desatualizada — não foi possível atualizá-la",
  "viewer.dragToRotate": "Arraste para girar",
  "viewer.scrollToZoom": "Role para dar zoom",
  "viewer.rightDragToPan": "Arraste com o botão direito para mover",

  // ── Combobox ───────────────────────────────────────────────────────────────
  "combobox.search": "Pesquisar…",
  "combobox.use": "Usar “{{value}}”",

  // ── Tipos de mod ───────────────────────────────────────────────────────────
  "modType.tracks": "Pistas",
  "modType.bikes": "Motos",
  "modType.rider": "Piloto",
  "modType.tracksInline": "pistas",
  "modType.bikesInline": "motos",
  "modType.riderInline": "equipamento do piloto",

  // ── Filtros de categoria ───────────────────────────────────────────────────
  "browseCat.all": "Tudo",
  "browseCat.beginner": "Iniciante",
  "browseCat.intermediate": "Intermediário",
  "browseCat.pro": "Pro",
  "browseCat.assets": "Recursos",
  "browseCat.newBikes": "Motos novas",
  "browseCat.liveries": "Pinturas",
  "browseCat.sounds": "Sons",
  "browseCat.riderKit": "Kit do piloto",
  "browseCat.helmets": "Capacetes",
  "browseCat.helmetPaints": "Pinturas de capacete",
  "browseCat.gloves": "Luvas",
  "browseCat.boots": "Botas",
  "browseCat.bootPaints": "Pinturas de botas",
  "browseCat.protection": "Proteções",
  "browseCat.protectionPaints": "Pinturas de proteções",

  // ── Explorar ───────────────────────────────────────────────────────────────
  "browse.help":
    "Descubra e instale mods do catálogo online — pesquise, filtre por tipo e abra um mod para baixá-lo no jogo.",
  "browse.searchPlaceholder": "Pesquisar {{type}}…",
  "browseSort.newest": "Mais recentes",
  "browseSort.oldest": "Mais antigos",
  "browseSort.popularAll": "Mais populares",
  "browseSort.popularMonth": "Populares este mês",
  "browseSort.popularWeek": "Populares esta semana",
  "browse.loadFailed": "Não foi possível carregar os mods",
  "browse.empty": "Nenhum resultado em {{type}}.",
  "browse.loadMore": "Carregar mais",
  "browse.selectedCount": "{{count}} selecionados",
  "browse.queuing": "Colocando na fila…",
  "browse.quickInstallCount": "Instalar {{count}} rapidamente",
  "browse.quickInstall": "Instalação rápida",
  "browse.quickReinstall": "Reinstalação rápida",
  "browse.openDetails": "Abrir detalhes",
  "browse.reinstallOne": "Reinstalar “{{title}}”?",
  "browse.reinstallMany": "Reinstalar os mods que você já tem?",
  "browse.reinstallOneBody":
    "Este mod já está na sua biblioteca. Reinstalar baixa tudo de novo e sobrescreve os arquivos instalados.",
  "browse.reinstallManyBody":
    "{{installed}} dos {{total}} selecionados já estão instalados. Se continuar, eles serão reinstalados e sobrescritos.",
  "browse.reinstall": "Reinstalar",
  "browse.reinstallAll": "Reinstalar tudo",
  "browse.queued": "“{{title}}” na fila",
  "browse.queuedDesc": "Instalando em {{folder}}.",
  "browse.rootFolder": "raiz",
  "browse.needsBrowser": "“{{title}}” precisa ser baixado pelo navegador",
  "browse.needsBrowserDesc":
    "O {{host}} bloqueia downloads dentro do app — abra a página dele para concluir.",
  "browse.noDownload": "Nenhum download encontrado para “{{title}}”",
  "browse.quickInstallFailed":
    "Não foi possível instalar “{{title}}” rapidamente",
  "browse.queuedBulk_one": "{{count}} mod na fila",
  "browse.queuedBulk_other": "{{count}} mods na fila",
  "browse.queuedBulkDesc": "Eles serão instalados um depois do outro.",
  "browse.queuedBulkSkipped_one":
    "{{count}} ignorado — host só pelo navegador.",
  "browse.queuedBulkSkipped_other":
    "{{count}} ignorados — host só pelo navegador.",
  "browse.bulkFailed": "Não foi possível instalar a seleção rapidamente",
  "browse.bulkFailedDesc_one": "Ele precisa ser baixado pelo navegador.",
  "browse.bulkFailedDesc_other":
    "Todos os {{count}} precisam ser baixados pelo navegador.",

  // ── Loja (MX Bikes Shop — downloads comprados) ─────────────────────────────
  "shop.help":
    "Navegue pelo catálogo do mxbikes-shop.com e instale o que você já comprou. A compra continua sendo feita no site da loja; entre em Minhas compras para instalar seus pedidos por aqui.",
  "shopTab.catalog": "Catálogo",
  "shopTab.purchases": "Minhas compras",
  "shop.myDownloads": "Minhas compras",
  "shop.signInTitle": "Entre na MX Bikes Shop",
  "shop.signInBody":
    "Entre no mxbikes-shop.com para ver e instalar tudo o que você comprou. Abrimos o site real — sua senha nunca passa por este aplicativo.",
  "shop.signIn": "Entrar",
  "shop.logOut": "Sair",
  "shop.signedIn": "Conectado à MX Bikes Shop",
  "shop.sessionFailed": "Não foi possível capturar sua sessão da MX Bikes Shop",
  "shop.loadFailed": "Não foi possível carregar suas compras: {{error}}",
  "shop.empty": "Nenhum download comprado encontrado na sua conta ainda.",
  "purchases.count_one": "{{count}} compra",
  "purchases.count_other": "{{count}} compras",
  "purchases.fileCount_one": "{{count}} arquivo",
  "purchases.fileCount_other": "{{count}} arquivos",
  "purchases.install": "Instalar",
  "purchases.reinstall": "Reinstalar",
  "purchases.installed": "Instalado",
  "purchases.downloading": "Baixando…",
  "purchases.downloadFailed": "Não foi possível baixar {{title}}",
  // ── Catálogo da MX Bikes Shop (só navegação; a compra é na loja) ───────────
  "shopCatalog.searchPlaceholder": "Buscar na loja…",
  "shopCatalog.allCategories": "Tudo",
  "shopCatalog.onSaleOnly": "Em promoção",
  "shopCatalog.loadMore": "Carregar mais",
  "shopCatalog.loadFailed": "Não foi possível carregar o catálogo da loja",
  "shopCatalog.empty": "Nada na loja corresponde a isso.",
  "shopCatalog.viewDetails": "Ver detalhes",
  "shopCatalog.openOnStore": "Abrir em mxbikes-shop.com",
  "shopCatalog.buyOnStore": "Comprar em mxbikes-shop.com",
  "shopCatalog.buyNote": "Abre no seu navegador. A compra e o download acontecem na loja.",
  "shopCatalog.noProductLink": "Este item não tem uma página de produto que possamos abrir.",
  "shopCatalog.noScreenshots": "Sem capturas",
  "shopCatalog.about": "Sobre este item",
  "shopCatalog.author": "Criador",
  "shopCatalog.category": "Categoria",
  "shopCatalog.updated": "Atualizado",
  "shopCatalog.priceUnknown": "Preço não informado",
  "shopCatalog.free": "Grátis",
  "shopCatalog.refresh": "Atualizar",
  "shopCatalog.refreshing": "Atualizando…",
  "shopCatalog.stale": "Preços verificados pela última vez {{when}}.",
  "shopCatalog.staleHard":
    "Estes preços foram verificados pela última vez {{when}} e podem estar desatualizados. Atualize antes de confiar neles.",
  "shopCatalog.saleEndsDays_one": "A promoção termina em 1 dia",
  "shopCatalog.saleEndsDays_other": "A promoção termina em {{count}} dias",
  "shopCatalog.saleEndsHours_one": "A promoção termina em 1 hora",
  "shopCatalog.saleEndsHours_other": "A promoção termina em {{count}} horas",
  "shopCatalog.saleEndsSoon": "A promoção termina em breve",
  "shopCatalog.agoJustNow": "agora mesmo",
  "shopCatalog.agoUnknown": "há algum tempo",
  "shopCatalog.agoMinutes_one": "há 1 minuto",
  "shopCatalog.agoMinutes_other": "há {{count}} minutos",
  "shopCatalog.agoHours_one": "há 1 hora",
  "shopCatalog.agoHours_other": "há {{count}} horas",
  "shopCatalog.agoDays_one": "há 1 dia",
  "shopCatalog.agoDays_other": "há {{count}} dias",
  "shopSort.newest": "Mais recentes",
  "shopSort.recentlyUpdated": "Atualizados recentemente",
  "shopSort.priceAsc": "Preço: do menor ao maior",
  "shopSort.priceDesc": "Preço: do maior ao menor",
  "shopSort.onSale": "Promoções primeiro",
  "shopSort.nameAsc": "Nome (A–Z)",

  // ── Janela de instalação ───────────────────────────────────────────────────
  "installDialog.installTo": "Instalar em",
  "installDialog.installToFolder": "Instalar em {{folder}}",
  "installDialog.change": "Alterar",
  "installDialog.searchBikes": "Pesquisar motos…",
  "installDialog.searchFolders": "Pesquisar pastas…",
  "installDialog.probably": "Provavelmente",
  "installDialog.allFolders": "Todas as pastas",
  "installDialog.noFolderMatch":
    "Nenhuma pasta corresponde — crie uma abaixo.",
  "installDialog.rememberedFor": "Lembrado para {{type}}",
  "installDialog.downloadFrom": "Baixar de",
  "installDialog.downloadPerBike": "Download (por moto)",
  "installDialog.opensInBrowser":
    "Abre no navegador — o MXB App conclui a instalação",
  "installDialog.matchedBike": "Combina com a sua moto",
  "installDialog.differentBike": "Moto / pacote diferente",
  "installDialog.directFastest": "Direto · o mais rápido",
  "installDialog.direct": "Direto",
  "installDialog.perBikeHint":
    "Cada download é uma moto diferente — selecionado automaticamente conforme a sua escolha. Escolha o pacote “all bikes” para todas de uma vez.",
  "installDialog.mirrorsHint":
    "Todos os espelhos têm o mesmo arquivo. Se um falhar, tente o próximo.",

  // ── Detalhes da biblioteca ─────────────────────────────────────────────────
  "libraryDetail.author": "Autor",
  "libraryDetail.length": "Extensão",
  "libraryDetail.altitude": "Altitude",
  "libraryDetail.location": "Local",
  "libraryDetail.type": "Tipo",
  "libraryDetail.mod": "Mod",
  "libraryDetail.belongsTo": "Pertence a",
  "libraryDetail.format": "Formato",
  "libraryDetail.extractedFolder": "Pasta extraída",
  "libraryDetail.paintFile": "Arquivo de pintura",
  "libraryDetail.packagedPkz": "Pacote .pkz",
  "libraryDetail.size": "Tamanho",
  "libraryDetail.folder": "Pasta",
  "libraryDetail.lockedWord": "bloqueada",
  "libraryDetail.lockedWithMeta":
    "Esta pista foi {{locked}} pelo criador. O nome, os detalhes e a prévia aparecem aqui, mas os arquivos continuam lacrados — não dá pra extrair nem ver em 3D.",
  "libraryDetail.lockedNoMeta":
    "Esta pista está {{locked}}, então o nome, a extensão e a prévia não podem ser lidos do arquivo — só o nome do arquivo e o tamanho.",

  // ── Página do mod ──────────────────────────────────────────────────────────
  "modDetail.stageResolve": "Resolver",
  "modDetail.stageDownload": "Baixar",
  "modDetail.stageExtract": "Extrair",
  "modDetail.stagePlace": "Posicionar",
  "modDetail.stageReload": "Recarregar",
  "modDetail.modFiles": "Arquivos de mod",
  "modDetail.copied": "Copiado",
  "modDetail.copy": "Copiar",
  "modDetail.addToLibrary": "Adicionar à biblioteca",
  "modDetail.host": "Host",
  "modDetail.installsTo": "Instala em",
  "modDetail.noDownloadLink": "Nenhum link de download foi encontrado nesta página — abra-a em {{site}}.",
  "modDetail.frostmodHint":
    "O FrostMod vai recarregar a lista de {{kind}} quando isso terminar.",
  "modDetail.kindRider": "piloto",
  "modDetail.kindBike": "motos",
  "modDetail.kindTrack": "pistas",
  "modDetail.details": "Detalhes",
  "modDetail.format": "Formato",
  "modDetail.mirrors": "Espelhos",
  "modDetail.type": "Tipo",
  "modDetail.addedToLibrary": "Adicionado à sua biblioteca",
  "modDetail.extracting": "Extraindo…",
  "modDetail.addingToLibrary": "Adicionando à biblioteca…",
  "modDetail.resolving": "Resolvendo o download…",
  "modDetail.finishInBrowser": "Conclua no seu navegador",
  "modDetail.viewOnSite": "Ver em {{site}}",

  // ── Configurações ──────────────────────────────────────────────────────────
  "settings.help":
    "Configure sua pasta do jogo, as atualizações e as preferências do app.",
  "settings.gameFolder": "Pasta do jogo",
  "settings.general": "Geral",
  "settings.appearance": "Aparência",
  "settings.frostmod": "FrostMod",
  "settings.about": "Sobre e atualizações",
  "settings.whatsNew": "Novidades",
  "settings.modsFolderDesc":
    "Onde os mods são instalados. Escolha a pasta que contém as pastas mods e profiles \u2014 a de cima de mods, não a pasta mods em si. Mudar isso faz uma nova varredura da biblioteca.",
  "settings.insideModsFolder": "Dentro da sua pasta do {{game}}",
  "settings.notSet": "Não definida",
  "settings.selectFolderFor": "Selecione uma pasta para {{game}}",
  "settings.gameDesc":
    "Qual jogo o MXB App está gerenciando. Suas pastas, sua biblioteca e seus presets pertencem todos ao jogo escolhido aqui.",
  "settings.change": "Alterar…",
  "settings.set": "Definir…",
  "settings.theme": "Tema",
  "settings.themeLight": "Claro",
  "settings.themeDark": "Escuro",
  "settings.themeSystem": "Sistema",
  "settings.language": "Idioma",
  "settings.languageSystem": "Sistema",
  "settings.runInBackground": "Continuar em segundo plano",
  "settings.runInBackgroundDesc":
    "Fechar a janela deixa o MXB App na bandeja do sistema para o FrostMod continuar conectado. Saia pelo ícone da bandeja.",
  "settings.launchAtStartup": "Iniciar junto com o sistema",
  "settings.launchAtStartupDesc":
    "Abrir o MXB App automaticamente quando você fizer login.",
  "settings.instantRefresh": "Atualização instantânea de presets",
  "settings.instantRefreshDesc":
    "Quando você aplica um preset com o {{game}} aberto, atualiza o visual no jogo na hora — sem reiniciar nem reselecionar o perfil. Se não der, você será avisado para selecionar o perfil de novo.",
  "settings.instantRefreshWindowsOnly":
    "Atualizar o visual no jogo sem reiniciar precisa do FrostMod, que só existe no Windows — em vez disso você será avisado para selecionar o perfil de novo.",
  "settings.autoRunFrostmod": "Iniciar o FrostMod automaticamente",
  "settings.autoRunFrostmodDesc":
    "Iniciar o FrostMod em segundo plano sempre que o MXB App abrir.",
  "settings.watchModsReload": "Recarregar automaticamente ao mudar a pasta",
  "settings.watchModsReloadDesc":
    "Recarregar o jogo automaticamente quando pistas ou motos forem adicionadas à sua pasta de mods — mesmo baixadas manualmente fora do MXB App.",
  "settings.checking": "Verificando…",
  "settings.runningConnected": "Em execução · jogo conectado",
  "settings.notRunning": "Não está em execução",
  "settings.frostmodInstalled": "Instalado{{suffix}}",
  "settings.notInstalled": "Não instalado",
  "settings.checkingGitHub": "Verificando a última versão no GitHub…",
  "settings.updateCheckFailed":
    "Não foi possível verificar as atualizações — sem conexão ou GitHub indisponível.",
  "settings.latestVersion": "Última: {{version}}",
  "settings.frostmodRuntimeMissing":
    "Falta ao Windows um componente do Visual C++ que o FrostMod precisa — instale-o para acabar com o erro \"dll was not found\".",
  "settings.frostmodNeedsRepair":
    "Os arquivos instalados não batem com esta versão — reinstalar resolve.",
  "settings.frostmodRepair": "Reparar instalação",
  "settings.frostmodUnsupportedForGame":
    "Esta versão do FrostMod não é segura no {{game}} — atualize para usar o FrostMod aqui.",
  "settings.frostmodUpdateRequired": "Atualização necessária",
  "settings.checkNewer": "Procurar uma versão mais nova do FrostMod",
  "settings.working": "Processando…",
  "settings.installFrostmod": "Instalar o FrostMod",
  "settings.updateTo": "Atualizar para {{version}}",
  "settings.reinstallLatest": "Reinstalar a mais recente",
  "settings.upToDate": "Atualizado",
  "settings.madeWith": "Feito com",
  "settings.updateFailed": "Não foi possível alterar a configuração",
  "settings.startupUpdateFailed":
    "Não foi possível alterar o início automático",
  "settings.folderUpdated": "Pasta do jogo atualizada",
  "settings.folderUpdatedDesc": "Sua biblioteca será varrida de novo.",
  "settings.folderUsedParent":
    "Essa era a pasta mods \u2014 foi usada a pasta acima dela: {{folder}}",
  "settings.setFolderFailed": "Não foi possível definir a pasta",
  "settings.reDetected": "Pasta do {{game}} detectada de novo",
  "settings.detectFolderFailed": "Não foi possível detectar a pasta",
  "settings.pickInstallFolder":
    "Selecione sua pasta de instalação do {{game}} (contém rider.pkz)",
  "settings.installSet": "Instalação do jogo definida",
  "settings.installSetDesc":
    "A prévia 3D do piloto já pode carregar o modelo real do corpo.",
  "settings.setInstallFailed":
    "Não foi possível definir a pasta de instalação",
  "settings.installNotFound": "Não foi possível encontrar o {{game}}",
  "settings.installNotFoundDesc":
    "Nenhuma instalação da Steam detectada — defina a pasta manualmente.",
  "settings.installFound": "Instalação do {{game}} encontrada",
  "settings.detectInstallFailed":
    "Não foi possível detectar a pasta de instalação",
  "settings.pickProfilesFolder":
    "Selecione sua pasta de perfis do {{game}}",
  "settings.profilesSet": "Pasta de perfis definida",
  "settings.profilesFound_one": "{{count}} perfil encontrado.",
  "settings.profilesFound_other": "{{count}} perfis encontrados.",
  "settings.noProfilesThere": "Nenhum perfil encontrado aí",
  "settings.noProfilesThereDesc":
    "Salvamos mesmo assim, mas criar presets precisa de uma pasta que contenha as pastas dos seus profile.ini.",
  "settings.setProfilesFailed":
    "Não foi possível definir a pasta de perfis",
  "settings.profilesReverted": "Voltou para a pasta de perfis padrão",
  "settings.resetProfilesFailed":
    "Não foi possível redefinir a pasta de perfis",
  "settings.frostmodNotRunningHint":
    "O FrostMod não está em execução — inicie ele para recarregar os mods na hora.",
  "settings.reloadUnavailable":
    "Recarregar não está disponível nesta plataforma.",

  // ── Início do jogo ─────────────────────────────────────────────────────────
  "game.play": "Jogar",
  "game.starting": "Iniciando…",
  "game.running": "{{game}} em execução",
  "game.launch": "Iniciar o {{game}}",
  "game.alreadyRunning": "O {{game}} já está em execução",
  "game.launching": "Iniciando o {{game}}…",
  "game.launchFailed": "Não foi possível iniciar o {{game}}",
  "join.title": "Entrar em um servidor",
  "join.desc":
    "Informe o endereço de um servidor para iniciar o {{game}} conectado diretamente a ele.",
  "join.address": "Endereço do servidor",
  "join.action": "Entrar",
  "join.joining": "Conectando…",
  "join.launching": "Conectando a {{address}}…",
  "join.alreadyRunning":
    "Feche o {{game}} primeiro — um jogo em execução não pode ser enviado para um servidor.",
  "join.failed": "Não foi possível entrar nesse servidor",

  "servers.title": "Servidores",
  "servers.subtitle":
    "Gerencie os servidores dedicados que você mantém. Cada um precisa do agente do MXB instalado.",
  "servers.empty": "Nenhum servidor ainda. Adicione um para gerenciá-lo por aqui.",
  "servers.add": "Adicionar servidor",
  "servers.remove": "Remover este servidor",
  "servers.namePlaceholder": "Nome do servidor",
  "servers.tokenPlaceholder": "Token do agente",
  "servers.track": "Pista",
  "servers.slots": "Vagas",
  "servers.uptime": "No ar há",
  "servers.restarts": "Reinícios",
  "servers.stopped": "Parado",
  "servers.start": "Iniciar",
  "servers.stop": "Parar",
  "servers.restart": "Reiniciar",
  "servers.setTrack": "Definir pista",
  "servers.trackPlaceholder": "ID da pista",
  "servers.actionDone": "Pronto",
  "servers.actionFailed": "Não deu certo",
  "servers.trackChanged": "Pista definida como {{track}} — o servidor reiniciou.",
  "servers.saveFailed": "Não foi possível salvar sua lista de servidores",

  "settings.experimental": "Experimental",
  "settings.experimentalServers": "Servidores e sincronização de pinturas",
  "settings.experimentalServersDesc":
    "Inacabado. Adiciona a aba Servidores, deixa você manter servidores dedicados e sincroniza as pinturas para todo mundo aparecer certo no servidor.",
  "settings.experimentalForced":
    "Ativado nesta execução pelo MXB_EXPERIMENTAL — a opção não faz nada enquanto ele estiver definido.",
  "settings.betaBadge": "Beta",

  "sync.title": "Sincronização de pinturas",
  "sync.desc":
    "O MX Bikes nunca envia as pinturas, então os outros pilotos aparecem com as de fábrica se você já não tiver o arquivo exato deles. Publique a sua e baixe a dos outros.",
  "sync.enroll": "Cadastrar",
  "sync.enrolled": "Cadastrado como {{name}}",
  "sync.enrollFailed": "Não foi possível cadastrar",
  "sync.codePlaceholder": "Código de convite",
  "sync.riderNamePlaceholder": "Nome do piloto no jogo",
  "sync.riderNameHint":
    "Precisa ser exatamente igual ao seu nome de piloto no MX Bikes — é assim que os apps dos outros sabem quais pinturas são suas.",
  "sync.ridingAs": "Publicando como {{name}}",
  "sync.pull": "Sincronizar pinturas",
  "sync.setGuid": "Salvar GUID",
  "sync.guidPlaceholder": "Seu GUID do MX Bikes",
  "sync.guidHint":
    "Seu GUID do MX Bikes (opcional). Ele identifica você mesmo se mudar o nome do piloto, e o servidor registra a cada conexão.",
  "sync.guidSaved": "GUID salvo",
  "sync.pulled": "{{installed}} instaladas de {{riders}} pilotos ({{had}} já tinha)",
  "sync.pullFailed": "Não foi possível sincronizar",
  "sync.rejected": "{{count}} ignoradas por destino inseguro",

  // ── Textos que a primeira varredura não pegou (JSX em várias linhas) ──────
  "libraryDetail.noEmbedded": "Nenhum detalhe embutido foi encontrado para este item.",
  "modDetail.downloadFromHost": "Baixar de {{host}}",
  "modDetail.openHost": "Abrir {{host}}",
  "modDetail.thenAddFile": "Depois adicione o arquivo",
  "modDetail.chooseDownloaded": "Escolha o arquivo baixado",
  "presets.chooseProfilesFolder": "Escolher a pasta de perfis…",
  "presets.viewInRider": "Ver no Piloto",
  "presets.noModelSwapsHere": "Nenhuma troca de modelo registrada para esta moto —",
  "presets.setUpInLocker": "configure no Armário",
  "presets.makeActiveBike": "Tornar esta a moto ativa",
  "presets.nameClash":
    "Já existe outro preset chamado “{{name}}” — salvar vai sobrescrever ele também.",
  "presets.shareWarning":
    "Envia para um link público e temporário — isso redistribui arquivos de mods feitos por outras pessoas, então compartilhe com responsabilidade.",
  "settings.profilesDesc":
    "Os presets leem seus perfis daqui — o caminho abaixo é onde o app está olhando agora. É a pasta {{profiles}} dentro da sua pasta do {{game}}, ou {{documents}} se você moveu sua pasta de mods. Defina só se a sua estiver em outro lugar.",
  "settings.resetToDefault": "Restaurar o padrão",
  "settings.gameInstallDesc":
    "Pasta de instalação do jogo (opcional) — onde o {{game}} está instalado (contém {{file}}). Defina para carregar o corpo real do piloto na prévia 3D.",
  "viewer.stockGearNote":
    "Mostrado no {{part}} original do jogo. Uma pintura feita para outro modelo pode não encaixar perfeitamente.",
  "viewer.paintNoChange":
    "Nenhuma das texturas desta pintura é usada pelas peças mostradas aqui, então a prévia não muda. Ela ainda pode pintar as rodas ou a corrente, que esta visão não renderiza.",
  "viewer.noPaintPreview": "Sem prévia da pintura ({{err}})",

  // ── Biblioteca ─────────────────────────────────────────────────────────────
  "library.help":
    "Seus mods instalados. Veja o que está instalado e remova o que não quiser mais.",
  "library.rootFolder": "(raiz)",
  "library.byAuthor": "de {{author}}",
  "library.locked": "Bloqueado — não dá pra ler o conteúdo",
  "library.searchPlaceholder": "Pesquisar entre os instalados…",
  "library.scanning": "Varrendo sua biblioteca…",
  "library.empty":
    "Nenhuma mod de {{type}} instalada — vá em Explorar e adicione uma.",
  "library.noMatches": "Nenhum resultado.",
  "library.quick3d": "Ver em 3D",
  "library.selectNone": "Desmarcar tudo",
  "library.move": "Mover",
  "library.uninstall": "Desinstalar",
  "library.uninstallAction": "Desinstalar…",
  "library.moveToFolder": "Mover para uma pasta…",
  "library.showInExplorer": "Mostrar no Explorador de Arquivos",
  "library.moveDialogTitle": "Mover para uma pasta",
  "library.moveCount_one": "Mover {{count}} item",
  "library.moveCount_other": "Mover {{count}} itens",
  "library.chooseDestination": "Escolha uma pasta de destino",
  "library.newFolder": "Nova pasta…",
  "library.newFolderName": "Nome da nova pasta",
  "library.createAndMove": "Criar e mover",
  "library.confirmUninstall": "Desinstalar {{name}}?",
  "library.confirmUninstallBody":
    "O item vai para a Lixeira — dá pra restaurar de lá.",
  "library.confirmBulkUninstall_one": "Desinstalar {{count}} item?",
  "library.confirmBulkUninstall_other": "Desinstalar {{count}} itens?",
  "library.confirmBulkUninstallBody":
    "Cada item vai para a Lixeira — dá pra restaurar de lá.",
  "library.uninstallCount": "Desinstalar {{count}}",
  "library.moveFailed": "Não foi possível mover o mod",
  "library.uninstallFailed": "Não foi possível desinstalar",
  "library.openFailed": "Não foi possível abrir",
  "library.uninstalledOne": "{{name}} desinstalado",
  "library.movedToBin": "Movido para a Lixeira.",
  "library.someNotRemoved": "Alguns itens não puderam ser removidos.",
  "library.bulkUninstalled_one": "{{count}} item desinstalado",
  "library.bulkUninstalled_other": "{{count}} itens desinstalados",
  "library.bulkUninstallPartial": "{{ok}} desinstalados, {{fail}} falharam",
  "library.bulkMovePartial": "{{ok}} movidos, {{fail}} falharam",
  "library.bulkMoved_one": "{{count}} item movido para {{folder}}",
  "library.bulkMoved_other": "{{count}} itens movidos para {{folder}}",

  // ── Armário ────────────────────────────────────────────────────────────────
  "locker.help":
    "Troque o modelo e o som do motor de cada moto entre os sets que você instalou.",
  "locker.rescan": "Varrer de novo",
  "locker.restore": "Restaurar",
  "locker.register": "Registrar",
  "locker.scanning": "Varrendo as motos…",
  "locker.scanForSwaps": "Procurar sets",
  "locker.orphanBanner":
    "Faltam os arquivos de setup de {{bike}} — uma versão anterior moveu eles para uma pasta de swap, e isso impede a moto de carregar no jogo. {{files}}",
  "locker.looseBanner_one":
    "{{count}} set de modelo / som encontrado solto nas suas motos — registre ele em {{modelsFolder}} / {{soundsFolder}}.",
  "locker.looseBanner_other":
    "{{count}} sets de modelo / som encontrados soltos nas suas motos — registre eles em {{modelsFolder}} / {{soundsFolder}}.",
  "locker.emptyTitle": "Ainda não há motos com troca disponível.",
  "locker.emptyIntro":
    "Duas coisas precisam ser verdade antes de uma troca acontecer:",
  "locker.unpacked": "extraída",
  "locker.emptyRuleUnpacked":
    "A moto está {{unpacked}} em {{path}}— um {{pkz}} compactado não pode ser trocado. Extraia uma pela Biblioteca.",
  "locker.emptyRuleMesh":
    "Cada modelo alternativo fica na própria pasta dentro dessa moto e contém uma malha ({{edf}}). Coloque em qualquer lugar da pasta da moto e clique em Procurar abaixo — vamos oferecer arquivar em {{folder}}.",
  "locker.summary": "{{model}} · som “{{sound}}”",
  "locker.modelNamed": "modelo “{{name}}”",
  "locker.noModelSwaps": "sem trocas de modelo",
  "locker.models": "Modelos",
  "locker.sounds": "Sons",
  "locker.onlyOneModel": "Só um modelo — instale mais para poder trocar",
  "locker.onlyStock": "Só Stock — instale um mod de som para poder trocar",
  "locker.noModel": "Sem modelo",
  "locker.stock": "Stock",
  "locker.activeModel": "Modelo ativo",
  "locker.activeSound": "Som ativo",
  "locker.switchToNoModel":
    "Mudar para nenhum modelo — remove os arquivos do modelo atual",
  "locker.switchToStock":
    "Mudar para Stock — remove o mod de som (toca o som original)",
  "locker.missingModelEdf": "Este set não tem model.edf",
  "locker.missingSoundFiles": "Falta engine.scl ou sfx.cfg neste set",
  "locker.switchTo": "Mudar para {{name}}",
  "locker.tiedToModel": "Vinculado ao modelo {{models}}",
  "locker.boundHint":
    "“{{sound}}” está vinculado ao modelo “{{model}}” — ele acompanha esse modelo. Clique para desvincular.",
  "locker.unboundHint":
    "Vincule o som ativo “{{sound}}” ao modelo “{{model}}” para que mudar pra ele traga o som junto.",
  "locker.tieAction": "Vincular “{{sound}}” a “{{model}}”",
  "locker.untieAction": "Desvincular “{{sound}}” de “{{model}}”",
  "locker.restored": "Arquivos de setup de {{bike}} restaurados.",
  "locker.restoredNote_one":
    "{{count}} arquivo de volta no lugar — a moto deve carregar de novo.",
  "locker.restoredNote_other":
    "{{count}} arquivos de volta no lugar — a moto deve carregar de novo.",
  "locker.switchedModel": "Modelo de {{bike}} mudado para “{{target}}”.",
  "locker.switchedSound": "Som de {{bike}} mudado para “{{target}}”.",
  "locker.tied": "“{{sound}}” vinculado ao modelo “{{model}}”.",
  "locker.untied": "“{{sound}}” desvinculado do modelo “{{model}}”.",
  "locker.refreshedLive": "Atualizado ao vivo dentro do jogo.",
  "locker.refreshFailed":
    "A atualização instantânea falhou — selecione seu perfil de novo no jogo para carregar.",
  "locker.reselectProfile":
    "Selecione seu perfil de novo no MX Bikes para carregar a troca.",
  "locker.loadsNextTime": "Carrega na próxima vez que o jogo abrir.",
  "locker.modelRefreshing":
    "Atualizando no jogo — se for a moto que você tem selecionada, ela muda agora.",
  "locker.modelFrostmodNotRunning":
    "Rode o FrostMod para ver as trocas de modelo ao vivo — por enquanto, selecione a moto de novo no jogo.",
  "locker.modelReselectBike":
    "Modelo trocado — selecione a moto de novo no MX Bikes para vê-lo.",
  "locker.modelFrostmodUnreachable":
    "Não deu pra falar com o FrostMod — selecione a moto de novo no jogo para carregar.",
  "locker.modelRefreshWindowsOnly":
    "A atualização de modelo ao vivo é só no Windows — selecione a moto de novo no jogo.",
  "locker.modelInstantRefreshOff":
    "Selecione a moto de novo no MX Bikes para carregar (a atualização instantânea está desligada).",

  // ── Registro de sets soltos ────────────────────────────────────────────────
  "swaps.model": "modelo",
  "swaps.modelSets_one": "{{count}} troca de modelo",
  "swaps.modelSets_other": "{{count}} trocas de modelo",
  "swaps.soundSets_one": "{{count}} mod de som",
  "swaps.soundSets_other": "{{count}} mods de som",
  "swaps.and": "{{a}} e {{b}}",
  "swaps.noSets": "0 sets",
  "swaps.foundTitle": "Encontrados {{summary}}",
  "swaps.description":
    "Estas pastas estão soltas dentro das suas motos. Registre elas para mover cada uma para a biblioteca certa — {{modelsFolder}} para modelos, {{soundsFolder}} para sons — e aparecerem no Armário.",
  "swaps.registered_one": "{{count}} set registrado.",
  "swaps.registered_other": "{{count}} sets registrados.",
  "swaps.nothingMoved": "Nada foi movido.",
  "swaps.skipped_one": "{{count}} ignorado (nome já em uso).",
  "swaps.skipped_other": "{{count}} ignorados (nomes já em uso).",
  "swaps.foldersCreated_one":
    "Pastas da biblioteca criadas para {{count}} moto.",
  "swaps.foldersCreated_other":
    "Pastas da biblioteca criadas para {{count}} motos.",
  "swaps.foldersCreatedDesc":
    "Suas pastas de modelo / som ficaram onde estavam.",
  "swaps.justCreateFolders": "Só criar as pastas",
  "swaps.registerAndMove": "Registrar e mover",
  "swaps.fileCount_one": "{{count}} arquivo",
  "swaps.fileCount_other": "{{count}} arquivos",

  // ── Instalação ─────────────────────────────────────────────────────────────
  "install.installed": "{{title}} instalado",
  "install.reloadedDesc": "Jogo recarregado pelo FrostMod — já está valendo.",
  "install.addedDesc": "Adicionado à sua biblioteca.",
  "install.failed": "Falha na instalação — {{title}}",
  "install.openModPage": "Abrir a página do mod",
  "install.clickToOpen": "Clique para abrir a página do mod",

  // ── Categorias (singular) ──────────────────────────────────────────────────
  "category.track": "Pista",
  "category.bike": "Moto",
  "category.bikePaint": "Pintura",
  "category.bikeModelSwap": "Troca de modelo",
  "category.sound": "Som",
  "category.helmet": "Capacete",
  "category.helmetPaint": "Pintura do capacete",
  "category.goggles": "Óculos",
  "category.boots": "Botas",
  "category.bootPaint": "Pintura das botas",
  "category.protection": "Proteções",
  "category.protectionPaint": "Pintura das proteções",
  "category.gloves": "Luvas",
  "category.outfit": "Uniforme / kit",
  "category.misc": "Outros",

  // ── Cabeçalhos de seção (plural) ───────────────────────────────────────────
  "section.bikePaint": "Pinturas",
  "section.bikeModelSwap": "Trocas de modelo",
  "section.sound": "Sons",
  "section.helmet": "Capacetes",
  "section.helmetPaint": "Pinturas de capacete",
  "section.boots": "Botas",
  "section.bootPaint": "Pinturas de botas",
  "section.protection": "Proteções",
  "section.protectionPaint": "Pinturas de proteções",
  "section.gloves": "Luvas",
  "section.outfit": "Uniforme / kit",

  // ── Destinos de instalação ─────────────────────────────────────────────────
  "dest.bikesRoot": "Motos (raiz)",
  "dest.tracksRoot": "Pistas (raiz)",
  "dest.bikeFolder": "{{name}} — pasta da moto",
  "dest.bikePaints": "{{name}} — pinturas",
  "dest.helmetsNewModel": "Capacetes (modelo novo)",
  "dest.bootsNewModel": "Botas (modelo novo)",
  "dest.protectionNewModel": "Proteções (modelo novo)",
  "dest.riderModelsNew": "Modelos de piloto (modelo novo)",
  "dest.animationsNewStyle": "Estilos de pilotagem (nova animação)",
  "dest.helmetPaintsFor": "{{name}} · pinturas de capacete",
  "dest.gogglesFor": "{{name}} · óculos",
  "dest.bootPaintsFor": "{{name}} · pinturas de botas",
  "dest.protectionPaintsFor": "{{name}} · pinturas de proteções",
  "dest.outfitFor": "{{name}} · uniforme / kit",
  "dest.suitPaintsFor": "{{name}} · pinturas de macacão",
  "dest.glovesFor": "{{name}} · luvas",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay no jogo",
  "overlay.enable": "Ativar o overlay no jogo",
  "overlay.enableDesc": "Aperte um atalho com o {{game}} aberto para abrir Presets, Locker e Browse por cima do jogo — sem alt-tab. Presets e trocas de modelo valem no jogo em andamento.",
  "overlay.shortcut": "Atalho do overlay",
  "overlay.shortcutDesc": "Funciona mesmo com o jogo em foco. Esc fecha o overlay e devolve o controle.",
  "overlay.borderlessTitle": "Jogue o {{game}} sem bordas ou em janela",
  "overlay.borderlessNote": "Nada é desenhado por cima de um jogo que segura a tela em modo exclusivo — nem o overlay. Deixe o {{game}} em Borderless (ou Windowed) nas Options → Video e ele aparece sobre o jogo como esperado.",
  "overlay.gameRunning": "O {{game}} está aberto",
  "overlay.gameNotRunning": "O {{game}} não está aberto",
  "overlay.showNow": "Mostrar o overlay agora",
  "overlay.showFailed": "Não foi possível abrir o overlay",
  "overlay.hotkeyTaken": "Outro app está usando este atalho",
  "overlay.hotkeyTakenDesc": "A combinação fica com o app que pediu primeiro, então o overlay nunca abre. Escolha outra acima — o mudo do Discord costuma ser o culpado.",
  "overlay.fullscreenNow": "O {{game}} está em tela cheia exclusiva agora",
  "overlay.fullscreenNowDesc": "O overlay abre mesmo assim — é o jogo que é desenhado por cima. Mude para sem bordas ou janela nas Options → Video.",
  "overlay.notWorking": "Apertou e não aconteceu nada?",
  "overlay.notWorkingDesc": "Confira o atalho acima: outro app pode já ter essa combinação, e escolher uma livre é o que resolve.",
  "overlay.pressKeys": "Aperte as teclas…",
  "overlay.needModifier": "Adicione um modificador",
  "overlay.needModifierDesc": "Segure Ctrl, Alt ou Shift para o atalho não disparar enquanto você digita.",
  "overlay.shortcutUpdated": "Atalho do overlay atualizado",
  "overlay.shortcutRejected": "Não deu para usar esse atalho",
  "overlay.registerFailed": "Não foi possível registrar o atalho do overlay",
  "overlay.toClose": "{{hotkey}} para fechar",
  "overlay.closeTitle": "Fechar overlay (Esc)",
  "overlay.openMain": "Abrir o app completo",
  "overlay.openMainTitle": "Fecha o overlay e abre a janela principal do MXB App",
  "overlay.needsSetup": "Termine a configuração do MXB App na janela principal primeiro — ele precisa saber onde fica a sua pasta do {{game}}.",
  "overlay.fullscreenBlocked": "O overlay não aparece por cima da tela cheia exclusiva",
  "overlay.fullscreenBlockedDesc": "Deixe o {{game}} em sem bordas ou em janela nas Options → Video e tente o atalho de novo.",

  // Vitrine da versão — a janela de novidades mostrada uma vez depois de atualizar.
  "showcase.eyebrow": "Recém-atualizado",
  "showcase.title": "Novidades da {{version}}",
  "showcase.subtitle": "A grande primeiro. Todo o resto desta versão está nas notas.",
  "showcase.whileGameRunning": "enquanto o MX Bikes está aberto",
  "showcase.releaseNotes": "Ler as notas da versão",
  "showcase.gotIt": "Entendi",
  "showcase.v080.hero.title": "O MXB App também comanda o GP Bikes",
  "showcase.v080.hero.body":
    "Escolha seu jogo no primeiro início, ou troque quando quiser nas Configurações — o app inteiro acompanha: Biblioteca, Gerenciar, Presets, Jogar e uma aba Explorar servida pelo gpb-mods.com. As pastas de piloto do GP são lidas como do GP, não como as do MX Bikes, e o FrostMod recarrega ao vivo lá também. Cada jogo guarda suas próprias pastas, então sua configuração do MX Bikes fica intacta.",
  "showcase.v080.shop":
    "Uma aba Loja navega pelo mxbikes-shop.com e instala o que você comprou, sem sair do app.",
  "showcase.v080.dropzone":
    "Arraste qualquer coisa para a janela. Ele descobre o que é cada arquivo, mostra para onde vai e o que substituiria, e deixa você remanejar qualquer linha antes de instalar.",
  "showcase.v080.destinations":
    "Os mods caem na pasta que o jogo realmente lê — uma pintura na moto dela, uma pintura de capacete no capacete dele, um macacão de GP no seu modelo de piloto.",
  "showcase.v080.protection":
    "O slot de proteções funciona: cada peça desenhada em pé e inteira, e instalada onde o jogo procura.",
  "showcase.v080.faster":
    "As miniaturas ficam em cache e são desenhadas no tamanho exibido, então Explorar e a Loja abrem bem mais rápido.",
  "showcase.v070.hero.title": "Um overlay dentro do jogo, num atalho",
  "showcase.v070.hero.body": "Abre Preset, Locker e Browse por cima do MX Bikes — sem alt-tab. Esc devolve o controle na hora, e um preset escolhido aqui cai na sessão que você já está pilotando. Jogue sem bordas ou em janela: nada é desenhado por cima da tela cheia exclusiva.",
  "showcase.v070.hero.action": "Configurar o overlay",
  "showcase.v070.languages": "O MXB App fala seis idiomas — escolha o seu em Configurações → Aparência.",
  "showcase.v070.browse": "O Browse ordena pelos mais populares, e os cards mostram a nota em estrelas.",
  "showcase.v070.play": "Um botão Play na barra lateral abre o MX Bikes.",
  "showcase.v070.paint": "As motos voltam a usar a pintura certa — Kawasaki KX e Yamaha YZ corrigidas.",
  "manage.help":
    "O MX Bikes carrega todos os mods da sua pasta ao iniciar. Dê a um preset a pista em que ele corre, clique em Modo corrida e todo o resto sai do caminho — nada é apagado, só vai para uma pasta de espera até você trazer de volta.",
  "manage.tabRace": "Presets de corrida",
  "manage.tabMods": "Mods",
  "manage.disabledCount_one": "{{count}} mod desativado",
  "manage.disabledCount_other": "{{count}} mods desativados",
  "manage.restoreAll": "Ativar tudo",
  "manage.restoreTitle": "Trazer todos os mods de volta?",
  "manage.restoreBody":
    "Todos os {{count}} mods desativados voltam exatamente para as pastas de onde saíram. O MX Bikes vai carregar todos de novo.",
  "manage.restored_one": "{{count}} mod de volta.",
  "manage.restored_other": "{{count}} mods de volta.",
  "manage.applyLookTo": "Aplicar o visual em",
  "manage.applyLookHelp":
    "O modo corrida grava a pintura e o equipamento do preset neste perfil e nesta moto, igual à aba Presets. Deixe um deles vazio para só mover o conteúdo sem mexer no seu visual.",
  "manage.noPresets": "Nenhum preset salvo ainda — crie um na aba Presets.",
  "manage.noContentYet": "Sem conteúdo de corrida — adicione uma pista para usar o modo corrida",
  "manage.noTrack": "Sem pista",
  "manage.pinnedCount_one": "{{count}} fixado",
  "manage.pinnedCount_other": "{{count}} fixados",
  "manage.editContent": "Editar conteúdo",
  "manage.raceMode": "Modo corrida",
  "manage.raceTitle": "Correr com “{{name}}”?",
  "manage.raceBody":
    "Mantém {{keep}} mods e tira {{disable}} do caminho, para o MX Bikes carregar só o conteúdo desta corrida.",
  "manage.raceReEnable_one": "{{count}} mod desativado que este preset precisa volta.",
  "manage.raceReEnable_other": "{{count}} mods desativados que este preset precisa voltam.",
  "manage.raceLook": "A pintura e o equipamento vão para {{bike}} no perfil {{profile}}.",
  "manage.raceNoLook": "Só conteúdo — escolha perfil e moto acima para aplicar o visual também.",
  "manage.raceNoBike":
    "Nenhum mod de moto será mantido — você ficaria só com as motos do jogo. Fixe a moto que você usa em Manter sempre.",
  "manage.raceGameRunning":
    "O MX Bikes está aberto. Arquivos que ele mantém em uso não podem ser movidos — feche o jogo primeiro.",
  "manage.raceUnresolved": "Não instalados, então vão aparecer de fábrica: {{slots}}",
  "manage.raceGo": "Preparar a corrida",
  "manage.raceApplied": "Pronto para correr “{{name}}” — {{count}} mods tirados do caminho.",
  "manage.contentSaved": "Conteúdo de corrida salvo para “{{name}}”.",
  "manage.contentTitle": "Conteúdo de corrida de “{{name}}”",
  "manage.contentBody":
    "A pintura, o equipamento e a troca de modelo do preset são encontrados sozinhos. Aqui fica o resto: a pista, os modelos de equipamento extras que valem a pena manter e os pacotes que uma corrida precisa de qualquer jeito.",
  "manage.paneTracks": "Pistas",
  "manage.paneHelmets": "Capacetes",
  "manage.paneBoots": "Botas",
  "manage.paneProtection": "Proteções",
  "manage.paneKeep": "Manter sempre",
  "manage.paneTracksHint": "A pista (ou pistas) para as quais este preset serve.",
  "manage.paneGearHint":
    "Modelos extras para deixar no seletor do jogo. O equipamento do próprio preset é mantido sozinho — marque aqui o que você ainda quer poder escolher. Tudo o que ficar desmarcado sai do caminho.",
  "manage.paneKeepHint":
    "Mods que continuam ativos aconteça o que acontecer — o pacote OEM, a moto deste preset, um mod de som.",
  "manage.notInstalled": "não instalado",
  "manage.off": "off",
  "manage.enabledOne": "{{name}} ativado.",
  "manage.disabledOne": "{{name}} desativado.",
  "manage.enabledMany_one": "{{count}} mod ativado.",
  "manage.enabledMany_other": "{{count}} mods ativados.",
  "manage.disabledMany_one": "{{count}} mod desativado.",
  "manage.disabledMany_other": "{{count}} mods desativados.",
  "manage.enableShown": "Ativar os exibidos ({{count}})",
  "manage.disableShown": "Desativar os exibidos ({{count}})",
  "manage.noMods": "Nenhum mod instalado ainda.",
  "manage.someFailed_one": "{{count}} mod não pôde ser movido: {{first}}",
  "manage.someFailed_other": "{{count}} mods não puderam ser movidos: {{first}}",
  "manage.deleteTitle": "Excluir {{name}}?",
  "manage.deleteBody": "Vai para a lixeira, então ainda dá para recuperar de lá.",
  "manage.deleted": "{{name}} excluído.",
  "game.label": "Jogo",
  "game.switch": "Trocar de jogo",
  "game.switchFailed": "Não foi possível trocar de jogo",
  "settings.instantRefreshMxOnly": "Somente MX Bikes — {{game}} não recarrega perfis em tempo real.",
  "modType.misc": "Diversos",
  "modType.miscInline": "extras",
  "browseCat.raceTracks": "Pistas de corrida",
  "browseCat.kartTracks": "Pistas de kart",
  "browseCat.others": "Outros",
  "browseCat.riderModels": "Modelos de piloto",
  "browseCat.suitPaints": "Pinturas de macacão",
  "browseCat.helmetModels": "Modelos de capacete",
  "browseCat.plugins": "Plugins",
  "browseCat.tools": "Ferramentas",
  "browseCat.menuBackgrounds": "Planos de fundo do menu",
  "category.animation": "Estilo de pilotagem",
  "section.animation": "Estilos de pilotagem",
  "modDetail.restartHint": "Reinicie o {{game}} para reconhecer os novos {{kind}}.",
  "modDetail.protonHint": "Arquivos do Proton Drive são criptografados, então não podem ser baixados automaticamente.",
  "setup.whichGame": "Qual jogo você está configurando? Você pode adicionar o outro depois.",
  "setup.switchLater": "Você pode trocar de jogo quando quiser nas Configurações.",
  "setup.chooseDifferentGame": "Escolher outro jogo",
  // ── Dropzone ───────────────────────────────────────────────────────────────
  "drop.dropHere": "Solte para instalar",
  "drop.dropHint": "Arquivos, .pkz, pinturas, pastas — qualquer coisa do {{game}}",
  "drop.scanning": "Descobrindo o que é isso…",
  "drop.found_one": "{{count}} item encontrado",
  "drop.found_other": "{{count}} itens encontrados",
  "drop.reviewHint": "Confira os destinos e depois instale.",
  "drop.install_one": "Instalar {{count}}",
  "drop.install_other": "Instalar {{count}}",
  "drop.fileCount_one": "{{count}} arquivo",
  "drop.fileCount_other": "{{count}} arquivos",
  "drop.replaces_one": "Substitui {{count}} arquivo existente",
  "drop.replaces_other": "Substitui {{count}} arquivos existentes",
  "drop.willReplace_one": "{{count}} arquivo existente será substituído",
  "drop.willReplace_other": "{{count}} arquivos existentes serão substituídos",
  "drop.nothingOverwritten": "Nada do que já existe será substituído.",
  "drop.needChoice_one": "{{count}} item ainda precisa de um destino",
  "drop.needChoice_other": "{{count}} itens ainda precisam de um destino",
  "drop.skipped_one": "{{count}} arquivo ignorado",
  "drop.skipped_other": "{{count}} arquivos ignorados",
  "drop.pickDestinationFirst": "Escolha para onde vai antes de instalar.",
  "drop.chooseDestination": "Escolher destino",
  "drop.searchDestinations": "Buscar motos e equipamento…",
  "drop.noDestinations": "Ainda não há nada instalado para colocar isso.",
  "drop.destAsPackaged": "Como veio",
  "drop.include": "Incluir este item",
  "drop.exclude": "Deixar este item de fora",
  "drop.installed_one": "{{count}} item instalado",
  "drop.installed_other": "{{count}} itens instalados",
  "drop.itemFailed": "Não foi possível instalar {{name}}",
  "drop.installFailed": "Falha na instalação",
  "drop.scanFailed": "Não foi possível ler o que você soltou",
  "drop.previewFailed": "Não foi possível verificar esse destino",
  "drop.nothingUsable": "Nada instalável nesse arquivo",
  "drop.kind.modsTree": "Pasta mods",
  "drop.kind.track": "Pista",
  "drop.kind.bike": "Moto",
  "drop.kind.bikePaint": "Pintura",
  "drop.kind.soundSet": "Som",
  "drop.kind.riderGear": "Equipamento",
  "drop.kind.reshadePreset": "Preset do ReShade",
  "drop.kind.unknown": "Desconhecido",
  "drop.reason.modsTree": "Contém uma pasta mods completa",
  "drop.reason.categoryDirs": "Contém pastas de motos/pistas/piloto",
  "drop.reason.paintsBundle": "Contém uma pasta paints",
  "drop.reason.soundMarkers": "engine.scl e sfx.cfg encontrados",
  "drop.reason.trackMarkers": "Arquivos de pista encontrados",
  "drop.reason.trackPackage": "Pista empacotada",
  "drop.reason.bikeConfig": "Configuração de moto encontrada",
  "drop.reason.loosePaint": "Pinturas soltas — nada diz de qual modelo são",
  "drop.reason.gearFolders": "Pastas de equipamento encontradas",
  "drop.reason.riderTexture": "Pinta o corpo do piloto — um equipamento",
  "drop.reason.gearTexture": "Pinta uma peça de equipamento",
  "drop.reason.reshadePreset": "Lista técnicas do ReShade",
  "drop.reason.unrecognised": "Não reconhecido — você precisa colocá-lo",

  // ── ReShade ────────────────────────────────────────────────────────────────
  "settings.reshade": "ReShade",
  "settings.reshadeDesc": "Presets de pós-processamento — como {{game}} aparece na tela.",
  "modType.reshade": "ReShade",
  "modType.reshadeInline": "presets do ReShade",
  "reshade.needsGameFolder":
    "Defina a pasta do {{game}} acima e os presets do ReShade aparecem aqui.",
  "reshade.intro":
    "O ReShade adiciona pós-processamento ao {{game}}. É uma ferramenta gratuita à parte: instale uma vez e depois escolha um preset aqui.",
  "reshade.wrongApi":
    "O ReShade está instalado como {{dll}}, que o {{game}} nunca carrega — ele usa OpenGL. Rode o instalador do ReShade de novo e escolha OpenGL.",
  "reshade.step1": "Baixe o instalador em reshade.me.",
  "reshade.step2": "Execute e escolha {{exe}} na sua pasta do {{game}}.",
  "reshade.step3": "Escolha OpenGL quando perguntar — não DirectX.",
  "reshade.getIt": "Obter o ReShade",
  "reshade.recheck": "Verificar de novo",
  "reshade.installed": "Instalado",
  "reshade.installedVersion": "Instalado · {{version}}",
  "reshade.off": "Desligado — sem efeitos",
  "reshade.delete": "Excluir preset",
  "reshade.deleted": "{{name}} excluído",
  "reshade.applied": "{{name}} está ativo agora",
  "reshade.appliedNextLaunch": "{{name}} está definido — vale na próxima vez que abrir",
  "reshade.loosePreset": "Na sua pasta do jogo — não foi o MXB App que instalou",
  "reshade.missingEffects_one": "Precisa de {{list}}, que não está instalado",
  "reshade.missingEffects_other":
    "Precisa de {{count}} efeitos que não estão instalados: {{list}}",
  "reshade.noShaders":
    "Nenhum efeito do ReShade está instalado, então os presets não vão mudar nada. Rode o instalador do ReShade de novo e escolha um pacote de shaders.",
  "reshade.noPresets":
    "Nenhum preset ainda — instale alguns em Explorar, ou solte um .ini aqui.",
  "reshade.browseHint": "Mais presets em Explorar → ReShade.",
  "reshade.nextLaunchHint":
    "{{game}} está aberto — a mudança vale na próxima vez que abrir.",
  // ── Paint studio ───────────────────────────────────────────────────────────
  "paints.help":
    "Transforma arquivos .tga ou .png feitos no GIMP ou Photoshop em um .pnt que o jogo carrega — e descompacta uma pintura existente para usar como base.",
  "paints.unpack": "Descompactar uma pintura…",
  "paints.unpacked": "{{count}} texturas extraídas — edite e depois salve.",
  "paints.whereTitle": "Onde vai",
  "paints.kind.bike": "Pintura da moto",
  "paints.kind.helmet": "Capacete",
  "paints.kind.goggles": "Óculos",
  "paints.kind.boots": "Botas",
  "paints.kind.protection": "Proteções",
  "paints.kind.kit": "Kit do piloto",
  "paints.kind.gloves": "Luvas",
  "paints.model": "Para",
  "paints.profile": "Perfil do piloto",
  "paints.noModels": "Ainda não há nada instalado para pintar.",
  "paints.destPath": "Instala em mods/{{rel}}",
  "paints.saveElsewhere": "Salvar em uma pasta…",
  "paints.saveTitle": "Nome e salvamento",
  "paints.namePlaceholder": "Dê um nome a esta pintura…",
  "paints.save": "Salvar pintura",
  "paints.saved": "Salva em {{path}}",
  "paints.preview3d": "Ver em 3D",
  "paints.openFolder": "Abrir pasta",
  "paints.sheetsTitle": "Texturas",
  "paints.reload": "Recarregar do disco",
  "paints.addImages": "Adicionar imagens…",
  "paints.expected": "As pinturas daqui usam:",
  "paints.empty":
    "Adicione um .tga ou .png para cada textura. O que importa são os nomes, não os arquivos: uma textura chamada “livery” vai para a peça que pede “livery”. Descompactar uma pintura existente dá os nomes certos.",
  "paints.resized": "Redimensionada {{from}} → {{to}} — o jogo exige potências de dois.",
  "paints.unknownName": "Nenhuma pintura aqui usa este nome — pode não aparecer no modelo.",
  "paints.needSheets": "Adicione ao menos uma imagem.",
  "paints.needName": "Dê um nome a esta pintura.",
  "paints.needTextureNames": "Cada textura precisa de um nome.",
  "paints.duplicateName": "Duas texturas se chamam “{{name}}”.",
  "paints.needTarget": "Escolha para onde vai a pintura.",
  "paints.replaceTitle": "Substituir esta pintura?",
  "paints.replaceBody": "{{path}} já existe. Salvar substitui o arquivo.",
  "paints.replace": "Substituir",
};
