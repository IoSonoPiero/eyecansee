# Salvare un'immagine modificata

L'editor non cambia mai l'originale se non lo chiedi. Il pulsante principale, **Salva copia**, scrive un file nuovo accanto all'originale.

## Salva copia

**Salva copia** (Ctrl+S) fa una copia accanto all'originale:

- Il nome è quello dell'originale con "(modificata)", per esempio `spiaggia (modificata).jpg`; "(modificata 2)" se quel nome è già preso.
- Il formato è quello dell'originale quando EyeCanSee sa scriverlo (JPEG, PNG, WebP, AVIF, TIFF, BMP, GIF, QOI). Altrimenti è un PNG se l'immagine ha trasparenza o viene da un file OpenEXR o Radiance HDR, e un JPEG negli altri casi (per esempio un HEIC, un RAW o un PSD).
- Prima di scrivere, una finestra dice il nome della copia e cosa perde, se perde qualcosa: ricodificata con una piccola perdita, 8 bit per colore invece di più, ciò che è più luminoso del bianco. **Salva** la scrive; **Annulla** torna indietro.

L'originale resta com'è. La pagina resta aperta, così puoi continuare. Quando la chiudi, la griglia seleziona la copia e il visore la mostra.

Se un salvataggio non riesce, un messaggio in rosso in fondo alla finestra dice perché, e la pagina resta aperta con le modifiche.

## Aggiorna copia

Dopo il primo salvataggio il pulsante diventa **Aggiorna copia**. Riscrive la stessa copia, sempre partendo dall'originale con tutte le modifiche, così niente viene compresso due volte. Se la copia è stata cancellata o un altro programma l'ha cambiata, ne fa una nuova e la barra di stato lo dice.

Il pulsante è spento quando non c'è niente di nuovo da salvare.

## Altri modi di salvare

La freccia ![Altri modi di salvare](icon://chevron-down) accanto a Salva copia ne ha altri due:

- ![Salva come…](icon://file-output) **Salva come…** (Ctrl+Maiusc+S): apre [Salvare in un altro formato](save-as.md) con il risultato, per scegliere nome, formato e cartella.
- ![Sovrascrivi l'originale…](icon://save) **Sovrascrivi l'originale…**: scrive l'immagine modificata sopra l'originale, dopo una domanda. L'originale va nel cestino, anche quando nelle [Opzioni](options.md) è scelta l'eliminazione definitiva. Se non può andare in un cestino, non viene sovrascritto niente. La pagina resta aperta; un secondo Sovrascrivi riparte dall'originale com'era all'apertura e sostituisce ciò che era stato scritto prima, così nel cestino c'è solo il vero originale. Vedi [Eliminare e cestino](delete.md).

Sovrascrivi è spento, e passandoci sopra dice perché, per un'immagine nuova, per un formato che EyeCanSee non riscrive (HEIC, RAW…) e nei posti dove EyeCanSee non scrive mai sopra un file (una cartella aperta tramite il collegamento gvfs del file manager, una copia temporanea fatta da Windows).

## Cosa conserva il file salvato

- Un JPEG salvato come JPEG mantiene la qualità con cui era stato salvato, così non viene compresso di più. Un'immagine di un altro formato salvata come JPEG viene scritta con qualità 95.
- I metadati (EXIF, XMP, IPTC, profilo colore) restano, con la misura nuova, senza le piccole anteprime dell'originale.
- Una foto Ultra HDR del telefono resta HDR. Il video di una foto in movimento non viene conservato, a meno che tu l'abbia solo girata o riflessa.
- Ogni oggetto, filtro e ritaglio viene applicato a piena grandezza.

## Chiudere con modifiche non salvate

Chiudi, la ✕ della finestra o Esc chiedono prima cosa fare:

- **Salva…**: una copia, come Salva copia (Salva come… per un'immagine nuova). Dopo la conferma, la pagina si chiude appena la copia è scritta. Se il salvataggio non riesce, la pagina resta aperta con le modifiche.
- **Salva come…**: chiude la pagina e apre Salvare in un altro formato.
- **Non salvare**: chiude e dimentica le modifiche.
- **Annulla**: torna alla pagina.

## Copiare negli appunti

![Copia](icon://copy) **Copia** (Ctrl+C) mette negli appunti tutto il risultato, a piena grandezza, da incollare in un altro programma. Sul disco non viene scritto niente.

## Telefoni e server

Un'immagine su un telefono o su un server si modifica in una copia su questo computer, e la barra di stato lo dice. Il file nuovo viene prima conservato sul computer, poi inviato. Se l'invio non riesce, resta sul computer e una finestra offre **Riprova**, **Salva come…** e **Apri la cartella**. Vedi [Telefoni Android](phone.md) e [Server (FTP e SFTP)](servers.md).

Una foto del telefono aperta da Esplora file su Windows è una copia temporanea fatta da Windows: lì Salva copia passa direttamente a Salva come….
