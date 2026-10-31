# EyeCanSee, English (reference language).
# Every other locale must contain the same message ids: `cargo test` checks it.

# Name of the language written in the language itself, for the choice in Options.
language-name = English

counter = { $n } of { $total }
folder-empty = This folder contains no images or subfolders
cannot-load = Cannot open { $file }: { $error }
video-no-mpv = Videos need libmpv, which was not found. Install the package with libmpv: mpv-libs on Fedora, libmpv2 on Debian, Ubuntu and openSUSE (libmpv1 on Ubuntu 22.04), mpv on Arch.
video-component-title = Video engine
video-component-restart-needed = The video engine is downloaded. EyeCanSee must start again to use it.
video-component-restart = Restart EyeCanSee
video-component-ask = To play videos EyeCanSee needs the video engine (mpv), downloaded once: { $size }.
video-component-download = Download the video engine
video-component-button = Download the video engine ({ $size })
video-component-progress = Downloading: { $done } of { $total }
video-component-error = The download failed: { $error }
video-engine-old = The video engine on this computer is too old: { $found }, while EyeCanSee needs at least libmpv { $required }. EyeCanSee can download its own, once ({ $size }).
video-engine-system-only = The video engine (libmpv) is not on this computer, and the Options say to use the system's only. Install it (the mpv package), or change the choice under Tools > Options > Video.
video-play = Play (Space)
video-pause = Pause (Space)
video-mute = Sound off (M)
video-unmute = Sound on (M)
video-volume = Volume { $volume } (up and down arrows, mouse wheel on the video)
video-no-audio = This video has no sound
error-unsupported = Unsupported protocol: { $detail }
error-no-host = The address has no host: { $detail }
error-path = Cannot open { $detail }
error-source = Cannot connect to the source: { $detail }
error-detail = { $detail }
error-login = The server refused the login: wrong user name or password.
error-login-ssh = The server { $detail } refused the login: it accepted none of your SSH keys and no password was given, or the password is wrong.
error-key-failed = The saved key of { $detail } could not be opened: wrong passphrase, or the file is missing or is not a key. Fix it in File > Server manager.
error-permission = Permission denied: { $detail }
error-no-sftp = The server answers over SSH but has no SFTP service ({ $detail }). On a NAS, turn SFTP on in its settings.
error-host-unknown = { $detail } has never been used from this computer: confirm its key to connect.
error-cert-unknown = The certificate of { $detail } is not signed by a known authority: confirm it to connect.
error-host-changed = The key of { $detail } is not the one recorded in ~/.ssh/known_hosts, so EyeCanSee does not connect: someone may be pretending to be that server. If the server was reinstalled, remove its old line from known_hosts.
host-ask =
    { "**" }First connection to { $host }.**

    Its key fingerprint is `{ $fingerprint }`. If it is the server you expect, EyeCanSee remembers it in ~/.ssh/known_hosts, as ssh does, and connects.
cert-ask =
    { "**" }First connection to { $host } (FTPS).**

    Its certificate is not vouched for by a known authority (common with NAS boxes, which make their own). Its SHA-256 fingerprint is `{ $fingerprint }`. If it is the server you expect, EyeCanSee remembers it and connects.
host-ok = Connect
sftp-rsa-key = **{ $host }** opened with an RSA key.

    It works, but the RSA library EyeCanSee uses has a known flaw with no fix yet (RUSTSEC-2023-0071): in theory whoever controls the server, by timing a great many logins, could work out the key. Better switch to an ed25519 key (ssh-keygen -t ed25519).

    This warning appears once per server each time EyeCanSee starts.
error-not-found = Not found: { $detail }
error-device-busy = The device is in use by another application (for example the file manager). Close it, or disconnect and reconnect the device. ({ $detail })
error-device-kde = KDE is using the phone (its kiod service). Press F5 to open it in EyeCanSee anyway. ({ $detail })
error-no-device = The device was not found: if it is connected, unlock it and choose "File transfer" on it. EyeCanSee retries by itself every 2 seconds. ({ $detail })
error-not-ready = The phone does not answer: unlock it and choose "File transfer" in its USB notification. EyeCanSee retries by itself every 2 seconds. ({ $detail })
error-use-mount = Direct access to the device failed. ({ $detail })
error-no-storage = The device shows no storage: unlock it and choose "File transfer" on it. EyeCanSee retries by itself every 2 seconds. ({ $detail })

menu-file = File
menu-open-location = Open location…
menu-servers = Server manager…
menu-options = Options…
menu-quit = Quit
menu-edit = Edit
menu-tools = Tools
menu-select-all = Select all
key-select-all = Ctrl+A
menu-view = View
menu-refresh = Refresh
menu-back-to-grid = Back to the grid
menu-help = Help
menu-manual = Manual
menu-copy = Copy
menu-cut = Cut
menu-paste = Paste
menu-paste-into = Paste into this folder
key-copy = Ctrl+C
key-cut = Ctrl+X
key-paste = Ctrl+V
paste-taken = **"{ $name }"** is already in this folder.
paste-skip = Skip
paste-all = Do the same for all the other items with a name already there
transfer-copying = Copying
transfer-moving = Moving
transfer-looking = Looking inside the folders: { $n } items found
transfer-counts = { $n } of { $total } files, { $done } of { $size }
transfer-to = To { $dir }
transfer-left = { $percent }%, { $speed }/s, about { $time } left
error-paste-inside = A folder cannot go inside itself: { $detail }
error-name-too-long = The name is too long for the phone: { $detail } characters, while a phone takes at most 254 (an emoji counts as two). Shorten it and try again; nothing was copied.
error-copy-not-made = The phone did not make the copy of { $detail }: the original was left as it was.
error-cancelled = Cancelled.
menu-about = About EyeCanSee
menu-meta = Metadata boxes
about-tagline = Image and video viewer and manager for Linux and Windows
about-license = Free software under the GNU General Public License, version 3 or later.
about-license-link = License text
about-source = Source code on GitHub
about-third-party = Built with open source libraries: their licenses come with the Windows zip and the AppImage.
menu-open-selected = Open
menu-next = Next
menu-prev = Previous
menu-first = First
menu-last = Last
menu-parent = Parent folder

key-enter = Enter
key-backspace = Backspace
key-home = Home
key-end = End

tb-up = Parent folder (Backspace)
tb-address-hint = Folder, ftp://… or sftp://…
tb-thumb-size = Thumbnail size

status-folders = Folders: { $n }
status-images = Images: { $n }
status-videos = Videos: { $n }
status-audio = Audio: { $n }
status-deleting = Deleting { $n }…
status-file-removed = { $name } was removed from the device.
status-phone-waiting = The phone has not answered for { $s } s ({ $name }): waiting…
status-phone-waiting-open = The phone has not answered for { $s } s: waiting…
status-phone-stalled = Windows has not talked to the phone for { $s } s
phone-stalled =
    { "**" }Windows has lost the connection with the phone.**

    Unplug the cable and plug it in again (then File transfer), or wait a few minutes: it recovers by itself. The phone and its files are fine.
phone-stalled-upload =
    { "**" }Windows has lost the connection with the phone.**

    Unplug the cable and plug it in again (then File transfer), or wait a few minutes: it recovers by itself. The phone and its files are fine.

    { $name } will need copying again.
phone-stalled-why = Why does this happen?
phone-stalled-url = https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.en.md
status-copied = { $n } copied: Ctrl+V to paste
status-cut = { $n } cut: Ctrl+V to move
status-through-mount = Opened through the file manager: slower. Direct access failed: { $detail }
status-marked = Selected: { $n } ({ $size })
decimal-separator = .
date-format = %Y-%m-%d %H:%M
date-day-format = %Y-%m-%d

options-title = Options
opt-remember-location = Reopen the last folder at startup
opt-remember-scroll = Remember the scroll position of each folder
opt-remember-scroll-hint = When you go back to a folder you already viewed, the grid starts where you left it instead of from the first thumbnail. It lasts until EyeCanSee is closed.
opt-wrap-around = In the viewer, after the last image start again from the first
opt-wrap-around-hint = And from the first image, going back shows the last one. When off, the viewer stops at the first and the last image.
opt-wheel = Mouse wheel in the viewer:
opt-wheel-zoom = Zooms in and out
opt-wheel-next = Shows the next or previous image
opt-wheel-hint = Holding Ctrl, the wheel always zooms.
opt-max-zoom = Largest zoom in the viewer:
opt-max-zoom-hint = How far the wheel and the + key can enlarge an image, in percent of its pixels (100% = one image pixel per screen pixel).
opt-remember-location-hint = When EyeCanSee starts without a folder (for example from the applications menu), it opens the folder you were viewing when you closed it. Remote locations open without the password, which you must type again.
opt-cache-maintenance = Clean up the thumbnail cache
opt-cache-maintenance-hint = Once per session, for each opened location: removes thumbnails of deleted images, recreates those of changed images and keeps the cache within the limit. Folders that cannot be found or reached are skipped.
opt-cache-limit = Cache limit
opt-window = Window at startup
opt-window-normal = Normal size
opt-window-maximized = Maximized
opt-window-remember = Size it had when closed
opt-language = Language
opt-language-system = Same as the system
opt-theme = Colors
opt-theme-system = Same as the system
opt-theme-light = Light
opt-theme-dark = Dark

devices = Devices
servers = Servers
this-computer = This computer
device-mount-hint = Not mounted ({ $device }): click to mount and open it
error-mount = Cannot mount the partition: { $detail }
opt-purge = Empty the thumbnail cache…
opt-purge-confirm = Delete { $n } thumbnails ({ $mb } MB)?

    Your images are not touched: thumbnails are created again when needed.
opt-purge-yes = Empty
opt-purge-no = Cancel

meta-title = Metadata
meta-collapse = Collapse the panel
meta-expand = Open the panel
meta-file = File
meta-file-name = Name
meta-file-size = Size
meta-file-date = File date
meta-pixels = Pixels
meta-bytes = bytes
thousands-separator = ,
meta-camera = Camera
meta-shot = Shot
meta-image = Image
meta-gps = Location
meta-make = Make
meta-model = Model
meta-lens = Lens
meta-software = Software
meta-taken = Date taken
meta-exposure = Exposure
meta-aperture = Aperture
meta-iso = ISO
meta-focal = Focal length
meta-flash = Flash
meta-flash-yes = Fired
meta-flash-no = Not fired
meta-width = Width
meta-height = Height
meta-format = Format
meta-color = Colour
meta-color-rgb = RGB
meta-color-rgba = RGB with transparency
meta-color-gray = Greyscale
meta-color-graya = Greyscale with transparency
meta-depth = Depth
meta-duration = Duration
meta-fps = Frame rate
meta-video-codec = Video
meta-audio = Audio
meta-bitrate = Bit rate
meta-color-uncalibrated = Uncalibrated
meta-colorspace = Color space
meta-altitude = Altitude
meta-gps-date = GPS date
meta-speed-avg = Average speed
meta-speed-max = Top speed
meta-coords = Coordinates
meta-map = Open on the map
meta-close = Close (reopen it from View or with the ℹ button)
meta-none-selected = Select an image
meta-loading = Reading…
meta-no-exif = This file contains no metadata
meta-empty = No data
tb-meta = Metadata: show or hide the boxes

sort-by = Sort by
sort-name = Name
sort-date = Modification date
sort-size = Size
sort-type = Type
sort-desc = Descending order
show-menu = Show
show-all = Images, videos and audio
show-images = Images only
show-videos = Videos only
show-audio = Audio only
sort-asc = Ascending order

menu-delete = Delete…
key-delete = Del
menu-delete-permanent = Delete for good…
menu-rename = Rename…
rename-title = New name
rename-ok = Rename
rename-bad = Not a valid name
rename-taken = There is already an item with this name in the folder
error-exists = There is already an item with this name: { $detail }
error-rename-unsupported = This source cannot rename this item. ({ $detail })
delete-confirm-dir-trash = Move the folder **"{ $name }"**, with everything inside it, **to the trash**?
delete-confirm-dir-folder = Move the folder **"{ $name }"**, with everything inside it, to **this folder's Trash** (the hidden folder .EyeCanSee-trash)?

    You can **restore** it in EyeCanSee with **Open the trash**, at the top of the grid.
delete-confirm-dir-permanent = Delete the folder **"{ $name }"** and everything inside it **for good**?

    { "**" }It cannot be undone.**
key-shift-delete = Shift+Del
delete-confirm-trash = Move **"{ $name }"** ({ $size }) **to the trash**?
delete-confirm-folder = Move **"{ $name }"** ({ $size }) to **this folder's Trash** (the hidden folder .EyeCanSee-trash)?

    You can **restore** it in EyeCanSee with **Open the trash**, at the top of the grid.
delete-confirm-permanent = Delete **"{ $name }"** ({ $size }) **for good**?

    { "**" }It cannot be undone.**
delete-confirm-many-trash = Move **{ $n } items to the trash**?

    Images: { $size }.{ $folders }
delete-confirm-many-folder = Move **{ $n } items** to **the Trash of their folder** (the hidden folder .EyeCanSee-trash)?

    Images: { $size }.{ $folders }

    You can **restore** them in EyeCanSee with **Open the trash**, at the top of the grid.
delete-confirm-many-permanent = Delete **{ $n } items for good**?

    Images: { $size }.{ $folders } **It cannot be undone.**
delete-many-folders = Folders go with everything inside them.
delete-yes = Delete
delete-no = Cancel
error-no-trash-folder = This source cannot move files into the trash folder: choose "Delete for good" in the Options. ({ $detail })
opt-local-permanent = Delete files on this computer for good, without the trash (dangerous)
opt-local-permanent-hint = Deleted images cannot be recovered. Off: they go to the system trash.
opt-remote-delete = Deleting on phones and servers (they have no trash):
opt-remote-delete-folder = Move to the hidden .EyeCanSee-trash folder next to the image
opt-remote-delete-permanent = Delete for good
opt-tab-general = General
opt-tab-look = Look
opt-tab-browsing = Browsing
opt-tab-deleting = Deleting
opt-tab-thumbnails = Thumbnails
trash = Trash
menu-restore = Restore
menu-restore-all = Restore all
menu-empty-trash = Empty the trash…
trash-bar-has = This folder has a trash
trash-open = Open the trash
trash-bar-inside = Trash of the folder { $folder }
delete-confirm-empty-trash = **Empty this trash?**

    Everything inside it is deleted **for good**. **It cannot be undone.**
restore-taken = In **"{ $folder }"** there is already an item named **"{ $name }"**.
restore-replace = Replace
restore-keep-both = Keep both
restore-merge = Merge
delete-ntfs = This is an **NTFS disk**: the Files app (Nautilus) cannot restore from its trash. The file can still be restored from a terminal with "gio trash --restore" (see the manual, Known limits).

opt-check-updates = Check for updates at startup
opt-check-updates-hint = When EyeCanSee starts, it asks GitHub for the latest release. A newer version is downloaded in the background and checked; it is installed only when you click "Install … and restart". Windows (installer and zip) and the AppImage; on Fedora updates come with dnf.
update-install = Install { $version } and restart
update-failed = Cannot install the update: { $detail }
menu-check-updates = Check for updates
update-checking = Checking for updates…
update-latest = EyeCanSee { $version } is the latest version.
update-found = EyeCanSee { $version } is ready: click the red button at the top right to install it.
update-store = This copy comes from the Microsoft Store, which updates it.
update-unavailable = This copy is not updated by EyeCanSee: on Fedora updates come with dnf, and development builds are never updated.
update-check-failed = Cannot check for updates: { $detail }
menu-redo-thumbs = Redo thumbnails
menu-redo-all-thumbs = Redo all thumbnails in the folder
key-shift-f5 = Shift+F5
convert-title = Save in another format
convert-original = Original
convert-result = Converted
convert-working = Converting…
convert-no-preview = No preview for this format here: the file is saved anyway.
convert-calculating = Working out the size…
convert-error = Cannot convert: { $detail }
convert-actual = 100% (drag the images to move around)
convert-pixels = { $w } × { $h } pixels
convert-file-size = { $size } on disk
convert-ratio = { $percent }% of the original
convert-format = Format
convert-quality = Quality
convert-speed = Speed
convert-speed-hint = Slower makes a smaller file of the same quality.
convert-lossless = Lossless: the same pixels as the original.
convert-gif = At most 256 colours.
convert-keep-meta = Keep the metadata (EXIF, colour profile)
convert-no-meta = The original has no metadata.
convert-meta-unsupported = This format cannot keep metadata.
convert-name = File name
convert-save = Save
convert-exists = An item with this name already exists: it will be saved as { $name }.
convert-chroma = Colour detail
convert-chroma-full = Full (4:4:4)
convert-chroma-half = Half (4:2:2)
convert-chroma-quarter = Quarter (4:2:0)
convert-chroma-grey = Greys only
convert-progressive = Progressive (appears blurry first while loading)
convert-optimize = Optimised Huffman tables (smaller, same quality)
convert-arithmetic = Arithmetic coding (smaller, but many programs cannot open it)
convert-jpeg-lossless = Lossless JPEG (many programs cannot open it)
convert-png-level = Compression level (0 = none)
convert-png-filter = Filter
convert-filter-adaptive = Adaptive
convert-none = None
convert-lossy = Lossy
convert-lossless-mode = Lossless
convert-near-lossless = Near lossless (100 = off)
convert-sharp-yuv = Sharper colour edges (slower)
convert-effort = Effort
convert-alpha-quality = Transparency quality
convert-ten-bit = 10 bits per channel
convert-avif-rgb = RGB inside the file (for graphics, larger)
convert-compression = Compression
convert-deflate-fast = Deflate, fast
convert-deflate-best = Deflate, maximum
convert-predictor = Horizontal predictor (smaller with LZW and Deflate)
convert-gif-quality = Colour reduction quality
convert-tga-rle = Compressed (RLE; a few old programs read only uncompressed TGA)
convert-ico-reduced = An icon holds at most 256 × 256 pixels: the image is reduced.
convert-on-white = No transparency in this format: transparent parts become white.
menu-image = Image
menu-transform = Transform
tf-flip-h = Flip horizontally
tf-flip-v = Flip vertically
tf-rotate-cw = Rotate 90° clockwise
tf-rotate-ccw = Rotate 90° anticlockwise
tf-rotate-180 = Rotate 180°
menu-rotate-crop = Rotate and crop…
rc-title = Rotate and crop
rc-rotation = Rotation
rc-reset = Back to 0°
rc-straighten = Straighten
rc-straighten-hint = Draw a line along the horizon, or along something upright: the image turns to make it level.
rc-one-only = Rotate and crop works on one image at a time. To turn several images together use the items above or Convert and resize.
rc-rotation-hint = Shift + mouse wheel on the image: 0.1° at a time.
rc-crop = Crop
rc-free = Free proportions
rc-original = As the original
rc-custom = Custom
rc-swap = Swap width and height
rc-inside = Image only
rc-inside-hint = The largest crop with no empty corners.
rc-whole = Whole image
rc-whole-hint = All of the rotated image, with the empty corners filled.
rc-empty = Empty corners
rc-transparent = Transparent
rc-color = Colour
rc-jpeg-white = JPEG has no transparency: Save makes them white.
rc-size = Result: { $w } × { $h } pixels
rc-apply = Apply
rc-apply-hint = Puts the rotation and the crop into the image (Enter): then you can keep editing it. The file is saved at the end, with Save.
rc-pending = Rotation and crop not applied yet.
rc-unsaved = Changes applied, not saved yet.
rc-save = Save…
rc-save-hint = Writes the edited image over the original, after asking.
rc-save-as-hint = Saves the result as a new file, in the format you choose; the original stays as it is.
rc-close = Close
rc-discard = Don't save
rc-leave = The changes to **{ $name }** are not saved.

    { "**" }Save** writes them over the original, **Save as…** makes a new file, **Don't save** loses them.
rc-help = Drag a side or a corner to crop, the inside to move the crop, outside it to move the view. Wheel: zoom. Double click: everything in view.
key-rotate-cw = Ctrl+R
key-rotate-ccw = Ctrl+Shift+R
menu-save = Save
key-save = Ctrl+S
menu-save-copy = Save as…
key-save-copy = Ctrl+Shift+S
save-confirm-trash = Save the changes over **{ $name }**?

    The original goes **to the trash**.
save-confirm-folder = Save the changes over **{ $name }**?

    The original goes to **this folder's Trash**, where **Open the trash** restores it.
save-confirm-permanent = Save the changes over **{ $name }**?

    The original is deleted **for good**.
save-yes = Save
save-failed = Cannot save: { $detail }
status-modified = Changed, not saved
status-saving = Saving…
status-preparing = Preparing to save…
batch-turn-confirm = { $n ->
    [one] **{ $op }**: **1 image** will be overwritten.
   *[other] **{ $op }**: up to **{ $n } images** will be overwritten.
}
batch-originals-trash = The originals go **to the trash**.
batch-originals-folder = The originals go to **this folder's Trash**, where **Open the trash** restores them.
batch-originals-permanent = The originals are deleted **for good**.
batch-lossy = JPEG files are rotated without losing quality when their sides are multiples of 16 pixels; the others, and WebP, AVIF and GIF, are encoded again and may lose a little quality.
batch-skipped = { $n ->
    [one] **1 image** is left out: its format cannot be saved back (HEIC, RAW…).
   *[other] **{ $n } images** are left out: their format cannot be saved back (HEIC, RAW…).
}
batch-none = None of these **{ $n } images** can be saved back: their format cannot be written (HEIC, RAW…). Open them one at a time and use **Save as…**.
batch-save-as-hint = **Save as…** saves it already turned to a new file, in the format you choose. The original stays as it is.
status-saving-n = Saving: { $n }…
save-lossy = This image is **encoded again** and may lose a little quality: a JPEG is rotated without loss only when its sides are multiples of 16 pixels, lossy WebP, AVIF and GIF are always encoded again.
save-skipped = The changes cannot be saved over **{ $detail }**: it is animated, has several pages or carries more than one picture (the video of a motion photo, a depth map), and those parts would be lost.
save-animated = The changes cannot be saved over **{ $detail }**: it is animated, and saving over it would keep only the first frame.
save-pages = The changes cannot be saved over **{ $detail }**: it has several pages, and saving over it would keep only the first one.
save-extra = The changes cannot be saved over **{ $detail }**: it also carries the video of a motion photo or a depth map, which would be lost.
save-no-rename = Images **cannot be saved over themselves** here: this source cannot rename files.
batch-animated = **Animated** and **multi-page** images and **motion photos** are skipped. Ultra HDR photos are turned with their HDR map.
batch-skipped-names = Left out, because they are animated, have several pages or are motion photos:
batch-done = Images saved: **{ $n }**.
save-psd-flatten = **This PSD will be flattened**: layers, text, effects and masks are lost, only the image as it looks remains. To keep them use **Save as…** or edit it in Photoshop.
batch-psd-flatten = PSD files with several layers, text or effects are **flattened**: only the image as it looks remains.
save-format = The changes cannot be saved over **{ $name }**: EyeCanSee cannot write this format (HEIC, RAW…).
save-as-hint = **Save as…** saves it to a new file, in the format you choose. The original stays as it is.
notice-close = Close

## File > Open location
addr-title = Open location
addr-or = or fill in the fields:
addr-protocol = Protocol
addr-host = Server
addr-port = Port
addr-user = User
addr-user-hint = anonymous
addr-password = Password
addr-folder = Folder
addr-bad-port = The port must be a number from 1 to 65535.
addr-open = Open
addr-name = Name
addr-key = Key file
addr-key-hint = ~/.ssh/id_ed25519
addr-passphrase = Key passphrase
srv-disconnect = Close connection
srv-edit = Edit…
vault-unlock-title = Saved servers
vault-unlock-text = The saved servers are encrypted with the master password. Type it to use them until EyeCanSee is closed.
vault-unlock = Unlock
vault-unlock-row = Unlock the saved servers…
vault-wrong = Wrong password.
vault-unreadable = The file of the servers cannot be read: { $detail }
vault-on = 🔒 Master password on: the file of the servers is encrypted.
vault-on-hint = Every change is written at once, encrypted. EyeCanSee asks for the password once per session, at the first server.
vault-off = ⚠ No master password: passwords and passphrases are saved in plain text.
vault-off-hint = Anyone who can read your files can read them. A master password encrypts the whole file of the servers.
vault-set = Set a master password…
vault-change = Change the master password…
vault-remove = Remove the master password…
vault-remove-warning = Without the master password the file of the servers goes back to plain text: passwords and passphrases can be read by anyone who reads your files.
vault-remove-yes = Remove it and save in plain text
vault-new = New master password
vault-again = Type it again
vault-strength-0 = Very weak
vault-strength-1 = Weak
vault-strength-2 = Fair
vault-strength-3 = Strong
vault-strength-4 = Very strong
vault-too-weak = Not strong enough: use a longer password, for example several unrelated words.
vault-different = The two passwords are different.
vault-forget = If you forget it, the saved servers cannot be recovered: keep it in a password manager.
vault-save = Save the master password
vault-plain-warning =
    { "**" }Servers saved in plain text.**

    Without a master password, passwords and passphrases of the saved servers are written in plain text in the file of the servers: anyone who can read your files can read them. You can set one at the top of File > Server manager. This warning is not shown again.
srv-title = Server manager
srv-new = New server
srv-connect = Save and connect
srv-save = Save
srv-delete = Delete
srv-close = Close
srv-empty = No saved server: fill in the fields and press Save.
srv-file = Saved in { $path }, passwords in plain text.
srv-file-vault = Saved encrypted in { $path }.
srv-delete-ask = Delete "{ $name }" from the saved servers?
srv-save-failed = The servers could not be saved: { $detail }
kiod-ask = KDE is holding the phone through its kiod service, the one Dolphin uses for phones. To open it, EyeCanSee has to close that service. KDE restarts it by itself, but it **forgets the passwords of network folders** (Samba, SFTP…) opened in Dolphin, and will ask for them again next time.

    This **does not depend on EyeCanSee**: KDE keeps phones and those passwords in the same service, and lets nobody release only the phone.
kiod-ok = OK

# Find duplicates
menu-duplicates = Find duplicates…
dup-ask = Look for identical images in **{ $folder }**?

    Identical means the same file, byte for byte.
dup-sub = Also in subfolders
dup-phone = On a phone this takes time: every image that could be a copy is read in full.
dup-search = Search
dup-searching = Looking for identical images…
dup-progress = Examined { $n } files out of { $m }, { $size } read
dup-title = Duplicates in { $folder }
dup-order = Order
dup-by-date = Date, oldest first
dup-by-name = Shortest name
dup-by-path = Path
dup-keep-first = Keep the first
dup-keep-last = Keep the last
dup-keep-all = Keep all
dup-group = { $n } identical copies, { $size } each
dup-stays = Stays
dup-mark = Delete this copy
dup-last-one = At least one copy stays
dup-view = Open in the viewer
dup-summary = Groups: { $groups }. Copies to delete: { $n }, freeing { $size }.
dup-delete = Delete the marked copies…
dup-none = No duplicates in this folder.
menu-batch = Convert and resize…
menu-copy-meta = Copy with the metadata…
menu-strip-audio = Remove the audio…
menu-extract-audio = Extract the audio…
batch-title = Convert and resize
batch-title-strip = Remove the audio
batch-title-extract = Extract the audio
batch-strip-hint = The video and its other tracks (subtitles, chapters) are copied as they are, without the audio: no re-encoding, no loss, the metadata kept. The new file has the format of the original.
batch-extract-hint = Every audio track is copied as it is into a file of its own, in the format of its codec (AAC as .m4a, Opus as .opus, MP3 as .mp3, FLAC as .flac): no re-encoding, no loss, the metadata kept. For another format, use Convert and resize.
batch-audio-not-ready = FFmpeg is not ready yet.
batch-images = { $n ->
    [one] **1 file** in { $folder }.
   *[other] **{ $n } files** in { $folder }.
}
batch-same-format = Same as the original
batch-same-format-hint = Each image in its own format, written as Save writes it (JPEG at quality 95). Formats EyeCanSee cannot write (HEIC, RAW…) are left out.
batch-size = Size
batch-size-same = Unchanged
batch-size-percent = Percent of the original
batch-size-fit = Inside
batch-pixels = pixels (proportions kept)
batch-enlarge = Enlarge smaller files too
batch-filter = Filter
batch-filter-lanczos = Lanczos (sharpest)
batch-filter-catmull = Catmull-Rom (sharp)
batch-filter-gaussian = Gaussian (soft)
batch-filter-bilinear = Bilinear (fast)
batch-filter-nearest = Nearest pixel (pixel art)
batch-where = Where
batch-same-folder = Next to the originals
batch-subfolder = In the subfolder
batch-subfolder-name = converted
batch-keep = The originals are not touched. A name already taken gets " (2)".
batch-nothing = Choose another format, another size or a turn: as they are, the images would only lose quality.
batch-bad-folder = Write the name of the subfolder (without / or \).
batch-start = Convert
batch-working = Converting
batch-counts = { $n } of { $total } files
batch-stopping = Stopping…
batch-result = Files converted: **{ $n }**, in { $folder }.
batch-cancelled = Stopped after { $n } of { $total } files.
batch-left-out = Left out:
batch-skip-format = { $detail }: EyeCanSee cannot write its format; choose another format.
batch-skip-inside = { $detail }: animated or with several pages; the new file would have only the first one.
batch-failed = { $detail }
batch-turn = Turn and flip
batch-turn-none = No turn
batch-turn-cw = 90° clockwise
batch-turn-180 = 180°
batch-turn-ccw = 90° anticlockwise
batch-turn-order = First the flips, then the turn.
batch-format-options = { $format } options
batch-other = In another folder, also on another device
batch-browse = Browse…
batch-other-hint = A folder of this computer, or an address as in the address bar (mtp://…, sftp://…). It must exist.
pick-title = Choose the folder
pick-use = Use this folder
pick-home = Home folder
pick-up = Parent folder
pick-empty = No subfolders
batch-no-dest = Choose the other folder.
batch-not-folder = { $detail } is a file, not a folder.
menu-slideshow = Slideshow
opt-tab-slideshow = Slideshow
opt-tab-video = Video
opt-engine = Video engine (libmpv) and FFmpeg to use
opt-engine-hint = Videos are played by libmpv and converted by FFmpeg. Both can come from the system (on Linux the distribution's packages, better integrated with the graphics card; on Windows a copy next to the program) or be downloaded once by EyeCanSee. EyeCanSee needs at least libmpv 1.109 (mpv 0.33) and FFmpeg 4.3.
opt-engine-system-recent = The system's, if recent enough; otherwise the downloaded one
opt-engine-system = Always the system's, whatever its version
opt-engine-downloaded = Always the downloaded one
opt-engine-restart = A change applies the next time EyeCanSee starts.
opt-engine-mpv = Video engine now: { $what }
opt-engine-ffmpeg = FFmpeg now: { $what }
engine-system = the system's, { $version }
engine-downloaded = the downloaded one, { $version }
engine-missing = none found; it is downloaded when a video needs it
engine-too-old = the system's is too old ({ $found }, at least { $required } is needed); the download is offered when a video needs it
engine-system-only = none found, and only the system's is allowed
opt-show-secs = Each image stays for
opt-show-fade = Passage between images
opt-show-loop = Start again after the last image
fade-none = None (straight cut)
fade-cross = Crossfade
fade-black = Fade through black
fade-slide = Slide
fade-zoom = Zoom and fade
show-hint = Esc: end · Space: pause · ← →: previous and next image
show-paused = Paused (Space to go on)
menu-new-folder = New folder…
menu-new-folder-in = New folder inside…
key-new-folder = Ctrl+Shift+N
new-folder-title = New folder
new-folder-ok = Create
new-folder-name = New folder

batch-video = Videos
batch-video-container = Container
batch-video-codec = Codec
batch-video-copy = Same as the original (no re-encoding)
batch-video-encoder = Encoder
batch-video-hw-note = In green: the graphics card does the work, much faster and lighter on the computer.
batch-video-unavailable = { $codec } (no encoder on this computer)
batch-video-none = No encoder for this codec works on this computer.
batch-video-quality = Quality:
batch-video-high = High
batch-video-medium = Medium
batch-video-small = Small file
batch-video-audio = Keep the sound
batch-video-meta = Keep the metadata (date, place, camera)
batch-video-checking = Checking which encoders work on this computer…
batch-video-need-ffmpeg = Videos are converted by FFmpeg, which is not on this computer. It is downloaded once ({ $size }) into the app's data folder.
batch-video-download = Download FFmpeg
batch-video-download-failed = Download failed: { $detail }
batch-video-old-ffmpeg = The FFmpeg on this computer is too old: { $found }, while EyeCanSee needs at least { $required }. EyeCanSee can download its own, once ({ $size }), into the app's data folder.
batch-video-system-only = FFmpeg is not on this computer, and the Options say to use the system's only. Install it, or change the choice under Tools > Options > Video.
batch-video-not-ready = Choose how to convert the videos.
batch-skip-video = { $detail }: a video, and FFmpeg is missing.
batch-no-audio = { $detail }: the video has no audio.

video-codec-h264 = Plays everywhere: phones, TVs, browsers, old computers. Larger files than HEVC and AV1.
video-codec-hevc = About half the size of H.264 at the same quality; plays on recent phones, TVs and computers.
video-codec-av1 = The smallest files, free of royalties; plays on recent browsers, phones and TVs, not on older devices.
video-codec-vp9 = Free of royalties, for the web (YouTube uses it); smaller than H.264, a little larger than AV1.
video-codec-vp8 = Older web format: only for compatibility with old browsers.
video-codec-mpeg4 = Old format (DivX, Xvid era): for old players only. Large files.
video-codec-prores = For editing programs (Final Cut, DaVinci Resolve): best quality, very large files.
video-codec-copy = Video and sound copied as they are, in seconds and without losing quality; the metadata are written (a dashcam's position too). The turn and the size do not apply.
video-codec-gif = Animated image: no sound, 256 colours, very large files. Only for short clips.
video-enc-nvenc = NVIDIA graphics card: very fast and light on the computer; files a little larger than with the processor at the same quality.
video-enc-qsv = Intel graphics (Quick Sync): fast and light on the computer; files a little larger than with the processor.
video-enc-amf = AMD graphics card: fast and light on the computer; files a little larger than with the processor.
video-enc-mf = Windows' own encoder, on the graphics card when it can: fast; fewer options.
video-enc-vaapi = Graphics card through VA-API (Intel, AMD on Linux): fast and light on the computer; files a little larger.
video-enc-x264 = Processor (x264): the best H.264 for quality and size; slower than the graphics card and uses every core.
video-enc-x265 = Processor (x265): very good quality and small files; slow, uses every core.
video-enc-svtav1 = Processor (SVT-AV1): small files at good speed; the best AV1 for most uses.
video-enc-aom = Processor (libaom, the reference AV1): the smallest files, but very slow.
video-enc-rav1e = Processor (rav1e): AV1 written in Rust; slower than SVT-AV1.
video-enc-vpx = Processor (libvpx): good quality, slow.
video-enc-openh264 = Processor (OpenH264, Cisco): simple H.264, larger files than x264. Often the only H.264 of distributions without patented codecs.
video-enc-plain = Processor (FFmpeg's own encoder).
