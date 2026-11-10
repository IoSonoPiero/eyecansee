# Eliminare e cestino

Elimina file e cartelle, e li recupera dal cestino. Sul computer i file vanno nel cestino di sistema; telefoni e server non hanno un cestino che EyeCanSee possa usare, così EyeCanSee tiene un cestino suo in ogni cartella in cui elimini qualcosa.

## Eliminare

1. Seleziona i file o le cartelle nella griglia, oppure apri l'immagine nel visore.
2. Premi **Canc**, oppure fai clic su ![Elimina…](icon://trash) nella barra strumenti, oppure scegli **File > Elimina…**, oppure fai clic destro e scegli **Elimina…**. Una cartella dell'albero a sinistra si elimina dal suo clic destro.
3. Una finestra chiede conferma, con il nome e il peso (con più elementi: quanti sono e il peso delle loro immagini). Il pulsante dice che cosa succede: **Sposta nel cestino**, oppure **Elimina** quando è definitivo.
4. **Invio** conferma, **Esc** annulla.

Nel visore compare subito l'immagine successiva.

## Eliminare una cartella

Una cartella viene eliminata con tutto quello che contiene. Quando tra gli elementi c'è una cartella, EyeCanSee legge il suo primo livello mentre si apre la finestra, e il pulsante di conferma aspetta un momento. Se la cartella contiene file o cartelle che la griglia non mostra (documenti, file nascosti e simili), che verrebbero eliminati anche loro, la finestra lo dice con un esempio; se la cartella non si riesce a leggere, dice anche questo. Con una cartella **Invio** annulla, come Esc: per eliminare fai clic sul pulsante.

## Dove vanno i file eliminati

- **I file del computer e dei dischi esterni** vanno nel cestino di sistema, da cui si ripristinano con il gestore dei file.
- **Su Windows, chiavette USB, schede di memoria e unità di rete** non hanno il Cestino: lì il file va nella cartella nascosta `.EyeCanSee-trash` accanto a lui, come sui telefoni, e la finestra lo dice.
- **Telefoni e server**: di solito il file va in una cartella nascosta `.EyeCanSee-trash` dentro la sua cartella. In **Strumenti > Opzioni… > Eliminazione** puoi scegliere invece **Elimina definitivamente** (vedi [Opzioni](options.md)).
- Con l'opzione **Elimina i file del computer definitivamente, senza cestino (pericoloso)**, i file del computer vengono eliminati per sempre.

**Maiusc+Canc**, oppure **File > Elimina definitivamente…**, elimina per sempre quella volta, qualunque cosa dicano le opzioni.

## Più elementi

Con più elementi selezionati, **Canc** li elimina tutti dopo una sola conferma. Se uno non riesce, i successivi non vengono eliminati. Mentre un'eliminazione è in corso, la barra di stato lo dice con un segno che gira, e non se ne può avviare un'altra: su un telefono una cartella con centinaia di file richiede diversi secondi.

## Il cestino di telefoni e server

Ogni cartella in cui hai eliminato qualcosa su un telefono o su un server (e su una chiavetta USB con Windows) ha il suo cestino.

- Una striscia in cima alla griglia dice **Questa cartella ha un cestino**, con il pulsante **Apri il cestino**.
- Dentro, la striscia dice di quale cartella è il cestino e ha **Ripristina tutto** e **Svuota il cestino…**.
- Nel cestino puoi guardare le immagini, ripristinarle (clic destro > **Ripristina**: tornano nella cartella che contiene il cestino) o eliminarle per sempre. Lì **Canc** elimina sempre per sempre, e Rinomina non c'è.
- Clic destro su uno spazio vuoto del cestino: **Ripristina tutto** e **Svuota il cestino…** (tutto eliminato per sempre, dopo una conferma).
- Un cestino rimasto vuoto sparisce insieme alla sua striscia.

![Eliminare e cestino](../img/trash-strip.webp)

Se il nome di un elemento da ripristinare è già preso, EyeCanSee chiede: per un file **Sostituisci** o **Tieni entrambi** (quello ripristinato diventa per esempio `foto (2).jpg`); per una cartella **Unisci** o **Tieni entrambi**. **Annulla** ferma anche il resto di un Ripristina tutto.

## Il cestino di Android

Sui telefoni Android, il cestino di una cartella mostra anche le foto e i video che la galleria del telefono ha messo nel suo cestino (file nascosti che Android elimina da solo dopo 30 giorni, come fanno Google Foto e Files di Google). Compaiono con il loro nome vero e si ripristinano come gli altri. **Svuota il cestino…** elimina anche loro. La Galleria Xiaomi tiene il suo cestino altrove, e EyeCanSee non lo vede.

## Da sapere

- Non si possono eliminare la cartella principale di un disco, di un telefono o di un server e le memorie di un telefono.
- Una cartella spostata su un altro dispositivo, o unita a una cartella con lo stesso nome, si porta dietro il suo cestino; una copia lo lascia fuori.
- Su Linux, su un disco **NTFS** (per esempio un disco esterno usato anche con Windows), l'app File non riesce a ripristinare dal cestino. I file però ci sono: la conferma te lo ricorda, e [Limiti noti](limits.md) spiega come ripristinarli dal terminale.
- In una cartella aperta tramite il collegamento del gestore dei file (su Linux), l'eliminazione segue la scelta per telefoni e server.
