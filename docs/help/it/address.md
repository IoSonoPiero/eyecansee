# Barra degli indirizzi e Apri indirizzo

La barra degli indirizzi, nella barra strumenti, mostra dove sei e porta ovunque: una cartella di questo computer, una foto, un telefono o un server.

## La barra degli indirizzi

Fai clic nella barra degli indirizzi, scrivi un indirizzo e premi **Invio**. Puoi scrivere:

- una cartella di questo computer o di un disco, per esempio `/home/anna/Immagini` su Linux o `C:\Users\Anna\Pictures` su Windows;
- una foto, per esempio `/home/anna/Immagini/spiaggia.jpg`: si apre nel [visore](viewer.md), con la sua cartella dietro;
- un server: `ftp://…`, `ftps://…` o `sftp://…`, per esempio `sftp://anna@nas.lan/Foto` o `ftp://nas.lan:2121/Foto` (vedi [Server (FTP e SFTP)](servers.md));
- una cartella di un telefono, come la mostra la barra quando il telefono è aperto: `mtp://…` (vedi [Telefoni Android](phone.md)).

Quando cambi cartella, la barra mostra quella nuova. Una password non compare mai nella barra, anche se l'hai scritta lì. Se l'indirizzo non si apre, un messaggio in rosso in fondo alla finestra spiega perché, e la barra torna alla cartella in cui eri.

## Apri indirizzo

**File > Apri indirizzo…** apre una finestra in cui scrivere l'indirizzo di un server un pezzo alla volta, senza ricordare come si scrive.

<!-- shot: open-location | ritaglio | the Open location window with SFTP chosen and Server, User and Folder filled in, Password hidden -->

1. Scrivi l'indirizzo intero nel campo in alto, oppure lascialo vuoto e compila i campi sotto.
2. **Protocollo**: FTP, FTPS o SFTP.
3. **Server**: il nome o l'indirizzo del server.
4. **Porta**: solo se non è quella solita (21 per FTP, 22 per SFTP).
5. **Utente** e **Password**. Con FTP, un utente vuoto entra come `anonymous`.
6. **Cartella**: la cartella da aprire (`/` se vuota).
7. **Apri** (o **Invio**) la apre; **Annulla** (o **Esc**) chiude la finestra.

I campi restano grigi finché il campo in alto contiene qualcosa.

## Quando un server chiede la password

Se un server rifiuta l'accesso, Apri indirizzo si apre da solo con l'indirizzo già compilato: scrivi la password e premi **Invio**. Succede lo stesso all'avvio quando l'ultima cartella era su un server FTP con un utente che non è tra i server salvati, perché EyeCanSee non conserva quella password.

Per non scrivere la password ogni volta, salva il server in **File > Gestione server…**: un indirizzo che corrisponde a un server salvato (stesso protocollo, server, porta e utente), anche scritto nella barra degli indirizzi, usa la sua password e la sua chiave. Vedi [Gestire i server](server-manager.md).

## Avviare EyeCanSee su un indirizzo

EyeCanSee si può avviare con una cartella, una foto o l'indirizzo di un server, per esempio da un altro programma con "Apri con": apre quel posto invece dell'ultima cartella. Se EyeCanSee è già aperto, il posto compare nella finestra che c'è già.
