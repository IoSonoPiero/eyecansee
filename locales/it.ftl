# EyeCanSee, italiano.

# Nome della lingua scritto in questa lingua, per la scelta nelle Opzioni.
language-name = Italiano

counter = { $n } di { $total }
folder-empty = Questa cartella non contiene immagini né sottocartelle
cannot-load = Impossibile aprire { $file }: { $error }
video-no-mpv = Per i video serve libmpv, che non è stata trovata. Installare il pacchetto con libmpv: mpv-libs su Fedora, libmpv2 su Debian, Ubuntu e openSUSE (libmpv1 su Ubuntu 22.04), mpv su Arch.
video-component-title = Motore video
video-component-restart-needed = Il motore video è scaricato. Per attivarlo EyeCanSee deve ripartire.
video-component-restart = Riavvia EyeCanSee
video-component-ask = Per guardare i video EyeCanSee ha bisogno del motore video (mpv), da scaricare una volta sola: { $size }.
video-component-download = Scarica il motore video
video-component-button = Scarica il motore video ({ $size })
video-component-progress = Scaricamento: { $done } di { $total }
video-component-error = Scaricamento non riuscito: { $error }
video-play = Riproduci (Spazio)
video-pause = Pausa (Spazio)
error-unsupported = Protocollo non supportato: { $detail }
error-no-host = L'indirizzo non contiene l'host: { $detail }
error-path = Impossibile aprire { $detail }
error-source = Impossibile collegarsi alla sorgente: { $detail }
error-detail = { $detail }
error-login = Il server ha rifiutato l'accesso: nome utente o password sbagliati.
error-not-found = Non trovato: { $detail }
error-device-busy = Il dispositivo è in uso da un'altra applicazione (per esempio il gestore dei file). Chiudila, oppure scollega e ricollega il dispositivo. ({ $detail })
error-device-kde = Il telefono è in uso da KDE (il suo servizio kiod). Premi F5 per aprirlo comunque in EyeCanSee. ({ $detail })
error-no-device = Il dispositivo non è stato trovato: se è collegato, sbloccalo e scegli "Trasferimento file". EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })
error-not-ready = Il telefono non risponde: sbloccalo e scegli "Trasferimento file" nella sua notifica USB. EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })
error-no-storage = Il dispositivo non mostra nessuna memoria: sbloccalo e scegli "Trasferimento file". EyeCanSee riprova da solo ogni 2 secondi. ({ $detail })

menu-file = File
menu-open-location = Apri indirizzo…
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
status-deleting = Eliminazione in corso: { $n }…
status-copied = { $n } copiati: Ctrl+V per incollare
status-cut = { $n } tagliati: Ctrl+V per spostare
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
show-all = Immagini e video
show-images = Solo immagini
show-videos = Solo video
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
convert-file-size = { $size } su disco
convert-ratio = { $percent }% dell'originale
convert-format = Formato
convert-quality = Qualità
convert-speed = Velocità
convert-speed-hint = Più lenta fa un file più piccolo a parità di qualità.
convert-lossless = Senza perdita: gli stessi pixel dell'originale.
convert-gif = Al massimo 256 colori.
convert-keep-meta = Mantieni i metadati (EXIF, profilo colore)
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
rc-rotation = Rotazione
rc-reset = Torna a 0°
rc-straighten = Raddrizza
rc-straighten-hint = Traccia una linea lungo l'orizzonte, o lungo qualcosa di verticale: l'immagine ruota per metterla dritta.
rc-one-only = Ruota e ritaglia lavora su una foto alla volta. Per ruotare più foto insieme usa le voci qui sopra o Converti e ridimensiona.
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
rc-apply = Applica
rc-apply-hint = Mette rotazione e ritaglio nell'immagine (Invio): poi puoi continuare a modificarla. Il file si salva alla fine, con Salva.
rc-pending = Rotazione e ritaglio non ancora applicati.
rc-unsaved = Modifiche applicate, non ancora salvate.
rc-save = Salva…
rc-save-hint = Scrive l'immagine modificata sopra l'originale, dopo aver chiesto.
rc-save-as-hint = Salva il risultato in un file nuovo, nel formato che scegli; l'originale resta com'è.
rc-close = Chiudi
rc-discard = Non salvare
rc-leave = Le modifiche a **{ $name }** non sono salvate.

    { "**" }Salva** le scrive sopra l'originale, **Salva come…** crea un file nuovo, **Non salvare** le perde.
rc-help = Trascina un lato o un angolo per ritagliare, l'interno per spostare il ritaglio, fuori per spostare la vista. Rotellina: zoom. Doppio clic: vista intera.
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
batch-title = Converti e ridimensiona
batch-images = { $n ->
    [one] **1 file** in { $folder }.
   *[other] **{ $n } file** in { $folder }.
}
batch-same-format = Uguale all'originale
batch-same-format-hint = Ogni immagine nel suo formato, scritta come la scrive Salva (JPEG con qualità 95). I formati che EyeCanSee non sa scrivere (HEIC, RAW…) vengono esclusi.
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
opt-show-secs = Ogni immagine resta per
opt-show-fade = Passaggio tra le immagini
opt-show-loop = Ricomincia dopo l'ultima immagine
fade-none = Nessuno (stacco netto)
fade-cross = Dissolvenza incrociata
fade-black = Dissolvenza al nero
fade-slide = Scorrimento
fade-zoom = Zoom e dissolvenza
show-hint = Esc: esci · Spazio: pausa · ← →: immagine precedente e successiva
show-paused = In pausa (Spazio per continuare)
menu-new-folder = Nuova cartella…
menu-new-folder-in = Nuova cartella qui dentro…
key-new-folder = Ctrl+Maiusc+N
new-folder-title = Nuova cartella
new-folder-ok = Crea
new-folder-name = Nuova cartella

batch-video = Video
batch-video-container = Contenitore
batch-video-codec = Codifica
batch-video-copy = Uguale all'originale (senza ricodifica)
batch-video-encoder = Codificatore
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
batch-video-not-ready = Scegli come convertire i video.
batch-skip-video = { $detail }: è un video e manca FFmpeg.

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
