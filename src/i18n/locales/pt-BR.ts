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
  "frostmod.installedToast": "FrostMod {{version}} instalado",
  "frostmod.installedToastDesc":
    "Ele recarrega o jogo na hora quando você adiciona mods.",
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
  "setup.tagline":
    "Explore o mxb-mods, instale com um clique e deixe o FrostMod recarregar o jogo pra você.",
  "setup.modsFolder": "Pasta do MX Bikes",
  "setup.autoDetect":
    "O MXB App vai detectar sua pasta {{hint}} automaticamente. Você também pode escolher você mesmo.",
  "setup.chooseManually": "Escolher a pasta manualmente…",
  "setup.chooseDifferent": "Escolher outra pasta…",
  "setup.gameInstall": "Instalação do MX Bikes",
  "setup.detecting": "Procurando sua instalação do MX Bikes…",
  "setup.found": "Encontrada",
  "setup.detectedAutomatically": "Detectada automaticamente",
  "setup.installNotFound":
    "Não deu pra encontrar sua instalação do MX Bikes automaticamente — é ela que alimenta a prévia 3D do piloto. Escolha manualmente, ou defina depois nas Configurações.",
  "setup.chooseInstallManually":
    "Escolher a pasta de instalação manualmente…",
  "setup.startBrowsing": "Começar a explorar mods",
  "setup.detectAndStart": "Detectar e começar",
  "setup.pickModsFolder": "Selecione sua pasta do MX Bikes",
  "setup.pickInstallFolder": "Selecione sua pasta de instalação do MX Bikes",

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

  // ── Tour guiado ────────────────────────────────────────────────────────────
  "tour.welcomeTour.title": "Faça um tour rápido",
  "tour.welcomeTour.body":
    "Alguns segundos para ver onde fica cada coisa. Você pode pular quando quiser.",
  "tour.browse.title": "Explorar mods",
  "tour.browse.body":
    "Pesquise no mxb-mods.com aqui mesmo e instale qualquer pista, moto ou pintura com um clique só.",
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
  "update.checkFailed": "Não foi possível verificar as atualizações",
  "update.failed": "A atualização falhou",

  // ── Visualizador 3D ────────────────────────────────────────────────────────
  "viewer.preview3d": "Prévia 3D",
  "viewer.expand": "Expandir",
  "viewer.paint": "Pintura",
  "viewer.loadingModel": "Carregando o modelo…",
  "viewer.loadingPaint": "Carregando a pintura…",
  "viewer.loadingRider": "Carregando o piloto…",
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
  "browse.sortedByNewest": "Ordenados pelos mais recentes",
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

  // ── Loja ───────────────────────────────────────────────────────────────────
  "shop.myDownloads": "Meus downloads",
  "shop.signInTitle": "Entre na MX Bikes Shop",
  "shop.signInBody":
    "Entre no mxbikes-shop.com para ver e instalar as pistas que você comprou. A gente abre o site de verdade — sua senha nunca passa por este app.",
  "shop.signIn": "Entrar",
  "shop.logOut": "Sair",
  "shop.signedIn": "Conectado à MX Bikes Shop",
  "shop.sessionFailed": "Não foi possível capturar sua sessão da MX Bikes Shop",
  "shop.queuedDesc": "Instalando na sua pasta de pistas.",
  "shop.loadFailed": "Não foi possível carregar seus downloads: {{error}}",
  "shop.empty": "Nenhum download comprado encontrado na sua conta ainda.",

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
  "modDetail.noDownloadLink":
    "Nenhum link de download encontrado nesta página — abra no mxb-mods.com.",
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
  "modDetail.viewOnSite": "Ver no mxb-mods.com",

  // ── Configurações ──────────────────────────────────────────────────────────
  "settings.help":
    "Configure sua pasta do jogo, as atualizações e as preferências do app.",
  "settings.gameFolder": "Pasta do jogo",
  "settings.general": "Geral",
  "settings.appearance": "Aparência",
  "settings.frostmod": "FrostMod",
  "settings.about": "Sobre e atualizações",
  "settings.modsFolderDesc":
    "Onde os mods são instalados. Mudar isso faz uma nova varredura da biblioteca.",
  "settings.insideModsFolder": "Dentro da sua pasta do MX Bikes",
  "settings.notSet": "Não definida",
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
    "Quando você aplica um preset com o MX Bikes aberto, atualiza o visual no jogo na hora — sem reiniciar nem reselecionar o perfil. Se não der, você será avisado para selecionar o perfil de novo.",
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
  "settings.setFolderFailed": "Não foi possível definir a pasta",
  "settings.reDetected": "Pasta do MX Bikes detectada de novo",
  "settings.detectFolderFailed": "Não foi possível detectar a pasta",
  "settings.pickInstallFolder":
    "Selecione sua pasta de instalação do MX Bikes (contém rider.pkz)",
  "settings.installSet": "Instalação do jogo definida",
  "settings.installSetDesc":
    "A prévia 3D do piloto já pode carregar o modelo real do corpo.",
  "settings.setInstallFailed":
    "Não foi possível definir a pasta de instalação",
  "settings.installNotFound": "Não foi possível encontrar o MX Bikes",
  "settings.installNotFoundDesc":
    "Nenhuma instalação da Steam detectada — defina a pasta manualmente.",
  "settings.installFound": "Instalação do MX Bikes encontrada",
  "settings.detectInstallFailed":
    "Não foi possível detectar a pasta de instalação",
  "settings.pickProfilesFolder":
    "Selecione sua pasta de perfis do MX Bikes",
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
  "game.running": "MX Bikes em execução",
  "game.launch": "Iniciar o MX Bikes",
  "game.alreadyRunning": "O MX Bikes já está em execução",
  "game.launching": "Iniciando o MX Bikes…",
  "game.launchFailed": "Não foi possível iniciar o MX Bikes",

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
    "Os presets leem seus perfis daqui — o caminho abaixo é onde o app está olhando agora. É a pasta {{profiles}} dentro da sua pasta do MX Bikes, ou {{documents}} se você moveu sua pasta de mods. Defina só se a sua estiver em outro lugar.",
  "settings.resetToDefault": "Restaurar o padrão",
  "settings.gameInstallDesc":
    "Pasta de instalação do jogo (opcional) — onde o MX Bikes está instalado (contém {{file}}). Defina para carregar o corpo real do piloto na prévia 3D.",
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
  "library.quick3d": "Prévia 3D rápida",
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
  "dest.helmetPaintsFor": "{{name}} · pinturas de capacete",
  "dest.gogglesFor": "{{name}} · óculos",
  "dest.bootPaintsFor": "{{name}} · pinturas de botas",
  "dest.protectionPaintsFor": "{{name}} · pinturas de proteções",
  "dest.outfitFor": "{{name}} · uniforme / kit",
  "dest.glovesFor": "{{name}} · luvas",

  // In-game overlay — the hotkey panel drawn over MX Bikes.
  "overlay.section": "Overlay no jogo",
  "overlay.enable": "Ativar o overlay no jogo",
  "overlay.enableDesc": "Aperte um atalho com o MX Bikes aberto para abrir Presets, Locker e Browse por cima do jogo — sem alt-tab. Presets e trocas de modelo valem no jogo em andamento.",
  "overlay.shortcut": "Atalho do overlay",
  "overlay.shortcutDesc": "Funciona mesmo com o jogo em foco. Esc fecha o overlay e devolve o controle.",
  "overlay.borderlessNote": "Deixe o MX Bikes em sem bordas ou em janela nas Options → Video. Nada é desenhado por cima de um jogo em tela cheia exclusiva — nem o overlay.",
  "overlay.pressKeys": "Aperte as teclas…",
  "overlay.needModifier": "Adicione um modificador",
  "overlay.needModifierDesc": "Segure Ctrl, Alt ou Shift para o atalho não disparar enquanto você digita.",
  "overlay.shortcutUpdated": "Atalho do overlay atualizado",
  "overlay.shortcutRejected": "Não deu para usar esse atalho",
  "overlay.registerFailed": "Não foi possível registrar o atalho do overlay",
  "overlay.toClose": "{{hotkey}} para fechar",
  "overlay.closeTitle": "Fechar overlay (Esc)",
  "overlay.needsSetup": "Termine a configuração do MXB App na janela principal primeiro — ele precisa saber onde fica a sua pasta do MX Bikes.",
  "overlay.fullscreenBlocked": "O overlay não aparece por cima da tela cheia exclusiva",
  "overlay.fullscreenBlockedDesc": "Deixe o MX Bikes em sem bordas ou em janela nas Options → Video e tente o atalho de novo.",
};
