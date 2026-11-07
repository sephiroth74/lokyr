# I testi dell'app, in italiano.

command-window-new = Nuova finestra
command-window-close = Chiudi finestra
command-window-set-title = Imposta titolo…
command-window-goto = Vai alla finestra
command-window-toggle-quick-terminal = Terminale rapido
command-app-about = Informazioni su Lokyr
command-app-quit = Esci da Lokyr
command-app-check-for-updates = Controlla aggiornamenti…
command-app-settings = Impostazioni…
command-app-report-issue = Segnala un problema…
command-app-secure-keyboard-entry = Immissione protetta da tastiera
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

# The notice of a plugin that a fault stopped, by its name.
plugin-stopped = Il plugin { $name } si è fermato, e le sue viste restano com'erano, senza rispondere ai clic: { $fault }

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
menu-look-up = Cerca info su “{ $word }”
menu-man-page = Apri la pagina man per “{ $page }”

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
