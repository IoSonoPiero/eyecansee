# Manuale di EyeCanSee

Premi **F1** in qualsiasi momento per aprire o chiudere questo manuale.

## La finestra

- **Barra dei menù**: File, Modifica, Immagine, Visualizza, Strumenti, Aiuto. A destra il numero di versione; **Aiuto → Informazioni su EyeCanSee** mostra versione, autore, licenza e il link al codice sorgente. Ogni azione che si fa con la tastiera o col doppio clic ha anche una voce di menù, con il tasto scritto accanto.
- **Barra strumenti**: cartella superiore, aggiorna, metadati (ℹ), barra degli indirizzi, ordine delle immagini e cursore per la dimensione delle miniature.
- **Albero delle cartelle** a sinistra, sotto **Questo computer** (e sotto **Dispositivi** se ne hai collegati): clic su una cartella per mostrarne il contenuto. Le cartelle vengono lette solo quando le espandi, così le sorgenti remote restano veloci. Quando cambi cartella, anche dalla griglia, l'albero si apre fino a quella cartella e la evidenzia.
- **Griglia** a destra: la cella **..** per salire di un livello (non in cima a un disco o a un dispositivo), le sottocartelle, poi le immagini come miniature, distribuite su tutta la larghezza. Entrando in una cartella nessun elemento è selezionato; risalendo è selezionata la cartella da cui vieni.
- **Ordine delle immagini** dalla tendina nella barra strumenti (con accanto il pulsante ▲/▼ per invertirlo) o da **Visualizza → Ordina per**: per nome, data di modifica, dimensione o tipo (estensione), più **Ordine decrescente**. Le cartelle restano prima, per nome; i file senza data restano in fondo. La scelta viene ricordata.
- **Metadati** sotto la griglia, per l'immagine selezionata: riquadri **Fotocamera** (marca, modello, obiettivo, software), **Scatto** (data, esposizione, diaframma, ISO, lunghezza focale, flash), **Immagine** (formato, dimensioni, colore e bit per canale, spazio colore; formato, dimensioni e colore anche per le immagini senza EXIF) e **Posizione** (coordinate GPS, altitudine, con il link per aprirle sulla mappa di OpenStreetMap). Ogni riquadro si chiude con la sua X e si riapre da **Visualizza → Riquadri dei metadati**; il pulsante ℹ nella barra strumenti li chiude o li riapre tutti. Il pannello si comprime e si riapre col pulsante ▼/▲ a destra della sua barra, o con un clic sulla barra stessa; compresso resta solo la barra, e i metadati non vengono letti. Si leggono i metadati EXIF di JPEG, HEIC, AVIF, PNG, WebP e TIFF; per leggerli EyeCanSee rilegge il file, quindi su un telefono ci vuole un attimo; una volta letti restano salvati con le miniature e la volta dopo compaiono subito (finché il file non cambia; svuotando la cache delle miniature si svuotano anche loro).
- **Barra di stato** in basso: quante cartelle e immagini ci sono, e nome, peso e data di modifica dell'elemento selezionato; con più elementi selezionati, quanti sono e il peso delle loro immagini.
- **Più elementi**: Ctrl+clic aggiunge o toglie un elemento, Maiusc+clic seleziona tutto dall'ultimo elemento cliccato fino a questo, Maiusc+frecce allargano la selezione, Ctrl+A seleziona tutto nella cartella. **Trascinando** col mouse si disegna un rettangolo che seleziona le celle che tocca (con Ctrl le aggiunge a quelle già selezionate); vicino al bordo in alto o in basso la griglia scorre. Un clic senza tasti, o una freccia, torna a un solo elemento.
- Frecce e Tab agiscono sulla parte su cui hai cliccato per ultima, sinistra o destra: è quella con la cornice sottile. Lì la selezione è blu pieno, nell'altra più tenue.

Doppio clic su un'immagine, oppure selezionala e premi Invio, per aprirla nel **visore**. Per tornare alla griglia premi Esc, fai doppio clic sull'immagine o usa il pulsante griglia nella barra strumenti. La barra in basso nel visore mostra il nome del file, la sua posizione nella cartella (per esempio 12 / 51), le dimensioni in pixel e su disco, e la data di scatto (la data di modifica se la foto non ce l'ha o se i riquadri dei metadati sono chiusi).

## Miniature

EyeCanSee crea le miniature in questo ordine: prima l'immagine selezionata, poi quelle visibili nella finestra dall'alto in basso, poi tutte le altre della cartella dall'alto in basso. Facendo clic su un'immagine la sua miniatura passa in cima alla coda. Fino a 1000 miniature per cartella restano in memoria, così scorrendo, anche trascinando la barra, non spariscono; nelle cartelle più grandi quelle lontane dalla parte visibile si ricaricano dal disco.

Le miniature vengono salvate su disco, così riaprire la stessa cartella è immediato: nel file `~/.cache/eyecansee/thumbs.db` su Linux e `%LOCALAPPDATA%\eyecansee\thumbs.db` su Windows. Se un'immagine cambia (dimensione o data di modifica), la miniatura viene rifatta. Puoi cancellare quel file (con EyeCanSee chiuso) in qualsiasi momento: le miniature si ricreano quando servono.

**Pulizia della cache miniature** in **Strumenti > Opzioni…** (spenta all'inizio): una volta per sessione, per ogni indirizzo aperto, elimina le miniature delle immagini cancellate e rifà quelle delle immagini modificate; poi elimina le miniature usate meno di recente finché la cache sta nel **Limite della cache** (1024 MB all'inizio). Le cartelle che non si trovano o non si raggiungono vengono saltate, così un disco staccato o un NAS spento non perdono le loro miniature. Le sorgenti remote vengono controllate solo quando le apri.

## Opzioni

**Strumenti > Opzioni…** apre la finestra delle opzioni, divisa in schede: **Generale** (ultima cartella, finestra all'avvio, lingua, aggiornamenti), **Aspetto** (colori), **Navigazione** (scorrimento, visore), **Eliminazione**, **Miniature** e **Carosello**. Ogni modifica viene salvata subito in `~/.config/eyecansee/settings.json` su Linux e `%APPDATA%\eyecansee\settings.json` su Windows, un file di testo che puoi anche modificare a mano.

- **Riapri l'ultima cartella all'avvio** (attiva all'inizio): quando EyeCanSee parte senza una cartella, per esempio dal menù delle applicazioni, apre quella che stavi guardando alla chiusura. Se quella cartella non esiste più, apre la tua cartella home. Gli indirizzi remoti si riaprono senza password, che non viene mai salvata: per un server FTP con utente si apre **File > Apri indirizzo…** con tutto già compilato tranne la password, pronta da scrivere (Annulla resta nella cartella home).
- **Ricorda la posizione di scorrimento di ogni cartella** (spenta all'inizio): tornando in una cartella già vista, la griglia riparte dal punto in cui l'avevi lasciata. Spenta, ogni cartella aperta parte dalla prima anteprima. Vale finché EyeCanSee resta aperto.
- **Nel visore, dopo l'ultima immagine ricomincia dalla prima** (spenta all'inizio): e dalla prima immagine, tornando indietro, mostra l'ultima. Spenta, il visore si ferma alla prima e all'ultima immagine.
- **Rotellina del mouse nel visore**: **Ingrandisce e rimpicciolisce** (predefinita) oppure **Mostra l'immagine successiva o precedente**. Tenendo premuto Ctrl, la rotellina ingrandisce sempre.
- **Ingrandimento massimo nel visore** (2000% all'inizio): fin dove la rotellina e il tasto + possono ingrandire un'immagine.
- **Finestra all'avvio**, una sola scelta: **Dimensione normale**, **Massimizzata** (occupa tutto lo schermo lasciando visibili le barre del sistema), oppure **Dimensione che aveva alla chiusura** (se era massimizzata si riapre massimizzata).
- **Controlla gli aggiornamenti all'avvio** (attiva all'inizio): vedi *Aggiornamenti*.
- **Lingua**: **Come il sistema** (predefinita: la lingua di Windows o del desktop, inglese se non è tra quelle disponibili) oppure una delle lingue elencate, ognuna col suo nome. La scelta vale subito, anche per questo manuale.
- **Colori**: **Come il sistema** (predefinito: chiari o scuri come il tema di Windows o del desktop, e cambiano insieme a lui), **Chiari** o **Scuri**. La scelta vale subito.
- **Pulizia della cache miniature** e **Limite della cache**: vedi *Miniature*.
- **Svuota la cache miniature…**: elimina tutte le miniature salvate, dopo una conferma con il numero di miniature e lo spazio liberato. Le immagini non vengono toccate; le miniature della cartella aperta si rifanno subito, le altre quando le riapri.
- **Elimina i file del computer definitivamente, senza cestino (pericoloso)** (spenta all'inizio): vedi *Eliminare e rinominare*.
- **Eliminare su telefoni e server**: vedi *Eliminare e rinominare*.
- **Carosello**: quanto resta ogni immagine, il passaggio tra le immagini e se ricomincia dopo l'ultima: vedi *Carosello*.

## Aggiornamenti

L'installer per Windows, lo zip per Windows e l'AppImage si aggiornano da soli. All'avvio EyeCanSee chiede a GitHub, in background, l'ultima versione pubblicata; se è più nuova la scarica e la controlla due volte: con l'impronta SHA-256 che GitHub pubblica per lei (un download rovinato) e con la firma dello sviluppatore, fatta con una chiave che non lascia mai il suo PC (un file che non viene dallo sviluppatore). Un file senza firma valida non viene mai installato, e Verifica aggiornamenti dice perché. Quando è pronta, in alto a destra nella barra dei menù compare un pulsante rosso **Installa la … e riavvia**: EyeCanSee si chiude, installa la versione nuova e si riapre. Finché non lo premi non viene installato niente.

- **Aiuto > Verifica aggiornamenti** fa lo stesso controllo quando vuoi e ti dice com'è andata.
- Il controllo si può spegnere in **Strumenti > Opzioni… > Generale**. Spento, EyeCanSee contatta GitHub solo quando scegli Verifica aggiornamenti. Non viene inviato niente su di te o sulle tue immagini: solo la richiesta dell'ultima versione.
- Su Fedora EyeCanSee si aggiorna con `dnf`, insieme al resto del sistema.

## Eliminare e rinominare

**File > Nuova cartella…**, **Nuova cartella…** dal clic destro su uno spazio vuoto della griglia, oppure **Ctrl+Maiusc+N** crea una cartella nella cartella aperta: il nome è già scritto ("Nuova cartella", con " (2)" se c'è già) e selezionato, così puoi scrivere quello che vuoi; **Crea** la crea e la seleziona nella griglia. Dal clic destro su una cartella (nella griglia o nell'albero), **Nuova cartella qui dentro…** la crea dentro quella cartella. Funziona sul computer, sui dischi, sui telefoni e sui server; non in un cestino né tra le memorie di un telefono.

**Canc** (o **File > Elimina…**) elimina l'immagine o la cartella selezionata nella griglia, o l'immagine aperta nel visore, dopo una conferma con il nome e la dimensione. Una cartella viene eliminata con tutto quello che contiene. Dall'albero a sinistra si eliminano le cartelle col clic destro. Nella conferma **Invio** elimina ed **Esc** annulla. Nel visore compare subito l'immagine successiva.

Con più elementi selezionati, **Canc** (o **Elimina…** dal clic destro su uno di loro) li elimina tutti, dopo una conferma con quanti sono e il peso delle immagini. Se uno non riesce, i successivi non vengono eliminati. Mentre un'eliminazione è in corso la barra di stato lo dice, con una rotellina, e non se ne può avviare un'altra (su un telefono una cartella con centinaia di file richiede diversi secondi).

- **File del computer e dischi esterni**: vanno nel cestino del sistema, da cui si possono ripristinare. Su Windows le chiavette, le schede SD e le unità di rete non hanno il Cestino: lì l'immagine va nella cartella nascosta `.EyeCanSee-trash` accanto a lei, come sui telefoni (vedi sotto), e la conferma lo dice. Con l'opzione **Elimina i file del computer definitivamente, senza cestino** vengono eliminati per sempre.
- **Telefoni e server (FTP, SFTP)** non hanno un cestino raggiungibile. Nelle Opzioni, sotto **Eliminare su telefoni e server**, scegli tra **Sposta nella cartella nascosta .EyeCanSee-trash accanto all'immagine** (predefinita: l'immagine finisce in una cartella `.EyeCanSee-trash` dentro la sua cartella, e si recupera con un gestore di file) ed **Elimina definitivamente**. L'FTP non sa spostare i file: lì serve **Elimina definitivamente**.

**Maiusc+Canc** (o **File > Elimina definitivamente…**) elimina per sempre solo quella volta, qualunque sia l'opzione.

**F2** (o **File > Rinomina…**) rinomina l'elemento selezionato o l'immagine aperta: scrivi il nuovo nome e premi **Invio** (**Esc** annulla). EyeCanSee non accetta un nome già presente nella cartella. Sui server SFTP le cartelle non si possono rinominare, e via FTP nulla.

Non si possono eliminare né rinominare la cartella principale di una sorgente e le memorie di un telefono.

**Il cestino di telefoni e server** (e delle chiavette su Windows): ogni cartella in cui hai eliminato qualcosa ha un **Cestino**: lo dice una striscia in cima alla griglia, col pulsante **Apri il cestino**. Dentro il cestino la striscia dice di quale cartella è e ha **Ripristina tutto** e **Svuota il cestino…**. Dentro puoi guardare le immagini, **ripristinarle** (tornano nella cartella che contiene il cestino) o eliminarle definitivamente: lì **Canc** elimina sempre per sempre, e Rinomina non c'è. Col clic destro su uno spazio vuoto dentro il Cestino: **Ripristina tutto** e **Svuota il cestino…** (tutto eliminato per sempre, dopo una conferma). Un cestino rimasto vuoto, dopo aver ripristinato o eliminato quello che conteneva, sparisce insieme alla sua striscia.

Se ripristinando un elemento il suo nome è già usato, EyeCanSee chiede cosa fare: per un file **Sostituisci** (quello presente viene eliminato) o **Tieni entrambi** (il ripristinato diventa per esempio `foto (2).jpg`); per una cartella **Unisci** (i contenuti si sommano; i file con lo stesso nome restano tutti e due) o **Tieni entrambi**. **Annulla** ferma anche il resto di un Ripristina tutto. Anche nel cestino un nome già presente diventa `foto (2).jpg`.

## Trovare i duplicati

**Strumenti > Trova duplicati…**, oppure **Trova duplicati…** dal clic destro su una cartella (o su uno spazio vuoto della griglia, per la cartella aperta), cerca le immagini identiche: lo stesso file byte per byte, anche con un altro nome o in un'altra cartella. Le immagini ridimensionate o salvate di nuovo non sono identiche e non vengono trovate. Funziona su tutte le sorgenti: computer, dischi esterni, telefoni e server.

Prima scegli se cercare **Anche nelle sottocartelle** (attiva all'inizio), poi **Cerca** (**Invio**; **Esc** annulla). Una finestra mostra quante immagini sono state esaminate su quelle trovate e quanto è stato letto; **Annulla** ferma la ricerca. Vengono lette e confrontate solo le immagini con la stessa dimensione, quindi sul computer è veloce; sul telefono ogni immagine che potrebbe essere una copia viene letta per intero, e ci vuole tempo.

I risultati occupano tutta la finestra. Ogni gruppo di immagini identiche ha la sua miniatura (un clic la apre nel visore; **Esc** riporta ai risultati) e una riga per ogni copia, con la data e il percorso sotto la cartella cercata (il percorso intero passandoci sopra col mouse). La casella di ogni riga vuol dire **elimina questa copia** (anche un clic sul testo della riga la spunta o la toglie): le copie da eliminare sono attenuate e barrate, quella che resta dice **Resta**. Almeno una copia di ogni gruppo resta sempre: l'ultima casella rimasta vuota non si può spuntare.

All'inizio ogni gruppo tiene la sua prima copia. **Ordine** decide qual è la prima, per tutti i gruppi: **Data, dalla più vecchia** (al minuto, come è mostrata; a parità di data, il percorso più corto), **Nome più corto** (le copie di solito hanno qualcosa in più nel nome, come "foto copia.jpg" o "foto (2).jpg": così resta l'originale anche quando le date sono uguali; a parità, come Data) oppure **Percorso** (ordine alfabetico di cartella e nome). **Tieni la prima**, **Tieni l'ultima** e **Tienile tutte** segnano tutti i gruppi insieme; poi puoi cambiare le singole caselle. La riga in basso dice quanti gruppi ci sono, quante copie verranno eliminate e quanto spazio si libera. **Elimina le copie segnate…** chiede conferma e le elimina come fa **Canc** (cestino, Cestino della cartella o definitivamente, come nelle Opzioni). **Chiudi** o **Esc** chiude i risultati.

Con la tastiera: **↑** e **↓** passano da una copia all'altra di tutti i gruppi, **Spazio** spunta o toglie la spunta, **Esc** chiude. Per eliminare si usa il pulsante **Elimina le copie segnate…**: Invio non elimina.

## Salvare in un altro formato

Clic destro su un'immagine (nella griglia o nel visore), poi **Salva come…**. Si apre una pagina grande quanto la finestra: a sinistra l'originale, a destra l'immagine convertita, ognuna con le dimensioni in pixel e su disco (per la convertita anche in percentuale dell'originale). Quello che vedi a destra è il file come verrà salvato: viene codificato e riletto a ogni cambio di opzione. Nel frattempo al posto della dimensione c'è "Calcolo della dimensione in corso…" e Salva aspetta: la dimensione mostrata è sempre quella del file che verrà salvato.

- **Formato**: JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI, ICO, TGA, PNM, OpenEXR, Radiance HDR o Farbfeld, con tutte le opzioni di ciascuno: qualità, dettaglio del colore, JPEG progressivo e senza perdita; livello di compressione e filtro per il PNG; WebP con o senza perdita; qualità, velocità e bit per canale per l'AVIF; compressione per il TIFF; riduzione dei colori per il GIF; compressione per il TGA. Un'icona (ICO) contiene al massimo 256 × 256 pixel, quindi un'immagine più grande viene ridotta; JPEG e HDR non hanno la trasparenza, quindi le parti trasparenti diventano bianche: la pagina lo dice. Questi sei formati valgono solo per Salva come: Salva riscrive sopra l'originale solo i primi otto. La tabella completa è in [docs/formats.md](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/formats.md).
- **100%** mostra i pixel veri; trascinando una delle due immagini si spostano entrambe, così confronti lo stesso punto.
- **Mantieni i metadati (EXIF, profilo colore)** è attiva all'inizio. È grigia se l'originale non ne ha o se il formato non può contenerli. L'EXIF salvato è quello che mostra il pannello dei metadati, anche dalle foto RAW e HEIC (fotocamera, scatto, GPS; non le note private del produttore). L'orientamento viene messo dritto, perché i pixel salvati sono già girati.
- **Nome del file** e **Salva**: il file nuovo va nella stessa cartella dell'originale. Se il nome c'è già viene aggiunto " (2)", e la pagina lo dice prima del salvataggio.
- Esc o ✕ chiudono la pagina senza salvare.

## Convertire e ridimensionare molte immagini

Seleziona le immagini nella griglia (o fai clic destro su una delle selezionate) e scegli **Strumenti > Converti e ridimensiona…** oppure **Converti e ridimensiona…** dal menù del clic destro. Una finestra chiede:

- **Formato**: uno di quelli di *Salva in un altro formato*, con le sue opzioni sotto **Opzioni**, oppure **Uguale all'originale**: ogni immagine nel suo formato, scritta come la scrive Salva (JPEG con qualità 95); le immagini in un formato che EyeCanSee non sa scrivere (HEIC, RAW…) vengono allora escluse.
- **Rotazione e ribaltamenti**: un quarto di giro, mezzo giro, e riflessioni orizzontali o verticali; prima i ribaltamenti, poi la rotazione. Se ruoti soltanto e lasci il formato, i JPEG vengono ruotati senza perdere qualità quando le dimensioni lo permettono, come con Salva.
- **Dimensioni**: invariate, una percentuale dell'originale, oppure entro un riquadro di pixel mantenendo le proporzioni (dopo la rotazione). Le immagini più piccole non vengono ingrandite, a meno che tu non lo chieda. Il **filtro** decide come si calcolano i pixel: Lanczos è il più nitido, Pixel più vicino tiene i bordi netti della pixel art.
- **Dove**: accanto agli originali, in una sottocartella (creata se manca), oppure in un'altra cartella, anche su un altro dispositivo: **Sfoglia…** apre una finestra di EyeCanSee per scegliere la cartella, con a sinistra la cartella home, le unità e i dispositivi collegati (telefoni compresi): un clic su una cartella ci entra, **Usa questa cartella** la sceglie, oppure scrivi un indirizzo come nella barra degli indirizzi (`mtp://…` per un telefono, `sftp://…`); quella cartella deve esistere. Così le foto del telefono si convertono direttamente in una cartella del computer. Gli originali non vengono mai toccati; un nome già presente prende " (2)".

**Converti** avvia il lavoro; un riquadro nell'angolo mostra a che punto è, e intanto il programma si può usare. **Annulla** si ferma dopo l'immagine in corso. Alla fine un messaggio dice quante immagini sono state salvate e dove, e quali sono state escluse e perché: le immagini animate e quelle con più pagine (resterebbe solo la prima). Le foto del telefono con la mappa HDR o il video della foto in movimento vengono convertite: il file nuovo è la foto, l'originale tiene il resto. Funziona su ogni dispositivo: computer, dischi, telefono.

## Carosello

**Visualizza > Carosello**, oppure **Carosello** nel menù del clic destro di un'immagine (nella griglia e nel visore), mostra le immagini una dopo l'altra a tutto schermo, su fondo nero. Con più immagini selezionate mostra quelle, sempre a partire dalla prima nella griglia; altrimenti tutte le immagini della cartella, a partire da quella scelta (o da quella aperta nel visore).

- **Esc** o un doppio clic lo chiudono: torni dov'eri, sull'ultima immagine mostrata. **Spazio** mette in pausa e riprende. **←** e **→** (o Pagina su e Pagina giù) mostrano subito l'immagine precedente e quella successiva; con **Scorrimento**, tornando indietro le immagini scorrono nel verso opposto.
- In **Strumenti > Opzioni… > Carosello**: quanti secondi resta ogni immagine (5 all'inizio), il passaggio tra le immagini (**Nessuno**, **Dissolvenza incrociata**, quello predefinito, **Dissolvenza al nero**, **Scorrimento**, **Zoom e dissolvenza**) e **Ricomincia dopo l'ultima immagine** (attivo all'inizio; se spento, il carosello finisce dopo l'ultima).
- L'immagine successiva viene letta in anticipo: se non è ancora pronta (un telefono, un file grande) quella in corso resta finché non lo è.

## Video

I video (MP4, MOV, MKV, WebM, AVI e altri) compaiono nella griglia insieme alle immagini, con un fotogramma del video come miniatura e il simbolo ▶. Quando la videocamera ha salvato un'anteprima dentro il video (lo fanno dashcam come Umino, e i droni DJI come copertina) la miniatura è quell'anteprima, pronta quasi subito. Li riproduce mpv, che deve essere installato (Linux: il pacchetto con libmpv, `mpv-libs` su Fedora, `libmpv2` su Debian e Ubuntu); se manca, il visore lo dice.

- **Invio** o un doppio clic aprono il video nel visore, dove parte con l'audio, adattato alla finestra.
- **Spazio**, un clic sul video o il pulsante sotto mettono in pausa e riprendono. La barra sotto il video mostra il tempo e serve a spostarsi: trascinala o fai clic su un punto. Alla fine resta l'ultimo fotogramma; riprendendo, il video riparte dall'inizio.
- **←** e **→** passano all'elemento precedente e successivo, come per le immagini; un doppio clic o **Esc** tornano alla griglia.
- Il pannello dei metadati mostra i dati del video selezionato, quando il file li contiene: marca, modello e versione di Android del telefono, data e ora della ripresa, durata, fotogrammi al secondo, dimensioni, formato video e audio, bit rate, e la posizione (la scrivono telefoni e iPhone). Per le dashcam che tengono la traccia GPS in un file `gps.sqlite` sulla scheda (come Umino) si vedono il punto di partenza del video, l'altitudine e la velocità media e massima durante il video; una copia del video senza quel file non ha la posizione.
- **Visualizza > Mostra**, o **Mostra** nel menù del clic destro sulla parte vuota della griglia: **Immagini e video** (all'inizio), **Solo immagini** o **Solo video**. La scelta vale per tutte le cartelle finché non la cambi; la barra di stato conta immagini e video a parte.
- I video su un telefono o un server si leggono un pezzo alla volta mentre scorrono, senza copiarli interi. Copiare, spostare, rinominare ed eliminare funzionano come per le immagini. Ruotare, salvare e convertire valgono solo per le immagini. Nel carosello un video scorre tutto, con l'audio, poi arriva l'elemento successivo; **Spazio** mette in pausa anche lui.

## Ruotare e riflettere

Nel visore, il menù **Immagine** riflette l'immagine in orizzontale o in verticale e la ruota di 90° in senso orario (Ctrl+R) o antiorario (Ctrl+Maiusc+R) o di 180°; le due rotazioni sono anche pulsanti nella barra degli strumenti, e tutto è nel menù del clic destro. La modifica è solo mostrata finché non la salvi (la barra di stato dice "Modificata, non salvata", in giallo). Lasciando l'immagine con la modifica non salvata (un'altra immagine, ritorno alla griglia, cartella superiore, chiusura di EyeCanSee) prima chiede se **Salvare**, **Salvare come…** o **Non salvare**; **Annulla** resta sull'immagine. Ruota e ritaglia chiede allo stesso modo.

- **File > Salva** (Ctrl+S) salva sopra l'originale, dopo una domanda: prima prepara il file nuovo (la barra di stato dice "Preparo il salvataggio…"), così la domanda può dire se si perde qualcosa; l'originale va dove lo manderebbe un'eliminazione (di solito il cestino). Un JPEG viene ruotato senza perdere qualità, come fa `jpegtran`, quando larghezza e altezza sono multipli di 16 pixel (di 8 per alcuni); altrimenti viene ricodificato con qualità 95. PNG, WebP, AVIF, TIFF, BMP, GIF e QOI vengono ricodificati nel loro formato. La conferma avvisa quando l'immagine verrà ricodificata con un po' di perdita di qualità (un JPEG di altre dimensioni, WebP con perdita, AVIF, GIF). Un PSD resta un PSD, con profilo colore, EXIF e XMP: se ha un solo livello semplice non perde niente, se ha più livelli, testi, effetti o maschere viene **appiattito** (resta solo l'immagine come si vede), e la conferma lo dice in grassetto. I formati che EyeCanSee non sa scrivere (HEIC, RAW…) possono usare solo Salva come, e così le immagini che perderebbero qualcosa: PNG, WebP, AVIF e GIF animati, TIFF con più pagine e foto JPEG che contengono più di un'immagine (il video di una foto in movimento, le mappe di profondità). Le foto **Ultra HDR** dei telefoni (Android 14 e successivi) invece si ruotano e si riflettono come le altre: EyeCanSee gira anche la loro mappa HDR, così restano HDR. Sui server FTP le immagini non si possono salvare sopra se stesse: lì non si può rinominare. Quando un'immagine non si può salvare sopra se stessa, Ctrl+S spiega perché in una finestra con il pulsante **Salva come…**.
- Il file nuovo viene scritto prima con un nome nascosto e prende il nome dell'immagine solo quando è completo; se qualcosa va storto, o il programma si chiude a metà, l'originale resta (o torna, la volta dopo che si apre la cartella) col suo nome. Se il programma si chiude subito dopo che il file nuovo ha preso il nome, accanto all'immagine salvata può restare un file nascosto `.<nome>.eyecansee-old`: è l'originale. Il Salva successivo di quell'immagine lo manda dove lo manderebbe un'eliminazione; fino ad allora puoi cancellarlo o rinominarlo tu.
- **File > Salva come…** (Ctrl+Maiusc+S) apre *Salva in un altro formato* con l'immagine già ruotata: l'originale non viene toccato.

Nella griglia le stesse operazioni (menù Immagine, o clic destro > Trasforma) valgono per l'immagine selezionata, o per tutte quelle selezionate, e le salvano subito: EyeCanSee prima chiede conferma, dice dove vanno gli originali e avvisa che i formati con perdita possono perdere un po' di qualità; le immagini in un formato che non si può risalvare vengono escluse, e così le immagini animate, multipagina e le foto in movimento, elencate per nome nella finestra che alla fine dice quante immagini sono state salvate. Poi le miniature vengono rifatte. Quando l'unica immagine scelta non si può risalvare (un PSD, un RAW…), la finestra dice perché e offre **Salva come…**, che apre *Salva in un altro formato* con l'immagine già ruotata.

### Ruota e ritaglia

**Immagine > Ruota e ritaglia…** (anche nel menù del clic destro) apre una pagina a sé per una sola immagine, per ruotarla di un angolo qualsiasi e ritagliarla, come fanno i programmi di fotoritocco. Con più immagini selezionate è disattivata: per ruotare più immagini insieme usa le rotazioni e i ribaltamenti sopra di lei o **Converti e ridimensiona**.

- **Rotazione**: il cursore o il numero accanto, da -180° a 180° a decimi di grado; −90° e +90° aggiungono un quarto di giro, **Torna a 0°** la annulla. Tenendo premuto Maiusc, la rotellina sull'immagine la ruota di 0,1° a scatto: comodo per raddrizzare un orizzonte con le linee dei terzi. I gradi si possono scrivere con la virgola o con il punto.
- **Raddrizza**: cliccalo, poi traccia una linea lungo l'orizzonte (o lungo qualcosa di verticale, come un muro): quando lasci il pulsante del mouse l'immagine ruota per mettere quella linea in orizzontale (o in verticale), e il pulsante si spegne da solo.
- **Ritaglio**: trascina un angolo o un lato della cornice per cambiarne la misura, l'interno per spostarla. Le proporzioni possono essere libere, come l'originale, 1:1, 5:4, 4:3, 3:2, 16:9 o personalizzate; ⇄ scambia larghezza e altezza. **Solo immagine** fa il ritaglio più grande senza angoli vuoti (segue la rotazione, ed è quello iniziale); **Tutta l'immagine** tiene tutta l'immagine ruotata.
- **Angoli vuoti**: dove il ritaglio esce dall'immagine sono trasparenti (quadretti grigi) o del colore che scegli. Il JPEG non ha trasparenza: lì diventano bianchi.
- La rotellina ingrandisce attorno al puntatore, trascinando fuori dalla cornice si sposta la vista, il doppio clic mostra di nuovo tutto. La misura del risultato è sotto i comandi.
- **Applica** (o Invio) mette la rotazione e il ritaglio nell'immagine mostrata, e i comandi ripartono da lì: puoi ruotare, ritagliare e applicare di nuovo quante volte vuoi. Il file non viene toccato: sotto Applica la pagina dice "Rotazione e ritaglio non ancora applicati" o "Modifiche applicate, non ancora salvate".
- Alla fine **Salva…** scrive il risultato sopra l'originale dopo aver chiesto, come Salva nel visore (accanto ai pulsanti "Preparo il salvataggio…" finché il file nuovo non è pronto; stessi formati, stesse esclusioni, l'originale va dove lo manderebbe un'eliminazione); l'immagine viene codificata di nuovo, quindi il JPEG perde un po' di qualità. **Salva come…** apre *Salva in un altro formato* con il risultato. **Chiudi**, ✕ o Esc chiudono la pagina; con modifiche non salvate prima chiedono se **Salvare**, **Salvare come…** o **Non salvare** (**Annulla** torna alla pagina).
- Le foto **Ultra HDR** dei telefoni (con la luminosità HDR) restano HDR: la loro parte HDR viene ruotata e ritagliata insieme alla foto.

## Copiare e spostare

**Ctrl+C** copia gli elementi selezionati (o l'immagine aperta nel visore), **Ctrl+X** li taglia; poi apri la cartella dove devono andare e premi **Ctrl+V**. Ogni copia o taglio si incolla una volta sola: per incollare di nuovo, copia di nuovo. Le stesse voci sono nel menù **File** e nel clic destro; il clic destro su una cartella ha anche **Incolla in questa cartella**. File e cartelle, con tutto quello che contengono, vanno ovunque: un'altra cartella dello stesso disco, un altro disco, un telefono, un server, anche da un dispositivo a un altro.

- Gli elementi copiati hanno un simbolo blu nell'angolo della cella, quelli tagliati uno arancione con le forbici e diventano più chiari; la barra di stato dice quanti aspettano **Ctrl+V**.
- Una finestra in basso a destra mostra il file in corso, la cartella di destinazione, quanti file e quanti dati sono stati copiati, la percentuale, la velocità e il tempo che manca, con **Annulla**: il file che si sta copiando quando annulli non resta a metà, quelli già copiati restano. Verso un telefono, Annulla aspetta la fine del file in corso (un telefono che riceve mezzo file smette di rispondere). Chiudendo la finestra durante una copia, la copia si annulla e la finestra si chiude appena finito il file in corso.
- Se c'è già un elemento con lo stesso nome, EyeCanSee chiede: **Sostituisci** (per una cartella **Unisci**: il suo contenuto va nella cartella esistente, sostituendo i file con lo stesso nome), **Salta** o **Tieni entrambi** (il nuovo diventa "nome (2)"). Con più elementi in questa situazione, una casella applica la stessa risposta a tutti.
- Incollati nella cartella da cui vengono, le copie diventano "nome (2)"; uno spostamento non fa nulla.
- **Spostare** sullo stesso disco cambia solo il posto, all'istante. Tra dispositivi diversi EyeCanSee copia e cancella l'originale solo dopo aver copiato tutto; l'originale non va nel cestino.
- Una cartella non si può incollare dentro sé stessa, e non si può incollare in un cestino né nell'elenco delle memorie di un telefono.

## Dispositivi esterni

Quando colleghi un telefono, una fotocamera, un disco esterno, una chiavetta o una scheda SD, in alto a sinistra compare il blocco **Dispositivi**, con la struttura delle sue cartelle da espandere come l'albero. Quando lo scolleghi, sparisce; se lo stavi guardando, EyeCanSee torna alla tua cartella personale.

- **Telefono Android**: sul telefono scegli **Trasferimento foto** (PTP) o **Trasferimento file** (MTP). Con Trasferimento foto si vedono solo le cartelle delle foto (`DCIM`, `Pictures`); con Trasferimento file tutta la memoria.
- EyeCanSee si collega al telefono direttamente, anche se il desktop lo ha già collegato (per esempio GNOME): così è molto più veloce, soprattutto nelle cartelle con centinaia di file. Il dispositivo compare entro un paio di secondi.
- Su GNOME e KDE, aprendo il telefono in EyeCanSee il file manager perde il suo collegamento Su KDE EyeCanSee prima chiede: per prendere il telefono deve chiudere il servizio kiod di KDE, che dimentica anche le password delle cartelle di rete aperte in Dolphin (le richiederà al prossimo accesso). Con Annulla il telefono resta a KDE; F5 lo richiede. Se mentre EyeCanSee ha il telefono lo apri anche in Dolphin, KDE ci rinuncia e poi non lo rivede più: chiudendo EyeCanSee o passando a un'altra cartella, EyeCanSee rimette a posto KDE da solo (se gli avevi dato l'OK). Altrimenti scollega e ricollega il telefono. Per riaverlo, chiudi EyeCanSee e fai clic sul telefono nel file manager, oppure scollegalo e ricollegalo.
- **"Il dispositivo è in uso da un'altra applicazione"**: un altro programma tiene il telefono. Se il desktop lo ha collegato, EyeCanSee passa da solo al suo collegamento (più lento: nelle cartelle con centinaia di file circa una miniatura al secondo); altrimenti chiudi l'altro programma o scollega e ricollega il telefono.
- **"Il dispositivo non mostra nessuna memoria"**: il telefono era bloccato o in "solo ricarica" quando è stato collegato: sbloccalo e scegli Trasferimento file. EyeCanSee riprova da solo ogni 2 secondi e mostra le memorie appena compaiono, senza bisogno di F5.
- **"Il dispositivo non è stato trovato"**: il telefono si sta ricollegando (lo fa quando scegli Trasferimento file) oppure è stato scollegato. EyeCanSee riprova da solo ogni 2 secondi, senza bisogno di F5.
- Aprendo un telefono collegato direttamente, il blocco mostra le sue cartelle e sotto c'è la voce per tornare alla tua cartella home.
- Le miniature dei dispositivi restano in cache anche dopo averli scollegati.
- I dischi esterni compaiono ovunque il sistema li monti. Una partizione che il sistema non ha montato da sola (per esempio perché ha una sua riga in `/etc/fstab`) compare col nome più chiaro: un clic la monta e la apre. Se serve, il sistema chiede la password.
- Su Windows compaiono le chiavette, le schede SD e i dischi collegati via USB, col nome che hanno in Esplora file (per esempio "Backup (D:)"). Le unità interne (C:, ed eventuali altre) stanno sotto **Questo computer**: un clic su un'altra unità la apre. I telefoni si collegano tramite il driver di Windows. Unità di rete e CD/DVD per ora non compaiono: aprili con **File > Apri indirizzo…** (per esempio `E:\`).

## Aprire le immagini

Scrivi una cartella, un'immagine o un indirizzo remoto nella barra degli indirizzi e premi Invio, oppure avvia EyeCanSee indicandolo. **File > Apri indirizzo…** apre una finestra dove scrivi l'indirizzo intero, oppure scegli il protocollo e compili server, porta, utente, password e cartella (senza utente, l'accesso FTP è anonimo, `anonymous`):

```text
eyecansee                                  la tua cartella home
eyecansee ~/Immagini/Vacanze               una cartella
eyecansee ~/Immagini/Vacanze/spiaggia.jpg  apre quell'immagine nel visore
eyecansee ftp://utente:password@nas.lan/Foto
eyecansee ftps://utente@nas.lan/Foto
eyecansee sftp://utente@server:2222/home/utente/Foto   (solo Linux)
```

EyeCanSee è aperto in una sola copia alla volta (un telefono può usarlo un solo programma): se è già aperto, avviarlo di nuovo, per esempio con "Apri con" su una foto, mostra quella cartella o immagine nella finestra già aperta.

Le cartelle remote si comportano come quelle locali: albero, griglia e visore funzionano allo stesso modo. Le password non compaiono mai nella barra degli indirizzi.

Con SFTP, EyeCanSee usa il client `ssh` del sistema, quindi alias, chiavi e agent di `~/.ssh/config` funzionano come sempre.

## Tastiera

Nella griglia:

| Tasto | Azione |
|---|---|
| Frecce | Spostano la selezione (la prima pressione seleziona il primo elemento; a sinistra dal primo elemento si va su **..**) |
| Inizio, Fine | Primo, ultimo elemento |
| Invio | Apre la cartella o l'immagine; su **..** sale di un livello |
| Backspace | Cartella superiore |
| F2 | Rinomina (vedi *Eliminare e rinominare*) |
| Ctrl+Maiusc+N | Nuova cartella (vedi *Eliminare e rinominare*) |
| Canc | Elimina |
| Ctrl+C, Ctrl+X, Ctrl+V | Copia, taglia, incolla (vedi *Copiare e spostare*) |
| Ctrl+A | Seleziona tutto nella cartella |
| Maiusc+frecce | Allargano la selezione |
| Maiusc+Canc | Elimina definitivamente |
| Maiusc+F5 | Rifà tutte le miniature della cartella |
| Tab | Elemento successivo; dall'ultimo passa ai dispositivi |
| Shift+Tab | Elemento precedente; dal primo passa alla cartella aperta a sinistra |

Nell'albero a sinistra:

| Tasto | Azione |
|---|---|
| Giù, su | Cartella successiva, precedente |
| Destra | Apre la cartella; se è già aperta, va alla prima sottocartella |
| Sinistra | Chiude la cartella; se è già chiusa, va a quella superiore |
| Backspace | Cartella superiore |
| Tab | Cartella successiva; dall'ultima passa alla griglia |
| Shift+Tab | Cartella precedente; dalla prima passa all'ultimo dispositivo |

Nei dispositivi: giù e su cambiano dispositivo (giù su un dispositivo aperto entra nella sua prima cartella), destra e sinistra lo aprono e lo chiudono, Invio lo apre a destra, Tab passa al successivo e dall'ultimo alle cartelle del computer, Shift+Tab al precedente e dal primo all'ultimo elemento della griglia.

Nel visore:

| Tasto | Azione |
|---|---|
| Freccia destra, Spazio, Pagina giù | Immagine successiva (su un video Spazio mette in pausa e riprende) |
| Freccia sinistra, Backspace, Pagina su | Immagine precedente |
| Inizio, Fine | Prima, ultima immagine |
| Esc | Torna alla griglia |
| F2, Canc, Maiusc+Canc | Rinomina ed elimina, come nella griglia |
| + e − (o la rotellina del mouse) | Ingrandisce e rimpicciolisce |
| 0 | Adatta l'immagine alla finestra |
| 1 | 100%: un pixel dell'immagine per pixel dello schermo |
| Ctrl+R, Ctrl+Maiusc+R | Ruota di 90° in senso orario, antiorario |
| Ctrl+S, Ctrl+Maiusc+S | Salva, Salva come |

La rotellina ingrandisce attorno al puntatore, + e − attorno al centro della finestra; Ctrl con la rotellina ingrandisce sempre, anche quando la rotellina è impostata per cambiare immagine (vedi *Opzioni*). Un'immagine ingrandita si sposta trascinandola. Non diventa mai più piccola di quando è adattata alla finestra, né più grande dell'ingrandimento massimo impostato nelle opzioni (2000% all'inizio); la barra di stato mostra l'ingrandimento. L'immagine successiva riparte adattata.

Una miniatura si fa una volta e resta: si rifà solo quando il file cambia dimensione o data. Se una sembra sbagliata (per esempio fatta da una versione vecchia), clic destro sull'immagine, o su più immagini selezionate, e scegli **Aggiorna miniature** (anche nel menù Visualizza). **Maiusc+F5** rifà tutte le miniature della cartella aperta.

Ovunque: **F1** apre questo manuale, **F5** rilegge le cartelle. Nel visore F5 resta sull'immagine aperta; se nel frattempo è stata cancellata, torna alla griglia.

Le cartelle del computer si aggiornano da sole: immagini aggiunte, eliminate, rinominate o ripristinate dal cestino da un altro programma compaiono e spariscono senza F5, anche nel visore, che resta sull'immagine aperta. Su telefoni e server serve F5.

Le stesse azioni sono nei menù: **Visualizza** (Apri, Torna alla griglia, Avanti, Indietro, Primo, Ultimo, Cartella superiore, Aggiorna) e **Modifica** (Taglia, Copia, Incolla, Seleziona tutto). Nella griglia Avanti e Indietro spostano la selezione, nel visore cambiano immagine. Con più immagini selezionate, Invio o Apri (anche dal clic destro) aprono la prima di quelle nella griglia, e il visore passa solo tra quelle, come il carosello, e il titolo conta solo quelle.

Il **clic destro** apre un menù con le azioni di ciò che hai sotto il mouse, e lo seleziona. Le voci seguono l'ordine della barra dei menù. Su un'immagine: Apri, Carosello, Aggiorna miniature; Trasforma, Salva come…, Converti e ridimensiona…; Taglia, Copia; Rinomina; poi, separate in fondo, Elimina ed Elimina definitivamente. Sull'immagine aperta nel visore le stesse, con Torna alla griglia al posto di Apri e Salva quando l'hai modificata. Su una cartella (nella griglia o nell'albero): Apri, Nuova cartella qui dentro…; Taglia, Copia (e Incolla in questa cartella, quando hai copiato qualcosa); Trova duplicati…; Rinomina; Elimina ed Elimina definitivamente. Su **..**: Cartella superiore. Su uno spazio vuoto della griglia: Incolla (quando hai copiato qualcosa), Seleziona tutto; Nuova cartella…; Cartella superiore, Aggiorna; Trova duplicati… (dentro il Cestino anche Ripristina tutto e Svuota il cestino).

Nel visore, dopo l'ultima immagine EyeCanSee si ferma; con l'opzione **Nel visore, dopo l'ultima immagine ricomincia dalla prima** torna alla prima, e viceversa. EyeCanSee carica in anticipo le immagini vicine a quella corrente, così il passaggio è immediato anche con connessioni lente.

## Formati supportati

- **Foto**: JPEG, PNG, WebP, HEIC/HEIF, AVIF, JPEG XL, TIFF, GIF (primo fotogramma), BMP.
- **RAW delle fotocamere**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW e altri. EyeCanSee mostra l'anteprima a piena risoluzione salvata dalla fotocamera; se manca, sviluppa i dati del sensore. I DNG dei telefoni Samsung Galaxy non si possono ancora sviluppare (verrebbero bianchi): EyeCanSee mostra l'anteprima a piena risoluzione che il telefono salva al loro interno.
- **Vettoriali**: SVG, SVGZ.
- **Grafica e HDR**: PSD (immagine unita), ORA, OpenEXR, HDR, DDS, ICNS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, WBMP, OTB, MPO, JPS.
- **Tramite ImageMagick**: PSB, XCF, PFM, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS e altri formati meno comuni (XWD solo su Linux). ImageMagick apre anche i PSD che il lettore interno non gestisce (per esempio quelli a 32 bit).

Le foto vengono ruotate in automatico secondo l'orientamento EXIF. L'estensione del file è solo un indizio: una foto HEIC di un iPhone rinominata in `.jpg` si apre lo stesso.

## Lingua

EyeCanSee usa la lingua del sistema se è disponibile, altrimenti l'inglese. Per forzare una lingua imposta `EYECANSEE_LANG`, per esempio `EYECANSEE_LANG=en eyecansee`.

## Limiti noti

- Sui dischi **NTFS** (per esempio un disco esterno usato anche con Windows) l'app File (Nautilus) non riesce a ripristinare dal cestino, nemmeno i file cancellati da lei stessa; EyeCanSee lo ricorda nella conferma. I file però sono nel cestino e si ripristinano dal terminale: `gio trash --list` mostra cosa c'è, `gio trash --restore "<indirizzo trash:/// della riga>"` lo rimette al suo posto.
- SFTP non è ancora disponibile su Windows.
- I video non si guardano ancora su Windows.
- Le immagini più grandi di 8192 pixel per lato vengono mostrate ridotte.
- I file sono ordinati alfabeticamente: `img10` viene prima di `img2`.
- File e cartelle nascosti (che iniziano con un punto, o su Windows con l'attributo "nascosto")
  non vengono mostrati.
- Le immagini HDR (EXR, HDR, PFM) sono mostrate senza tone mapping e possono apparire sovraesposte.
