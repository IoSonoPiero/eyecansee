# Trovare i doppioni

Trova i file identici in una cartella e nelle sue sottocartelle, così puoi eliminare le copie in più e liberare spazio. Identici vuol dire lo stesso file byte per byte, anche con un altro nome o in un'altra cartella. Le foto ridimensionate, convertite o salvate di nuovo non sono identiche, e non vengono trovate. Funziona su tutte le sorgenti: computer, dischi esterni, telefoni e server.

## Cercare

1. Apri la cartella in cui cercare, oppure fai clic destro su una cartella.
2. Fai clic su ![Trova duplicati…](icon://scan-search) nella barra strumenti, oppure scegli **Strumenti > Trova duplicati…** (**Ctrl+D**), oppure **Trova duplicati…** dal clic destro su una cartella o su uno spazio vuoto della griglia.
3. Scegli se cercare **Anche nelle sottocartelle** (acceso all'inizio).
4. Fai clic su **Cerca** (o premi **Invio**; **Esc** annulla).

Una finestra mostra quanti file sono stati esaminati su quelli trovati e quanti dati sono stati letti; **Annulla** ferma la ricerca. Vengono letti e confrontati solo i file con lo stesso peso, quindi sul computer è veloce. Su un telefono ogni file che potrebbe essere una copia viene letto per intero, e ci vuole tempo: la finestra lo dice.

Le cartelle del cestino e le cartelle nascoste restano fuori.

## I risultati

<!-- shot: duplicates | panoramica | The duplicates page: two or three groups of identical photos, each with its thumbnail and one line per copy, the first copy with Stays, the others ticked, faded and struck through; Order, Keep the first, Keep the last, Keep all at the top; the summary and Delete the marked copies at the bottom -->

I risultati riempiono la finestra. Ogni gruppo di file identici ha la sua miniatura e una riga per copia, con la data e il percorso sotto la cartella cercata (il percorso completo passandoci sopra).

- La casella di ogni riga vuol dire **Elimina questa copia**. Anche un clic sul testo della riga la spunta o la toglie.
- Le copie da eliminare sono più chiare e barrate; quella che resta dice **Resta**.
- Almeno una copia di ogni gruppo resta sempre: l'ultima casella non spuntata non si può spuntare.
- Fai clic su una miniatura per aprire l'immagine nel visore; **Esc** ti riporta ai risultati.

## Scegliere quale copia resta

All'inizio ogni gruppo tiene la prima copia e segna le altre. **Ordine** decide quale copia è la prima, per tutti i gruppi:

- **Data, dalla più vecchia** (quello predefinito): al minuto, come si vede; a parità di data, il percorso più corto.
- **Nome più corto**: le copie di solito hanno qualcosa in più nel nome, come "foto copia.jpg" o "foto (2).jpg", così l'originale resta anche quando le date sono uguali.
- **Percorso**: ordine alfabetico di cartella e nome.

**Tieni la prima**, **Tieni l'ultima** e **Tienile tutte** segnano tutti i gruppi in una volta; poi puoi cambiare le singole caselle.

## Eliminare

La riga in fondo dice quanti gruppi ci sono, quante copie verranno eliminate e quanto spazio si libera. **Elimina le copie segnate…** chiede conferma e le elimina come fa **Canc**: nel cestino, nel cestino della cartella o per sempre, come scelto nelle opzioni (vedi [Eliminare e cestino](delete.md)). **Chiudi** o **Esc** chiudono i risultati.

## Tasti

| Tasto | Azione |
|---|---|
| ↑, ↓ | Passano tra le copie di tutti i gruppi |
| Spazio | Spunta o toglie la copia |
| Esc | Chiude i risultati |

Invio non elimina: lo fa solo il pulsante **Elimina le copie segnate…**.
