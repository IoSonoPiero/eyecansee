# Servers (FTP and SFTP)

EyeCanSee opens folders on a server, for example a NAS at home or a web space, through FTP, FTPS or SFTP. A server folder works like a folder of the computer: the tree, the grid and the viewer are the same, and you can copy and paste between the server, the computer, disks and phones.

## Opening a server

- Type its address in the address bar and press **Enter**, or choose **File > Open location…** and fill in protocol, server, port, user, password and folder (see [Address bar and Open location](address.md)).
- Or save it once in **File > Server manager…** and then open it with one click from the **Servers** section of the panel on the left (see [Managing servers](server-manager.md)).

Examples of addresses:

```text
ftp://user@example.com/Photos
ftps://user@example.com/Photos
sftp://user@example.com:2222/home/user/Photos
sftp://my-nas/
```

The last one is an alias of `~/.ssh/config`. Without a user, FTP logs in as `anonymous`. An address without a folder opens in the folder where the server starts (usually your home on it). Passwords are never shown in the address bar.

## The first connection

- **SFTP**: the first time, EyeCanSee shows the server's key fingerprint in a highlighted box. Compare it letter by letter with the one the server shows, then click **Connect**. Once trusted, it goes into `~/.ssh/known_hosts`, as ssh does. A server whose key has changed is refused: someone could be pretending to be that server.
- **FTPS**: the server's certificate is checked as a browser does. One that no known authority vouches for (common with NAS boxes, which make their own) is shown with its fingerprint the first time; once confirmed, EyeCanSee remembers it.

## Logging in with SFTP

- EyeCanSee reads `~/.ssh/config` as ssh does (aliases, HostName, User, Port, IdentityFile; not ProxyJump), on Linux and on Windows.
- It logs in with key files without a passphrase, or with the password. It never uses the system's SSH agent. For a key with a passphrase, save the server in the server manager.
- A password typed in the address is tried first.
- If the server refuses, **File > Open location…** opens with everything filled in, ready for the password.
- RSA keys work, but after a login with one EyeCanSee warns once per server: the library it uses has a known flaw with no fix yet. An ed25519 key is the better choice.

## Working on a server

- Browse, view, copy, move, rename, delete and convert as on the computer. Renames and moves, folders included, are done by the server itself, so they are quick.
- Deleting moves the file into a hidden `.EyeCanSee-trash` folder next to it, unless you chose otherwise in the options (see [Deleting and the trash](delete.md)).
- A changed image is first kept on this computer and then sent: if sending fails, the original on the server stays as it was, and a window offers **Try again**, **Save as…** and **Open the folder**.
- Videos play while they are read, a piece at a time, without being copied whole.
- Changes made on the server by other programs are not seen by themselves: press **F5**.

## Things to know

- A server folder that no longer exists (moved, deleted, or reopened at startup) gives way to the saved server's folder, or to the folder where the server starts.
- If an SFTP server stops answering, EyeCanSee leaves that connection, usually after about 20 seconds of silence, and repeats the request on a new one.
- EyeCanSee keeps one connection per SFTP server, and up to four per FTP server.
- To close the connection to a saved server, right-click it in the panel and choose **Close connection**.
