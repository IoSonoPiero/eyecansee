# Server (FTP e SFTP)

EyeCanSee apre le cartelle di un server, per esempio un NAS di casa o uno spazio web, tramite FTP, FTPS o SFTP. Una cartella del server funziona come una cartella del computer: l'albero, la griglia e il visore sono gli stessi, e puoi copiare e incollare tra il server, il computer, i dischi e i telefoni.

## Aprire un server

- Scrivi il suo indirizzo nella barra degli indirizzi e premi **Invio**, oppure scegli **File > Apri indirizzo…** e compila protocollo, server, porta, utente, password e cartella (vedi [Barra degli indirizzi e Apri indirizzo](address.md)).
- Oppure salvalo una volta in **File > Gestione server…** e poi aprilo con un clic dalla sezione **Server** del pannello a sinistra (vedi [Gestire i server](server-manager.md)).

Esempi di indirizzi:

```text
ftp://utente@example.com/Foto
ftps://utente@example.com/Foto
sftp://utente@example.com:2222/home/utente/Foto
sftp://mio-nas/
```

L'ultimo è un alias di `~/.ssh/config`. Senza utente, FTP entra come `anonymous`. Un indirizzo senza cartella si apre nella cartella da cui parte il server (di solito la tua home lì). Le password non compaiono mai nella barra degli indirizzi.

## La prima connessione

- **SFTP**: la prima volta EyeCanSee mostra l'impronta della chiave del server in un riquadro evidenziato. Confrontala lettera per lettera con quella che mostra il server, poi fai clic su **Collegati**. Una volta accettata va in `~/.ssh/known_hosts`, come fa ssh. Un server la cui chiave è cambiata viene rifiutato: qualcuno potrebbe fingersi quel server.
- **FTPS**: il certificato del server viene controllato come fa un browser. Uno che nessuna autorità conosciuta garantisce (capita spesso con i NAS, che se lo fanno da soli) viene mostrato con la sua impronta la prima volta; una volta confermato, EyeCanSee lo ricorda.

## Accedere con SFTP

- EyeCanSee legge `~/.ssh/config` come fa ssh (alias, HostName, User, Port, IdentityFile; non ProxyJump), su Linux e su Windows.
- Entra con i file di chiave senza passphrase, oppure con la password. Non usa mai l'agente SSH del sistema. Per una chiave con passphrase, salva il server nella gestione server.
- Una password scritta nell'indirizzo viene provata per prima.
- Se il server rifiuta, si apre **File > Apri indirizzo…** con tutto compilato, pronto per la password.
- Le chiavi RSA funzionano, ma dopo un accesso con una di esse EyeCanSee avvisa una volta per server: la libreria che usa ha un difetto noto ancora senza correzione. Una chiave ed25519 è la scelta migliore.

## Lavorare su un server

- Naviga, guarda, copia, sposta, rinomina, elimina e converti come sul computer. Rinomine e spostamenti, cartelle comprese, li fa il server stesso, quindi sono veloci.
- Eliminare sposta il file in una cartella nascosta `.EyeCanSee-trash` accanto a lui, a meno che tu non abbia scelto altro nelle opzioni (vedi [Eliminare e cestino](delete.md)).
- Un'immagine modificata viene prima tenuta su questo computer e poi mandata: se l'invio non riesce, l'originale sul server resta com'era, e una finestra propone **Riprova**, **Salva come…** e **Apri la cartella**.
- I video partono mentre vengono letti, un pezzo alla volta, senza essere copiati interi.
- Le modifiche fatte sul server da altri programmi non si vedono da sole: premi **F5**.

## Da sapere

- Una cartella del server che non esiste più (spostata, eliminata, o riaperta all'avvio) lascia il posto alla cartella del server salvato, o a quella da cui parte il server.
- Se un server SFTP smette di rispondere, EyeCanSee lascia quella connessione, di solito dopo circa 20 secondi di silenzio, e ripete la richiesta su una nuova.
- EyeCanSee tiene una connessione per ogni server SFTP, e fino a quattro per ogni server FTP.
- Per chiudere la connessione a un server salvato, fai clic destro su di esso nel pannello e scegli **Chiudi connessione**.
