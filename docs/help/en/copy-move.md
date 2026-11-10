# Copying and moving

Copies or moves files and folders, with everything inside them, to another folder: on the same disk, on another disk, on a phone or a server, also from one device to another. Moving a folder of the phone to the computer, or photos from the computer to a server, works the same way.

## Copy or move

1. Select the files or folders (see [Selecting files](selection.md)).
2. Press **Ctrl+C** to copy them, or **Ctrl+X** to cut them (to move them). You can also click ![Copy](icon://copy) or ![Cut](icon://scissors) in the toolbar, use the **Edit** menu or the right-click menu.
3. Open the folder where they should go: on this computer, a disk, a phone or a server (see [Folders and the side panel](folders.md)).
4. Press **Ctrl+V**, or click ![Paste](icon://clipboard-paste), or choose **Edit > Paste**.

To paste straight into a folder without opening it, right-click it (in the grid or in the tree) and choose **Paste into this folder**. In the viewer, **Ctrl+C** and **Ctrl+X** take the image shown.

## What you see

- Copied items get a blue badge ![Copied](icon://color-copied) in the corner of their cell; cut items an orange one with scissors ![Cut](icon://color-cut), and they fade. The status bar says how many are waiting for Ctrl+V.
- During the copy a box at the bottom right shows the file being copied, the destination folder, how many files and how much data are done, the percentage, the speed of the last seconds and the time left. With many folders it first says how many items it found while looking inside them.
- The destination folder fills while the copy runs.

## When a name is already there

If an item with the same name is already in the destination, EyeCanSee asks what to do:

- **Replace**: the file there is replaced. For a folder the button is **Merge**: its contents go into the folder already there, and files with the same name are replaced.
- **Skip**: this item is not copied.
- **Keep both**: the new one becomes "name (2)".
- **Cancel**: nothing is pasted.

With several such items, tick **Do the same for all the other items with a name already there** to answer once.

## Cancelling

**Cancel** in the box stops the copy. The files already copied stay; the file being copied is not left half-written. Towards a phone, Cancel waits for the end of the file being copied, because a phone that receives half a file stops answering.

## Things to know

- Each copy or cut is pasted once: to paste again, copy again.
- After you copy something else in another program, Ctrl+V and Paste no longer paste the items copied in EyeCanSee.
- Pasted into the folder they came from, copies become "name (2)"; a move does nothing.
- Moving on the same disk only changes the place, at once. Between different devices, EyeCanSee copies everything first and deletes the originals only after the copy worked; the originals do not go to the trash.
- Copies inside the same phone are made by the phone itself.
- A folder cannot be pasted inside itself. Nothing can be pasted into a trash or onto the list of a phone's storages, and items inside a trash cannot be copied.
- A folder moved takes its EyeCanSee trash along (see [Deleting and the trash](delete.md)); a copy leaves it out.
