# Password salvate

Le password e le passphrase delle chiavi dei server salvati si possono proteggere con una password principale: l'elenco dei server viene cifrato, ed EyeCanSee chiede la password principale una volta per sessione. Senza, sono salvate in chiaro.

## Senza password principale

Il riquadro giallo in cima a **File > Gestione server…**, con il triangolo di avviso, dice **Nessuna password principale: password e passphrase sono salvate in chiaro.** Sono scritte nel file `servers.json` della cartella delle impostazioni (su Linux leggibile solo da te). Chi può leggere i tuoi file può leggere anche queste password. La prima volta che salvi un server, nella gestione server o con **Salva e apri** in **File > Apri indirizzo…**, EyeCanSee te lo dice con un avviso, **Server salvati in chiaro**.

## Impostare una password principale

1. Apri **File > Gestione server…**.
2. Nel riquadro in alto fai clic su **Imposta una password principale…**.
3. Scrivila in **Nuova password principale** e di nuovo in **Scrivila di nuovo**.
4. La barra mostra quanto è robusta, dal rosso (**Molto debole**) al verde (**Molto forte**). Serve almeno **Forte**: una password più lunga, per esempio più parole senza legame tra loro, è più robusta.
5. Fai clic su **Salva la password principale**.

Da quel momento tutto l'elenco dei server è cifrato, nel file `servers.vault`, e ogni modifica viene scritta subito, cifrata. Il riquadro dice **Password principale attiva: l'elenco dei server è cifrato.**

## Usare i server salvati

EyeCanSee chiede la password principale una volta per sessione, la prima volta che apri un server salvato o la gestione server. Finché l'elenco è bloccato, la sezione **Server** del pannello mostra **Sblocca i server salvati…**. Scrivi la password nella finestra **Server salvati** e fai clic su **Sblocca** (o premi **Invio**). Resta in memoria finché EyeCanSee non si chiude.

## Cambiarla o toglierla

- **Cambia la password principale…** ne imposta una nuova, allo stesso modo.
- **Togli la password principale…** riporta il file in chiaro, dopo un avviso: **Toglila e salva in chiaro** conferma.

Ogni passaggio prima scrive il file nuovo, lo rilegge e lo controlla, e solo dopo sostituisce quello vecchio: il file resta sempre intero.

## Da sapere

- Se dimentichi la password principale, i server salvati non si recuperano. Conservala in un gestore di password.
- La cifratura usa Argon2id e ChaCha20-Poly1305.
- La cartella delle impostazioni è `~/.config/eyecansee` su Linux e `%APPDATA%\eyecansee` su Windows.
- I segnalibri e l'ultima cartella riaperta all'avvio non tengono mai una password.
