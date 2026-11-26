# Riprodurre i video

I video compaiono nella griglia insieme alle foto, con un fotogramma del video come miniatura e il simbolo ![Video](icon://color-video). Si aprono nel visore, dove partono con l'audio. Lì si ascoltano anche i file audio.

![Riprodurre i video](../img/video.webp)

## Guardare un video

Fai doppio clic su un video, oppure selezionalo e premi **Invio**: si apre nel visore, adattato alla finestra, e parte.

- **Spazio**, un clic sul video, oppure ![Riproduci](icon://play) / ![Pausa](icon://pause) sotto il video, avviano e mettono in pausa.
- La barra sotto il video mostra il tempo e il punto in cui sei: trascinala, o fai clic su un punto, per andare lì. Col puntatore sopra la barra, il tempo di quel punto compare prima del clic. Tieni premuto **Maiusc** mentre trascini per muoverti dieci volte più piano, e arrivare a un momento preciso.
- **Ctrl + ←** e **Ctrl + →** vanno indietro e avanti di 5 secondi.
- **.** e **,** vanno avanti e indietro di un fotogramma, e mettono in pausa il video lì. Tornare indietro chiede più tempo, anche un secondo su un video grande: intanto la barra di stato dice **Preparo il fotogramma precedente…**, e ogni pressione viene tenuta, un fotogramma dopo l'altro.
- Alla fine resta l'ultimo fotogramma; facendo ripartire il video, riparte dall'inizio.

## Salvare un fotogramma come immagine

**Ctrl + Maiusc + S**, ![Salva il fotogramma come…](icon://file-output) **Salva il fotogramma come…** nella barra strumenti, o la stessa voce nel menù File e nel menù del clic destro, mettono in pausa il video e aprono [Salva come](save-as.md) con il fotogramma mostrato, alle dimensioni del video e ruotato come lo vedi. Lì scegli formato, opzioni e nome; il nome proposto è quello del video con il tempo del fotogramma (per esempio `clip 1m02s345.jpg`). Dopo il salvataggio il visore resta sul video, in pausa.

Con **Mantieni i metadati** l'immagine tiene ciò che il video dice di sé: la fotocamera, la data e l'ora di quel fotogramma e il luogo: la traccia della dashcam in quel momento, la posizione scritta nel video o il punto di partenza di un drone.

## Audio

I comandi dell'audio sono a destra della barra: ![Silenzia](icon://volume-2) toglie l'audio e ![Riattiva l'audio](icon://volume-x) lo rimette, e il cursore regola il volume da 0 a 100, di cinque in cinque, con una tacca ogni 10 e il volume scritto accanto.

- **↑** e **↓**, o la rotellina del mouse sul video, alzano e abbassano il volume di 5.
- **M** toglie e rimette l'audio.

Volume e audio spento restano per i video successivi e alla prossima apertura di EyeCanSee. Su un video senza audio i due comandi sono grigi.

## Passare da un file all'altro

| Tasto | Cosa fa |
|---|---|
| →, Pag↓ | File successivo |
| ←, Backspace, Pag↑ | File precedente |
| Inizio, Fine | Primo, ultimo file |
| Esc | Torna alla griglia |
| Doppio clic | Torna alla griglia, oppure schermo intero e ritorno, come scelto nelle [Opzioni](options.md) |
| F11 | Schermo intero: solo il video, con la sua barra sopra l'immagine finché si muove il mouse |

**F2** rinomina il video e **Canc** lo elimina, come per le foto. Il pulsante **Tasti** nella barra di stato elenca questi tasti.

## File audio

I file audio (per esempio MP3, M4A, FLAC, Opus) compaiono nella griglia con la loro copertina e il simbolo ![Audio](icon://color-audio), oppure con una nota se non hanno copertina. Il visore li riproduce con la stessa barra e lo stesso volume; la copertina si vede come immagine.

## Il motore video

I video li riproduce mpv. Quando EyeCanSee ha bisogno di una copia sua (di solito su Windows; su Linux quando il sistema non ce l'ha, ne ha una troppo vecchia o una che non sa riprodurre H.264 e HEVC, i formati in cui registrano i telefoni), il visore propone di scaricarla la prima volta che apri un video, e in alto a destra nella barra dei menù compare il pulsante rosso **Scarica il motore video** appena la cartella aperta contiene un video o un file audio. Si scarica una volta sola. Vedi [Componenti aggiuntivi](components.md).

Fino ad allora la miniatura di un video è l'anteprima salvata dalla videocamera, quando c'è, oppure il simbolo del video con ![Download](icon://color-download).

## Da sapere

- I video su un telefono o su un server si leggono un pezzo alla volta mentre scorrono, anche da un telefono su Windows. Solo un telefono che non permette di saltare in mezzo a un file (Windows se ne accorge alla prima lettura lenta) ha i video copiati tutti prima, con una barra che mostra quanto ne è stato letto.
- Un file che non si riproduce dice perché in mezzo al visore, con il messaggio intero nella barra in fondo, come una foto che non si apre: **Il file non è un video, o è rovinato**, **Nessun fotogramma si può mostrare: il video è rovinato**, oppure, per un file di solo audio quando non si può aprire nessuna uscita audio (nessuna scheda audio, il server dell'audio spento), **L'audio non si può riprodurre: non si è potuta aprire nessuna uscita audio**. Quando un telefono o un server smette di dare il video (staccato, sparito), si vede il suo messaggio. Finché un telefono o un server è solo lento, il video lo aspetta, per tutto il tempo che serve; quando per 20 secondi non arrivano dati, un messaggio in mezzo al visore lo dice. Sparisce appena i dati tornano ad arrivare, e il video parte; **Esc** torna subito alle miniature.
- Modificare nel visore vale per le foto; di un video, Salva come… salva il fotogramma mostrato. Per ruotare, rimpicciolire o convertire un video usa [Convertire i video](convert-video.md).
- Il pannello dei metadati mostra i dati di un video selezionato: vedi [Informazioni sul file](metadata.md).
- Quali formati si riproducono: vedi [Formati supportati](formats.md).
