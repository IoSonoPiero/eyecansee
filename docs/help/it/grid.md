# La griglia delle miniature

La griglia mostra cosa c'è nella cartella aperta: prima la cella ![..](icon://color-up) **..**, che sale di un livello (non in cima a un disco, a un telefono o a un server), poi le sottocartelle, poi i file come miniature.

## Cosa si vede nella griglia

- ![Cartella](icon://color-folder) Le **cartelle**, in ordine di nome.
- Le **foto**, ognuna nella sua casella con il nome sotto.
- I **video**, con un fotogramma del video e il simbolo ![Video](icon://color-video) nell'angolo. Vedi [Riprodurre i video](video.md).
- I **file audio**, con la loro copertina e il simbolo ![Audio](icon://color-audio), oppure con una nota se non hanno copertina.
- I **modelli 3D** e i **disegni CAD**, con un'immagine del modello o del disegno. Vedi [Modelli 3D](model-3d.md) e [Disegni CAD](cad.md).

Tieni il mouse fermo su un file per vederne i dettagli, in due colonne: il nome intero con l'estensione (quello sotto la miniatura può essere tagliato), il peso, la data del file e, quando i suoi metadati sono già stati letti, le dimensioni in pixel. Su una cartella compare il suo nome.

Un cerchio che gira vuol dire che la miniatura è ancora in preparazione. Il simbolo ![Download](icon://color-download) vuol dire che per mostrare quel file bisogna prima scaricare un componente (vedi [Componenti aggiuntivi](components.md)). I file e le cartelle nascosti non si vedono. Una cartella vuota dice "Questa cartella non contiene immagini né sottocartelle".

I file che hai copiato o tagliato portano il simbolo ![Copiati](icon://color-copied) o ![Tagliati](icon://color-cut) finché non li incolli, e quelli tagliati sono sbiaditi (vedi [Copiare e spostare](copy-move.md)). Una cartella in cui hai eliminato qualcosa su un telefono o su un server ha in alto una striscia con **Apri il cestino** (vedi [Eliminare e cestino](delete.md)).

## Muoversi

- Un clic seleziona un elemento. Un doppio clic, o **Invio**, lo apre: una cartella mostra il suo contenuto, un file si apre nel [visore](viewer.md).
- **Backspace**, ![Cartella superiore](icon://arrow-up) **Cartella superiore** nella barra strumenti, o un doppio clic sulla cella **..** salgono di un livello. Viene selezionata la cartella da cui vieni.
- Le **frecce** spostano la selezione; **Inizio** e **Fine** vanno al primo e all'ultimo elemento. Dal primo elemento, la freccia a sinistra arriva alla cella **..**.
- **Tab** passa all'elemento successivo; dall'ultimo passa al pannello laterale. Vedi [Cartelle e pannello laterale](folders.md).

Entrando in una cartella non è selezionato niente: la prima freccia che premi seleziona il primo elemento.

## Ordine dei file

La tendina accanto alla barra degli indirizzi sceglie l'ordine: **Nome**, **Data di modifica**, **Dimensione** o **Tipo** (l'estensione del file). Il pulsante accanto inverte l'ordine: ![Ordine crescente](icon://arrow-up-narrow-wide) **Ordine crescente** o ![Ordine decrescente](icon://arrow-down-wide-narrow) **Ordine decrescente**. Le stesse scelte sono in **Visualizza > Ordina per**.

Le cartelle vengono sempre prima, in ordine di nome. Ordinando per data, i file senza data vanno in fondo. EyeCanSee si ricorda la scelta.

## Quali file mostrare

**Visualizza > Mostra** (anche nel menù del clic destro su uno spazio vuoto della griglia) sceglie cosa mostra la griglia:

- **Immagini, video e audio** (all'inizio): tutto.
- **Solo immagini**: foto, modelli 3D e disegni CAD, senza video e audio.
- **Solo video** o **Solo audio**.

La scelta vale per tutte le cartelle finché non la cambi. La barra di stato conta a parte immagini, video e file audio.

## Grandezza delle miniature

Il cursore in fondo a destra nella barra strumenti, accanto a ![Dimensione miniature](icon://image), ingrandisce o rimpicciolisce le miniature. Le celle si allargano su tutta la larghezza della griglia.

## Miniature

EyeCanSee prepara prima la miniatura del file selezionato, poi quelle visibili nella finestra, poi le altre della cartella. Le miniature restano salvate su disco, così la volta dopo la cartella si apre subito; una miniatura viene rifatta quando il suo file cambia.

Se una miniatura sembra sbagliata, fai clic destro sul file (o su più file selezionati) e scegli **Aggiorna miniature**. **Visualizza > Aggiorna tutte le miniature della cartella** (**Maiusc+F5**) le rifà tutte. Lo spazio che le miniature occupano su disco si regola nelle [Opzioni](options.md).

## Aggiornare

**F5**, oppure ![Aggiorna](icon://refresh-cw) **Aggiorna** nella barra strumenti, rilegge la cartella. Una cartella di questo computer si aggiorna da sola quando un altro programma vi aggiunge, elimina o rinomina dei file; su un server premi **F5**.

## Il menù del clic destro

- Su un **file**: le azioni che funzionano su di lui, come Apri, Carosello, Modifica immagine…, Converti…, Ridimensiona, Taglia, Copia, Rinomina… ed Elimina…. Con più file selezionati, il menù agisce su tutti (vedi [Selezionare i file](selection.md)).
- Su una **cartella**: Apri, Nuova cartella qui dentro…, Taglia, Copia, Incolla in questa cartella, Aggiungi questa cartella ai segnalibri (vedi [Segnalibri](bookmarks.md)), Trova duplicati…, Rinomina…, Elimina…, Elimina definitivamente….
- Su uno **spazio vuoto**: Incolla (quando hai copiato qualcosa), Seleziona tutto, Nuova cartella…, Cartella superiore, Aggiorna, Mostra e Trova duplicati….

Una voce segnata con **\*** funziona solo su una parte dei file scelti (per esempio Converti… sulle foto e sui video che ci sono in mezzo, Ridimensiona sulle foto); una riga in fondo al menù lo dice. Una voce che non funziona su nessuno resta al suo posto, grigia: tieni il mouse sopra per leggere il perché (per esempio "Nessuna immagine tra i file scelti").

## La barra di stato

In fondo alla finestra: quante cartelle, immagini, video e file audio ci sono nella cartella, e nome, peso e data del file selezionato (di una cartella, il nome). Con più elementi selezionati dice quanti sono e quanto pesano i loro file. Dopo Copia o Taglia ricorda di premere Ctrl+V.
