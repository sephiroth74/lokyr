# The quick tips' texts, in Italian.

command-quick-tips-show = Mostra un suggerimento

card-title = Lo sapevi?
card-dismiss = Chiudi
card-more = Altro
card-never = Non mostrare più

tip-palette = Premi { $keys } per la palette dei comandi, che esegue qualsiasi comando dal suo nome.
tip-search = Premi { $keys } per cercare un testo nel pannello, anche nello scrollback.
tip-split = Premi { $keys } per dividere il pannello; trascina il divisore per ridimensionarlo, fai doppio clic per pareggiare le metà.
tip-switch-tab = Premi { $keys } per tornare al tab che hai usato prima. Tieni premuto Control e premi di nuovo Tab per andare più indietro.
tip-reopen-tab = Hai chiuso un tab per sbaglio? Premi subito { $keys } per riaverlo com'era, scrollback compreso.
tip-next-agent = Premi { $keys } per andare al prossimo agente AI che ti aspetta, in qualsiasi finestra.
tip-cycle-windows = Premi { $keys } per passare alla finestra successiva, anche a quella delle impostazioni, come fa Finestra › Passa alla finestra successiva.
tip-claude-code = Lokyr › Installa l'integrazione con Claude Code… fa dire a Claude Code alla sua scheda esattamente quando lavora, ti aspetta o ha finito.
tip-agents-list = Premi { $keys } per vedere tutti gli agenti AI di tutte le finestre, prima quelli che ti aspettano.
tip-agents = Una scheda segna l'agente AI che contiene, come Claude Code: una scintilla mentre lavora, un punto mentre ti aspetta, con la sua richiesta nel tooltip della scheda, una spunta quando ha finito.
tip-agent-status = Gli agenti che dichiarano il loro stato, come Claude Code e Codex, lo mostrano esatto sulla loro scheda: che cosa chiedono, il loro messaggio e il loro avanzamento, che la configurazione li elenchi o no.
tip-prompts = Premi { $keys } per tornare al prompt del comando precedente.
tip-copy-mode = Premi { $keys } per la modalità copia: i tasti di vi spostano e selezionano, senza il mouse.
tip-zoom = Premi { $keys } per ingrandire il pannello a tutto il tab, e di nuovo per rimetterlo a posto.
tip-focus-pane = Premi { $keys }, o lo stesso con un'altra freccia, per passare la tastiera al pannello accanto.
tip-goto-tab = Premi { $keys } e i numeri seguenti per andare dritto a un tab dalla sua posizione.
tip-quick-terminal = Premi { $keys } da qualsiasi app per far scendere il terminale rapido.
tip-double-tap = Metti [plugins.quick-terminal] double-tap = "ctrl" nella config, poi premi Control due volte, da solo, per mostrare il terminale rapido da qualsiasi app.
tip-clear = Premi { $keys } per pulire lo scrollback e lo schermo, tenendo il prompt.
tip-font-size = Premi { $keys } per ingrandire il testo, in ogni finestra.

tip-links = Tieni premuto ⌘ su un link per sottolinearlo, poi fai clic per aprirlo.
tip-prompt-click = Fai clic nella riga di comando per spostarci il cursore, in zsh, bash e fish; selezionane una parte e premi Backspace, o scrivi, per sostituirla.
tip-copy-output = Premi { $keys } per copiare l'output dell'ultimo comando, senza il prompt; il menu contestuale copia quello di qualsiasi comando.
tip-editor-line = Imposta editor in [links] a "vscode", "cursor" o "zed", e ⌘-clic su src/main.rs:42 apre il tuo editor alla riga 42.
tip-block-header = Sei risalito in un output lungo? Il prompt del comando resta in cima al pannello: fai clic per tornare al comando.
tip-block-copy = Fai clic destro sull'output di un comando e apri Blocco per copiare il comando, il suo output o entrambi, o per selezionarne l'output o tutto il blocco. Lo stesso menu si apre col clic destro sulle righe fissate in cima.
tip-block-details = Fai clic destro sull'output di un comando e scegli Blocco › Dettagli del blocco per vederne lo stato d'uscita, l'ora d'inizio, la durata e la cartella in cui è stato eseguito.
tip-select-block = Fai clic sul blocco di un comando, o premi { $keys } per l'ultimo, per selezionarlo: ↑ e ↓ passano agli altri, ⌘C lo copia ed Esc termina la selezione.
tip-block-rerun = Fai clic destro sull'output di un comando e scegli Blocco › Esegui di nuovo il comando per eseguirlo un'altra volta al prompt.
tip-block-spacing = Vuoi più spazio fra i comandi? Imposta spacing = true in [plugins.blocks] per una riga vuota prima di ogni prompt.
tip-tab-drag = Trascina un tab fuori dalla barra per una finestra tutta sua, o su un pannello per affiancarlo.
tip-tab-menu = Fai clic destro su un tab per dargli un colore, o doppio clic per rinominarlo.
tip-pin-tab = Fai clic destro su un tab e scegli Fissa tab per tenerlo stretto all'inizio della barra. Né ⌘W né Chiudi gli altri tab lo chiudono.
tip-tab-groups = Fai clic destro su un tab e scegli Nuovo gruppo con il tab per raccogliere i tab sotto un nome e un colore. Fai clic sulla pillola del gruppo per comprimerlo, o trascina un tab subito dopo per metterlo dentro.
tip-restore-scrollback = Imposta restore-windows = "always" e restore-scrollback = true nella sezione [app] della config: le finestre si riaprono come le hai lasciate, e ogni pane mostra ciò che mostrava quando sei uscito, comandi compresi.
tip-hints = Premi { $keys } per etichettare link, percorsi e hash sullo schermo, poi scrivi un'etichetta per aprirlo; in maiuscolo lo copi.
tip-themes = Vista › Scegli tema… mostra in anteprima ogni tema mentre scorri l'elenco.
tip-themes-folder = Metti un tema .toml o .itermcolors nella cartella che Vista › Apri la cartella dei temi mostra nel Finder, e Scegli tema… lo elenca.
tip-new-theme = Nuovo… nella pagina Temi crea un tema con tutte le chiavi di quelli di Lokyr; Vista › Nuovo tema dall'attuale… uno dal tema in uso, e il menu contestuale di una scheda duplica qualsiasi tema.
tip-import-themes = Vista › Importa temi…, o Importa… nella pagina Temi, copia i temi .itermcolors e .toml nella tua cartella dei temi, anche più di uno alla volta.
tip-long-command = Un comando lungo ti avvisa quando finisce mentre Lokyr è in secondo piano: [notifications] long-command nel config dice quanto lungo.
tip-look-up = Fai clic destro su una parola per cercarla nel dizionario, sul web o nelle pagine di man.
tip-services = Nel Finder, fai clic destro su una cartella e scegli Servizi › Nuovo tab Lokyr qui per un terminale lì.
tip-itermcolors = Fai doppio clic su un file .itermcolors, o trascinalo sull'icona di Lokyr nel Dock, per installarne il tema.
tip-float = Finestra › Sempre in primo piano tiene la finestra sopra quelle di ogni altra app.
tip-smooth-scroll = Imposta smooth-scroll = true in [window] nella config, e il trackpad scorre il testo a pixel, continuando a scivolare quando alzi le dita; anche lo scatto della rotella del mouse scivola.
tip-cursor-guide = Premi { $keys }, o Vista › Mostra la guida del cursore, per una banda sulla riga del cursore che mostra dov'è.
tip-transparency = Premi { $keys }, o Vista › Usa la trasparenza, per mostrare opaca una finestra traslucida mentre leggi, e di nuovo per vederci attraverso; la config resta com'è.
tip-sleip = I plugin vengono dal registro: `sleip search` li elenca, e `sleip add` ne installa uno.
tip-settings-window = Lokyr › Apri la finestra delle impostazioni mostra ogni impostazione per sezione, anche quelle dei plugin, con cosa fa, e la cambia nel tuo file di configurazione, commenti compresi.
tip-settings-keys = Nella pagina Tasti della finestra delle impostazioni, scrivi nel filtro per trovare una scorciatoia, fai clic sui suoi tasti e premi quelli nuovi, o Aggiungi una scorciatoia tua: ci sono tutte quelle predefinite, e Ripristina ne rimette una.
tip-settings-themes = La pagina Temi della finestra delle impostazioni mostra ogni tema con un'anteprima dei suoi colori: un clic lo usa.
tip-theme-editor = Editor, nella pagina Temi della finestra delle impostazioni, mostra ogni colore di un tema e da dove viene, e modifica i tuoi: fai clic sulla riga di un colore per scriverne il valore, o sul suo campione per sceglierlo nel pannello dei colori. Modifica, nel menu contestuale della scheda di un tema, mostra quel tema lì.
tip-settings-search = Nella finestra delle impostazioni, scrivi nel campo di ricerca in cima alla barra laterale per trovare un'impostazione per nome, per cosa fa o per i tasti di una scorciatoia.
tip-settings-plugin-on = Attiva o disattiva un plugin incluso, come Blocks, con l'interruttore Attivo in cima alla sua sezione nelle Impostazioni.
tip-plugins = Lokyr › Plugin… elenca ogni plugin, il suo stato e cosa può fare, lo disattiva, lo aggiorna o lo rimuove, e installa quelli del registro.

tip-plugin-keys = Premi { $keys } per «{ $title }», un comando di un plugin installato.
tip-plugin-palette = «{ $title }», un comando di un plugin installato, si esegue dalla palette dei comandi: premi { $keys } e scrivine il nome.
