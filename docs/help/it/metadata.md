# Informazioni sul file

Sotto la griglia, il pannello dei metadati mostra quello che si sa del file selezionato: il nome e il peso, e quello che la fotocamera o il telefono ci hanno scritto dentro (quando, con quale fotocamera, dove).

<!-- shot: metadata-panel | ritaglio | the metadata panel under the grid for a phone photo, with the boxes File, Camera, Shot, Image and Location filled in, and the Open on the map link -->

## I riquadri

- **File**: nome, peso (anche in byte), data del file e dimensioni in pixel. C'è per ogni file, anche senza metadati.
- **Fotocamera**: marca, modello, obiettivo e software.
- **Scatto**: data dello scatto, esposizione, diaframma, ISO, lunghezza focale e flash.
- **Immagine**: formato, larghezza e altezza, colore e bit per canale, spazio colore.
- **Posizione**: coordinate GPS e altitudine, con **Apri sulla mappa**, che apre il posto su OpenStreetMap nel browser.

Un riquadro dice "Nessun dato" quando il file non ha niente per lui, e "Questo file non contiene metadati" quando il file non ha nessun metadato.

## Video

Per un video il pannello mostra, quando il file li contiene: marca e modello del telefono o della videocamera (e la versione di Android), data e ora della ripresa, durata, fotogrammi al secondo, dimensioni, formato video e audio, bit rate, e il posto in cui è stato girato. Alcune dashcam tengono la traccia GPS in un file accanto ai video: allora il pannello mostra dove parte il video, l'altitudine e la velocità media e massima durante il video. Vedi [Riprodurre i video](video.md).

## Mostrare e nascondere i riquadri

- Ogni riquadro si chiude con la sua ![Chiudi](icon://x) e si riapre da **Visualizza > Riquadri dei metadati**.
- ![Metadati: mostra o nascondi i riquadri](icon://info) nella barra strumenti chiude tutti i riquadri se ce n'è almeno uno aperto, altrimenti li riapre tutti.
- ![Comprimi il pannello](icon://chevron-down) a destra della barra del titolo del pannello, o un clic sulla barra stessa, riduce il pannello alla sola barra e lo riapre. Finché il pannello è compresso, i metadati non vengono letti.

EyeCanSee si ricorda quali riquadri sono aperti e se il pannello è compresso.

## Da sapere

- Il pannello segue il file selezionato nella griglia; nel visore non c'è. La barra di stato del visore mostra la data dello scatto se i metadati sono già stati letti nella griglia (vedi [Guardare le immagini](viewer.md)).
- Per leggere i metadati EyeCanSee rilegge il file, quindi su un telefono o su un server ci vuole un attimo. Una volta letti restano salvati con le miniature e la volta dopo compaiono subito, finché il file non cambia.
- Quando converti o salvi una foto, i suoi metadati restano nel file nuovo, se il formato li sa contenere. Vedi [Salvare in un altro formato](save-as.md).
