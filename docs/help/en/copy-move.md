# Copying and moving

Copies or moves files and folders, with everything inside them, to another folder: on the same disk, on another disk, on a phone or a server, also from one device to another. Moving a folder of the phone to the computer, or photos from the computer to a server, works the same way.

## Copy or move

1. Select the files or folders (see [Selecting files](selection.md)).
2. Press **Ctrl + C** to copy them, or **Ctrl + X** to cut them (to move them). You can also click ![Copy](icon://copy) or ![Cut](icon://scissors) in the toolbar, use the **Edit** menu or the right-click menu.
3. Open the folder where they should go: on this computer, a disk, a phone or a server (see [Folders and the side panel](folders.md)).
4. Press **Ctrl + V**, or click ![Paste](icon://clipboard-paste), or choose **Edit > Paste**. When it ends, what was pasted is selected in the folder (the last item, when there are several).

To paste straight into a folder without opening it, right-click it (in the grid or in the tree) and choose **Paste into this folder**. In the viewer, **Ctrl + C** and **Ctrl + X** take the image shown; **Ctrl + V** does nothing there, as **Edit > Paste**: go back to the grid to paste.

## What you see

- Copied items get a blue badge ![Copied](icon://color-copied) in the corner of their cell; cut items an orange one with scissors ![Cut](icon://color-cut), and they fade. The status bar says how many are waiting for **Ctrl + V**.
- During the copy a box at the bottom right shows the file being copied, the destination folder, how many files and how much data are done, the percentage, the speed of the last seconds and the time left. With many folders it first says how many items it found while looking inside them.
- The destination folder fills while the copy runs.
- When the copy ends the box goes. A copy or a move that took 10 seconds or more ends with a message (**Copy finished** or **Move finished**): how many files went where (a folder moved whole, on the same disk or inside a phone, counts as a folder), how much data in how long and how fast on average, and how many items were skipped because their name was already there. When the destination is not the folder open, **Open the folder** opens it. If something was left out, the message opens also after a shorter copy and, under those lines, says what: each thing under its heading, with how many they are and their names in order. A shorter copy with nothing left out opens no window. If a conversion ends close to it (see [Converting](convert.md)), the two say it in one report, **Jobs finished**; then a copy with nothing to say has its line there too (**Copy finished** with how many files, or **Copy cancelled** with how many were done before **Cancel**). A message that comes while another is open waits for it to close.
- If links to folders on a server were left out (see [Known limits](limits.md)), a message at the end says how many and names them (the first ten, then how many more; ![Copy the message](icon://copy) copies every name). A link left out is not copied; in a move it is not deleted either: only what was copied goes, so the link stays where it was, and so do the folders that hold it.
- A file that cannot be read (a server that stops sending it, a file that went away meanwhile) is left out and the copy goes on with the others. At the end the message names the files not read, each with why in a few words when it is known (![Copy the message](icon://copy) copies the whole reason); in a move they stay where they were, with their folders. If the source itself cannot be reached any more (a server gone, a phone unplugged), the copy stops there.

## When a name is already there

If an item with the same name is already in the destination, EyeCanSee asks what to do:

- **Replace**: the file there is replaced. For a folder the button is **Merge**: its contents go into the folder already there, and files with the same name are replaced.
- **Skip**: this item is not copied.
- **Keep both**: the new one becomes "name (2)".
- **Cancel**: nothing is pasted.

On Windows, on FAT and exFAT disks, on phones and on an SFTP server that ignores capitals, a name that differs only in capitals ("IMG_001.JPG" and "img_001.jpg") is the same name, and EyeCanSee asks; **Replace** keeps the name already there. Elsewhere the two are different names, but **Keep both** never gives a name that differs from one there only in capitals.

With several such items, tick **Do the same for all the other items with a name already there** to answer once.

On Linux, in a folder opened through the file manager's connection (gvfs, for example a phone or a network folder opened from the Files app), **Replace** and **Merge** are not offered, and the window says why: there replacing a file can lose the original. Choose **Keep both** or **Skip**; a move there keeps both too.

## Cancelling

**Cancel** in the box stops the copy. The files already copied stay; the file being copied is not left half-written. Towards a phone, Cancel waits for the end of the file being copied, because a phone that receives half a file stops answering.

## Things to know

- A photo's `.xmp` file (its "sidecar", see [Image information](metadata.md)) goes with it, also from one device to another: with **Keep both** it takes the photo's new name ("IMG_0001 (2).CR2" with "IMG_0001 (2).CR2.xmp"), with **Replace** it replaces the one there. A name taken only by an `.xmp` file in the destination counts as a name already there. Cancelled halfway, a photo is never left without its own `.xmp` file in the folder it came from.
- Each copy or cut is pasted once: to paste again, copy again.
- After you copy something else in another program, **Ctrl + V** and Paste no longer paste the items copied in EyeCanSee.
- Pasted into the folder they came from, copies become "name (2)"; a move does nothing. This holds also when that folder is reached through a link, or opened as another source: on this computer, or on the same SFTP server. Where EyeCanSee cannot tell (the same server opened through SFTP and through FTP), a move answered **Replace** or **Merge** that would replace an item with itself stops before writing or deleting anything, and says so.
- A copy that fails, for example because the disk is full, leaves nothing half-written: the file being copied is removed, and a file it was replacing stays as it was. For this, while a file is replaced, the disk needs room for both versions.
- On this computer a file that is replaced keeps its permissions, attributes and links; a read-only file is not replaced, and the message says so.
- Moving on the same disk only changes the place, at once. Between different devices, EyeCanSee copies everything first and deletes the originals only after the copy worked; the originals do not go to the trash. Only what was copied is deleted, file by file, then each folder left empty, and the box says how many files have gone; **Cancel** stops there, and what has not gone stays where it was. Something that arrives in a folder while it moves (a photo taken on the phone meanwhile) stays where it was, with its folder, and a message at the end names it (the first ten, then how many more; ![Copy the message](icon://copy) copies every name). A link to a folder inside a folder moved from this computer or from an SFTP server goes, after what it leads to is copied; the folder it leads to stays.
- Copies inside the same phone are made by the phone itself, except files of 32 MB or more: those go through this computer, with a real bar, and need free space on it (see [Android phones](phone.md)).
- A folder cannot be pasted inside itself, also through a link or from another source on this computer or on the same SFTP server. Nothing can be pasted into a trash or onto the list of a phone's storages, and items inside a trash cannot be copied.
- A folder moved takes its EyeCanSee trash along (see [Deleting and the trash](delete.md)); a copy leaves it out.
