# Dischi e chiavette

Dischi esterni, chiavette USB e schede di memoria compaiono nel pannello a sinistra appena li colleghi, e funzionano come le cartelle del computer: navigare, guardare, copiare, spostare, eliminare e convertire. I telefoni hanno una pagina loro: [Telefoni Android](phone.md).

## Dove compaiono

- Quando colleghi un disco, una chiavetta USB, una scheda di memoria o un telefono, in alto a sinistra compare il blocco **Dispositivi**, con le sue cartelle da aprire come nell'albero. I dischi hanno l'icona ![Disco](icon://color-disk), i telefoni ![Telefono](icon://color-phone).
- Sotto **Questo computer** c'è l'albero delle cartelle del computer; su Windows anche le sue unità (![Disco interno](icon://color-internal-disk)).
- Fai clic su un dispositivo per aprirlo a destra. Quando lo scolleghi sparisce; se lo stavi guardando, EyeCanSee torna alla tua cartella home.

<!-- shot: devices | panoramica | The main window with the Devices block on the left showing a USB stick and an external disk, This computer below it, and the grid of a folder of the USB stick on the right -->

## Su Linux

- I dischi esterni compaiono ovunque il sistema li monti, anche in `/mnt`.
- Una partizione che il sistema non ha montato da solo (per esempio perché ha una sua riga in `/etc/fstab`) compare con il nome più chiaro: un clic la monta e la apre. Se serve, il sistema chiede la tua password.
- Fotocamere e iPhone si aprono tramite il collegamento del desktop.

## Su Windows

- Chiavette USB, schede di memoria e dischi collegati via USB compaiono con il nome che hanno in Esplora file, per esempio "Backup (D:)".
- Le unità interne (C: ed eventuali altre) sono sotto **Questo computer**: un clic apre un'altra unità.
- Le unità di rete e i CD o DVD non compaiono nel pannello: aprili con **File > Apri indirizzo…**, per esempio `E:\` (vedi [Barra degli indirizzi e Apri posizione](address.md)).
- Chiavette USB, schede di memoria e unità di rete non hanno il Cestino: un file eliminato lì va in una cartella nascosta `.EyeCanSee-trash` accanto a lui, che puoi aprire da EyeCanSee (vedi [Eliminare e cestino](delete.md)).

## Da sapere

- Le miniature di un dispositivo restano nella cache quando lo scolleghi, così la volta dopo si apre subito.
- Le cartelle del computer e dei dischi si aggiornano da sole: i file aggiunti, eliminati o rinominati da un altro programma compaiono e spariscono senza F5.
- Su Linux, in una cartella aperta tramite il collegamento del gestore dei file (gvfs, per esempio una cartella di rete aperta dall'app File) EyeCanSee non salva mai sopra un file: lì sostituire un file può far perdere l'originale. Usa **Salva come…**.
- Su Linux, su un disco **NTFS** l'app File non riesce a ripristinare dal cestino: vedi [Limiti noti](limits.md).
