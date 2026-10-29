# I testi dell'app, in italiano.

command-window-new = Nuova finestra
command-window-close = Chiudi finestra
command-window-set-title = Imposta titolo…
command-app-quit = Esci da Lokyr
command-appearance-system = Aspetto automatico
command-appearance-light = Aspetto chiaro
command-appearance-dark = Aspetto scuro
command-pane-look-up = Cerca info
command-pane-man-page = Apri pagina man

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

set-title-message = Il titolo della finestra
set-title-detail = Lascialo vuoto per quello della finestra: il titolo del programma, o la sua cartella, poi il programma in esecuzione.
set-title-set = Imposta
set-title-cancel = Annulla
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
