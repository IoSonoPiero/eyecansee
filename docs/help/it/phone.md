# Telefoni Android

Collega un telefono Android con il cavo USB per guardare le sue foto e i suoi video, copiarli sul computer, convertirli o eliminarli, senza altri programmi. EyeCanSee parla direttamente con il telefono, molto più velocemente del gestore dei file del desktop, soprattutto nelle cartelle con centinaia di foto.

## Collegare il telefono

1. Collega il telefono con il cavo e sbloccalo.
2. Nella notifica USB del telefono scegli **Trasferimento file** (MTP) per vedere tutta la memoria, oppure **Trasferimento foto** (PTP) per vedere solo le cartelle delle foto (`DCIM`, `Pictures`).
3. Il telefono compare nel blocco **Dispositivi** in alto a sinistra con l'icona ![Telefono](icon://color-phone), entro un paio di secondi. Fai clic per aprirlo.

Il primo livello mostra le memorie del telefono (per esempio "Memoria condivisa interna"): aprine una per vedere le cartelle. Sotto **Questo computer** c'è la tua cartella home, per tornare al computer.

<!-- shot: phone | panoramica | The main window with a phone open: the phone in the Devices block on the left with its DCIM and Pictures folders, This computer with the home folder below, the grid of the Camera folder on the right -->

## Che cosa puoi fare

Tutto quello che fai con le cartelle del computer: guardare foto e video, copiarli e spostarli dal computer e verso il computer (vedi [Copiare e spostare](copy-move.md)), rinominare, eliminare, convertire, cercare i duplicati, creare cartelle. Quando salvi un'immagine modificata, viene prima tenuta su questo computer e poi mandata al telefono: se l'invio non riesce (cavo staccato), l'originale sul telefono resta com'era, e una finestra dice dov'è l'immagine modificata, con **Riprova**, **Salva come…** e **Apri la cartella**.

Le foto scattate, eliminate o spostate sul telefono mentre la sua cartella è aperta compaiono e spariscono da sole, pochi secondi dopo che il telefono ha finito. Se il file era aperto nel visore, il visore si chiude e un messaggio dice quale file è stato tolto.

## Messaggi che puoi vedere

- **"Il dispositivo non mostra nessuna memoria"**: il telefono era bloccato o in sola ricarica quando l'hai collegato. Sbloccalo e scegli Trasferimento file: EyeCanSee riprova da solo ogni 2 secondi.
- **"Il dispositivo non è stato trovato"**: il telefono si sta ricollegando (lo fa quando scegli Trasferimento file) oppure è stato scollegato. EyeCanSee riprova da solo ogni 2 secondi.
- **"EyeCanSee non riesce ad aprire il dispositivo"**: il telefono lo sta usando un altro programma. Se l'aveva collegato il desktop, EyeCanSee passa da solo a quel collegamento (più lento). Altrimenti chiudi il gestore dei file e gli altri programmi che usano il telefono e premi **F5**, oppure scollega il telefono, ricollegalo, scegli Trasferimento file e aprilo prima in EyeCanSee.
- **"Aperto tramite il gestore dei file: più lento"**: il collegamento diretto non è riuscito ed EyeCanSee usa quello del desktop. Nelle cartelle con centinaia di file fa circa una miniatura al secondo.
- Un telefono che non risponde a una richiesta: la barra di stato lo dice, con i secondi e il file. EyeCanSee riprende da solo appena il telefono risponde.

## Linux: GNOME e KDE

- Su GNOME e KDE, aprire il telefono in EyeCanSee lo toglie al gestore dei file. Per rivederlo lì, chiudi EyeCanSee e fai clic sul telefono nel gestore dei file, oppure scollegalo e ricollegalo.
- Una foto del telefono aperta dal gestore dei file (doppio clic nell'app File di GNOME) si apre anch'essa con il collegamento diretto, e la barra di stato lo dice.
- Su KDE EyeCanSee chiede prima: per prendere il telefono deve chiudere il servizio kiod di KDE, che dimentica anche le password delle cartelle di rete aperte in Dolphin. **Annulla** lascia il telefono a KDE; **F5** chiede di nuovo.
- **Flatpak su KDE**: il telefono può restare tenuto da kiod anche dopo aver chiuso Dolphin. EyeCanSee propone **Libera il telefono**: chiudi Dolphin e premilo. Quando EyeCanSee lascia il telefono (alla chiusura, o andando in un'altra cartella), KDE lo rivede.

## Windows

- Subito dopo aver scattato una foto, o eliminato un file, sul telefono, Windows può smettere di parlare con il telefono per circa sei minuti. Dopo 15 secondi un messaggio lo dice, e si chiude da solo quando il telefono risponde di nuovo. Staccare il cavo e ricollegarlo (poi Trasferimento file) lo sblocca subito. Di più in [Windows smette di parlare con il telefono](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.it.md).
- Un video sul telefono viene prima copiato tutto sul computer e poi parte, con una barra che mostra quanto è stato letto.
- Una foto del telefono aperta con un doppio clic in Esplora file è una copia temporanea fatta da Windows, non il file sul telefono: EyeCanSee non può salvarci sopra, e la barra di stato lo dice. Usa **Salva come…**.

## Da sapere

- Eliminare su un telefono sposta il file in una cartella nascosta `.EyeCanSee-trash` accanto a lui, a meno che tu non abbia scelto altro nelle opzioni (vedi [Eliminare e cestino](delete.md)).
- Un telefono accetta nomi di file lunghi al massimo 254 caratteri (un'emoji conta per due): un nome più lungo viene rifiutato, e non viene copiato niente.
- Su un telefono i video ricevono la miniatura dopo le foto, perché per farla un video spesso va letto tutto.
- Si apre un solo EyeCanSee alla volta, perché un telefono può essere usato da un solo programma.
