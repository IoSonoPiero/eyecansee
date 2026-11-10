# Limiti noti

Che cosa EyeCanSee non fa ancora, o fa solo in parte. L'elenco aggiornato dei problemi noti è nella sezione "Known limitations" della [roadmap](https://github.com/IoSonoPiero/eyecansee/blob/main/ROADMAP.md) (in inglese).

## Immagini

- Le immagini più grandi di 8192 pixel per lato vengono mostrate ridotte.
- Le immagini HDR (EXR, HDR, PFM) vengono mostrate senza tone mapping e possono sembrare sovraesposte.
- EyeCanSee non sa salvare file HEIC o JPEG XL: vedi [Formati supportati](formats.md).
- Le foto RAW convertite non tengono la maker note, i dati propri del produttore della fotocamera.
- Su Windows i file JNG non si aprono: li blocca la politica di sicurezza di ImageMagick che arriva con il programma.

## Navigare

- I file sono in ordine alfabetico: `img10` viene prima di `img2`.
- I file e le cartelle nascosti (il cui nome inizia con un punto, o su Windows con l'attributo "nascosto") non vengono mostrati.
- Un file o una cartella il cui nome non è UTF-8 valido (su Linux, per esempio creato da un vecchio programma o copiato da un vecchio disco) compare con � al posto dei caratteri che non si possono leggere, ma non si può aprire, copiare né convertire: rinominalo nel gestore dei file.
- Le modifiche fatte su un server da altri programmi si vedono solo dopo **F5**.

## Telefoni

- Su Windows, subito dopo aver scattato una foto, o eliminato un file, sul telefono, il driver di Windows può smettere di parlare con il telefono per circa sei minuti. Staccare il cavo e ricollegarlo lo sblocca subito. Di più in [Windows smette di parlare con il telefono](https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.it.md).
- Su Windows, se la connessione al telefono si interrompe mentre Windows vede ancora il telefono, dopo **Riprova** la cartella aperta continua a dire che il telefono è stato scollegato finché EyeCanSee non viene riavviato. Il file viene salvato comunque.
- A volte un telefono non risponde a una richiesta per molto tempo (visto su Windows, eliminando file appena creati). La barra di stato lo dice, con i secondi e il file, ed EyeCanSee riprende da solo appena il telefono risponde.
- Un telefono aperto tramite il collegamento del gestore dei file è più lento di quello diretto: nelle cartelle con centinaia di file circa una miniatura al secondo.
- Le foto eliminate dalla Galleria Xiaomi vanno nel cestino privato della Galleria, che EyeCanSee non vede.

## Dischi e server

- Su Linux, sui dischi **NTFS** (per esempio un disco esterno usato anche con Windows) l'app File (Nautilus) non riesce a ripristinare dal cestino, nemmeno i file che ha eliminato lei. I file sono comunque nel cestino e si ripristinano dal terminale: `gio trash --list` mostra che cosa c'è, `gio trash --restore "<indirizzo trash:/// di quella riga>"` lo rimette a posto.
- Su Windows le unità di rete e i CD o DVD non compaiono nel pannello: aprili con **File > Apri indirizzo…**.
- FTPS: un server che vuole che la connessione dei dati riprenda la sessione TLS di quella di controllo può non mandare mai i dati.
- SFTP: ProxyJump e ProxyCommand di `~/.ssh/config` non sono supportati.

## Modelli 3D e disegni CAD

- **Modelli 3D**: c'è una sola luce e non si riflette nessun ambiente, quindi i metalli sembrano meno lucidi che nei programmi 3D. In OBJ, Collada, 3DS, 3MF, VRML e X3D si usano solo il colore di base e la sua immagine. I file glTF 1.0 e VRML 1.0 non si aprono. Le animazioni non vengono riprodotte: i modelli si vedono nella posa di riposo. I testi in VRML e X3D usano un carattere del sistema.
- I **disegni CAD** si vedono dall'alto: i solidi 3D non vengono disegnati, le finestre mostrano il modello dall'alto e tagliato al loro rettangolo, i tratteggi troppo fitti per vederli appaiono come un riempimento chiaro, un tipo di linea troppo fine per l'ingrandimento attuale viene disegnato continuo, i testi di cui non si trova il carattere SHX usano un carattere del sistema, e un'immagine collegata al disegno il cui file non è accanto a lui appare come un riquadro con una croce.
