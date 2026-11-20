# Guardare le immagini

Il visore mostra una foto alla volta, grande quanto la finestra permette. Da lì passi alla foto successiva, ingrandisci, ruoti la foto e torni alla griglia.

![Guardare le immagini](../img/viewer.webp)

## Aprire e chiudere il visore

- Fai doppio clic su una foto nella griglia, oppure selezionala e premi **Invio** (anche **Visualizza > Apri**).
- Per tornare alla griglia: **Esc**, un doppio clic sulla foto, ![Torna alla griglia](icon://layout-grid) **Torna alla griglia** nella barra strumenti, oppure **Visualizza > Torna alla griglia**. Nella griglia resta selezionata la foto che stavi guardando.
- Anche la X della finestra torna alla griglia invece di chiudere EyeCanSee. Si può cambiare nelle [Opzioni](options.md).

Con più file selezionati, Invio apre il primo, e il visore si muove solo tra quelli.

## Passare da una foto all'altra

| Tasto | Cosa fa |
|---|---|
| →, Spazio, Pag↓ | Foto successiva |
| ←, Backspace, Pag↑ | Foto precedente |
| Inizio, Fine | Prima, ultima foto |

Le stesse voci sono nel menù **Visualizza** (Avanti, Indietro, Primo, Ultimo). Dopo l'ultima foto il visore si ferma; nelle [Opzioni](options.md) puoi farlo ricominciare dalla prima. Le foto vicine a quella mostrata vengono lette in anticipo, così si passa dall'una all'altra in fretta anche su un telefono o su un server.

## Immagini molto grandi

Un'immagine più grande di 8192 pixel sul lato lungo viene mostrata ridotta a 8192 pixel; un JPEG viene letto già ridotto, così occupa meno memoria. Un'immagine che per aprirsi chiederebbe più di metà della memoria del computer (per esempio un HEIC, un TIFF a 16 bit, un file di Photoshop o un JPEG XL di centinaia di megapixel, su un computer da 8 GB) non si apre subito: una finestra dice quanta memoria le serve e propone **Apri comunque**. Se la apri comunque, il computer può rallentare molto, oppure il sistema può chiudere EyeCanSee. **Annulla** è la scelta predefinita (anche con **Invio** ed **Esc**): l'immagine resta chiusa e il visore torna all'immagine di prima, o alla griglia; lo chiede di nuovo quando torni su quell'immagine. Un'immagine così grande non viene mai letta in anticipo come vicina, e la presentazione non la mostra. Nella griglia non ha la miniatura (vedi [La griglia delle miniature](grid.md)).

## Animazioni e pagine

Le immagini animate GIF, WebP e PNG (APNG) si muovono nel visore, ripetendosi. **Spazio** le mette in pausa e le fa ripartire, come il pulsante nella piccola barra in fondo all'immagine, che mostra anche il fotogramma (per esempio Fotogramma 3 / 20, anche nella barra di stato). L'animazione si ferma quando passi a un'altra immagine e riparte dal primo fotogramma quando torni. Un'animazione molto lunga o molto grande (più di 256 MB una volta decodificata) non viene riprodotta: il visore mostra il primo fotogramma e dice perché.

Un TIFF con più pagine (un documento scansionato, un fax) si apre sulla prima pagina. **Pag↓** e **Pag↑** sfogliano le pagine, come le frecce nella barra in fondo, che mostra la pagina (per esempio Pagina 2 / 5, anche nella barra di stato); dopo l'ultima pagina Pag↓ passa al file successivo, e prima della prima Pag↑ al precedente. Ogni pagina viene adattata alla finestra.

## Foto in movimento

Una foto in movimento (Google Pixel, Samsung, Xiaomi e altri telefoni) contiene un breve video registrato insieme alla foto. Il visore mostra la foto con un pulsante **Riproduci il movimento** in fondo; il pulsante o **Spazio** riproducono il video una volta, con la barra di un [video](video.md), poi torna la foto. Il video ha bisogno del motore video, come tutti i video.

## Ingrandire

- La **rotellina del mouse** ingrandisce e rimpicciolisce attorno al puntatore; **+** e **−** attorno al centro della finestra. Sul touchpad, avvicina o allontana due dita.
- **0** adatta di nuovo la foto alla finestra; **1** la mostra al 100%, un pixel della foto per ogni pixel dello schermo. Le percentuali contano i pixel del file, anche per una foto molto grande mostrata ridotta e per un disegno SVG (alla grandezza che dichiara).
- A destra nella barra di stato, accanto a **Tasti**: ![Riduci](icon://zoom-out) e ![Ingrandisci](icon://zoom-in) fanno come **−** e **+**; fai clic sulla percentuale e scrivine un'altra (per esempio **37** e **Invio**; **Esc** la lascia com'era), oppure trascina il cursore accanto: le sue tacche stanno su zoom tondi (10%, 25%, 50%, 100%, 200%…), e più è a sinistra, più la foto è piccola.
- Trascina una foto ingrandita per spostarla.
- Sotto il 100% ogni punto dello schermo mostra la media dei pixel che copre, a ogni zoom: i bordi restano lisci e i dettagli fini (erba, capelli) non diventano granulosi. Si può spegnere nelle [Opzioni](options.md) > Aspetto.

Una foto si apre adattata alla finestra, e una foto piccola alla sua grandezza vera, senza ingrandirla. Puoi rimpicciolirla sotto la misura adattata, fino all'1% (un panorama lungo si ferma quando il lato corto è di 16 pixel sullo schermo): rimpicciolendo, la rotellina e **−** si fermano una volta sulla misura adattata, poi proseguono; più piccola della finestra, la foto resta al centro. Un disegno SVG si ingrandisce fino a riempire la finestra, anche quando è più piccolo. Un disegno CAD non si vede mai più piccolo che adattato alla finestra. L'ingrandimento massimo (2000% all'inizio) si regola nelle [Opzioni](options.md); per un disegno SVG più piccolo di 2048 pixel si conta sui 2048 pixel a cui viene disegnato, quindi la sua percentuale può salire molto di più. Nelle Opzioni puoi anche far cambiare foto alla rotellina invece di ingrandire: allora **Ctrl** e la rotellina ingrandiscono comunque. La foto successiva parte di nuovo adattata.

## La barra di stato del visore

In fondo: il nome del file, la sua posizione nella cartella (per esempio 12 / 51), le dimensioni in pixel (le sue, come nel pannello dei metadati, anche quando è mostrata ridotta, e per un disegno SVG quelle che dichiara), il peso su disco e la sua data: quella dello scatto se i metadati sono già stati letti nella griglia, altrimenti quella di modifica del file. Dopo una rotazione non ancora salvata dice anche **Modificata, non salvata**. A destra, su una foto, lo zoom con i suoi pulsanti e il cursore (vedi Ingrandire, sopra).

Il pulsante **Tasti** a destra elenca i tasti di ciò che stai guardando (una foto, un video, un modello 3D o un disegno CAD): finché il mouse ci sta sopra, o finché non ci fai clic di nuovo.

## La barra strumenti nel visore

La barra strumenti tiene solo ciò che lavora sulla foto mostrata: ![Rinomina…](icon://text-cursor-input) **Rinomina…**, ![Elimina…](icon://trash) **Elimina…**, ![Ruota di 90° in senso antiorario](icon://rotate-ccw-square) e ![Ruota di 90° in senso orario](icon://rotate-cw-square) per ruotarla, ![Modifica immagine…](icon://square-pen) **Modifica immagine…**, ![Salva](icon://save) **Salva** e ![Salva come…](icon://file-output) **Salva come…**, poi i segnalibri e la barra degli indirizzi. Nuova cartella, taglia, copia, incolla e gli strumenti sono per la griglia; restano nei menù. Su un video o un file audio i pulsanti per le immagini restano al loro posto, grigi: passando col mouse su uno compare il motivo.

## Schermo intero

**F11** mostra la foto da sola, senza menù né barre, su fondo nero. **Esc** prima esce dallo schermo intero, poi torna alla griglia.

Un video a schermo intero riempie lo schermo: la sua barra compare sopra il fondo dell'immagine quando si muove il mouse e sparisce col puntatore dopo 2 secondi di mouse fermo (resta finché il puntatore è sopra).

## Cos'altro puoi fare

- Ruotare e riflettere la foto: vedi [Ruotare e riflettere](rotate.md).
- Disegnarci sopra, ritagliarla, nasconderne delle parti: vedi [Modificare un'immagine](editor.md) (**Ctrl+E**).
- Salvarne una copia in un altro formato: vedi [Salva come](save-as.md) (**Ctrl+Maiusc+S**).
- **F2** rinomina la foto e **Canc** la elimina, come nella griglia; dopo l'eliminazione compare la foto successiva.
- Il clic destro sulla foto apre un menù con queste azioni e altre.
