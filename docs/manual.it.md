# Manuale di EyeCanSee

Premi **F1** in qualsiasi momento per aprire o chiudere questo manuale.

## La finestra

- **Barra dei menù**: File, Visualizza, Vai, Aiuto. A destra il numero di versione. Ogni azione che si fa con la tastiera o col doppio clic ha anche una voce di menù, con il tasto scritto accanto.
- **Barra strumenti**: cartella superiore, aggiorna, metadati (ℹ), barra degli indirizzi, ordine delle immagini e cursore per la dimensione delle miniature.
- **Albero delle cartelle** a sinistra, sotto **Questo computer** (e sotto **Dispositivi** se ne hai collegati): clic su una cartella per mostrarne il contenuto. Le cartelle vengono lette solo quando le espandi, così le sorgenti remote restano veloci. Quando cambi cartella, anche dalla griglia, l'albero si apre fino a quella cartella e la evidenzia.
- **Griglia** a destra: prima le sottocartelle, poi le immagini come miniature. Entrando in una cartella nessun elemento è selezionato; risalendo è selezionata la cartella da cui vieni.
- **Ordine delle immagini** dalla tendina nella barra strumenti (con accanto il pulsante ▲/▼ per invertirlo) o dal menù **Visualizza**: per nome, data di modifica, dimensione o tipo (estensione), più **Ordine decrescente**. Le cartelle restano prima, per nome; i file senza data restano in fondo. La scelta viene ricordata.
- **Metadati** sotto la griglia, per l'immagine selezionata: riquadri **Fotocamera** (marca, modello, obiettivo, software), **Scatto** (data, esposizione, diaframma, ISO, lunghezza focale, flash), **Immagine** (dimensioni, spazio colore) e **Posizione** (coordinate GPS, altitudine, con il link per aprirle sulla mappa di OpenStreetMap). Ogni riquadro si chiude con la sua X e si riapre dal menù **Visualizza**; il pulsante ℹ nella barra strumenti li chiude o li riapre tutti. Il pannello si comprime e si riapre col pulsante ▼/▲ a destra della sua barra, o con un clic sulla barra stessa; compresso resta solo la barra, e i metadati non vengono letti. Si leggono i metadati EXIF di JPEG, HEIC, AVIF, PNG, WebP e TIFF; per leggerli EyeCanSee rilegge il file, quindi su un telefono ci vuole un attimo.
- **Barra di stato** in basso: quante cartelle e immagini ci sono, e nome, peso e data di modifica dell'elemento selezionato.
- Frecce e Tab agiscono sulla parte su cui hai cliccato per ultima, sinistra o destra: è quella con la cornice sottile. Lì la selezione è blu pieno, nell'altra più tenue.

Doppio clic su un'immagine, oppure selezionala e premi Invio, per aprirla nel **visore**. Per tornare alla griglia premi Esc, fai doppio clic sull'immagine o usa il pulsante griglia nella barra strumenti.

## Miniature

EyeCanSee crea le miniature in questo ordine: prima l'immagine selezionata, poi quelle visibili nella finestra dall'alto in basso, poi tutte le altre della cartella dall'alto in basso. Facendo clic su un'immagine la sua miniatura passa in cima alla coda. Fino a 1000 miniature per cartella restano in memoria, così scorrendo, anche trascinando la barra, non spariscono; nelle cartelle più grandi quelle lontane dalla parte visibile si ricaricano dal disco.

Le miniature vengono salvate su disco, così riaprire la stessa cartella è immediato: nel file `~/.cache/eyecansee/thumbs.db` su Linux e `%LOCALAPPDATA%\eyecansee\thumbs.db` su Windows. Se un'immagine cambia (dimensione o data di modifica), la miniatura viene rifatta. Puoi cancellare quel file (con EyeCanSee chiuso) in qualsiasi momento: le miniature si ricreano quando servono.

**Pulizia della cache miniature** in **File > Opzioni…** (spenta all'inizio): una volta per sessione, per ogni indirizzo aperto, elimina le miniature delle immagini cancellate e rifà quelle delle immagini modificate; poi elimina le miniature usate meno di recente finché la cache sta nel **Limite della cache** (1024 MB all'inizio). Le cartelle che non si trovano o non si raggiungono vengono saltate, così un disco staccato o un NAS spento non perdono le loro miniature. Le sorgenti remote vengono controllate solo quando le apri.

## Opzioni

**File > Opzioni…** apre la finestra delle opzioni. Ogni modifica viene salvata subito in `~/.config/eyecansee/settings.json` su Linux e `%APPDATA%\eyecansee\settings.json` su Windows, un file di testo che puoi anche modificare a mano.

- **Riapri l'ultima cartella all'avvio** (attiva all'inizio): quando EyeCanSee parte senza una cartella, per esempio dal menù delle applicazioni, apre quella che stavi guardando alla chiusura. Se quella cartella non esiste più, apre la tua cartella home. Gli indirizzi remoti si riaprono senza password, che va riscritta.
- **Ricorda la posizione di scorrimento di ogni cartella** (spenta all'inizio): tornando in una cartella già vista, la griglia riparte dal punto in cui l'avevi lasciata. Spenta, ogni cartella aperta parte dalla prima anteprima. Vale finché EyeCanSee resta aperto.
- **Nel visore, dopo l'ultima immagine ricomincia dalla prima** (spenta all'inizio): e dalla prima immagine, tornando indietro, mostra l'ultima. Spenta, il visore si ferma alla prima e all'ultima immagine.
- **Finestra all'avvio**, una sola scelta: **Dimensione normale**, **Massimizzata** (occupa tutto lo schermo lasciando visibili le barre del sistema), oppure **Dimensione che aveva alla chiusura** (se era massimizzata si riapre massimizzata).
- **Lingua**: **Come il sistema** (predefinita: la lingua di Windows o del desktop, inglese se non è tra quelle disponibili) oppure una delle lingue elencate, ognuna col suo nome. La scelta vale subito, anche per questo manuale.
- **Colori**: **Come il sistema** (predefinito: chiari o scuri come il tema di Windows o del desktop, e cambiano insieme a lui), **Chiari** o **Scuri**. La scelta vale subito.
- **Pulizia della cache miniature** e **Limite della cache**: vedi *Miniature*.
- **Svuota la cache miniature…**: elimina tutte le miniature salvate, dopo una conferma con il numero di miniature e lo spazio liberato. Le immagini non vengono toccate; le miniature della cartella aperta si rifanno subito, le altre quando le riapri.
- **Elimina i file del computer definitivamente, senza cestino (pericoloso)** (spenta all'inizio): vedi *Eliminare le immagini*.
- **Eliminare su telefoni e server**: vedi *Eliminare le immagini*.

## Eliminare le immagini

**Canc** (o **File > Elimina…**) elimina l'immagine selezionata nella griglia o quella aperta nel visore, dopo una conferma con il nome e la dimensione. Nella conferma **Invio** elimina ed **Esc** annulla. Nel visore compare subito l'immagine successiva.

- **File del computer e dischi esterni**: vanno nel cestino del sistema, da cui si possono ripristinare. Con l'opzione **Elimina i file del computer definitivamente, senza cestino** vengono eliminati per sempre.
- **Telefoni e server (FTP, SFTP)** non hanno un cestino raggiungibile. Nelle Opzioni, sotto **Eliminare su telefoni e server**, scegli tra **Sposta nella cartella nascosta .EyeCanSee-trash accanto all'immagine** (predefinita: l'immagine finisce in una cartella `.EyeCanSee-trash` dentro la sua cartella, e si recupera con un gestore di file) ed **Elimina definitivamente**. L'FTP non sa spostare i file: lì serve **Elimina definitivamente**.

**Maiusc+Canc** (o **File > Elimina definitivamente…**) elimina per sempre solo quella volta, qualunque sia l'opzione.

## Dispositivi esterni

Quando colleghi un telefono, una fotocamera, un disco esterno, una chiavetta o una scheda SD, in alto a sinistra compare il blocco **Dispositivi**, con la struttura delle sue cartelle da espandere come l'albero. Quando lo scolleghi, sparisce; se lo stavi guardando, EyeCanSee torna alla tua cartella personale.

- **Telefono Android**: sul telefono scegli **Trasferimento foto** (PTP) o **Trasferimento file** (MTP). Con Trasferimento foto si vedono solo le cartelle delle foto (`DCIM`, `Pictures`); con Trasferimento file tutta la memoria.
- EyeCanSee si collega al telefono direttamente, anche se il desktop lo ha già collegato (per esempio GNOME): così è molto più veloce, soprattutto nelle cartelle con centinaia di file. Il dispositivo compare entro un paio di secondi.
- Su GNOME, aprendo il telefono in EyeCanSee il file manager perde il suo collegamento. Per riaverlo, chiudi EyeCanSee e fai clic sul telefono nel file manager, oppure scollegalo e ricollegalo.
- **"Il dispositivo è in uso da un'altra applicazione"**: un altro programma tiene il telefono. Se il desktop lo ha collegato, EyeCanSee passa da solo al suo collegamento (più lento: nelle cartelle con centinaia di file circa una miniatura al secondo); altrimenti chiudi l'altro programma o scollega e ricollega il telefono.
- **"Il dispositivo non mostra nessuna memoria"**: il telefono era bloccato o in "solo ricarica" quando è stato collegato: sbloccalo e scegli Trasferimento file. EyeCanSee riprova da solo ogni 2 secondi e mostra le memorie appena compaiono, senza bisogno di F5.
- **"Il dispositivo non è stato trovato"**: il telefono si sta ricollegando (lo fa quando scegli Trasferimento file) oppure è stato scollegato. EyeCanSee riprova da solo ogni 2 secondi, senza bisogno di F5.
- Aprendo un telefono collegato direttamente, il blocco mostra le sue cartelle e sotto c'è la voce per tornare alla tua cartella home.
- Le miniature dei dispositivi restano in cache anche dopo averli scollegati.
- I dischi esterni compaiono ovunque il sistema li monti. Una partizione che il sistema non ha montato da sola (per esempio perché ha una sua riga in `/etc/fstab`) compare col nome più chiaro: un clic la monta e la apre. Se serve, il sistema chiede la password.
- Su Windows compaiono le chiavette, le schede SD e i dischi collegati via USB, col nome che hanno in Esplora file (per esempio "Backup (D:)"). Le unità interne (C:, ed eventuali altre) stanno sotto **Questo computer**: un clic su un'altra unità la apre. I telefoni si collegano tramite il driver di Windows. Unità di rete e CD/DVD per ora non compaiono: aprili con **File > Apri indirizzo…** (per esempio `E:\`).

## Aprire le immagini

Scrivi una cartella, un'immagine o un indirizzo remoto nella barra degli indirizzi e premi Invio (File > Apri indirizzo ci porta il cursore), oppure avvia EyeCanSee indicandolo:

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
| Frecce | Spostano la selezione (la prima pressione seleziona il primo elemento) |
| Inizio, Fine | Primo, ultimo elemento |
| Invio | Apre la cartella o l'immagine |
| Backspace | Cartella superiore |
| Canc | Elimina l'immagine (vedi *Eliminare le immagini*) |
| Maiusc+Canc | Elimina l'immagine definitivamente |
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
| Canc, Maiusc+Canc | Elimina l'immagine, come nella griglia |

Ovunque: **F1** apre questo manuale, **F5** rilegge le cartelle. Nel visore F5 resta sull'immagine aperta; se nel frattempo è stata cancellata, torna alla griglia.

Le cartelle del computer si aggiornano da sole: immagini aggiunte, eliminate, rinominate o ripristinate dal cestino da un altro programma compaiono e spariscono senza F5, anche nel visore, che resta sull'immagine aperta. Su telefoni e server serve F5.

Le stesse azioni sono nei menù: **Visualizza** (Apri, Torna alla griglia, Aggiorna) e **Vai** (Avanti, Indietro, Primo, Ultimo, Cartella superiore). Nella griglia Avanti e Indietro spostano la selezione, nel visore cambiano immagine.

Nel visore, dopo l'ultima immagine EyeCanSee si ferma; con l'opzione **Nel visore, dopo l'ultima immagine ricomincia dalla prima** torna alla prima, e viceversa. EyeCanSee carica in anticipo le immagini vicine a quella corrente, così il passaggio è immediato anche con connessioni lente.

## Formati supportati

- **Foto**: JPEG, PNG, WebP, HEIC/HEIF, AVIF, JPEG XL, TIFF, GIF (primo fotogramma), BMP.
- **RAW delle fotocamere**: ARW, CR2, CR3, CRW, DNG, NEF, NRW, ORF, PEF, RAF, RW2, SRW, 3FR, IIQ, ERF, KDC, MEF, MOS, MRW e altri. EyeCanSee mostra l'anteprima a piena risoluzione salvata dalla fotocamera; se manca, sviluppa i dati del sensore.
- **Vettoriali**: SVG, SVGZ.
- **Grafica e HDR**: PSD/PSB (immagine unita), XCF, ORA, OpenEXR, HDR, PFM, DDS, QOI, TGA, ICO, CUR, PCX, JPEG 2000, PNM, XPM, XBM, SGI, Sun Raster, PICT, MIFF, MNG, JNG, DPX, Cineon, FITS, MPO e altri formati meno comuni.

Le foto vengono ruotate in automatico secondo l'orientamento EXIF. L'estensione del file è solo un indizio: una foto HEIC di un iPhone rinominata in `.jpg` si apre lo stesso.

Su Windows, per ora, HEIC/HEIF, AVIF e i formati della voce *Grafica e HDR* gestiti da ImageMagick (PSD, XCF, PCX, JPEG 2000 e simili) non sono ancora disponibili: quei file non compaiono nell'elenco.

## Lingua

EyeCanSee usa la lingua del sistema se è disponibile, altrimenti l'inglese. Per forzare una lingua imposta `EYECANSEE_LANG`, per esempio `EYECANSEE_LANG=en eyecansee`.

## Limiti noti

- SFTP non è ancora disponibile su Windows.
- Le immagini più grandi di 8192 pixel per lato vengono mostrate ridotte.
- I file sono ordinati alfabeticamente: `img10` viene prima di `img2`.
- File e cartelle nascosti (che iniziano con un punto, o su Windows con l'attributo "nascosto")
  non vengono mostrati.
- Le immagini HDR (EXR, HDR, PFM) sono mostrate senza tone mapping e possono apparire sovraesposte.
