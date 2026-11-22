# Componenti aggiuntivi

Alcune parti di EyeCanSee sono grandi e non servono a tutti, quindi non sono nel pacchetto: EyeCanSee le scarica la prima volta che servono, una volta sola, dopo avertelo chiesto.

## I componenti

| Componente | A che cosa serve | Quando viene proposto |
|---|---|---|
| Motore video (libmpv) | Riprodurre video e file audio | Quando la cartella aperta contiene un video o un file audio e sul computer non c'è una libmpv utilizzabile (su Windows, di solito la prima volta) |
| FFmpeg | Convertire i video | Nella finestra Converti, quando sul computer non c'è un FFmpeg utilizzabile |
| Motore CAD (Open CASCADE) | Aprire i modelli STEP, IGES e BREP | Quando la cartella aperta contiene uno di questi file |

Su Linux EyeCanSee usa libmpv e FFmpeg del sistema quando sono abbastanza recenti (libmpv 1.109, cioè mpv 0.33, e FFmpeg 4.3 o più recenti); lo scaricamento viene proposto solo se mancano o sono troppo vecchi, oppure se non sanno riprodurre H.264 e HEVC, i formati in cui registrano i telefoni (per esempio i pacchetti di Fedora non hanno nessuno dei due). Un FFmpeg così copia comunque video e audio così come sono, senza scaricamento.

## Scaricarne uno

- **Motore video**: in alto a destra nella barra dei menù compare il pulsante rosso **Scarica il motore video**, con il suo peso; lo propone anche il visore, la prima volta che apri un video. Fino ad allora nella griglia video e file audio mostrano il loro segno (il tasto play, una nota) con una freccia di scaricamento. Di solito i video partono subito; se il motore non riesce ad avviarsi, il pulsante diventa **Riavvia EyeCanSee**.
- **FFmpeg**: la finestra Converti mostra **Scarica FFmpeg**, con il suo peso, e una barra mentre scarica (vedi [Convertire i video](convert-video.md)).
- **Motore CAD**: in alto a destra nella barra dei menù compare il pulsante rosso **Scarica il motore CAD**, con il suo peso. Fino ad allora quei modelli mostrano un cubo con una freccia di scaricamento, e da un telefono o da un server non viene letto niente per fare la loro miniatura. Appena scaricato, le miniature e il modello aperto nel visore compaiono da soli.

Passando sopra un pulsante rosso si legge che cos'è il componente e quanto pesa.

## Al primo avvio e nelle opzioni

La prima volta che EyeCanSee si avvia, dopo la pagina in cui scegli la lingua e i colori (su Windows una pagina chiede poi se vuoi un'icona di EyeCanSee sul desktop: scegli **Sì**, che la crea subito, oppure **No**, e poi **Avanti**), una pagina elenca i componenti, una scheda ciascuno con a cosa serve e quanto pesa. Niente è spuntato: fai clic sulle schede di quelli che vuoi e premi **Scarica**, oppure **Fine** per andare avanti senza. Se chiudi la finestra con delle schede spuntate e non scaricate, anche con **Salta** dalla prima pagina, EyeCanSee prima chiede: **Scarica e chiudi**, **Chiudi senza scaricare**, oppure **Indietro** per tornare alla pagina in cui eri. **Scarica e chiudi** li scarica nella finestra, ognuno con la sua barra, e la chiude da sola quando sono arrivati tutti. Uno scaricamento che non riesce dice il perché nella sua scheda, che resta spuntata: premi **Scarica** per riprovare, oppure **Fine** per chiudere. Ogni scaricamento mostra l'avanzamento nella sua scheda e continua anche dopo aver chiuso la finestra. Un componente già presente su questo computer ha un segno verde. La finestra torna da sola solo quando una versione nuova di EyeCanSee porta un componente nuovo, e allora mostra solo quello.

La stessa lista è sempre in **Strumenti > Opzioni… > Componenti aggiuntivi**, con **Scarica** o **Aggiorna** su ogni componente (vedi [Opzioni](options.md)).

## Dove vanno

Ogni componente viene dalle release di EyeCanSee su GitHub. La sua impronta SHA-256 è scritta dentro il programma, quindi un file diverso viene rifiutato, e non resta niente al suo posto se lo scaricamento non è completo e controllato. I componenti vanno nella cartella dei dati dell'app:

- Linux: `~/.local/share/eyecansee/components`
- Flatpak: `~/.var/app/<id dell'applicazione>/data/eyecansee/components`
- Windows: `%LOCALAPPDATA%\eyecansee\components` (la copia del Microsoft Store: `components-store`, sua)

## Da sapere

- Quali libmpv e FFmpeg usare, quelli del sistema o quelli scaricati, si sceglie in **Strumenti > Opzioni… > Video** (vedi [Opzioni](options.md)). La modifica vale dal prossimo avvio di EyeCanSee.
- Quando una versione nuova di EyeCanSee ha bisogno di un componente più nuovo, lo chiede di nuovo, e quello vecchio viene tolto quando il nuovo è arrivato.
- Su Windows, disinstallando EyeCanSee si tolgono anche i componenti scaricati.
- Per togliere un componente, chiudi EyeCanSee ed elimina la sua cartella: verrà proposto di nuovo quando serve.
