# Saved passwords

The passwords and key passphrases of the saved servers can be protected with a master password: the list of servers is then encrypted, and EyeCanSee asks for the master password once per session. Without it, they are saved in plain text.

## Without a master password

The yellow box at the top of **File > Server manager…**, with the warning triangle, says **No master password: passwords and passphrases are saved in plain text.** They are written in the file `servers.json` of the settings folder (on Linux, readable only by you). Anyone who can read your files can read these passwords too. The first time you save a server, in the server manager or with **Save and open** in **File > Open location…**, EyeCanSee says so in a warning, **Servers saved in plain text**.

## Setting a master password

1. Open **File > Server manager…**.
2. In the box at the top click **Set a master password…**.
3. Type it in **New master password** and again in **Type it again**.
4. The bar shows how strong it is, from red (**Very weak**) to green (**Very strong**). At least **Strong** is needed: a longer password, for example several unrelated words, is stronger.
5. Click **Save the master password**.

From then on the whole list of servers is encrypted, in the file `servers.vault`, and every change is written at once, encrypted. The box says **Master password on: the list of servers is encrypted.**

## Using the saved servers

EyeCanSee asks for the master password once per session, the first time you open a saved server or the server manager. While the list is locked, the **Servers** section of the panel shows **Unlock the saved servers…**. Type the password in the **Saved servers** window and click **Unlock** (or press **Enter**). It stays in memory until EyeCanSee is closed.

## Changing or removing it

- **Change the master password…** sets a new one, in the same way.
- **Remove the master password…** brings the file back to plain text, after a warning: **Remove it and save in plain text** confirms.

Every switch first writes the new file, reads it back and checks it, and only then replaces the old one: the file always stays whole.

## Things to know

- If you forget the master password, the saved servers cannot be recovered. Keep it in a password manager.
- The encryption uses Argon2id and ChaCha20-Poly1305.
- The settings folder is `~/.config/eyecansee` on Linux and `%APPDATA%\eyecansee` on Windows.
- Bookmarks and the last folder reopened at startup never keep a password.
