# Esportare i modelli 3D

EyeCanSee sa salvare un modello 3D in un altro formato 3D, per esempio per aprire un modello di un programma CAD in un programma di stampa 3D, o per mandare un modello in un formato che leggono più programmi.

## Come esportare

1. Seleziona uno o più modelli 3D nella griglia (oppure aprine uno nel visore).
2. Fai clic destro e scegli **Esporta come modello 3D**, oppure usa **File > Esporta come modello 3D**.
3. Scegli il formato: **STL**, **OBJ**, **PLY** o **glTF (GLB)**.

Ogni file nuovo viene salvato accanto al suo originale, con lo stesso nome e la nuova estensione. Se quel nome è già preso si aggiunge " (2)", per esempio `staffa (2).stl`. La barra di stato dice il nome che ha preso; se qualcosa va storto, dice perché.

Se sono selezionati anche file di altro tipo, la voce è segnata con **\*** e lavora solo sui modelli.

## Cosa tiene ogni formato

EyeCanSee tiene tutto quello dell'originale che il nuovo formato sa contenere:

- **glTF (GLB)** tiene le immagini sulle superfici (texture), le normali e i materiali (quanto sono lisci o metallici, il rilievo, le ombre nelle pieghe, la luce propria).
- **OBJ** tiene le normali e i colori.
- **PLY** e **STL** tengono un colore per ogni triangolo.

Dove un formato non sa contenere un'immagine, i colori vengono presi dall'immagine, così il modello non esce grigio.

## Dov'è l'alto

Ogni file viene scritto con l'asse verso l'alto che si aspettano gli altri programmi: Z per STL e PLY, Y per OBJ e glTF. Così il modello si apre in piedi anche lì. Per questo gli assi nell'angolo del visore possono essere diversi da quelli dell'originale.

## Da sapere

- Per salvare invece un modello come immagine, vedi [Modelli 3D](model-3d.md) (Ctrl+S nel visore).
- I modelli che EyeCanSee apre sono elencati in [Formati supportati](formats.md).
