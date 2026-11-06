# EyeCanSee, italiano.

# Nome della lingua scritto in questa lingua, per la scelta nelle Opzioni.
language-name = Italiano

counter = { $n } di { $total }
folder-empty = Questa cartella non contiene immagini né sottocartelle
cannot-load = Impossibile aprire { $file }: { $error }
video-no-mpv = Per i video serve libmpv, che non è stata trovata. Installare il pacchetto con libmpv: mpv-libs su Fedora, libmpv2 su Debian, Ubuntu e openSUSE (libmpv1 su Ubuntu 22.04), mpv su Arch.
video-component-title = Motore video
video-component-restart-needed = Il motore video è scaricato, ma non è stato possibile avviarlo. Di solito basta riavviare EyeCanSee.
video-component-restart = Riavvia EyeCanSee
video-component-ask = Per guardare i video EyeCanSee ha bisogno del motore video (mpv), da scaricare una volta sola: { $size }.
video-component-download = Scarica il motore video
video-component-button = Scarica il motore video ({ $size })
video-copying = Lettura dal telefono: { $done } di { $total }
video-component-progress = Scaricamento: { $done } di { $total }
video-component-error = Scaricamento non riuscito: { $error }
video-engine-old = Il motore video di questo computer è troppo vecchio: { $found }, mentre a EyeCanSee serve almeno libmpv { $required }. EyeCanSee può scaricare il suo, una volta sola ({ $size }).
video-engine-system-only = Il motore video (libmpv) su questo computer non c'è, e nelle Opzioni è scelto di usare solo quello di sistema. Installalo (il pacchetto mpv), oppure cambia la scelta in Strumenti > Opzioni > Video.
video-play = Riproduci (Spazio)
video-pause = Pausa (Spazio)
video-mute = Silenzia (M)
video-unmute = Riattiva l'audio (M)
video-volume = Volume { $volume } (frecce su e giù, rotellina del mouse sul video)
video-no-audio = Questo video non ha l'audio
error-unsupported = Protocollo non supportato: { $detail }
error-no-host = L'indirizzo non contiene l'host: { $detail }
error-path = Impossibile aprire { $detail }
error-source = Impossibile collegarsi alla sorgente: { $detail }
error-detail = { $detail }
error-login = Il server ha rifiutato l'accesso: nome utente o password sbagliati.
error-login-ssh = Il server { $detail } ha rifiutato l'accesso: non ha accettato nessuna delle tue chiavi SSH e non c'è una password, oppure la password è sbagliata.
error-key-failed = La chiave salvata per { $detail } non si apre: la passphrase è sbagliata, oppure il file manca o non è una chiave. Correggila in File > Gestione server.
error-permission = Permesso negato: { $detail }
error-no-sftp = Il server risponde in SSH ma non ha il servizio SFTP ({ $detail }). Su un NAS, attiva SFTP nelle sue impostazioni.
error-host-unknown = { $detail } non è mai stato usato da questo computer: conferma la sua chiave per collegarti.
error-cert-unknown = Il certificato di { $detail } non è firmato da un'autorità riconosciuta: confermalo per collegarti.
error-host-changed = La chiave di { $detail } non è quella registrata in ~/.ssh/known_hosts, quindi EyeCanSee non si collega: qualcuno potrebbe fingersi quel server. Se il server è stato reinstallato, togli la sua riga vecchia da known_hosts.
host-ask =
    { "**" }Primo collegamento a { $host }.**

    L'impronta della sua chiave, da confrontare con quella che mostra il server (nelle sue impostazioni, o con ssh-keygen -lf sul server):

    `{ $fingerprint }`

    Se è il server che ti aspetti, EyeCanSee la ricorda in ~/.ssh/known_hosts, come fa ssh, e si collega.
cert-ask =
    { "**" }Primo collegamento a { $host } (FTPS).**

    Il suo certificato non è garantito da un'autorità riconosciuta (succede con i NAS, che se lo creano da soli). L'impronta SHA-256, da confrontare con quella che mostra il server nelle sue impostazioni:

    `{ $fingerprint }`

    Se è il server che ti aspetti, EyeCanSee lo ricorda e si collega.
host-ok = Collegati
sftp-rsa-key = **{ $host }** aperto con una chiave RSA.

    Funziona, ma la libreria RSA usata da EyeCanSee ha un difetto noto non ancora corretto (RUSTSEC-2023-0071): in teoria chi controlla il server, misurando i tempi di moltissimi accessi, potrebbe ricavare la chiave. Meglio passare a una chiave ed25519 (ssh-keygen -t ed25519).

    Questo avviso compare una volta per server a ogni avvio di EyeCanSee.
error-trash-failed = Non è stato possibile spostare l'originale nel cestino: l'originale è intatto. ({ $detail })
error-not-found = Non trovato: { $detail }
error-device-busy = Il dispositivo è in uso da un'altra applicazione (per esempio il gestore dei file). Chiudila, oppure scollega e ricollega il dispositivo. ({ $detail })
error-device-kde = Il telefono è in uso da KDE (il suo servizio kiod). Premi F5 per aprirlo comunque in EyeCanSee. ({ $detail })
error-no-device = Il dispositivo non è stato trovato: se è collegato, sbloccalo e scegli "Trasferimento file". EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })
error-phone-disconnected = Il telefono è stato scollegato. ({ $detail })
error-phone-timeout = Il telefono ha smesso di rispondere: forse è stato scollegato o bloccato. ({ $detail })
error-phone-name-taken = Il telefono ha rifiutato la copia: nella cartella c'è già un file con quel nome. Non è cambiato niente. ({ $detail })
error-not-ready = Il telefono non risponde: sbloccalo e scegli "Trasferimento file" nella sua notifica USB. EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })
error-use-mount = L'accesso diretto al dispositivo non è riuscito. ({ $detail })
error-no-storage = Il dispositivo non mostra nessuna memoria: sbloccalo e scegli "Trasferimento file". EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })

menu-file = File
menu-open-location = Apri indirizzo…
menu-servers = Gestione server…
menu-options = Opzioni…
menu-quit = Esci
menu-edit = Modifica
menu-tools = Strumenti
menu-select-all = Seleziona tutto
key-select-all = Ctrl+A
menu-view = Visualizza
menu-refresh = Aggiorna
menu-back-to-grid = Torna alla griglia
menu-help = Aiuto
menu-manual = Manuale
menu-copy = Copia
menu-cut = Taglia
menu-paste = Incolla
menu-paste-into = Incolla in questa cartella
key-copy = Ctrl+C
key-cut = Ctrl+X
key-paste = Ctrl+V
paste-taken = **"{ $name }"** è già in questa cartella.
paste-skip = Salta
paste-all = Fai lo stesso per tutti gli altri elementi con un nome già presente
transfer-copying = Copia in corso
transfer-moving = Spostamento in corso
transfer-looking = Esamino le cartelle: { $n } elementi trovati
transfer-counts = { $n } di { $total } file, { $done } di { $size }
transfer-to = In { $dir }
transfer-left = { $percent } %, { $speed }/s, circa { $time } alla fine
error-name-too-long = Il nome è troppo lungo per il telefono: { $detail } caratteri, mentre un telefono ne accetta al massimo 254 (un'emoji conta per due). Accorcialo e riprova; non è stato copiato niente.
error-copy-not-made = Il telefono non ha fatto la copia di { $detail }: l'originale non è stato toccato.
error-paste-inside = Una cartella non può andare dentro sé stessa: { $detail }
error-cancelled = Annullato.
menu-about = Informazioni su EyeCanSee
menu-meta = Riquadri dei metadati
about-tagline = Visualizzatore e gestore di immagini e video per Linux e Windows
about-license = Software libero con licenza GNU General Public License, versione 3 o successiva.
about-license-link = Testo della licenza
about-source = Codice sorgente su GitHub
about-third-party = Realizzato con librerie open source: le loro licenze sono incluse nello zip per Windows e nell'AppImage.
menu-open-selected = Apri
menu-next = Avanti
menu-prev = Indietro
menu-first = Primo
menu-last = Ultimo
menu-parent = Cartella superiore

key-enter = Invio
key-backspace = Backspace
key-home = Inizio
key-end = Fine

tb-up = Cartella superiore (Backspace)
tb-address-hint = Cartella, ftp://… o sftp://…
tb-thumb-size = Dimensione miniature

status-folders = Cartelle: { $n }
status-images = Immagini: { $n }
status-videos = Video: { $n }
status-audio = Audio: { $n }
status-deleting = Eliminazione in corso: { $n }…
status-file-removed = { $name } è stato rimosso dal dispositivo.
status-phone-waiting = Il telefono non risponde da { $s } s ({ $name }): aspetto…
status-phone-waiting-open = Il telefono non risponde da { $s } s: aspetto…
status-phone-stalled = Windows non comunica col telefono da { $s } s
phone-stalled =
    { "**" }Windows ha perso il collegamento con il telefono.**

    Stacca e ricollega il cavo (poi Trasferimento file), oppure aspetta qualche minuto: si sblocca da solo. Telefono e file sono a posto.
phone-stalled-upload =
    { "**" }Windows ha perso il collegamento con il telefono.**

    Stacca e ricollega il cavo (poi Trasferimento file), oppure aspetta qualche minuto: si sblocca da solo. Telefono e file sono a posto.

    { $name } andrà ricopiato.
phone-stalled-why = Perché succede?
phone-stalled-url = https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.it.md
status-copied = { $n } copiati: Ctrl+V per incollare
status-cut = { $n } tagliati: Ctrl+V per spostare
status-temp-copy = Questo file è una copia temporanea fatta da Windows (per esempio di una foto del telefono), non l'originale: non si può salvarci sopra. Usa Salva come… per tenere le modifiche.
status-taken-from-gvfs = Il telefono era aperto nel file manager: ora lo usa EyeCanSee direttamente, che è più veloce. Per rivederlo nel file manager chiudi EyeCanSee e fai clic sul telefono lì.
status-through-mount = Aperto tramite il gestore dei file: più lento. Accesso diretto non riuscito: { $detail }
status-marked = Selezionati: { $n } ({ $size })
decimal-separator = ,
date-format = %d/%m/%Y %H:%M
date-day-format = %d/%m/%Y

options-title = Opzioni
opt-remember-location = Riapri l'ultima cartella all'avvio
opt-remember-scroll = Ricorda la posizione di scorrimento di ogni cartella
opt-remember-scroll-hint = Tornando in una cartella già vista, la griglia riparte dal punto in cui l'avevi lasciata invece che dalla prima anteprima. Vale finché EyeCanSee resta aperto.
opt-wrap-around = Nel visore, dopo l'ultima immagine ricomincia dalla prima
opt-wrap-around-hint = E dalla prima immagine, tornando indietro, mostra l'ultima. Spenta, il visore si ferma alla prima e all'ultima immagine.
opt-close-to-grid = Nel visore, la X della finestra torna alla griglia
opt-close-to-grid-hint = Come Esc. Spenta, la X chiude EyeCanSee anche dal visore.
opt-wheel = Rotellina del mouse nel visore:
opt-wheel-zoom = Ingrandisce e rimpicciolisce
opt-wheel-next = Mostra l'immagine successiva o precedente
opt-wheel-hint = Tenendo premuto Ctrl, la rotellina ingrandisce sempre.
opt-max-zoom = Ingrandimento massimo nel visore:
opt-max-zoom-hint = Fin dove la rotellina e il tasto + possono ingrandire un'immagine, in percentuale dei suoi pixel (100% = un pixel dell'immagine per pixel dello schermo).
opt-remember-location-hint = Quando EyeCanSee parte senza una cartella (per esempio dal menù delle applicazioni), apre quella che stavi guardando alla chiusura. Gli indirizzi remoti si riaprono senza password, che va riscritta.
opt-cache-maintenance = Pulizia della cache miniature
opt-cache-maintenance-hint = Una volta per sessione, per ogni indirizzo aperto: elimina le miniature delle immagini cancellate, rifà quelle delle immagini modificate e tiene la cache entro il limite. Le cartelle che non si trovano o non si raggiungono vengono saltate.
opt-cache-limit = Limite della cache
opt-window = Finestra all'avvio
opt-window-normal = Dimensione normale
opt-window-maximized = Massimizzata
opt-window-remember = Dimensione che aveva alla chiusura
opt-language = Lingua
opt-language-system = Come il sistema
opt-theme = Colori
opt-theme-system = Come il sistema
opt-theme-light = Chiari
opt-theme-dark = Scuri

devices = Dispositivi
servers = Server
this-computer = Questo computer
device-mount-hint = Non montata ({ $device }): clic per montarla e aprirla
error-mount = Impossibile montare la partizione: { $detail }
opt-purge = Svuota la cache miniature…
opt-purge-confirm = Eliminare { $n } miniature ({ $mb } MB)?

    Le tue immagini non vengono toccate: le miniature si rifanno quando servono.
opt-purge-yes = Svuota
opt-purge-no = Annulla

meta-title = Metadati
meta-collapse = Comprimi il pannello
meta-expand = Apri il pannello
meta-file = File
meta-file-name = Nome
meta-file-size = Dimensione
meta-file-date = Data del file
meta-pixels = Pixel
meta-bytes = byte
thousands-separator = .
meta-camera = Fotocamera
meta-shot = Scatto
meta-image = Immagine
meta-gps = Posizione
meta-make = Marca
meta-model = Modello
meta-lens = Obiettivo
meta-software = Software
meta-taken = Data dello scatto
meta-exposure = Esposizione
meta-aperture = Diaframma
meta-iso = ISO
meta-focal = Lunghezza focale
meta-flash = Flash
meta-flash-yes = Scattato
meta-flash-no = Non scattato
meta-width = Larghezza
meta-height = Altezza
meta-format = Formato
meta-color = Colore
meta-color-rgb = RGB
meta-color-rgba = RGB con trasparenza
meta-color-gray = Scala di grigi
meta-color-graya = Scala di grigi con trasparenza
meta-depth = Profondità
meta-duration = Durata
meta-fps = Fotogrammi al secondo
meta-video-codec = Video
meta-audio = Audio
meta-bitrate = Bit rate
meta-color-uncalibrated = Non calibrato
meta-colorspace = Spazio colore
meta-altitude = Altitudine
meta-gps-date = Data GPS
meta-speed-avg = Velocità media
meta-speed-max = Velocità massima
meta-coords = Coordinate
meta-map = Apri sulla mappa
meta-close = Chiudi (si riapre da Visualizza o col pulsante ℹ)
meta-none-selected = Seleziona un'immagine
meta-loading = Lettura…
loading-file = Caricamento del file in corso…
meta-no-exif = Questo file non contiene metadati
meta-empty = Nessun dato
tb-meta = Metadati: mostra o nascondi i riquadri

sort-by = Ordina per
sort-name = Nome
sort-date = Data di modifica
sort-size = Dimensione
sort-type = Tipo
sort-desc = Ordine decrescente
show-menu = Mostra
show-all = Immagini, video e audio
show-images = Solo immagini
show-videos = Solo video
show-audio = Solo audio
sort-asc = Ordine crescente

menu-delete = Elimina…
key-delete = Canc
menu-delete-permanent = Elimina definitivamente…
menu-rename = Rinomina…
rename-title = Nuovo nome
rename-ok = Rinomina
rename-bad = Nome non valido
rename-taken = Nella cartella c'è già un elemento con questo nome
error-exists = Esiste già un elemento con questo nome: { $detail }
error-save-temp-copy = Non salvata sopra se stessa: questo file è una copia temporanea fatta da Windows, non l'originale (per esempio la foto sul telefono). ({ $detail })
error-save-gvfs = Non salvata sopra se stessa: questa cartella è aperta tramite il file manager (gvfs), dove sostituire un file può far perdere l'originale. ({ $detail })
error-rename-unsupported = Questa sorgente non può rinominare questo elemento. ({ $detail })
delete-confirm-dir-trash = Spostare la cartella **"{ $name }"**, con tutto quello che contiene, **nel cestino**?
delete-confirm-dir-folder = Spostare la cartella **"{ $name }"**, con tutto quello che contiene, nel **Cestino di questa cartella** (la cartella nascosta .EyeCanSee-trash)?

    Potrai **ripristinarla** da EyeCanSee con **Apri il cestino**, in cima alla griglia.
delete-confirm-dir-permanent = Eliminare **definitivamente** la cartella **"{ $name }"** e tutto quello che contiene?

    { "**" }Non si può annullare.**
key-shift-delete = Maiusc+Canc
delete-confirm-trash = Spostare **"{ $name }"** ({ $size }) **nel cestino**?
delete-confirm-folder = Spostare **"{ $name }"** ({ $size }) nel **Cestino di questa cartella** (la cartella nascosta .EyeCanSee-trash)?

    Potrai **ripristinarlo** da EyeCanSee con **Apri il cestino**, in cima alla griglia.
delete-confirm-permanent = Eliminare **definitivamente** **"{ $name }"** ({ $size })?

    { "**" }Non si può annullare.**
delete-confirm-many-trash = Spostare **{ $n } elementi nel cestino**?

    Immagini: { $size }.{ $folders }
delete-confirm-many-folder = Spostare **{ $n } elementi** nel **Cestino della loro cartella** (la cartella nascosta .EyeCanSee-trash)?

    Immagini: { $size }.{ $folders }

    Potrai **ripristinarli** da EyeCanSee con **Apri il cestino**, in cima alla griglia.
delete-confirm-many-permanent = Eliminare **definitivamente { $n } elementi**?

    Immagini: { $size }.{ $folders } **Non si può annullare.**
delete-many-folders = Le cartelle vanno con tutto il loro contenuto.
delete-unseen-found = { $n ->
    [one] Questa cartella contiene
   *[other] La cartella **{ $folder }** contiene
} file o cartelle che EyeCanSee non mostra, per esempio **{ $name }**. { $how ->
    [permanent] Verranno eliminati anche quelli
   *[trash] Finiranno nel cestino anche quelli
}: controllala con il file manager se non sei sicuro.
delete-unseen-failed = Non è stato possibile controllare la cartella: potrebbe contenere file o cartelle che EyeCanSee non mostra.
delete-move-to-trash = Sposta nel cestino
delete-yes = Elimina
delete-no = Annulla
error-no-trash-folder = Questa sorgente non può spostare i file nella cartella cestino: scegli "Elimina definitivamente" nelle Opzioni. ({ $detail })
opt-local-permanent = Elimina i file del computer definitivamente, senza cestino (pericoloso)
opt-local-permanent-hint = Le immagini eliminate non si possono recuperare. Spenta: vanno nel cestino di sistema.
opt-remote-delete = Eliminare su telefoni e server (non hanno un cestino):
opt-remote-delete-folder = Sposta nella cartella nascosta .EyeCanSee-trash accanto all'immagine
opt-remote-delete-permanent = Elimina definitivamente
opt-tab-general = Generale
opt-tab-look = Aspetto
opt-tab-browsing = Navigazione
opt-tab-deleting = Eliminazione
opt-tab-thumbnails = Miniature
trash = Cestino
menu-restore = Ripristina
menu-restore-all = Ripristina tutto
menu-empty-trash = Svuota il cestino…
trash-bar-has = Questa cartella ha un cestino
trash-open = Apri il cestino
trash-bar-inside = Cestino della cartella { $folder }
delete-confirm-empty-trash = **Svuotare questo cestino?**

    Tutto quello che contiene viene eliminato **definitivamente**. **Non si può annullare.**
restore-taken = In **"{ $folder }"** c'è già un elemento chiamato **"{ $name }"**.
restore-replace = Sostituisci
restore-keep-both = Tieni entrambi
restore-merge = Unisci
delete-ntfs = Questo è un **disco NTFS**: l'app File (Nautilus) non riesce a ripristinare dal suo cestino. Il file si può comunque ripristinare dal terminale con "gio trash --restore" (vedi il manuale, Limiti noti).

opt-check-updates = Controlla gli aggiornamenti all'avvio
opt-check-updates-hint = All'avvio EyeCanSee chiede a GitHub l'ultima versione pubblicata. Se è più nuova la scarica in background e la controlla; la installa solo quando fai clic su "Installa … e riavvia". Windows (installer e zip) e AppImage; su Fedora gli aggiornamenti arrivano con dnf.
update-install = Installa la { $version } e riavvia
update-failed = Impossibile installare l'aggiornamento: { $detail }
menu-check-updates = Verifica aggiornamenti
update-checking = Controllo degli aggiornamenti…
update-latest = EyeCanSee { $version } è l'ultima versione.
update-found = EyeCanSee { $version } è pronto: per installarlo fai clic sul pulsante rosso in alto a destra.
update-store = Questa copia viene dal Microsoft Store, che la aggiorna.
update-unavailable = Questa copia non viene aggiornata da EyeCanSee: su Fedora gli aggiornamenti arrivano con dnf, e le versioni di sviluppo non si aggiornano mai.
update-check-failed = Impossibile controllare gli aggiornamenti: { $detail }
menu-redo-thumbs = Aggiorna miniature
menu-redo-all-thumbs = Aggiorna tutte le miniature della cartella
key-shift-f5 = Maiusc+F5
convert-title = Salva in un altro formato
convert-original = Originale
convert-result = Convertita
convert-working = Conversione in corso…
convert-no-preview = Anteprima non disponibile per questo formato: il file si salva comunque.
convert-calculating = Calcolo della dimensione in corso…
convert-error = Impossibile convertire: { $detail }
convert-actual = 100% (trascina le immagini per spostarti)
convert-pixels = { $w } × { $h } pixel
model-triangles = { $n } triangoli
# The keys of the viewer, under the status bar.
hint-cad = Rotellina, + e −: zoom | Trascinare: spostare | 0: tutto il disegno | ← →: precedente, successivo | Ctrl+Maiusc+S: salva come immagine | F11: schermo intero | Esc: indietro
hint-cad-layouts = Rotellina, + e −: zoom | Trascinare: spostare | 0: tutto il disegno | L, Maiusc+L: layout successivo, precedente | ← →: file precedente, successivo | Ctrl+Maiusc+S: salva come immagine | F11: schermo intero | Esc: indietro
cad-model = Modello
cad-units = unità
keys-button = Tasti
menu-export-3d = Esporta come modello 3D
status-exported = Salvato come { $name }
export-failed = Esportazione non riuscita: { $error }
hint-model = Frecce o trascinare: ruotare | Maiusc+trascinare: spostare | Rotellina, + e −: zoom | 0: vista di partenza | Pag↑, Pag↓: precedente, successivo | Ctrl+S: salva come immagine | F11: schermo intero | Esc: indietro
hint-edit-select = Clic: seleziona un oggetto | Trascinare: lo sposta | Trascinare un angolo o un capo: ridimensiona | Doppio clic su un testo o un fumetto: cambia le lettere | Canc: elimina | Ctrl+C: copia il risultato | Esc: lascia l'oggetto
hint-edit-marquee = Trascinare: sceglie una zona | Maiusc+trascinare: un quadrato | Ctrl+C: copia la zona | Ctrl+X: la taglia | Ctrl+V: incolla come oggetto | Clic o Esc: lascia la zona
hint-edit-arrow = Trascinare: disegna | Trascinare il punto di mezzo: curva | Trascinare un capo: lo sposta | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-line = Trascinare: disegna | Trascinare il punto di mezzo: curva | Trascinare un capo: lo sposta | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-pen = Trascinare: disegna | Trascinare un angolo: allarga | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-rect = Trascinare: disegna | Maiusc+trascinare: un quadrato | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-ellipse = Trascinare: disegna | Maiusc+trascinare: un cerchio | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-highlight = Trascinare: evidenzia | Maiusc+trascinare: un quadrato | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-spotlight = Trascinare: illumina una zona | Maiusc+trascinare: un quadrato o un cerchio | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | Canc: elimina | Esc: lascia
hint-edit-text = Clic: mette un testo, poi lo scrivi | Invio: a capo | Doppio clic su un testo: lo cambia | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina (fuori dal campo) | Esc: finisce il testo
hint-edit-bubble = Trascinare: disegna un fumetto, poi scrivi il testo | Invio: a capo | Trascinare la punta della coda: la orienta | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | Doppio clic: cambia il testo | Canc: elimina | Esc: finisce il testo
hint-edit-step = Clic: numero seguente | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-stamp = Clic: mette il timbro | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-lens = Trascinare: disegna una lente | Maiusc+trascinare: un quadrato o un cerchio | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | Canc: elimina | Esc: lascia
hint-edit-cover = Trascinare: copre | Maiusc+trascinare: un quadrato | Trascinare un angolo: ridimensiona (Maiusc: stesse proporzioni) | I: contagocce | Alt+clic: prende un colore, lo strumento resta | Canc: elimina | Esc: lascia
hint-edit-crop = Trascinare un lato o un angolo: ritaglia | Trascinare dentro: sposta il ritaglio | Maiusc+rotellina: ruota di 0,1° | Ctrl+R, Ctrl+Maiusc+R: ruota a destra, a sinistra | Invio: applica | Esc: chiude
hint-image = →, Spazio, Pag↓: successiva | ←, Backspace, Pag↑: precedente | Inizio, Fine: prima, ultima | Rotellina, + e −: zoom | Trascinare: spostare un'immagine ingrandita | 0: adatta alla finestra | 1: 100% | Ctrl+R, Ctrl+Maiusc+R: ruota a destra, a sinistra | Ctrl+E: modifica immagine | Ctrl+S, Ctrl+Maiusc+S: salva, salva come | F2: rinomina | Canc: elimina | F11: schermo intero | Esc: indietro
hint-video = Spazio: pausa e riprendi | Ctrl+← →: 5 secondi indietro, avanti | ↑ ↓, rotellina: volume | M: audio sì e no | →, Pag↓: successivo | ←, Backspace, Pag↑: precedente | Inizio, Fine: primo, ultimo | F2: rinomina | Canc: elimina | F11: schermo intero | Esc: indietro
cad-engine-title = Motore CAD
cad-engine-ask = Per aprire i modelli STEP, IGES e BREP dei programmi CAD EyeCanSee ha bisogno del motore CAD (Open CASCADE), da scaricare una volta sola: { $size }.
cad-engine-download = Scarica il motore CAD
cad-engine-button = Scarica il motore CAD ({ $size })
convert-file-size = { $size } su disco
convert-ratio = { $percent }% dell'originale
convert-format = Formato
convert-quality = Qualità
convert-speed = Velocità
convert-speed-hint = Più lenta fa un file più piccolo a parità di qualità.
convert-lossless = Senza perdita: gli stessi pixel dell'originale.
convert-gif = Al massimo 256 colori.
convert-keep-meta = Mantieni i metadati (EXIF, XMP, IPTC, profilo colore)
convert-no-meta = L'originale non ha metadati.
convert-meta-unsupported = Questo formato non può contenere metadati.
convert-name = Nome del file
convert-save = Salva
convert-exists = Esiste già un elemento con questo nome: verrà salvato come { $name }.
convert-chroma = Dettaglio del colore
convert-chroma-full = Pieno (4:4:4)
convert-chroma-half = Metà (4:2:2)
convert-chroma-quarter = Un quarto (4:2:0)
convert-chroma-grey = Solo grigi
convert-progressive = Progressivo (si vede prima sfocato mentre si carica)
convert-optimize = Tabelle di Huffman ottimizzate (più piccolo, stessa qualità)
convert-arithmetic = Codifica aritmetica (più piccolo, ma molti programmi non lo aprono)
convert-jpeg-lossless = JPEG senza perdita (molti programmi non lo aprono)
convert-png-level = Livello di compressione (0 = nessuna)
convert-png-filter = Filtro
convert-filter-adaptive = Adattivo
convert-none = Nessuna
convert-lossy = Con perdita
convert-lossless-mode = Senza perdita
convert-near-lossless = Quasi senza perdita (100 = spento)
convert-sharp-yuv = Bordi dei colori più nitidi (più lento)
convert-effort = Sforzo
convert-alpha-quality = Qualità della trasparenza
convert-ten-bit = 10 bit per canale
convert-avif-rgb = RGB dentro il file (per la grafica, più grande)
convert-compression = Compressione
convert-deflate-fast = Deflate, veloce
convert-deflate-best = Deflate, massima
convert-predictor = Predittore orizzontale (più piccolo con LZW e Deflate)
convert-gif-quality = Qualità della riduzione dei colori
convert-tga-rle = Compresso (RLE; pochi programmi vecchi leggono solo i TGA non compressi)
convert-ico-reduced = Un'icona contiene al massimo 256 × 256 pixel: l'immagine viene ridotta.
convert-on-white = Questo formato non ha la trasparenza: le parti trasparenti diventano bianche.
menu-image = Immagine
menu-transform = Trasforma
tf-flip-h = Rifletti orizzontalmente
tf-flip-v = Rifletti verticalmente
tf-rotate-cw = Ruota di 90° in senso orario
tf-rotate-ccw = Ruota di 90° in senso antiorario
tf-rotate-180 = Ruota di 180°
menu-rotate-crop = Ruota e ritaglia…
rc-title = Ruota e ritaglia
# The copy an edit saves next to the original: "photo (edited).jpg".
edit-copy-word = modificata
rc-rotation = Rotazione
rc-reset = Torna a 0°
rc-straighten = Raddrizza
rc-straighten-hint = Traccia una linea lungo l'orizzonte, o lungo qualcosa di verticale: l'immagine ruota per metterla dritta.
rc-one-only = Modifica immagine e Ruota e ritaglia lavorano su una foto alla volta. Per ruotare più foto insieme usa le voci qui sopra o Converti e ridimensiona.
rename-one-only = Rinomina lavora su un file alla volta.
rc-rotation-hint = Maiusc + rotellina sull'immagine: 0,1° alla volta.
rc-crop = Ritaglio
rc-free = Proporzioni libere
rc-original = Come l'originale
rc-custom = Personalizzate
rc-swap = Scambia larghezza e altezza
rc-inside = Solo immagine
rc-inside-hint = Il ritaglio più grande senza angoli vuoti.
rc-whole = Tutta l'immagine
rc-whole-hint = Tutta l'immagine ruotata, con gli angoli vuoti riempiti.
rc-empty = Angoli vuoti
rc-transparent = Trasparenti
rc-color = Colore
rc-jpeg-white = Il JPEG non ha trasparenza: Salva li rende bianchi.
rc-size = Risultato: { $w } × { $h } pixel
rc-save = Salva…
rc-save-hint = Salva una copia accanto all'originale, col suo nome e "(modificata)"; l'originale resta com'è.
rc-save-as-hint = Salva il risultato in un file nuovo, nel formato che scegli; l'originale resta com'è.
rc-close = Chiudi
rc-discard = Non salvare
rc-leave = Le modifiche a **{ $name }** non sono salvate.

    { "**" }Salva** le scrive sopra l'originale, **Salva come…** crea un file nuovo, **Non salvare** le perde.
menu-edit-image = Modifica immagine…
key-edit = Ctrl+E
edit-tool-select = Seleziona
edit-tool-arrow = Freccia
edit-tool-line = Linea
edit-tool-pen = Mano libera
edit-tool-rect = Rettangolo
edit-tool-ellipse = Ellisse
edit-tool-highlight = Evidenziatore
edit-tool-spotlight = Riflettore
edit-tool-text = Testo
edit-tool-stamp = Timbri
edit-tool-lens = Lente
edit-tool-bubble = Fumetto
edit-tool-step = Numeri di passaggio
edit-tool-cover = Copertura
edit-tool-crop = Ritaglia e ruota
edit-save-copy = Salva copia
edit-update-copy = Aggiorna copia
edit-update-hint = Riscrive la copia salvata prima, dall'originale con tutte le modifiche; se non c'è più o un altro programma l'ha cambiata, ne fa una nuova.
edit-update-confirm = **{ $copy }** viene riscritta con le modifiche. L'originale resta com'è.
edit-overwrite-again = **{ $name }**, già sovrascritto, viene riscritto con le modifiche, partendo dall'originale com'era all'apertura. L'originale è già nel cestino: ciò che era stato scritto prima viene sostituito.
edit-copy-gone = La copia salvata prima non c'è più o è stata cambiata da un altro programma: ne è stata fatta una nuova.
edit-off-sending = Il salvataggio di prima è ancora in invio.
edit-save-more = Altri modi di salvare
edit-off-unchanged = Nessuna modifica da salvare.
edit-off-preparing = Il file si sta preparando.
edit-off-undo = Niente da annullare.
edit-off-redo = Niente da ripetere.
edit-off-copying = La copia negli appunti è in corso.
edit-zoom-in = Ingrandisci
edit-zoom-out = Riduci
edit-zoom-fit = Tutto in vista (anche doppio clic)
edit-col-red = Rosso
edit-col-orange = Arancione
edit-col-yellow = Giallo
edit-col-green = Verde
edit-col-blue = Blu
edit-col-purple = Viola
edit-col-black = Nero
edit-col-white = Bianco
edit-col-other = Altro colore
edit-tip-select = Clic su un oggetto per selezionarlo, trascinalo per spostarlo.
edit-tip-draw = Trascina sull'immagine per disegnare (con Maiusc un quadrato o un cerchio); trascina il punto di mezzo di una linea o di una freccia per curvarla.
edit-tip-text = Clic sull'immagine dove va il testo.
edit-tip-stamp = Clic sull'immagine per mettere il timbro scelto nella barra.
edit-stamp-check = Spunta
edit-stamp-cross = Croce
edit-stamp-warning = Avviso
edit-stamp-info = Informazione
edit-stamp-question = Domanda
edit-stamp-star = Stella
edit-stamp-thumbs-up = Pollice in su
edit-stamp-thumbs-down = Pollice in giù
edit-stamp-heart = Cuore
edit-stamp-smile = Sorriso
edit-stamp-grin = Risata
edit-stamp-frown = Faccia triste
edit-stamp-circle-check = Spunta nel cerchio
edit-stamp-circle-x = Croce nel cerchio
edit-stamp-exclamation = Punto esclamativo
edit-stamp-ban = Divieto
edit-stamp-plus = Più
edit-stamp-minus = Meno
edit-stamp-up = Freccia su
edit-stamp-down = Freccia giù
edit-stamp-left = Freccia a sinistra
edit-stamp-right = Freccia a destra
edit-stamp-flag = Bandiera
edit-stamp-pin = Luogo
edit-stamp-target = Bersaglio
edit-stamp-idea = Idea
edit-stamp-bell = Campanella
edit-stamp-lock = Chiuso
edit-stamp-unlock = Aperto
edit-stamp-eye = Guarda
edit-stamp-clock = Orologio
edit-stamp-calendar = Calendario
edit-stamp-mail = Posta
edit-stamp-phone = Telefono
edit-stamp-home = Casa
edit-stamp-camera = Fotocamera
edit-stamp-person = Persona
edit-stamp-people = Persone
edit-stamp-award = Medaglia
edit-stamp-trophy = Trofeo
edit-stamp-crown = Corona
edit-stamp-verified = Verificato
edit-stamp-shield = Sicuro
edit-stamp-bookmark = Segnalibro
edit-stamp-gift = Regalo
edit-stamp-party = Festa
edit-stamp-rocket = Razzo
edit-stamp-fire = Fuoco
edit-stamp-bolt = Fulmine
edit-stamp-sparkles = Scintille
edit-stamp-sun = Sole
edit-stamp-moon = Luna
edit-stamp-cloud = Nuvola
edit-stamp-coffee = Caffè
edit-stamp-car = Auto
edit-stamp-plane = Aereo
edit-stamp-hand = Mano
edit-stamp-more = Altri timbri
edit-tip-spotlight = Trascina sull'immagine sopra ciò che deve risaltare: il resto si scurisce (con più riflettori restano chiari tutti); con Maiusc un quadrato o un cerchio.
edit-intensity = Intensità
edit-tip-lens = Trascina sull'immagine per disegnare una lente: mostra più grande la parte che copre; con Maiusc un quadrato o un cerchio.
edit-zoom = Ingrandimento:
edit-lens-objects = Con gli oggetti
edit-lens-objects-hint = La lente mostra anche gli oggetti disegnati prima di lei (quelli disegnati dopo le restano sopra); coperture e filtri li mostra sempre.
edit-round = Rotonda
edit-square = Quadrata
edit-tip-bubble = Trascina sull'immagine per disegnare un fumetto, poi scrivine il testo; trascina la punta della coda per orientarla.
edit-bubble-rounded = Arrotondato
edit-bubble-oval = Ovale
edit-bubble-square = Squadrato
edit-bubble-cloud = Pensiero (nuvola)
edit-bubble-shout = Urlo
edit-letters = Lettere:
edit-tip-step = Clic sull'immagine per mettere il numero seguente (il più alto più uno); il numero si cambia nella barra.
edit-step-number = Numero
edit-tip-cover = Trascina sopra ciò che va nascosto: nel file salvato non ne resta niente.
edit-tip-crop = Trascina lati e angoli per ritagliare; Maiusc + rotellina ruota.
rc-apply = Applica
rc-apply-hint = Torna allo strumento di prima, con il risultato ritagliato. Ctrl+Z annulla il ritaglio.
rc-reset-off = L'angolo è già 0°.
rc-swap-off = Con proporzioni libere o 1:1 non c'è niente da scambiare.
rc-color-off = Scegli prima Colore.
edit-head-filled = Punta piena
edit-head-open = Punta aperta
edit-head-double = Punta ai due capi
edit-head-dot = Pallino
edit-font-default = Predefinito
edit-font-hint = Passa sopra un font nell'elenco per vederlo sul testo; scegline uno con un clic.
edit-pick = Contagocce: da qualsiasi punto dello schermo con il selettore del sistema (pulsante, Maiusc+I); I lo prende sull'immagine, disegni compresi, con un riquadro che mostra il colore
edit-pick-image = Contagocce: prendi un colore dall'immagine
edit-picked = Colore preso: { $codes } (il codice esadecimale è negli appunti)
edit-pick-size = Un pixel, o la media dei pixel intorno
edit-pick-screen-busy = Scegli il punto dello schermo nella finestra del sistema.
edit-pick-failed = Il sistema non ha dato il colore: { $detail }
edit-tip-pick = Clic sull'immagine per prenderne il colore; Esc annulla.
edit-props-next = Prossimo oggetto:
edit-props-next-hint = Colore, spessore e opacità valgono per il prossimo oggetto che disegni con questo strumento. Per cambiare un oggetto già disegnato, selezionalo.
edit-props-selected = Oggetto selezionato:
edit-props-selected-hint = Le modifiche vanno all'oggetto selezionato, e restano per i prossimi dello stesso tipo.
edit-border = Bordo:
edit-fill = Riempimento:
edit-fill-none = Nessun riempimento
edit-tool-marquee = Selezione
edit-tip-marquee = Trascina sull'immagine per scegliere una zona; Ctrl+C la copia, Ctrl+X la taglia, Ctrl+V la incolla come oggetto.
edit-copy-part = Copia la zona
edit-cut-part = Taglia la zona
edit-paste-part = Incolla
edit-off-no-marquee = Prima scegli una zona trascinando sull'immagine.
edit-off-no-clip = Niente da incollare: prima copia o taglia una zona.
edit-part-copied = Zona copiata: Ctrl+V la incolla come oggetto, anche in un altro programma.
edit-filters = Filtri
edit-filters-part = Sulla zona scelta:
edit-filters-whole = Su tutta l'immagine (scegli prima una zona con Selezione per un pezzo solo):
edit-filter-blur = Sfoca
edit-filter-pixelate = Pixel grossi
edit-filter-sharpen = Nitidezza
edit-filter-gray = Bianco e nero
edit-filter-sepia = Seppia
edit-filter-brightness = Luminosità
edit-filter-contrast = Contrasto
edit-filter-done = Fatto
edit-filter-done-hint = Il filtro è già applicato: resta così e finisce nel file con Salva copia. Un clic sul filtro lo riapre per modificarlo.
edit-filter-amount = Intensità
edit-filter-not-cover = Sfoca e Pixel grossi non bastano per nascondere testi o numeri sensibili: con poca intensità si leggono, o si possono ricostruire. Per quelli usa Copertura (B).
edit-text-curve = Su una curva
edit-text-curve-hint = Il testo segue una linea: trascina il punto di mezzo per curvarla, i capi per allungarla o girarla.
edit-outline = Contorno
edit-outline-hint = Una linea intorno alle lettere, per leggerle su qualsiasi foto.
edit-text-back = Sfondo
edit-text-back-hint = Un riquadro dietro le lettere (su una curva: una fascia lungo tutta la sua linea).
edit-margin = Margine
edit-corners = Angoli
menu-new-image = Nuova immagine…
key-new-image = Ctrl+N
new-title = Nuova immagine
new-name = Nuova immagine
new-size = Misura
new-custom = Personalizzata
new-width-height = Larghezza × altezza
new-swap = Scambia larghezza e altezza
new-dpi = Risoluzione
new-dpi-off = La risoluzione serve per le misure in millimetri, centimetri o pollici.
new-background = Sfondo
new-white = Bianco
new-transparent = Trasparente
new-colour = Colore
new-pixels = { $w } × { $h } pixel
new-create = Crea
new-letter = Lettera
new-legal = Legale
new-square = Quadrata
new-story = Verticale per le storie
edit-new-off = È un'immagine nuova: non c'è ancora un file da sovrascrivere. Usa Salva come….
edit-undo = Annulla
edit-redo = Ripeti
edit-copy = Copia
edit-copied = Il risultato è negli appunti.
edit-color = Colore
edit-width = Spessore
edit-size = Dimensione
edit-opacity = Opacità
edit-dash = Tratto
edit-solid = Continuo
edit-dashed = Tratteggiato
edit-dotted = Puntinato
edit-delete = Elimina
edit-select-hint = Seleziona un oggetto per cambiarlo, o scegli uno strumento.
edit-text-hint = Fai clic sull'immagine dove va il testo, poi scrivilo qui.
edit-animated = **{ $name }** non si può modificare: è animata o ha più pagine, e si salverebbe solo la prima immagine.
edit-animated-off = Questa immagine è animata o ha più pagine: si salverebbe solo la prima immagine.
edit-overwrite = Sovrascrivi l'originale…
edit-overwrite-hint = Scrive l'immagine modificata sopra l'originale, dopo aver chiesto; l'originale va nel cestino, anche quando nelle Opzioni è scelta l'eliminazione definitiva.
edit-overwrite-yes = Sovrascrivi
edit-save-confirm = Una copia di **{ $name }** viene salvata accanto all'originale come **{ $copy }**. L'originale resta com'è.
edit-loss-lossy = La copia viene **ricodificata** e può perdere un po' di qualità.
edit-loss-8bit = La copia ha **8 bit per colore**, l'originale di più: l'originale resta com'è, per ciò che li richiede.
edit-loss-hdr = La copia è un PNG a 16 bit: ciò che nell'originale è **più luminoso del bianco** viene tagliato.
edit-overwrite-trash = **{ $name }** viene sostituito dall'immagine modificata.

    L'originale va **nel cestino**.
edit-overwrite-folder = **{ $name }** viene sostituito dall'immagine modificata.

    L'originale va nel **Cestino di questa cartella**, da cui si ripristina con **Apri il cestino**.
edit-over-loss-lossy = Il file nuovo viene **ricodificato** e può perdere un po' di qualità.
edit-over-loss-8bit = Il file nuovo ha **8 bit per colore**, l'originale di più.
edit-over-loss-hdr = Il file nuovo ha 16 bit per colore: ciò che nell'originale è **più luminoso del bianco** viene tagliato.
edit-leave = Le modifiche a **{ $name }** non sono salvate.

    { "**" }Salva** crea una copia accanto all'originale, **Salva come…** un file nuovo dove scegli, **Non salvare** le perde.
edit-temp-copy = Questa immagine è una copia temporanea che Windows ha fatto di un file del telefono o di un altro dispositivo: una copia accanto a lei potrebbe essere cancellata da Windows. Scegli dove salvarla.
edit-remote-phone = L'immagine si modifica in una copia su questo computer: Salva manda il risultato al telefono.
edit-remote-server = L'immagine si modifica in una copia su questo computer: Salva manda il risultato al server.
key-rotate-cw = Ctrl+R
key-rotate-ccw = Ctrl+Maiusc+R
menu-save = Salva
key-save = Ctrl+S
menu-save-copy = Salva come…
key-save-copy = Ctrl+Maiusc+S
save-confirm-trash = Salvare le modifiche sopra **{ $name }**?

    L'originale va **nel cestino**.
save-confirm-folder = Salvare le modifiche sopra **{ $name }**?

    L'originale va nel **Cestino di questa cartella**, da cui si ripristina con **Apri il cestino**.
save-confirm-permanent = Salvare le modifiche sopra **{ $name }**?

    L'originale viene eliminato **definitivamente**.
save-yes = Salva
save-failed = Impossibile salvare: { $detail }
kept-failed-phone = **{ $name }** non è stato salvato sul telefono: { $error }

    L'originale sul telefono è intatto.

    L'immagine modificata resta su questo computer finché non viene salvata lì, in:

    { $folder }
kept-failed-server = **{ $name }** non è stato salvato sul server: { $error }

    L'originale sul server è intatto.

    L'immagine modificata resta su questo computer finché non viene salvata lì, in:

    { $folder }
kept-retry = Riprova
kept-open-folder = Apri la cartella
kept-saved = Salvata come **{ $path }**.

    La copia su questo computer resta dov'era, finché l'immagine non viene salvata al suo posto.
status-modified = Modificata, non salvata
status-saving = Salvataggio in corso…
status-preparing = Preparo il salvataggio…
batch-turn-confirm = { $n ->
    [one] **{ $op }**: **1 immagine** verrà sovrascritta.
   *[other] **{ $op }**: fino a **{ $n } immagini** verranno sovrascritte.
}
batch-originals-trash = Gli originali vanno **nel cestino**.
batch-originals-folder = Gli originali vanno nel **Cestino di questa cartella**, da cui si ripristinano con **Apri il cestino**.
batch-originals-permanent = Gli originali vengono eliminati **definitivamente**.
batch-lossy = I JPEG vengono ruotati senza perdere qualità quando i lati sono multipli di 16 pixel; gli altri, e i WebP, gli AVIF e i GIF, vengono ricodificati e possono perdere un po' di qualità.
batch-skipped = { $n ->
    [one] **1 immagine** viene esclusa: il suo formato non si può risalvare (HEIC, RAW…).
   *[other] **{ $n } immagini** vengono escluse: il loro formato non si può risalvare (HEIC, RAW…).
}
batch-none = Nessuna di queste **{ $n } immagini** si può risalvare: il loro formato non si può scrivere (HEIC, RAW…). Aprile una alla volta e usa **Salva come…**.
batch-save-as-hint = Con **Salva come…** la salvi già ruotata in un file nuovo, nel formato che scegli. L'originale resta com'è.
status-saving-n = Salvataggio in corso: { $n }…
save-lossy = Questa immagine viene **ricodificata** e può perdere un po' di qualità: un JPEG viene ruotato senza perdita solo quando i lati sono multipli di 16 pixel, i WebP con perdita, gli AVIF e i GIF vengono sempre ricodificati.
save-skipped = Le modifiche non si possono salvare sopra **{ $detail }**: è animata, ha più pagine o contiene più di un'immagine (il video di una foto in movimento, una mappa di profondità), e queste parti andrebbero perse.
save-animated = Le modifiche non si possono salvare sopra **{ $detail }**: è un'immagine animata, e salvandoci sopra resterebbe solo il primo fotogramma.
save-pages = Le modifiche non si possono salvare sopra **{ $detail }**: ha più pagine, e salvandoci sopra resterebbe solo la prima.
save-extra = Le modifiche non si possono salvare sopra **{ $detail }**: contiene anche il video di una foto in movimento o una mappa di profondità, che andrebbero persi.
save-temp-copy = Questa immagine **non si può salvare sopra se stessa**: è una copia temporanea che Windows ha fatto aprendo il file da un telefono o da un altro dispositivo, non il file che sta lì. Salvarci sopra cambierebbe solo la copia, che Windows può cancellare. Usa **Salva come…** per tenere le modifiche.
save-gvfs = Qui le immagini **non si possono salvare sopra se stesse**: questa cartella è aperta tramite il file manager (gvfs), dove sostituire un file può far perdere l'originale.
save-no-rename = Qui le immagini **non si possono salvare sopra se stesse**: questa sorgente non sa rinominare i file.
batch-animated = I file **animati**, **multipagina** e le **foto in movimento** vengono saltati. Le foto Ultra HDR vengono ruotate insieme alla loro mappa HDR.
batch-skipped-names = Saltate perché animate, multipagina o foto in movimento:
batch-done = Immagini salvate: **{ $n }**.
save-psd-flatten = **Questo PSD verrà appiattito**: livelli, testi, effetti e maschere si perdono, resta solo l'immagine come si vede. Per tenerli usa **Salva come…** o modificalo con Photoshop.
batch-psd-flatten = I PSD con più livelli, testi o effetti vengono **appiattiti**: resta solo l'immagine come si vede.
save-format = Le modifiche non si possono salvare sopra **{ $name }**: EyeCanSee non sa scrivere questo formato (HEIC, RAW…).
save-as-hint = Con **Salva come…** la salvi in un file nuovo, nel formato che scegli. L'originale resta com'è.
notice-close = Chiudi

## File > Apri indirizzo
addr-title = Apri indirizzo
addr-or = oppure compila i campi:
addr-protocol = Protocollo
addr-host = Server
addr-port = Porta
addr-user = Utente
addr-user-hint = anonimo
addr-password = Password
addr-folder = Cartella
addr-bad-port = La porta deve essere un numero da 1 a 65535.
addr-open = Apri
addr-name = Nome
addr-key = File della chiave
addr-key-hint = ~/.ssh/id_ed25519
addr-passphrase = Passphrase della chiave
srv-disconnect = Chiudi connessione
srv-edit = Modifica…
vault-unlock-title = Server salvati
vault-unlock-text = I server salvati sono cifrati con la password principale. Scrivila per usarli fino alla chiusura di EyeCanSee.
vault-unlock = Sblocca
vault-unlock-row = Sblocca i server salvati…
vault-wrong = Password sbagliata.
vault-unreadable = Il file dei server non si legge: { $detail }
vault-on = 🔒 Password principale attiva: il file dei server è cifrato.
vault-on-hint = Ogni modifica viene scritta subito, cifrata. EyeCanSee chiede la password una volta per sessione, al primo server.
vault-off = ⚠ Nessuna password principale: password e passphrase sono salvate in chiaro.
vault-off-hint = Chiunque possa leggere i tuoi file può leggerle. Una password principale cifra tutto il file dei server.
vault-set = Imposta una password principale…
vault-change = Cambia la password principale…
vault-remove = Togli la password principale…
vault-remove-warning = Senza la password principale il file dei server torna in chiaro: password e passphrase si possono leggere da chiunque legga i tuoi file.
vault-remove-yes = Toglila e salva in chiaro
vault-new = Nuova password principale
vault-again = Scrivila di nuovo
vault-strength-0 = Molto debole
vault-strength-1 = Debole
vault-strength-2 = Discreta
vault-strength-3 = Forte
vault-strength-4 = Molto forte
vault-too-weak = Non abbastanza forte: usa una password più lunga, per esempio più parole senza legame tra loro.
vault-different = Le due password sono diverse.
vault-forget = Se la dimentichi, i server salvati non si recuperano: conservala in un gestore di password.
vault-save = Salva la password principale
vault-plain-warning =
    { "**" }Server salvati in chiaro.**

    Senza una password principale, password e passphrase dei server salvati sono scritte in chiaro nel file dei server: chi può leggere i tuoi file può leggerle. Puoi impostarla in alto in File > Gestione server. Questo avviso non compare più.
srv-title = Gestione server
srv-new = Nuovo server
srv-connect = Salva e connetti
srv-save = Salva
srv-delete = Elimina
srv-close = Chiudi
srv-empty = Nessun server salvato: compila i campi e premi Salva.
srv-file = Salvati in { $path }, password in chiaro.
srv-file-vault = Salvati cifrati in { $path }.
srv-delete-ask = Eliminare "{ $name }" dai server salvati?
srv-save-failed = Impossibile salvare i server: { $detail }
kiod-ask = KDE tiene il telefono con il suo servizio kiod, quello che Dolphin usa per i telefoni. Per aprirlo, EyeCanSee deve chiudere quel servizio. KDE lo riavvia da solo, ma **dimentica le password delle cartelle di rete** (Samba, SFTP…) aperte in Dolphin e le chiederà di nuovo al prossimo accesso.

    Questo **non dipende da EyeCanSee**: KDE tiene i telefoni e quelle password nello stesso servizio, e non permette di liberare solo il telefono.
kiod-ok = OK

# Trova duplicati
menu-duplicates = Trova duplicati…
dup-ask = Cercare le immagini identiche in **{ $folder }**?

    Identiche vuol dire lo stesso file, byte per byte.
dup-sub = Anche nelle sottocartelle
dup-phone = Sul telefono ci vuole tempo: ogni immagine che potrebbe essere una copia viene letta per intero.
dup-search = Cerca
dup-searching = Ricerca delle immagini identiche…
dup-progress = Esaminati { $n } file su { $m }, { $size } letti
dup-title = Duplicati in { $folder }
dup-order = Ordine
dup-by-date = Data, dalla più vecchia
dup-by-name = Nome più corto
dup-by-path = Percorso
dup-keep-first = Tieni la prima
dup-keep-last = Tieni l'ultima
dup-keep-all = Tienile tutte
dup-group = { $n } copie identiche, { $size } ciascuna
dup-stays = Resta
dup-mark = Elimina questa copia
dup-last-one = Almeno una copia resta
dup-view = Apri nel visore
dup-summary = Gruppi: { $groups }. Copie da eliminare: { $n }, spazio liberato { $size }.
dup-delete = Elimina le copie segnate…
dup-none = Nessun duplicato in questa cartella.
menu-batch = Converti e ridimensiona…
menu-export-drawings = Esporta disegno…
menu-partial = * Solo su una parte dei file scelti
menu-partial-hint = Le voci con * lavorano solo sui file dei tipi per cui sono fatte (per esempio Converti e ridimensiona su foto e video); gli altri file scelti vengono esclusi.
menu-copy-meta = Copia con i metadati…
menu-strip-audio = Copia senza audio…
menu-extract-audio = Salva l'audio in un file…
batch-title = Converti e ridimensiona
batch-title-strip = Copia senza audio
batch-title-extract = Salva l'audio in un file
batch-strip-hint = Crea un video nuovo, uguale all'originale ma senza audio. L'originale non viene toccato. Il video e le sue altre tracce (sottotitoli, capitoli) vengono copiati così come sono, senza l'audio: nessuna ricodifica, nessuna perdita, metadati conservati. Il file nuovo ha il formato dell'originale.
batch-extract-hint = Salva l'audio del video in un file a parte (per esempio .m4a). Il video non viene toccato. Ogni traccia audio viene copiata così com'è in un file suo, nel formato del suo codec (AAC come .m4a, Opus come .opus, MP3 come .mp3, FLAC come .flac): nessuna ricodifica, nessuna perdita, metadati conservati. Per un altro formato usa Converti e ridimensiona.
batch-audio-not-ready = FFmpeg non è ancora pronto.
batch-images = { $n ->
    [one] **1 file** in { $folder }.
   *[other] **{ $n } file** in { $folder }.
}
batch-same-format = Uguale all'originale
batch-same-format-hint = Ogni immagine nel suo formato, scritta come la scrive Salva (un JPEG con la sua qualità). I formati che EyeCanSee non sa scrivere (HEIC, RAW…) vengono esclusi.
batch-size = Dimensioni
batch-size-same = Invariate
batch-size-percent = Percentuale dell'originale
batch-size-fit = Entro
batch-pixels = pixel (proporzioni mantenute)
batch-enlarge = Ingrandisci anche i file più piccoli
batch-filter = Filtro
batch-filter-lanczos = Lanczos (il più nitido)
batch-filter-catmull = Catmull-Rom (nitido)
batch-filter-gaussian = Gaussiano (morbido)
batch-filter-bilinear = Bilineare (veloce)
batch-filter-nearest = Pixel più vicino (pixel art)
batch-where = Dove
batch-same-folder = Accanto agli originali
batch-subfolder = Nella sottocartella
batch-subfolder-name = convertite
batch-keep = Gli originali non vengono toccati. Un nome già presente prende " (2)".
batch-nothing = Scegli un altro formato, altre dimensioni o una rotazione: così le immagini perderebbero solo qualità.
batch-bad-folder = Scrivi il nome della sottocartella (senza / né \).
batch-start = Converti
batch-working = Conversione in corso
batch-counts = { $n } di { $total } file
batch-stopping = Mi fermo…
batch-result = File convertiti: **{ $n }**, in { $folder }.
batch-cancelled = Fermato dopo { $n } di { $total } file.
batch-left-out = Escluse:
batch-left-out-kind = { $n ->
    [one] 1 file scelto è escluso: questa voce non lavora su quel tipo di file.
   *[other] { $n } file scelti sono esclusi: questa voce non lavora su quei tipi di file.
}
drawing-title = Esporta disegno
drawing-start = Esporta
drawing-side = Lato lungo
drawing-pixels = pixel
drawing-side-hint = Il disegno viene ridisegnato a questa misura, quindi resta nitido anche grande.
drawing-svg-hint = Un disegno che resta nitido a ogni ingrandimento, che si apre nei browser e nei programmi di grafica. Livelli e blocchi non vengono conservati.
drawing-cad-hint = Tutto il disegno nell'altro formato AutoCAD, nella versione in cui era salvato: livelli, blocchi, layout e stili. Ogni file viene riletto e confrontato con l'originale: se qualcosa andrebbe perso non viene salvato, e il riepilogo lo dice.
batch-skip-format = { $detail }: EyeCanSee non sa scrivere il suo formato; scegli un altro formato.
batch-skip-inside = { $detail }: animata o con più pagine; il file nuovo avrebbe solo la prima.
batch-failed = { $detail }
batch-turn = Rotazione e ribaltamenti
batch-turn-none = Nessuna rotazione
batch-turn-cw = 90° in senso orario
batch-turn-180 = 180°
batch-turn-ccw = 90° in senso antiorario
batch-turn-order = Prima i ribaltamenti, poi la rotazione.
batch-format-options = Opzioni { $format }
batch-other = In un'altra cartella, anche su un altro dispositivo
batch-browse = Sfoglia…
batch-other-hint = Una cartella di questo computer, oppure un indirizzo come nella barra degli indirizzi (mtp://…, sftp://…). Deve esistere.
pick-title = Scegli la cartella
pick-use = Usa questa cartella
pick-home = Cartella home
pick-up = Cartella superiore
pick-empty = Nessuna sottocartella
batch-no-dest = Scegli l'altra cartella.
batch-not-folder = { $detail } è un file, non una cartella.
menu-slideshow = Carosello
opt-tab-slideshow = Carosello
opt-tab-video = Video
opt-engine = Motore video (libmpv) e FFmpeg da usare
opt-engine-hint = I video li riproduce libmpv e li converte FFmpeg. Entrambi possono venire dal sistema (su Linux i pacchetti della distribuzione, meglio integrati con la scheda video; su Windows una copia accanto al programma) oppure essere scaricati una volta sola da EyeCanSee. A EyeCanSee servono almeno libmpv 1.109 (mpv 0.33) e FFmpeg 4.3.
opt-engine-system-recent = Di sistema, se abbastanza recente; altrimenti quello scaricato
opt-engine-system = Sempre di sistema, qualunque versione sia
opt-engine-downloaded = Sempre quello scaricato
opt-engine-restart = Una modifica vale dal prossimo avvio di EyeCanSee.
opt-engine-mpv = Motore video adesso: { $what }
opt-engine-ffmpeg = FFmpeg adesso: { $what }
engine-system = di sistema, { $version }
engine-downloaded = quello scaricato, { $version }
engine-missing = non trovato; si scarica quando un video ne ha bisogno
engine-too-old = quello di sistema è troppo vecchio ({ $found }, serve almeno { $required }); lo scaricamento viene proposto quando un video ne ha bisogno
engine-system-only = non trovato, ed è permesso solo quello di sistema
opt-show-secs = Ogni immagine resta per
opt-show-fade = Passaggio tra le immagini
opt-show-loop = Ricomincia dopo l'ultima immagine
fade-none = Nessuno (stacco netto)
fade-cross = Dissolvenza incrociata
fade-black = Dissolvenza al nero
fade-slide = Scorrimento
fade-zoom = Zoom e dissolvenza
show-hint = Esc: esci | Spazio: pausa | ← →: immagine precedente e successiva
show-paused = In pausa (Spazio per continuare)
menu-new-folder = Nuova cartella…
menu-new-folder-in = Nuova cartella qui dentro…
key-new-folder = Ctrl+Maiusc+N
key-batch = Ctrl+B
key-duplicates = Ctrl+D
tb-more = Altre operazioni
tb-off-nothing = Seleziona prima un file o una cartella.
tb-off-viewer = Non nel visore: torna alla griglia.
tb-off-paste = Niente da incollare: prima Copia o Taglia.
tb-off-paste-here = Qui non si può incollare (cestino, elenco delle memorie di un telefono, o una copia è già in corso).
tb-off-folder = Qui non si può creare una cartella (cestino, elenco delle memorie di un telefono).
tb-off-no-picture = Seleziona prima una o più immagini.
tb-off-not-picture = Solo le immagini si ruotano e si modificano.
tb-off-save = Niente da salvare: prima ruota l'immagine nel visore.
tb-off-one-picture = Seleziona prima una sola immagine.
tb-off-busy = Un'elaborazione è già in corso.
tb-off-batch = Seleziona prima immagini o video.
tb-off-no-folder = Apri prima una cartella.
tb-off-trash = Non nel cestino.
new-folder-title = Nuova cartella
new-folder-ok = Crea
new-folder-name = Nuova cartella

batch-video = Video
batch-video-container = Contenitore
batch-video-codec = Codifica
batch-video-copy = Uguale all'originale (senza ricodifica)
batch-video-encoder = Codificatore
batch-video-hw-note = In verde: lavora la scheda video, molto più veloce e senza appesantire il computer.
batch-video-unavailable = { $codec } (nessun codificatore su questo computer)
batch-video-none = Su questo computer nessun codificatore funziona per questa codifica.
batch-video-quality = Qualità:
batch-video-high = Alta
batch-video-medium = Media
batch-video-small = File piccolo
batch-video-audio = Mantieni l'audio
batch-video-meta = Mantieni i metadati (data, posizione, fotocamera)
batch-video-checking = Controllo quali codificatori funzionano su questo computer…
batch-video-need-ffmpeg = I video li converte FFmpeg, che su questo computer non c'è. Si scarica una volta sola ({ $size }) nella cartella dei dati dell'app.
batch-video-download = Scarica FFmpeg
batch-video-download-failed = Scaricamento non riuscito: { $detail }
batch-video-old-ffmpeg = L'FFmpeg di questo computer è troppo vecchio: { $found }, mentre a EyeCanSee serve almeno { $required }. EyeCanSee può scaricare il suo, una volta sola ({ $size }), nella cartella dei dati dell'app.
batch-video-system-only = FFmpeg su questo computer non c'è, e nelle Opzioni è scelto di usare solo quello di sistema. Installalo, oppure cambia la scelta in Strumenti > Opzioni > Video.
batch-video-not-ready = Scegli come convertire i video.
batch-skip-video = { $detail }: è un video e manca FFmpeg.
drawing-skip-not-autocad = { $detail }: non è un disegno AutoCAD (DWG o DXF), quindi non si può salvare come tale.
drawing-skip-would-lose = { $detail }: nell'altro formato una parte del disegno andrebbe persa, quindi non è stato salvato.
batch-no-audio = { $detail }: il video non ha audio.

video-codec-h264 = Si apre ovunque: telefoni, TV, browser, computer vecchi. File più grandi di HEVC e AV1.
video-codec-hevc = Circa metà del peso di H.264 a parità di qualità; si apre su telefoni, TV e computer recenti.
video-codec-av1 = I file più piccoli, senza royalty; si apre su browser, telefoni e TV recenti, non sui dispositivi vecchi.
video-codec-vp9 = Senza royalty, per il web (lo usa YouTube); più piccolo di H.264, un po' più grande di AV1.
video-codec-vp8 = Formato web vecchio: solo per i browser vecchi.
video-codec-mpeg4 = Formato vecchio (epoca DivX, Xvid): solo per lettori vecchi. File grandi.
video-codec-prores = Per i programmi di montaggio (Final Cut, DaVinci Resolve): qualità massima, file molto grandi.
video-codec-copy = Video e audio copiati così come sono, in pochi secondi e senza perdere qualità; scrive i metadati (anche la posizione delle dashcam). Rotazione e ridimensionamento non si applicano.
video-codec-gif = Immagine animata: niente audio, 256 colori, file molto grandi. Solo per spezzoni brevi.
video-enc-nvenc = Scheda video NVIDIA: molto veloce e non appesantisce il computer; file un po' più grandi che con il processore a parità di qualità.
video-enc-qsv = Grafica Intel (Quick Sync): veloce e non appesantisce il computer; file un po' più grandi che con il processore.
video-enc-amf = Scheda video AMD: veloce e non appesantisce il computer; file un po' più grandi che con il processore.
video-enc-mf = Il codificatore di Windows, sulla scheda video quando può: veloce; meno opzioni.
video-enc-vaapi = Scheda video tramite VA-API (Intel, AMD su Linux): veloce e non appesantisce il computer; file un po' più grandi.
video-enc-x264 = Processore (x264): il miglior H.264 per qualità e peso; più lento della scheda video e usa tutti i core.
video-enc-x265 = Processore (x265): qualità molto buona e file piccoli; lento, usa tutti i core.
video-enc-svtav1 = Processore (SVT-AV1): file piccoli a buona velocità; il miglior AV1 per quasi tutti gli usi.
video-enc-aom = Processore (libaom, l'AV1 di riferimento): i file più piccoli, ma molto lento.
video-enc-rav1e = Processore (rav1e): AV1 scritto in Rust; più lento di SVT-AV1.
video-enc-vpx = Processore (libvpx): buona qualità, lento.
video-enc-openh264 = Processore (OpenH264, di Cisco): H.264 semplice, file più grandi di x264. Spesso l'unico H.264 delle distribuzioni senza codec brevettati.
video-enc-plain = Processore (codificatore interno di FFmpeg).
