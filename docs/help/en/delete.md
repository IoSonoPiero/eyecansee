# Deleting and the trash

Deletes files and folders, and gets them back from the trash. On this computer files go to the system trash; phones and servers have no trash EyeCanSee can reach, so EyeCanSee keeps a trash of its own in every folder where you delete something.

## Deleting

1. Select the files or folders in the grid, or open the image in the viewer.
2. Press **Del**, or click ![Delete…](icon://trash) in the toolbar, or choose **File > Delete…**, or right-click and choose **Delete…**. A folder of the tree on the left is deleted from its right-click menu.
3. A window asks for confirmation, with the name and size (with several items: how many, and the size of their images). The button says what happens: **Move to trash**, or **Delete** when it is for good.
4. **Enter** confirms, **Esc** cancels.

In the viewer, the next image appears at once.

## Deleting a folder

A folder is deleted with everything inside it. When a folder is among the items, EyeCanSee reads its first level while the window opens, and the confirmation button waits a moment for it. If the folder holds files or folders the grid does not show (documents, hidden files and the like), which would be deleted too, the window says so with an example; if the folder cannot be read, it says that too. With a folder, **Enter** cancels, like Esc: click the button to delete.

## Where deleted files go

- **Files on this computer and external disks** go to the system trash, where they can be restored with your file manager.
- **On Windows, USB sticks, memory cards and network drives** have no Recycle Bin: there the file goes into the hidden `.EyeCanSee-trash` folder next to it, as on phones, and the window says so.
- **Phones and servers**: by default the file goes into a hidden `.EyeCanSee-trash` folder inside its own folder. In **Tools > Options… > Deleting** you can choose **Delete for good** instead (see [Options](options.md)).
- With the option **Delete files on this computer for good, without the trash (dangerous)**, files on this computer are deleted forever.

**Shift+Del**, or **File > Delete for good…**, deletes forever that one time, whatever the options say.

## Several items

With several items selected, **Del** deletes all of them after one confirmation. If one fails, the following ones are not deleted. While a deletion runs, the status bar says so with a spinner, and no other deletion can start: on a phone, a folder with hundreds of files takes several seconds.

## The trash of phones and servers

Every folder where you deleted something on a phone or a server (and on a USB stick on Windows) has its own trash.

- A strip at the top of the grid says **This folder has a trash**, with the **Open the trash** button.
- Inside, the strip says whose trash it is and has **Restore all** and **Empty the trash…**.
- Inside the trash you can look at the images, restore them (right click > **Restore**: they go back to the folder that holds the trash) or delete them for good. There **Del** always deletes forever, and there is no Rename.
- Right click on an empty spot inside the trash: **Restore all** and **Empty the trash…** (everything deleted forever, after a confirmation).
- A trash left empty goes away together with its strip.

![Deleting and the trash](../img/trash-strip.webp)

If the name of an item being restored is already taken, EyeCanSee asks: for a file **Replace** or **Keep both** (the restored one becomes for example `photo (2).jpg`); for a folder **Merge** or **Keep both**. **Cancel** also stops the rest of a Restore all.

## Android's own trash

On Android phones, the trash of a folder also shows the photos and videos the phone's gallery put in its own trash (hidden files that Android deletes by itself after 30 days, as Google Photos and Files by Google do). They appear with their real name and are restored like the others. **Empty the trash…** deletes them too. The Xiaomi Gallery keeps its own trash elsewhere, which EyeCanSee cannot see.

## Things to know

- The top folder of a disk, phone or server and the storages of a phone cannot be deleted.
- A folder moved to another device, or merged into a folder with the same name, takes its trash along; a copy leaves it out.
- On Linux, on an **NTFS** disk (for example an external disk also used with Windows), the Files app cannot restore from the trash. The files are there all the same: the confirmation reminds you, and [Known limits](limits.md) says how to restore them from a terminal.
- In a folder opened through the file manager's connection (on Linux), deleting follows the choice for phones and servers.
