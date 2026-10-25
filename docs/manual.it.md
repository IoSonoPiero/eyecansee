# Manuale di EyeCanSee

Premi **F1** in qualsiasi momento per aprire o chiudere questo manuale.

## La finestra

- **Barra dei menù**: File, Visualizza, Vai, Aiuto. A destra il numero di versione; **Aiuto → Informazioni su EyeCanSee** mostra versione, autore, licenza e il link al codice sorgente. Ogni azione che si fa con la tastiera o col doppio clic ha anche una voce di menù, con il tasto scritto accanto.
- **Barra strumenti**: cartella superiore, aggiorna, metadati (ℹ), barra degli indirizzi, ordine delle immagini e cursore per la dimensione delle miniature.
- **Albero delle cartelle** a sinistra, sotto **Questo computer** (e sotto **Dispositivi** se ne hai collegati): clic su una cartella per mostrarne il contenuto. Le cartelle vengono lette solo quando le espandi, così le sorgenti remote restano veloci. Quando cambi cartella, anche dalla griglia, l'albero si apre fino a quella cartella e la evidenzia.
- **Griglia** a destra: la cella **..** per salire di un livello (non in cima a un disco o a un dispositivo), le sottocartelle, poi le immagini come miniature, distribuite su tutta la larghezza. Entrando in una cartella nessun elemento è selezionato; risalendo è selezionata la cartella da cui vieni.
- **Ordine delle immagini** dalla tendina nella barra strumenti (con accanto il pulsante ▲/▼ per invertirlo) o da **Visualizza → Ordina per**: per nome, data di modifica, dimensione o tipo (estensione), più **Ordine decrescente**. Le cartelle restano prima, per nome; i file senza data restano in fondo. La scelta viene ricordata.
- **Metadati** sotto la griglia, per l'immagine selezionata: riquadri **Fotocamera** (marca, modello, obiettivo, software), **Scatto** (data, esposizione, diaframma, ISO, lunghezza focale, flash), **Immagine** (dimensioni, spazio colore) e **Posizione** (coordinate GPS, altitudine, con il link per aprirle sulla mappa di OpenStreetMap). Ogni riquadro si chiude con la sua X e si riapre da **Visualizza → Riquadri dei metadati**; il pulsante ℹ nella barra strumenti li chiude o li riapre tutti. Il pannello si comprime e si riapre col pulsante ▼/▲ a destra della sua barra, o con un clic sulla barra stessa; compresso resta solo la barra, e i metadati non vengono letti. Si leggono i metadati EXIF di JPEG, HEIC, AVIF, PNG, WebP e TIFF; per leggerli EyeCanSee rilegge il file, quindi su un telefono ci vuole un attimo.
- **Barra di stato** in basso: quante cartelle e immagini ci sono, e nome, peso e data di modifica dell'elemento selezionato; con più elementi selezionati, quanti sono e il peso delle loro immagini.
- **Più elementi**: Ctrl+clic aggiunge o toglie un elemento, Maiusc+clic seleziona tutto dall'ultimo elemento cliccato fino a questo, Maiusc+frecce allargano la selezione, Ctrl+A seleziona tutto nella cartella. **Trascinando** col mouse si disegna un rettangolo che seleziona le celle che tocca (con Ctrl le aggiunge a quelle già selezionate); vicino al bordo in alto o in basso la griglia scorre. Un clic senza tasti, o una freccia, torna a un solo elemento.
- Frecce e Tab agiscono sulla parte su cui hai cliccato per ultima, sinistra o destra: è quella con la cornice sottile. Lì la selezione è blu pieno, nell'altra più tenue.

Doppio clic su un'immagine, oppure selezionala e premi Invio, per aprirla nel **visore**. Per tornare alla griglia premi Esc, fai doppio clic sull'immagine o usa il pulsante griglia nella barra strumenti. La barra in basso nel visore mostra il nome del file, la sua posizione nella cartella (per esempio 12 / 51), le dimensioni in pixel e su disco, e la data di scatto (la data di modifica se la foto non ce l'ha o se i riquadri dei metadati sono chiusi).

## Miniature

EyeCanSee crea le miniature in questo ordine: prima l'immagine selezionata, poi quelle visibili nella finestra dall'alto in basso, poi tutte le altre della cartella dall'alto in basso. Facendo clic su un'immagine la sua miniatura passa in cima alla coda. Fino a 1000 miniature per cartella restano in memoria, così scorrendo, anche trascinando la barra, non spariscono; nelle cartelle più grandi quelle lontane dalla parte visibile si ricaricano dal disco.

Le miniature vengono salvate su disco, così riaprire la stessa cartella è immediato: nel file `~/.cache/eyecansee/thumbs.db` su Linux e `%LOCALAPPDATA%\eyecansee\thumbs.db` su Windows. Se un'immagine cambia (dimensione o data di modifica), la miniatura viene rifatta. Puoi cancellare quel file (con EyeCanSee chiuso) in qualsiasi momento: le miniature si ricreano quando servono.

**Pulizia della cache miniature** in **File > Opzioni…** (spenta all'inizio): una volta per sessione, per ogni indirizzo aperto, elimina le miniature delle immagini cancellate e rifà quelle delle immagini modificate; poi elimina le miniature usate meno di recente finché la cache sta nel **Limite della cache** (1024 MB all'inizio). Le cartelle che non si trovano o non si raggiungono vengono saltate, così un disco staccato o un NAS spento non perdono le loro miniature. Le sorgenti remote vengono controllate solo quando le apri.

## Opzioni

**File > Opzioni…** apre la finestra delle opzioni, divisa in schede: **Generale** (ultima cartella, finestra all'avvio, lingua, aggiornamenti), **Aspetto** (colori), **Navigazione** (scorrimento, visore), **Eliminazione** e **Miniature**. Ogni modifica viene salvata subito in `~/.config/eyecansee/settings.json` su Linux e `%APPDATA%\eyecansee\settings.json` su Windows, un file di testo che puoi anche modificare a mano.

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

## Aggiornamenti

L'installer per Windows, lo zip per Windows e l'AppImage si aggiornano da soli. All'avvio EyeCanSee chiede a GitHub, in background, l'ultima versione pubblicata; se è più nuova la scarica e la controlla due volte: con l'impronta SHA-256 che GitHub pubblica per lei (un download rovinato) e con la firma dello sviluppatore, fatta con una chiave che non lascia mai il suo PC (un file che non viene dallo sviluppatore). Un file senza firma valida non viene mai installato, e Verifica aggiornamenti dice perché. Quando è pronta, in alto a destra nella barra dei menù compare un pulsante rosso **Installa la … e riavvia**: EyeCanSee si chiude, installa la versione nuova e si riapre. Finché non lo premi non viene installato niente.

- **Aiuto > Verifica aggiornamenti** fa lo stesso controllo quando vuoi e ti dice com'è andata.
- Il controllo si può spegnere in **File > Opzioni… > Generale**. Spento, EyeCanSee contatta GitHub solo quando scegli Verifica aggiornamenti. Non viene inviato niente su di te o sulle tue immagini: solo la richiesta dell'ultima versione.
- Su Fedora EyeCanSee si aggiorna con `dnf`, insieme al resto del sistema.

## Eliminare e rinominare

**Canc** (o **File > Elimina…**) elimina l'immagine o la cartella selezionata nella griglia, o l'immagine aperta nel visore, dopo una conferma con il nome e la dimensione. Una cartella viene eliminata con tutto quello che contiene. Dall'albero a sinistra si eliminano le cartelle col clic destro. Nella conferma **Invio** elimina ed **Esc** annulla. Nel visore compare subito l'immagine successiva.

Con più elementi selezionati, **Canc** (o **Elimina…** dal clic destro su uno di loro) li elimina tutti, dopo una conferma con quanti sono e il peso delle immagini. Se uno non riesce, i successivi non vengono eliminati. Mentre un'eliminazione è in corso la barra di stato lo dice, con una rotellina, e non se ne può avviare un'altra (su un telefono una cartella con centinaia di file richiede diversi secondi).

- **File del computer e dischi esterni**: vanno nel cestino del sistema, da cui si possono ripristinare. Con l'opzione **Elimina i file del computer definitivamente, senza cestino** vengono eliminati per sempre.
- **Telefoni e server (FTP, SFTP)** non hanno un cestino raggiungibile. Nelle Opzioni, sotto **Eliminare su telefoni e server**, scegli tra **Sposta nella cartella nascosta .EyeCanSee-trash accanto all'immagine** (predefinita: l'immagine finisce in una cartella `.EyeCanSee-trash` dentro la sua cartella, e si recupera con un gestore di file) ed **Elimina definitivamente**. L'FTP non sa spostare i file: lì serve **Elimina definitivamente**.

**Maiusc+Canc** (o **File > Elimina definitivamente…**) elimina per sempre solo quella volta, qualunque sia l'opzione.

**F2** (o **File > Rinomina…**) rinomina l'elemento selezionato o l'immagine aperta: scrivi il nuovo nome e premi **Invio** (**Esc** annulla). EyeCanSee non accetta un nome già presente nella cartella. Sui server SFTP le cartelle non si possono rinominare, e via FTP nulla.

Non si possono eliminare né rinominare la cartella principale di una sorgente e le memorie di un telefono.

**Il cestino di telefoni e server**: ogni cartella in cui hai eliminato qualcosa ha un **Cestino**: lo dice una striscia in cima alla griglia, col pulsante **Apri il cestino**. Dentro il cestino la striscia dice di quale cartella è e ha **Ripristina tutto** e **Svuota il cestino…**. Dentro puoi guardare le immagini, **ripristinarle** (tornano nella cartella che contiene il cestino) o eliminarle definitivamente: lì **Canc** elimina sempre per sempre, e Rinomina non c'è. Col clic destro su uno spazio vuoto dentro il Cestino: **Ripristina tutto** e **Svuota il cestino…** (tutto eliminato per sempre, dopo una conferma). Un cestino rimasto vuoto, dopo aver ripristinato o eliminato quello che conteneva, sparisce insieme alla sua striscia.

Se ripristinando un elemento il suo nome è già usato, EyeCanSee chiede cosa fare: per un file **Sostituisci** (quello presente viene eliminato) o **Tieni entrambi** (il ripristinato diventa per esempio `foto (2).jpg`); per una cartella **Unisci** (i contenuti si sommano; i file con lo stesso nome restano tutti e due) o **Tieni entrambi**. **Annulla** ferma anche il resto di un Ripristina tutto. Anche nel cestino un nome già presente diventa `foto (2).jpg`.

## Salvare in un altro formato

Clic destro su un'immagine (nella griglia o nel visore), poi **Salva come…**. Si apre una pagina grande quanto la finestra: a sinistra l'originale, a destra l'immagine convertita, ognuna con le dimensioni in pixel e su disco (per la convertita anche in percentuale dell'originale). Quello che vedi a destra è il file come verrà salvato: viene codificato e riletto a ogni cambio di opzione.

- **Formato**: JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF o QOI, con tutte le opzioni di ciascuno: qualità, dettaglio del colore, JPEG progressivo e senza perdita; livello di compressione e filtro per il PNG; WebP con o senza perdita; qualità, velocità e bit per canale per l'AVIF; compressione per il TIFF; riduzione dei colori per il GIF. La tabella completa è in [docs/formats.md](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/formats.md).
- **100%** mostra i pixel veri; trascinando una delle due immagini si spostano entrambe, così confronti lo stesso punto.
- **Mantieni i metadati (EXIF, profilo colore)** è attiva all'inizio. È grigia se l'originale non ne ha o se il formato non può contenerli. L'EXIF salvato è quello che mostra il pannello dei metadati, anche dalle foto RAW e HEIC (fotocamera, scatto, GPS; non le note private del produttore). L'orientamento viene messo dritto, perché i pixel salvati sono già girati.
- **Nome del file** e **Salva**: il file nuovo va nella stessa cartella dell'originale. Se il nome c'è già viene aggiunto " (2)", e la pagina lo dice prima del salvataggio.
- Esc o ✕ chiudono la pagina senza salvare.

## Ruotare e riflettere

Nel visore, **Immagine > Trasforma** riflette l'immagine in orizzontale o in verticale e la ruota di 90° in senso orario (Ctrl+R) o antiorario (Ctrl+Maiusc+R) o di 180°; le due rotazioni sono anche pulsanti nella barra degli strumenti, e tutto è nel menù del clic destro. La modifica è solo mostrata finché non la salvi (la barra di stato dice "Modificata, non salvata"); aprendo un'altra immagine si perde.

- **File > Salva** (Ctrl+S) salva sopra l'originale, dopo una domanda; l'originale va dove lo manderebbe un'eliminazione (di solito il cestino). Un JPEG viene ruotato senza perdere qualità, come fa `jpegtran`, quando larghezza e altezza sono multipli di 16 pixel (di 8 per alcuni); altrimenti viene ricodificato con qualità 95. PNG, WebP, AVIF, TIFF, BMP, GIF e QOI vengono ricodificati nel loro formato. La conferma avvisa quando l'immagine verrà ricodificata con un po' di perdita di qualità (un JPEG di altre dimensioni, WebP con perdita, AVIF, GIF). I formati che EyeCanSee non sa scrivere (HEIC, RAW, PSD…) possono usare solo Salva come, e così le immagini che perderebbero qualcosa: PNG, WebP, AVIF e GIF animati, TIFF con più pagine e foto JPEG che contengono più di un'immagine (il video di una foto in movimento, le mappe di profondità). Sui server FTP le immagini non si possono salvare sopra se stesse: lì non si può rinominare. Quando un'immagine non si può salvare sopra se stessa, Ctrl+S spiega perché in una finestra con il pulsante **Salva come…**.
- Il file nuovo viene scritto prima con un nome nascosto e prende il nome dell'immagine solo quando è completo; se qualcosa va storto, o il programma si chiude a metà, l'originale resta (o torna, la volta dopo che si apre la cartella) col suo nome. Se il programma si chiude subito dopo che il file nuovo ha preso il nome, accanto all'immagine salvata può restare un file nascosto `.<nome>.eyecansee-old`: è l'originale. Il Salva successivo di quell'immagine lo manda dove lo manderebbe un'eliminazione; fino ad allora puoi cancellarlo o rinominarlo tu.
- **File > Salva come…** (Ctrl+Maiusc+S) apre *Salva in un altro formato* con l'immagine già ruotata: l'originale non viene toccato.

Nella griglia le stesse operazioni (Immagine > Trasforma, o clic destro > Trasforma) valgono per l'immagine selezionata, o per tutte quelle selezionate, e le salvano subito: EyeCanSee prima chiede conferma, dice dove vanno gli originali e avvisa che i formati con perdita possono perdere un po' di qualità; le immagini in un formato che non si può risalvare vengono escluse, e così le immagini animate, multipagina e le foto in movimento, elencate per nome nella finestra che alla fine dice quante immagini sono state salvate. Poi le miniature vengono rifatte.

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
- Su GNOME e KDE, aprendo il telefono in EyeCanSee il file manager perde il suo collegamento Su KDE EyeCanSee prima chiede: per prendere il telefono deve chiudere il servizio kiod di KDE, che dimentica anche le password delle cartelle di rete aperte in Dolphin (le richiederà al prossimo accesso). Con Annulla il telefono resta a KDE; F5 lo richiede. Per riaverlo, chiudi EyeCanSee e fai clic sul telefono nel file manager, oppure scollegalo e ricollegalo.
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
| Freccia destra, Spazio, Pagina giù | Immagine successiva |
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

Le stesse azioni sono nei menù: **Visualizza** (Apri, Torna alla griglia, Aggiorna) e **Vai** (Avanti, Indietro, Primo, Ultimo, Cartella superiore). Nella griglia Avanti e Indietro spostano la selezione, nel visore cambiano immagine.

Il **clic destro** apre un menù con le azioni di ciò che hai sotto il mouse, e lo seleziona. Su un'immagine o una cartella (nella griglia o nell'albero): Apri, Taglia, Copia (su una cartella anche Incolla in questa cartella, quando hai copiato qualcosa), Rinomina, poi, separate in fondo, Elimina ed Elimina definitivamente. Sull'immagine aperta nel visore le stesse, con Torna alla griglia al posto di Apri. Su **..**: Cartella superiore. Su uno spazio vuoto della griglia: Incolla (quando hai copiato qualcosa), Aggiorna e Cartella superiore (dentro il Cestino anche Ripristina tutto e Svuota il cestino).

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
- Le immagini più grandi di 8192 pixel per lato vengono mostrate ridotte.
- I file sono ordinati alfabeticamente: `img10` viene prima di `img2`.
- File e cartelle nascosti (che iniziano con un punto, o su Windows con l'attributo "nascosto")
  non vengono mostrati.
- Le immagini HDR (EXR, HDR, PFM) sono mostrate senza tone mapping e possono apparire sovraesposte.
