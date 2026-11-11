# The app's texts, in Italian.

command-window-new = Nuova finestra
command-window-close = Chiudi finestra
command-window-set-title = Imposta titolo…
command-window-goto = Vai alla finestra
command-window-toggle-float-on-top = Sempre in primo piano
command-app-about = Informazioni su Lokyr
command-app-quit = Esci da Lokyr
command-app-check-for-updates = Controlla aggiornamenti…
command-app-settings = Impostazioni…
command-app-report-issue = Segnala un problema…
command-app-secure-keyboard-entry = Immissione protetta da tastiera
command-app-make-default-terminal = Rendi Lokyr il terminale predefinito
command-app-install-command-line-tools = Installa gli strumenti da riga di comando…
command-app-performance = Mostra prestazioni
performance-measuring = Misurazione…
performance-frames = { $rate } FPS
performance-cpu = CPU { $median } ms, al più { $longest }
performance-wait = Attesa dello schermo { $median } ms, al più { $longest }
performance-gpu = GPU { $mean } ms, al più { $longest }
performance-wakeups = { $wakeups } risvegli/s, { $idle } dall'inattività
performance-memory = Memoria { $memory } MB
performance-output = Output { $output } MB/s
command-appearance-system = Aspetto automatico
command-appearance-light = Aspetto chiaro
command-appearance-dark = Aspetto scuro
command-pane-look-up = Cerca info
command-pane-man-page = Apri pagina man
command-pane-man-search = Cerca nell'indice delle pagine man
command-pane-search-web = Cerca sul web

# The notice of a plugin that a fault stopped, by its name.
plugin-stopped = Il plugin { $name } si è fermato, e le sue viste restano com'erano, senza rispondere ai clic: { $fault }
plugin-capability-withheld = Il plugin { $name } chiede { $capability }, che nessun plugin ha ancora: funziona senza.
plugin-capability-first-party = Il plugin { $name } chiede { $capability }, che hanno solo i plugin di Lokyr: funziona senza.
plugin-not-loaded = Il plugin { $name } non si carica: { $why }
plugin-other-api = Il plugin { $name } usa l'API dei plugin { $api }, e questo Lokyr ha la { $version }: non si carica finché non si aggiorna il plugin o Lokyr.
plugins-lock-unread = I plugin installati non si caricano, perché plugins.lock non si legge: { $error }

# The question of the capabilities of a plugin that activates, which the user
# allows or not: a line of each capability, what it allows and its risk.
consent-message = Consentire a { $name } di usare queste capability?
consent-detail = Lokyr lo chiede una volta sola: la risposta resta in permissions.toml, accanto al file di configurazione, dove si può cambiare.
consent-allow = Consenti
consent-refuse = Non consentire
consent-line = • { $what }: { $risk }
consent-risk-very-high = rischio molto alto
consent-risk-high = rischio alto
consent-risk-medium = rischio medio
consent-risk-low = rischio basso
consent-capability-terminal-read = leggere il testo dello schermo e dello scrollback, e la selezione
consent-capability-pty-write = scrivere nei programmi dei pannelli, come nella shell
consent-capability-clipboard-read = leggere gli appunti
consent-capability-clipboard-write = scrivere negli appunti
consent-capability-notifications = mostrare notifiche
consent-capability-open = aprire pagine web e file con le loro app
consent-capability-process-spawn = avviare programmi
consent-capability-net = connettersi a { $parameter }
consent-capability-fs-read = leggere { $parameter } e quello che contiene
consent-capability-fs-write = scrivere in { $parameter } e in quello che contiene
consent-capability-global-hotkey = prendere tasti in ogni app, e far scorrere finestre rapide sopra di esse
consent-capability-env-read = leggere la variabile d'ambiente { $parameter }
consent-capability-env-read-any = leggere tutte le variabili d'ambiente
consent-capability-input-read = vedere i tasti premuti prima dei programmi
consent-capability-history-read = leggere i comandi eseguiti dalla shell, e il loro output
consent-capability-process-read = vedere i programmi dei pannelli, le loro righe di comando e cartelle
consent-capability-user-commands = eseguire qualsiasi comando come faresti tu, anche scrivere nella shell
consent-capability-workspace-write = aprire, chiudere e spostare finestre, tab e pannelli, e uscire dall'app
consent-capability-pane-view = scorrere e selezionare nei pannelli, cancellarli, e cambiare titoli e colori dei tab
consent-capability-decorate = disegnare sopra il testo dei pannelli
consent-capability-pointer-read = vedere dove si trova il puntatore sui pannelli
consent-capability-keep-awake = tenere svegli il Mac e lo schermo, il che impedisce il blocco dello schermo e consuma la batteria
consent-capability-packages = installare, aggiornare e rimuovere plugin, che eseguono codice proprio
permissions-unread = Le decisioni sulle capability dei plugin non si leggono, e nessuna si applica: { $error }

# The credits of the About panel, under the version and the copyright: the
# license, and the links to the source code and to the licenses' notices of
# the software that Lokyr includes.
about-license = Distribuito con licenza MIT.
about-repository = Codice sorgente su GitHub
about-licenses = Licenze del software incluso

menu-shell = Shell
menu-edit = Composizione
menu-view = Vista
menu-window = Finestra
menu-help = Aiuto
menu-hide = Nascondi Lokyr
menu-hide-others = Nascondi altre
menu-show-all = Mostra tutte
menu-minimize = Contrai
menu-zoom = Zoom
menu-bring-all-to-front = Porta tutto in primo piano
menu-emoji-symbols = Emoji e simboli
menu-appearance = Aspetto
menu-appearance-system = Automatico
menu-appearance-light = Chiaro
menu-appearance-dark = Scuro
title-buttons-more = Altro
save-panel-message = { $name } salva questo file.
save-failed = Impossibile salvare { $path }: { $error }
menu-look-up = Cerca info su “{ $word }”
menu-man-page = Apri la pagina man per “{ $page }”
menu-man-search = Cerca “{ $words }” nell'indice delle pagine man
menu-search-web = Cerca “{ $text }” sul web

clipboard-read-message = Un programma in questa finestra vuole leggere gli appunti.
clipboard-read-detail = Riceverebbe il testo che hai copiato per ultimo, che può contenere una password. Consentilo solo se ti fidi del programma e del computer su cui gira.
clipboard-read-deny = Non consentire
clipboard-read-allow = Consenti

paste-lines-message = { $lines ->
    [one] Incollare una riga con il suo a capo?
   *[other] Incollare { $lines } righe?
}
paste-lines-detail = Il programma in questa finestra riceve il testo incollato come se fosse digitato: ogni a capo gli arriva come Invio, che può eseguire un comando.
paste-lines-paste = Incolla
paste-lines-one-line = Incolla su una riga
paste-lines-cancel = Annulla
paste-lines-dont-ask = Non chiedere più

close-running-message = Chiudere questa finestra?
close-running-detail = In questa finestra è in esecuzione { $program }: chiuderla lo termina.
close-running-close = Chiudi
close-running-cancel = Annulla
close-tab-running-message = Chiudere questo tab?
close-tab-running-detail = In questo tab è in esecuzione { $program }: chiuderlo lo termina.
close-pane-running-message = Chiudere questo pannello?
close-pane-running-detail = In questo pannello è in esecuzione { $program }: chiuderlo lo termina.

set-title-message = Il titolo della finestra
set-title-detail = Lascialo vuoto per quello della finestra: il titolo del programma, o la sua cartella, poi il programma in esecuzione.
set-title-set = Imposta
set-title-cancel = Annulla
ask-text-ok = OK
ask-text-cancel = Annulla

settings-failed-message = Lokyr non riesce ad aprire il file di configurazione
settings-no-home = Senza una cartella home non c'è un file di configurazione: valgono i valori predefiniti.
quit-running-message = Uscire da Lokyr?
quit-running-detail = { $windows ->
    [one] In una finestra è in esecuzione { $programs }: uscire lo termina.
   *[other] In { $windows } finestre sono in esecuzione dei programmi: { $programs }. Uscire li termina.
}
quit-running-quit = Esci
quit-running-cancel = Annulla
program-unknown = un programma

open-url-message = Aprire questo link?
open-file-message = Aprire questo file?
open-open = Apri
open-reveal = Mostra nel Finder
open-cancel = Annulla
open-url-dont-ask = Non chiedere più per i link
open-file-dont-ask = Non chiedere più per i file

notice-config = Il file di configurazione non si applica:
notice-font = Manca un font:
notice-global-keys = Un tasto globale non funziona:
notice-term = Il tipo di terminale è sconosciuto:
notice-theme = Un tema non si applica:
notice-plugin-config = La sezione di un plugin nella configurazione:
notice-kept-windows = Le finestre dell'ultima sessione non si riaprono:
notice-permissions = I permessi dei plugin non si leggono:
notice-installed-plugins = I plugin installati non si caricano:
notice-failed = Un comando non è riuscito:
notice-fault = Un plugin si è fermato:

# What an accessibility client, as VoiceOver, calls a pane.
access-pane = Terminale

theme-install-message = Installare il tema «{ $name }»?
theme-install-detail = Lokyr lo copia nella sua cartella dei temi, { $folder }, dove la sezione [theme] della configurazione può nominarlo.
theme-install-replace = Un tema con questo nome c'è già, e questo lo sostituisce.
theme-install-use = Installa e usa
theme-install-only = Installa
theme-install-cancel = Annulla
theme-install-variant = { $variant ->
    [light] È un tema chiaro: Lokyr lo usa con l'aspetto chiaro, che ora non è attivo.
   *[dark] È un tema scuro: Lokyr lo usa con l'aspetto scuro, che ora non è attivo.
}
theme-install-switch = { $variant ->
    [light] Passa Lokyr all'aspetto chiaro
   *[dark] Passa Lokyr all'aspetto scuro
}
theme-install-failed = Il tema non si può installare.
theme-install-no-folder = Lokyr non ha una cartella della configurazione che tenga i suoi temi.
theme-install-not-copied = Non si può copiare in { $folder }: { $error }

command-succeeded = Riuscito dopo { $took }
command-failed = Fallito con stato { $status } dopo { $took }
command-ended = Terminato dopo { $took }
command-took-seconds = { $seconds } s
command-took-minutes = { $minutes } min { $seconds } s
command-took-hours = { $hours } h { $minutes } min

script-downloaded-message = “{ $name }” è stato scaricato da internet.
script-downloaded-detail = Eseguire uno script scaricato da internet può danneggiare il Mac. Eseguilo solo se ti fidi della sua provenienza.
script-downloaded-run = Esegui
script-downloaded-cancel = Annulla

tools-reason = Lokyr vuole collegare lokyr e sleip in { $folder }.
tools-linked-message = Gli strumenti da riga di comando sono installati
tools-linked-detail = Gli altri terminali e gli script ora eseguono lokyr e sleip, collegati in { $value }.
tools-there-message = Gli strumenti da riga di comando sono già installati
tools-there-detail = lokyr e sleip in { $value } sono quelli di questo Lokyr.
tools-replace-message = Sostituire quello che c'è in { $folder }?
tools-replace-link = { $path } è un collegamento a { $target }.
tools-replace-file = { $path } è un file di un altro programma.
tools-replace-replace = Sostituisci
tools-replace-cancel = Annulla
tools-folder-message = { $value } è una cartella
tools-folder-detail = Lokyr la lascia com'è: spostala, poi installa di nuovo gli strumenti da riga di comando.
tools-missing-message = Gli strumenti da riga di comando non sono accanto a Lokyr
tools-missing-detail = { $value } non c'è, come quando Lokyr gira fuori dal suo bundle.
tools-temporary-message = Prima sposta Lokyr nella cartella Applicazioni
tools-temporary-detail = Lokyr è aperto dalla sua immagine disco, o da una copia che macOS tiene da parte finché non viene spostato: i collegamenti non porterebbero più a nulla quando sparisce. Sposta Lokyr nella cartella Applicazioni, aprilo da lì, poi installa gli strumenti da riga di comando.
tools-failed-message = Non è stato possibile installare gli strumenti da riga di comando
tools-failed-detail = { $value }
