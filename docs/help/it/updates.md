# Aggiornamenti

EyeCanSee può tenersi aggiornato da solo. Come, dipende da come l'hai installato.

## Quali copie si aggiornano da sole

- **Installer di Windows, zip di Windows e AppImage**: EyeCanSee si aggiorna da solo, come spiegato sotto.
- **Microsoft Store**: lo aggiorna lo Store.
- **Flatpak**: si aggiorna con `flatpak update` o con il centro software del sistema (per esempio GNOME Software, Discover o il Gestore aggiornamenti di Linux Mint).
- **Pacchetto Fedora**: lo aggiorna `dnf` insieme al resto del sistema.

## Come funziona

1. All'avvio EyeCanSee chiede a GitHub, in background, l'ultima versione pubblicata.
2. Se è più nuova, EyeCanSee la scarica e la controlla due volte: con l'impronta SHA-256 che GitHub pubblica (uno scaricamento rovinato) e con la firma dello sviluppatore, fatta con una chiave che non lascia mai il suo computer (un file che non viene dallo sviluppatore). Un file senza una firma valida non viene mai installato.
3. Quando è pronta, in alto a destra nella barra dei menù compare un pulsante rosso **Installa la … e riavvia**, con il numero della versione nuova.
4. Fai clic: EyeCanSee si chiude, installa la versione nuova e si riapre. Finché non fai clic, non viene installato niente.

## Controllare a mano

**Aiuto > Verifica aggiornamenti** (![Verifica aggiornamenti](icon://download)) fa lo stesso controllo quando vuoi e dice com'è andata: che hai l'ultima versione, che una nuova è pronta (allora fai clic sul pulsante rosso), oppure perché questa copia non viene aggiornata da EyeCanSee (Store, Flatpak, pacchetto Fedora o una versione di sviluppo). Se il controllo non riesce, dice perché.

## Spegnere il controllo

Togli la spunta a **Controlla gli aggiornamenti all'avvio** in **Strumenti > Opzioni… > Generale**. EyeCanSee contatta allora GitHub solo quando scegli **Verifica aggiornamenti**. Non viene mai inviato niente su di te o sulle tue immagini: solo la richiesta dell'ultima versione.
