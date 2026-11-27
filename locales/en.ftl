# EyeCanSee, English (reference language).
# Every other locale must contain the same message ids: `cargo test` checks it.

# Name of the language written in the language itself, for the choice in Options.
language-name = English

counter = { $n } of { $total }
folder-empty = This folder contains no images or subfolders
cannot-load =
    Cannot open { $file }
    { $error }
video-no-mpv = Videos need libmpv, which was not found. Install the package with libmpv: mpv-libs on Fedora, libmpv2 on Debian, Ubuntu and openSUSE (libmpv1 on Ubuntu 22.04), mpv on Arch.
video-component-title = Video engine
video-component-restart-needed = The video engine is downloaded, but it could not be started. Starting EyeCanSee again usually solves it.
video-component-restart = Restart EyeCanSee
video-component-ask = To play videos EyeCanSee needs the video engine (mpv), downloaded once: { $size }.
video-component-download = Download the video engine
video-component-button = Download the video engine ({ $size })
video-component-update = Update the video engine
video-component-update-button = Update the video engine ({ $size })
video-copying = Reading from the phone: { $done } of { $total }
video-component-progress = Downloading: { $done } of { $total }
video-component-error = The download failed: { $error }
video-engine-old = The video engine on this computer is too old: { $found }, while EyeCanSee needs at least libmpv { $required }. EyeCanSee can download its own, once ({ $size }).
video-engine-lacks = The video engine on this computer ({ $found }) cannot play { $missing }, the formats phones record in. EyeCanSee can download its own, which plays them, once ({ $size }).
video-engine-system-only = The video engine (libmpv) is not on this computer, and the Options say to use the system's only. Install it (the mpv package), or change the choice under Tools > Options > Video.
meta-video-engine = The details of videos and audio files are read by the video engine, which is not installed. Download it once ({ $size }) with the Download the video engine button in the menu bar, or open the file.
video-play = Play (Space)
video-pause = Pause (Space)
video-mute = Audio off (M)
video-unmute = Audio on (M)
video-volume = Volume { $volume } (up and down arrows, mouse wheel on the video)
video-no-audio = This video has no audio
video-not-video = The file is not a video, or it is damaged.
video-no-frame = No frame can be shown: the video is damaged.
video-no-sound = The audio cannot be played: no audio output could be opened.
video-frame-back-busy = Preparing the previous frame…
video-stalled =
    No data for 20 seconds: the phone or the server is not answering.
    The video will start as soon as it answers. Press Esc to go back to the thumbnails.
error-unsupported = Unsupported protocol: { $detail }
error-no-host = The address has no host: { $detail }
error-path = Cannot open { $detail }
error-source = Cannot connect to the source: { $detail }
error-detail = { $detail }
error-login = The server refused the login: wrong user name or password.
error-login-ssh = The server { $detail } refused the login.
    It accepted none of your SSH keys and no password was given, or the password is wrong.
error-key-failed = The saved key of { $detail } could not be opened: wrong passphrase, or the file is missing or is not a key.
    Fix it in File > Server manager.
error-key-failed-open = The saved key of { $detail } could not be opened: wrong passphrase, or the file is missing or is not a key.
    Fix it in the Server manager, open on this server.
error-permission = Permission denied: { $detail }
error-no-sftp = The server answers over SSH but has no SFTP service.
    On a NAS, turn SFTP on in its settings.
    ({ $detail })
error-host-unknown = { $detail } has never been used from this computer: confirm its key to connect.
error-cert-unknown = The certificate of { $detail } is not signed by a known authority: confirm it to connect.
error-host-changed = The key of { $detail } has changed: it is not the one recorded in ~/.ssh/known_hosts, so EyeCanSee does not connect.
    Someone may be pretending to be that server.
    If the server was reinstalled, remove its old line from known_hosts.
error-server-silent = The server { $detail } does not answer: check that it is on and reachable, then try again.
error-server-cut = The server stopped sending "{ $name }" before its end.
error-server-cut-brief = the server stopped sending it before its end
host-ask =
    { "**" }First connection to { $host }.**

    Its key fingerprint, to compare with the one the server shows (in its settings, or with ssh-keygen -lf on it):

    `{ $fingerprint }`

    If it is the server you expect, EyeCanSee remembers it in ~/.ssh/known_hosts, as ssh does, and connects.
cert-ask =
    { "**" }First connection to { $host } (FTPS).**

    Its certificate is not vouched for by a known authority (common with NAS boxes, which make their own).
    Its SHA-256 fingerprint, to compare with the one the server shows in its settings:

    `{ $fingerprint }`

    If it is the server you expect, EyeCanSee remembers it and connects.
host-ok = Connect
sftp-rsa-key = **{ $host }** opened with an RSA key.

    It works, but the RSA library EyeCanSee uses has a known flaw with no fix yet (RUSTSEC-2023-0071): in theory whoever controls the server, by timing a great many logins, could work out the key.
    Better switch to an ed25519 key (ssh-keygen -t ed25519).

    This warning appears once per server each time EyeCanSee starts.
error-trash-failed = The original could not be moved to the trash: the original is unchanged.
    ({ $detail })
error-not-found = Not found: { $detail }
error-bad-name = The name of "{ $detail }" cannot be read (it is not valid UTF-8).
    Rename it in the file manager to open it.
error-image-too-large = The picture is too large: { $detail } pixels, while EyeCanSee opens pictures of up to 512 megapixels.
view-too-large-title = Picture too large for the memory
view-too-large-ask =
    The picture needs about { $need } of memory, more than half of the { $ram } this computer has.

    If you open it anyway, the computer may become very slow or EyeCanSee may close.
view-open-anyway = Open anyway
error-device-busy =
    EyeCanSee cannot open the device: another program is using it.
    Close the file manager and the other programs using it, then press F5.
    If that is not enough, unplug the device, plug it back in (on a phone choose File transfer) and open it in EyeCanSee first.
error-device-kde-back = KDE took the phone back: close Dolphin and press Release the phone again.
    ({ $detail })
error-device-kde = KDE is using the phone (its kiod service).
    Press F5 to open it in EyeCanSee anyway.
    ({ $detail })
error-no-device = The device was not found: if it is connected, unlock it and choose "File transfer" on it.
    EyeCanSee retries by itself every 2 seconds.
    ({ $detail })
error-phone-disconnected = The phone was disconnected.
    ({ $detail })
error-phone-busy =
    The phone is busy with another program, for example a photo import.
    Wait for it to finish or close it, then press F5.
error-phone-timeout = The phone stopped answering: it may have been disconnected or locked.
    ({ $detail })
error-phone-name-taken = The phone refused the copy: the folder already has a file with that name.
    Nothing was changed.
    ({ $detail })
error-not-ready = The phone does not answer: unlock it and choose "File transfer" in its USB notification.
    EyeCanSee retries by itself every 2 seconds.
    ({ $detail })
error-use-mount = Direct access to the device failed.
    ({ $detail })
error-no-storage = The device shows no storage: unlock it and choose "File transfer" on it.
    EyeCanSee retries by itself every 2 seconds.
    ({ $detail })

menu-file = File
menu-open-location = Open location…
menu-servers = Server manager…
menu-options = Options…
menu-quit = Quit
menu-edit = Edit
menu-tools = Tools
menu-select-all = Select all
key-select-all = Ctrl + A
menu-view = View
menu-refresh = Refresh
menu-back-to-grid = Back to the grid
menu-full-screen = Full screen
menu-leave-full-screen = Leave full screen
menu-help = Help
menu-manual = Guide
help-back = Back
help-forward = Forward
help-contents = Contents
help-search = Search the guide
help-clear-search = Clear the search
help-nothing-found = Nothing found.
help-previous-match = Previous match
help-next-match = Next match
menu-copy = Copy
menu-cut = Cut
menu-paste = Paste
menu-paste-into = Paste into this folder
key-copy = Ctrl + C
key-cut = Ctrl + X
key-paste = Ctrl + V
key-undo = Ctrl + Z
key-redo = Ctrl + Y
key-pick = I
key-pick-screen = I, Shift + I
paste-taken = **"{ $name }"** is already in this folder.
paste-skip = Skip
paste-all = Do the same for all the other items with a name already there
paste-gvfs = Here a file **cannot be replaced**, nor a folder merged: this folder was opened through the file manager (gvfs), where replacing a file can lose the original.
    Choose **Keep both** or **Skip**.
transfer-copying = Copying
transfer-moving = Moving
transfer-looking = Looking inside the folders: { $n } items found
transfer-counts = { $n } of { $total } files, { $done } of { $size }
transfer-to = To { $dir }
transfer-reading-phone = Reading { $name } from the phone
transfer-writing-phone = Writing { $name } to { $storage }
transfer-stop-at-end = Stopping after { $name }: the phone must receive it whole
transfer-why-pc = Why through the computer?
# The title of the section of the guide's page phone.md that `transfer-why-pc` opens: the same words.
transfer-why-pc-section = Big files: through the computer
# The copy window while the phone copies by itself inside one storage (#354).
phone-copy-title = The phone is copying
phone-copy-counting = Counting the files…
phone-copy-nth = File { $n } of { $total }
phone-copy-for = { $size }, copying for { $time }
phone-copy-total = { $size } in all
phone-copy-done = { $done } of { $size } copied so far
phone-copy-speed = { $speed }/s on the big files copied so far, about { $time } left
phone-copy-notice = The phone copies by itself and answers nothing meanwhile: that is why EyeCanSee waits.
phone-copy-keep = Do not unplug the phone.
phone-copy-long = This file has taken more than 3 minutes. The phone goes on until it is done: all you can do is wait.
phone-copy-cancel-tip = The phone finishes the file it is copying, then EyeCanSee removes that copy. The files already copied stay.
phone-copy-cancelled = Cancelled
phone-copy-leave = Stop waiting
phone-copy-leave-tip = EyeCanSee stops waiting for the phone. The phone may finish this file by itself: the copy then stays on the phone.
phone-copy-leaving = Stopping the wait…
phone-copy-through-pc = This file is big and goes through this computer: the bar is real. Cancel stops the reading at once; the writing stops when the file is whole.
phone-copy-leaving-note = EyeCanSee stops waiting. If the phone finishes this file by itself, the copy stays on the phone.
phone-copy-cancelled-note = The phone finishes this file, then EyeCanSee removes the copy just made.
phone-copy-others = { $n ->
    [one] The other file is not copied.
   *[other] The other { $n } files are not copied.
}
phone-copy-why = Why do I have to wait?
# The title of the section of the guide's page phone.md that `phone-copy-why` opens: the same words.
phone-copy-why-section = Copies inside one storage: the phone works alone
transfer-left = { $percent }%, { $speed }/s, about { $time } left
transfer-removing = Removing the originals
transfer-removed = { $n } of { $total } files removed from where they were
transfer-summary = { $size } in { $time }, { $speed }/s on average.
transfer-copied = Copy finished
transfer-moved = Move finished
jobs-report-title = Jobs finished
transfer-copy-cancelled = Copy cancelled
transfer-move-cancelled = Move cancelled
transfer-files-copied = { $n ->
    [one] **1 file** copied to { $dir }.
   *[other] **{ $n } files** copied to { $dir }.
}
transfer-files-moved = { $n ->
    [one] **1 file** moved to { $dir }.
   *[other] **{ $n } files** moved to { $dir }.
}
transfer-folders-moved = { $n ->
    [one] **1 folder** moved to { $dir }.
   *[other] **{ $n } folders** moved to { $dir }.
}
transfer-files-folders-moved = { $files ->
    [one] **1 file**
   *[other] **{ $files } files**
} and { $folders ->
    [one] **1 folder**
   *[other] **{ $folders } folders**
} moved to { $dir }.
transfer-skipped = { $n ->
    [one] **1 item** skipped: its name was already there.
   *[other] **{ $n } items** skipped: their names were already there.
}
transfer-done-before-cancel = **{ $n } of { $total } files** copied to { $dir } before Cancel.
transfer-moved-before-cancel = **{ $n } of { $total } files** moved to { $dir } before Cancel.
transfer-done-before-error = **{ $n } of { $total } files** copied to { $dir } before it stopped.
transfer-moved-before-error = **{ $n } of { $total } files** moved to { $dir } before it stopped.
transfer-copy-stopped = Copy stopped
transfer-move-stopped = Move stopped
transfer-links-heading = { $n ->
    [one] 1 link to a folder skipped:
   *[other] { $n } links to folders skipped:
}
transfer-links-why = A link to a folder on an FTP server, or one that leads back into the folders copied, is not copied.
transfer-links-why-moved = A link to a folder on an FTP server, or one that leads back into the folders copied, stays where it was, with the folder that holds it.
notice-guide = More in the guide (F1).
error-move-same-item = What it would replace is this same item, reached in another way: { $detail }
    The move stopped, and the item stayed as it was.
error-move-not-told = It cannot be told whether what it would replace is this same item: { $detail }
    The move stopped before deleting it: it was copied, and it also stayed where it was.
error-paste-inside = A folder cannot go inside itself: { $detail }
error-name-too-long = The name is too long for the phone: { $detail } characters, while a phone takes at most 254 (an emoji counts as two).
    Shorten it and try again; nothing was copied.
name-refused-heading = { $n ->
    [one] 1 file not copied for its name:
   *[other] { $n } files not copied for their names:
}
name-refused-why = { $n ->
    [one] Its name is not valid on this computer: it stayed where it was.
   *[other] Their names are not valid on this computer: they stayed where they were.
}
move-left-heading = { $n ->
    [one] 1 item not moved:
   *[other] { $n } items not moved:
}
move-left-why = { $n ->
    [one] It arrived during the move, or it is not shown here: it stayed where it was, with its folder.
   *[other] They arrived during the move, or they are not shown here: they stayed where they were, with their folders.
}
copy-failed-heading = { $n ->
    [one] 1 file not read:
   *[other] { $n } files not read:
}
copy-failed-why = { $n ->
    [one] It was not copied: it stayed where it was.
   *[other] They were not copied: they stayed where they were.
}
list-more = { $n ->
    [one] and 1 more
   *[other] and { $n } more
}
# The same, in a list of pictures ("immagini" is feminine in Italian).
list-more-images = { $n ->
    [one] and 1 more
   *[other] and { $n } more
}
error-phone-left = EyeCanSee stopped waiting for the phone, which was still working on { $detail }. If the phone finishes by itself, what it made stays on the phone. It answers nothing until it is done.
error-phone-copy-unseen = The phone copied { $detail }, but it took longer than Windows waits, and Windows does not show the copy. Unplug the phone and plug it in again, then check whether the copy is there: if it is missing, copy it again.
error-phone-copy-lost = The phone took longer to copy { $detail } than Windows waits, and Windows does not show what the phone does. Wait a few minutes, unplug the phone and plug it in again, then check whether the copy is there: if it is missing, copy it again.
error-copy-not-made = The phone did not make the copy of { $detail }: the original was left as it was.
error-copy-not-whole = The copy of { $detail } did not arrive whole: the original was left as it was.
error-pc-space = Not enough free space on this computer for { $detail }: a big file inside the phone goes through it. Free some space and try again.
error-phone-space = Not enough free space on the phone for { $detail }.
error-cancelled = Cancelled.
error-too-deep = Too many folders one inside another, so the copy stopped: { $detail }
menu-about = About EyeCanSee
menu-meta = Metadata boxes
about-tagline = Viewer, manager and editor for photos, videos, 3D models and CAD drawings, for Linux and Windows
about-license = Free software under the GNU General Public License, version 3 or later.
about-license-link = License text
about-source = Source code on GitHub
about-third-party = Built with open source libraries: their licenses come with every package of EyeCanSee.
about-gpu = Graphics card in use:
about-gpu-copy = Copy (to report a problem)
gpu-discrete = dedicated card
gpu-integrated = integrated card
gpu-virtual = virtual card
gpu-cpu = processor (no graphics card)
gpu-other = other card
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
tb-address-copy = Copy the address
tb-address-paste = Paste an address (then Enter)
tb-thumb-size = Thumbnail size

status-folders = Folders: { $n }
status-images = Images: { $n }
status-videos = Videos: { $n }
status-audio = Audio: { $n }
status-deleting = Deleting { $n }…
status-file-removed = { $name } was removed from the device.
delete-phone-busy = The phone is { $what ->
        [move] moving
       *[other] copying
    } { $name } ({ $s } s): **{ $button }** turns on when it is done.
quit-phone-title = The phone is still working
quit-phone-ask = The phone is { $what ->
        [move] moving
       *[other] copying
    } { $name } by itself and cannot be stopped. Close EyeCanSee now?

    The phone may finish this file by itself: what it made then stays on the phone.
quit-phone-yes = Close EyeCanSee
quit-phone-no = Keep waiting
status-phone-working = The phone is { $what ->
        [move] moving
       *[other] copying
    } { $name } ({ $s } s)…
status-phone-deaf = Please wait: the phone is writing { $name }. Its folders open when it is done.
status-phone-waiting = The phone has not answered for { $s } s ({ $name }): waiting…
status-phone-waiting-open = The phone has not answered for { $s } s: waiting…
status-phone-stalled = Windows has not talked to the phone for { $s } s: unplug the cable and plug it in again, or wait
phone-stalled =
    { "**" }Windows has lost the connection with the phone.**

    Unplug the cable and plug it in again (then File transfer), or wait a few minutes: it recovers by itself.
    The phone and its files are fine.
phone-stalled-upload =
    { "**" }Windows has lost the connection with the phone.**

    Unplug the cable and plug it in again (then File transfer), or wait a few minutes: it recovers by itself.
    The phone and its files are fine.

    { $name } will need copying again.
phone-stalled-why = Why does this happen?
phone-stalled-url = https://github.com/IoSonoPiero/eyecansee/blob/main/docs/phone-stall.en.md
status-copied = { $n } copied: Ctrl + V to paste
device-unplugged = Device unplugged: { $name }. The home folder is open now.
status-cut = { $n } cut: Ctrl + V to move
status-temp-copy = This file is a temporary copy that Windows made (for example of a photo on a phone), not the original: it cannot be saved over. Use Save as… to keep the changes.
status-taken-from-gvfs = The phone was held by the system (it takes it by itself when the phone is plugged in, or when it is opened in the file manager): EyeCanSee now uses it directly, which is faster. To see it in the file manager again, close EyeCanSee and click the phone there.
status-through-mount = Opened through the file manager: slower. Direct access failed: { $detail }
status-marked = Selected: { $n } ({ $size })
decimal-separator = .
date-format = %Y-%m-%d %H:%M
date-day-format = %Y-%m-%d

options-title = Options
opt-remember-location = Reopen the last folder at startup
opt-remember-scroll = Remember the scroll position of each folder
opt-remember-scroll-hint = When you go back to a folder you already viewed, the grid starts where you left it instead of from the first thumbnail. It lasts until EyeCanSee is closed.
opt-wrap-around = After the last image start again from the first
opt-wrap-around-hint = And from the first image, going back shows the last one. When off, the viewer stops at the first and the last image.
opt-close-to-grid = The window's ✕ goes back to the grid
opt-close-to-grid-hint = On: the ✕ does as Esc, one step at a time: the editor, then the viewer, then EyeCanSee. Off: from the viewer both the ✕ and Esc close EyeCanSee (Esc leaves full screen first), and the ✕ closes it from the editor too. Changes not saved are asked about first.
opt-wheel = Mouse wheel in the viewer:
opt-wheel-zoom = Zooms in and out
opt-wheel-next = Shows the next or previous image
opt-wheel-hint = Holding Ctrl, the wheel always zooms.
opt-double = Double click in the viewer:
opt-double-grid = Goes back to the grid
opt-double-full = Goes to full screen
opt-double-hint = At full screen another double click goes back to the window; Esc does the same.
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
opt-smooth = Smooth pictures shown below 100%
opt-smooth-hint = A picture shown smaller than its own pixels is filtered: edges stay smooth, and fine details (grass, hair, thin lines) do not turn into grain or stripes. In the viewer, the slideshow, the editor and the thumbnails. Off: shown as up to version 0.76, a little sharper but with jagged edges and grain. Above 100% nothing changes.
opt-smooth-halves = Fast: from the halved copies
opt-smooth-halves-hint = The filter starts from a copy of the picture already reduced to a half, a quarter…: light for the graphics card. On very fine patterns some faint false patterns may stay.
opt-smooth-whole = Exact: from the whole picture
opt-smooth-whole-hint = The filter always starts from the whole picture: almost no false patterns, but more work for the graphics card, the more so the smaller the picture is shown.
opt-smooth-mitchell = Soft: Mitchell from the whole picture
opt-smooth-mitchell-hint = A softer filter than the other two (Lanczos), from the whole picture: less halo around edges, a little less sharp.
opt-graphics = Graphics: draw with
opt-graphics-hint = What EyeCanSee draws on the graphics card with. DirectX 12: the screen does not flash when entering full screen (F11, the slideshow). Vulkan: as up to version 0.76; choose it if pictures or videos are not drawn well with DirectX 12.
opt-graphics-directx12 = DirectX 12
opt-graphics-vulkan = Vulkan
vulkan-failed-title = Vulkan did not start
vulkan-failed-text = The last start with Vulkan did not get as far as showing the window: EyeCanSee now draws with DirectX 12.
    You can change it in Options > Look.
graphics-broken-title = The graphics card's driver does not work
graphics-broken-text = At the last start the graphics card's driver closed EyeCanSee, and the card cannot draw with DirectX 12.
    EyeCanSee now draws without the graphics card, more slowly.
    Update the driver from the website of the computer's maker or of the graphics card's maker.
    Then, to try the card again, choose Vulkan in Options > Look.
graphics-no-window = EyeCanSee could not open its window: the graphics card's driver does not work.
    Update it from the website of the computer's maker or of the graphics card's maker, then start EyeCanSee again.

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
meta-drawing = Drawing
cad-props-none = No properties or title block in this drawing
cad-prop-title = Title
cad-prop-subject = Subject
cad-prop-author = Author
cad-prop-keywords = Keywords
cad-prop-comments = Comments
cad-prop-revision = Revision number
cad-prop-saved-by = Last saved by
cad-title-block = Title block
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
meta-info = Description
meta-xmp-title = Title
meta-xmp-caption = Caption
meta-xmp-keywords = Keywords
meta-xmp-rating = Rating
meta-xmp-rejected = Rejected
meta-xmp-label = Label
meta-xmp-creator = Author
meta-xmp-rights = Copyright
meta-xmp-sidecar = XMP file
meta-label-red = Red
meta-label-yellow = Yellow
meta-label-green = Green
meta-label-blue = Blue
meta-label-purple = Purple
meta-map = Open on the map
meta-close = Close (reopen it from View or with the ℹ button)
meta-none-selected = Select an image
meta-loading = Reading…
loading-file = Loading the file…
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
error-save-temp-copy = Not saved over itself: this file is a temporary copy made by Windows, not the original (for example the photo on the phone).
    ({ $detail })
error-save-gvfs = Not saved over itself: this folder was opened through the file manager (gvfs), where replacing a file can lose the original.
    ({ $detail })
error-rename-unsupported = This source cannot rename this item.
    ({ $detail })
delete-confirm-dir-trash = Move the folder **"{ $name }"**, with everything inside it, **to the trash**?
delete-confirm-dir-folder = Move the folder **"{ $name }"**, with everything inside it, to **this folder's Trash** (the hidden folder .EyeCanSee-trash)?

    You can **restore** it in EyeCanSee with **Open the trash**, at the top of the grid.
delete-confirm-dir-permanent = Delete the folder **"{ $name }"** and everything inside it **for good**?

    { "**" }It cannot be undone.**
key-shift-delete = Shift + Del
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

    Images: { $size }.{ $folders }
    { "**" }It cannot be undone.**
delete-many-folders = Folders go with everything inside them.
delete-unseen-found = { $n ->
    [one] This folder contains
   *[other] The folder **{ $folder }** contains
} files or folders that EyeCanSee does not show, for example **{ $name }**.
    { $how ->
        [permanent] They will be deleted too
       *[trash] They will be moved to the trash too
    }: check it with your file manager if you are not sure.
delete-unseen-failed = The folder could not be checked: it may contain files or folders that EyeCanSee does not show.
delete-move-to-trash = Move to trash
delete-yes = Delete
delete-no = Cancel
error-no-trash-folder = This source cannot move files into the trash folder: choose "Delete for good" in the Options.
    ({ $detail })
opt-local-permanent = Delete files on this computer for good, without the trash (dangerous)
opt-local-permanent-hint = Deleted images cannot be recovered. Off: they go to the system trash.
opt-remote-delete = Deleting on phones and servers (they have no trash):
opt-remote-delete-folder = Move to the hidden .EyeCanSee-trash folder next to the image
opt-remote-delete-permanent = Delete for good
opt-tab-general = General
opt-tab-look = Look
opt-tab-viewer = Viewer
opt-tab-deleting = Deleting
opt-tab-thumbnails = Thumbnails
opt-tab-addons = Add-ons
opt-tab-diagnostics = Diagnostics
opt-log = Write a diagnostic log, from the next start
opt-log-hint = Every start of EyeCanSee writes what it does to its own file in the logs folder: attach it when you report a problem. At 5 MB a new file is started; the 10 newest files are kept and the older ones are deleted.
opt-log-private = The log contains the names of the folders and files you open, and the addresses and user names of your servers: read it before attaching it to a public report.
opt-log-variable = The EYECANSEE_LOG environment variable is set: it decides where the log goes, and this switch has no effect.
opt-log-now = This session writes its log to { $file }
opt-log-folder = Open the logs folder
opt-addons-text = Parts of EyeCanSee that are downloaded only if you want them. Each one is also offered the first time it is needed.
opt-under-thumbs = Under the thumbnails
opt-under-thumbs-hint = The details chosen go on one line under the name. With small thumbnails only the name stays.
opt-under-name = Name
opt-under-pixels = Dimensions in pixels
opt-under-pixels-hint = Read from each photo with its thumbnail, and kept with it: the next time they come at once.
opt-under-size = Size
opt-under-date = Date
opt-under-date-hint = When the photo was taken, if the file says so; otherwise when the file was modified. With the mouse over the date, a tip says which one it is.
opt-under-duration = Duration
opt-under-duration-hint = Of videos and audio files, beside the date. Read once from each file and kept with its thumbnail.
opt-under-drawing = Drawing number and revision
opt-under-drawing-hint = Of DWG and DXF drawings, before the size: from the title block (attributes such as DWG_NO or NUMBER, and REV), otherwise from the drawing's title and revision number. Read once from each drawing and kept with its thumbnail.
thumb-pixels = { $w } × { $h }
thumb-taken-on = Taken on { $date }
thumb-recorded-on = Recorded on { $date }
tip-taken = Taken
tip-recorded = Recorded
thumb-modified-on = Modified on { $date }
thumb-too-large = Too large for a thumbnail
trash = Trash
menu-restore = Restore
menu-restore-all = Restore all
menu-empty-trash = Empty the trash…
trash-bar-has = This folder has a trash
trash-open = Open the trash
trash-bar-inside = Trash of the folder { $folder }
delete-confirm-empty-trash = **Empty this trash?**

    Everything inside it is deleted **for good**.
    { "**" }It cannot be undone.**
restore-taken = In **"{ $folder }"** there is already an item named **"{ $name }"**.
restore-replace = Replace
restore-keep-both = Keep both
restore-merge = Merge
delete-ntfs = This is an **NTFS disk**: the Files app (Nautilus) cannot restore from its trash.
    The file can still be restored from a terminal with "gio trash --restore" (see the guide, Known limits).

opt-check-updates = Check for updates at startup
opt-check-updates-hint = When EyeCanSee starts, it asks GitHub for the latest release. A newer version is downloaded in the background and checked; it is installed only when you click "Install … and restart". Windows (installer and zip) and the AppImage; the copy from the Microsoft Store asks the Store instead, and the button opens its page.
update-install = Install { $version } and restart
update-failed = Cannot install the update: { $detail }
menu-check-updates = Check for updates
update-checking = Checking for updates…
update-latest = EyeCanSee { $version } is the latest version.
update-found = EyeCanSee { $version } is ready: click the red button at the top right to install it.
update-found-store = A newer EyeCanSee is in the Microsoft Store.
    Click the red button at the top right to open its page and update.
update-in-store = Update in the Microsoft Store
update-flatpak = This copy is a Flatpak.
    It is updated through flatpak update or your software center.
update-rpm = This copy is the Fedora package.
    It is updated with the rest of the system, by sudo dnf upgrade or by your software center (GNOME Software, Discover).
update-rpm-ostree = This copy is the Fedora package, layered with rpm-ostree on Fedora Atomic (Silverblue, Kinoite).
    It is updated with the system, by rpm-ostree upgrade and a restart, or by your software center (GNOME Software, Discover).
update-unavailable = This copy is not updated by EyeCanSee.
    Development builds and copies built from the sources are never updated.
update-check-failed = Cannot check for updates: { $detail }
# After "The download failed:" or "Cannot check for updates:".
download-stalled = nothing has arrived from the server for a minute. Try again later.
menu-redo-thumbs = Redo thumbnails
menu-redo-all-thumbs = Redo all thumbnails in the folder
key-shift-f5 = Shift + F5
convert-title = Save as
convert-original = Original
convert-result = Converted
convert-working = Converting…
convert-no-preview = No preview for this format here: the file is saved anyway.
convert-calculating = Working out the size…
convert-error = Cannot convert: { $detail }
convert-actual = 100% (drag the images to move around)
convert-pixels = { $w } × { $h } pixels
convert-frame-at = frame at { $time }
convert-turned = { $turn ->
    [cw] rotated 90° clockwise in the viewer
    [half] rotated 180° in the viewer
    [ccw] rotated 90° counter-clockwise in the viewer
    [flip-h] flipped horizontally in the viewer
    [flip-v] flipped vertically in the viewer
    [flip-h-cw] flipped horizontally and rotated 90° clockwise in the viewer
   *[flip-h-ccw] flipped horizontally and rotated 90° counter-clockwise in the viewer
}
model-triangles = { $n } triangles
model-node-again = A part of the model is placed more than once in its hierarchy, which glTF does not allow: shown once
model-node-again-saved = { $name }: a part of the model appeared more than once in its hierarchy; the new file has it only once
error-gltf-bin-missing = The file with the model's data is missing: { $detail }
error-gltf-bin-outside = The file with the model's data is not read: EyeCanSee does not read files outside the model's folder or from the internet.
    { $detail }
# { $detail }: empty, or the model's name in brackets (Convert).
error-gltf-data-damaged = The model's data written inside the file is damaged{ $detail }
error-glb-incomplete = The file is incomplete: the block with the model's data is missing{ $detail }
error-glb-cut = The file is incomplete{ $detail }
# { $expected }, { $actual }: sizes ("12.4 MB"), the one the file's header gives and the file's.
error-glb-cut-sizes = The file is incomplete{ $detail }: it should be { $expected } but it is { $actual }.
    Perhaps the download or the copy stopped: try downloading or copying it again.
# A 3D model with more triangles than EyeCanSee opens in its format: { $max } in millions (10, 20, 50), { $format } the kind of file.
# The formats read whole before the cap is met (OFF, Collada, VRML, X3D, FBX, IFC, a binary PLY left to the old parser, the CAD engine's) do not say why.
error-model-too-many-triangles = The model has more than { $max } million triangles.
    EyeCanSee does not open { $format ->
        [obj] OBJ files
        [three-mf] 3MF files
        [ply-text] text PLY files
        [ply-binary] binary PLY files
        [ply-binary-whole] binary PLY files
        [stl] STL files
        [stl-text] text STL files
        [glb] GLB files
        [gltf] glTF files
        [three-ds] 3DS files
        [off] OFF files
        [collada] Collada files
        [vrml] VRML files
        [x3d] X3D files
        [fbx] FBX files
        [amf] AMF files
        [ifc] IFC files
       *[other] 3D models
    } with more than { $max } million triangles{ $format ->
        [off] .
        [collada] .
        [vrml] .
        [x3d] .
        [fbx] .
        [ply-binary-whole] .
        [ifc] .
        [other] .
       *[read] , so that reading them does not fill the computer's memory.
    }
# A point cloud (PLY) with more points than EyeCanSee opens: { $max } in millions (2.5, 12.5), { $format } the kind of file.
error-model-too-many-points = The point cloud has more than { $max } million points.
    EyeCanSee does not open { $format ->
        [ply-binary] binary PLY files
        [ply-binary-whole] binary PLY files
       *[ply-text] text PLY files
    } with more than { $max } million points{ $format ->
        [ply-binary-whole] .
       *[read] , so that reading them does not fill the computer's memory.
    }
# The rarer refusals of a model or a drawing too large: { $max } in millions (1, 1.25, 200) where the sentence says "million", else as it is (1000 files, "2 GB" with its unit, 100 times).
error-model-too-many-offered = The model has more than { $max } million points and triangles, too many to open.
error-model-too-many-lines = The model has more than { $max } million lines and points, too many to open.
error-model-too-many-placed = The model repeats its parts more than { $max } million times, too many to open.
error-model-too-many-inlined = The model includes other files more than { $max } times, too many to open.
error-model-unpacks-too-large = Unpacked, the file would take more than { $max }, too much to open.
error-model-unpacks-too-many-times = Unpacked, the file would be more than { $max } times larger, too much to open.
error-drawing-too-many-points = The drawing has more than { $max } million points, too many to open.
error-drawing-too-many-entities = The drawing has more than { $max } million elements, too many to open.
error-drawing-too-many-placed = The drawing repeats its blocks more than { $max } million times, too many to open.
# A few words after a file's name in the list of a message ("a.gltf: the file a.bin is missing"): no capital, no full stop.
error-gltf-bin-missing-brief = the file { $detail } is missing
error-gltf-bin-outside-brief = the data is outside the model's folder ({ $detail })
error-gltf-data-damaged-brief = the data written inside the file is damaged
error-glb-incomplete-brief = the block with the data is missing
error-glb-cut-brief = the file is incomplete
error-glb-cut-sizes-brief = the file is incomplete
error-model-too-many-triangles-brief = more than { $max } million triangles
error-model-too-many-points-brief = more than { $max } million points
error-model-too-many-offered-brief = more than { $max } million points and triangles
error-model-too-many-lines-brief = more than { $max } million lines and points
error-model-too-many-placed-brief = parts repeated more than { $max } million times
error-model-too-many-inlined-brief = other files included more than { $max } times
error-model-unpacks-too-large-brief = unpacked, more than { $max }
error-model-unpacks-too-many-times-brief = unpacked, more than { $max } times larger
error-drawing-too-many-points-brief = more than { $max } million points
error-drawing-too-many-entities-brief = more than { $max } million elements
error-drawing-too-many-placed-brief = blocks repeated more than { $max } million times
# The keys of the viewer, under the status bar.
hint-cad = Wheel, + and −: zoom (Ctrl + wheel when the wheel changes file) | Drag: move | 0: whole drawing | Ctrl + L: layers | M: measure | N, Shift + N: next, previous change of a comparison | →, Space, Page Down: next | ←, Backspace, Page Up: previous | Home, End: first, last | Ctrl + Shift + S: save as an image | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
hint-cad-layouts = Wheel, + and −: zoom (Ctrl + wheel when the wheel changes file) | Drag: move | 0: whole drawing | L, Shift + L: next, previous layout | Ctrl + L: layers | M: measure | N, Shift + N: next, previous change of a comparison | →, Space, Page Down: next file | ←, Backspace, Page Up: previous file | Home, End: first, last file | Ctrl + Shift + S: save as an image | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
menu-layers = Layers of the drawing
key-layers = Ctrl + L
cad-layers-title = Layers
cad-layers-close = Close the layers panel (Ctrl + L)
cad-layers-off = Only for a DWG or DXF drawing open in the viewer, with something drawn on its layers
cad-layers-count = { $shown } of { $total } shown
cad-layers-search = Search
cad-layers-all-on = All on
cad-layers-all-off = All off
cad-layers-all-hint = The layers listed: all of them, or those found by the search
cad-layers-none = No layer with this name
menu-measure = Measure
menu-compare = Compare drawings
compare-off = Choose two CAD drawings in the grid to compare them
compare-reading = Comparing the drawings…
compare-failed =
    The drawings could not be compared
    { $error }
compare-both = In both
compare-only = Only in { $name }
compare-status = Compared with { $name }: { $n ->
    [one] 1 change
   *[other] { $n } changes
} (N: next)
compare-units = ⚠ Different units: { $first } in { $a }, { $second } in { $b }. Compared as they are, not converted.
compare-no-unit = no unit
measure-off = Only for a CAD drawing or a 3D model in the viewer
measure-title = Measure
measure-distance = Distance
measure-area = Area
measure-angle = Angle
measure-perimeter = Perimeter
measure-hint-distance = click two points; they snap to ends, middles, centres and crossings
measure-hint-area = click the corners; Enter or a double click closes the shape
measure-hint-angle = click a point, the corner, then a point on the other side
measure-hint-3d = click two points on the model for their distance
hint-measure-cad = Click: a point, snapped to ends, middles, centres and crossings | D: distance | A: area | G: angle | Enter, double click: close an area | Backspace: take back the last point | Ctrl + C: copy the results | Esc: clear the points, then close Measure | M: close Measure | Wheel, + and −: zoom | Drag: move | L, Shift + L: next, previous layout | →, ←, Space, Page Down, Page Up, Home, End: other files | Ctrl + L, F2, Del, F11, F1: as without Measure
hint-measure-3d = Click two points: their distance | Backspace: take back the last point | Ctrl + C: copy the results | Esc: clear the points, then close Measure | M: close Measure | Wheel, + and −: zoom | Arrows or drag: rotate | Shift + drag, middle button drag: move | 0, S: starting view, section | Space, Page Down, Page Up, Home, End: other files | F2, Del, F11, F1: as without Measure | Enter: back to the grid
measure-measuring = Measuring the model…
measure-size = Size (X × Y × Z)
measure-surface = Surface
measure-volume = Volume
measure-open = none: the model is not closed ({ $n } open edges)
measure-flipped = none: some faces are flipped ({ $n } edges between faces turned opposite ways)
measure-no-unit = the file does not say its unit
measure-copy = Copy
section-title = Section
section-flip = Other side
section-keys = Alt + wheel, comma, full stop: move · X, Y, Z: axis · F: other side · S: off
section-open = not filled: the model is not closed
cad-model = Model
cad-units = units
keys-button = Keys
menu-export-3d = Export as a 3D model…
status-exported = Saved as { $name }
model-export-title = Export as a 3D model
model-export-count = { $n ->
    [one] 1 3D model chosen.
   *[other] { $n } 3D models chosen.
}
model-export-format = Format:
model-export-where = Each copy goes next to its model, with the same name; a name already taken gets " (2)".
model-export-ok = Export
model-export-done-title = Export finished
model-export-cancelled = Export cancelled
model-export-pictures-heading = { $n ->
    [one] 1 model exported without its pictures:
   *[other] { $n } models exported without their pictures:
}
model-export-pictures-why = { $n ->
    [one] Its new format cannot hold pictures: their colours were put on the model instead. glTF (GLB) and OBJ keep them.
   *[other] Their new format cannot hold pictures: their colours were put on the models instead. glTF (GLB) and OBJ keep them.
}
model-export-surfaces-heading = { $n ->
    [one] 1 model exported without all of its materials:
   *[other] { $n } models exported without all of their materials:
}
model-export-surfaces-why = { $n ->
    [one] Its new format cannot hold everything about its materials (how smooth or metallic they are, bumps, shadows in the creases, their own light). glTF (GLB) keeps it all.
   *[other] Their new format cannot hold everything about their materials (how smooth or metallic they are, bumps, shadows in the creases, their own light). glTF (GLB) keeps it all.
}
model-export-smaller-heading = { $n ->
    [one] 1 model exported with smaller pictures:
   *[other] { $n } models exported with smaller pictures:
}
model-export-smaller-why = { $n ->
    [one] EyeCanSee reads pictures larger than 2048 pixels at 2048, and saves them so.
   *[other] EyeCanSee reads pictures larger than 2048 pixels at 2048, and saves them so.
}
model-export-left-out-heading = { $n ->
    [one] 1 model exported without some of its pictures:
   *[other] { $n } models exported without some of their pictures:
}
model-export-left-out-why = { $n ->
    [one] It has more than 64 pictures: EyeCanSee reads the first 64, and saves those.
   *[other] They have more than 64 pictures: EyeCanSee reads the first 64, and saves those.
}
model-export-running = Exporting 3D models
model-export-counts = Model { $n } of { $total }
model-export-files = { $n ->
    [one] **1 model** exported.
   *[other] **{ $n } models** exported.
}
model-export-before-cancel = **{ $n } of { $total } models** exported before Cancel.
model-export-failed-heading = { $n ->
    [one] 1 model not exported:
   *[other] { $n } models not exported:
}
model-export-again-heading = { $n ->
    [one] 1 model exported with a change:
   *[other] { $n } models exported with a change:
}
model-export-again-why = { $n ->
    [one] It placed one of its parts more than once in its hierarchy, which glTF does not allow: the new file has it only once.
   *[other] They placed one of their parts more than once in their hierarchy, which glTF does not allow: the new files have it only once.
}
# The window of Export, when models already read have more triangles than EyeCanSee opens in the format chosen: { $n } of them, { $all } yes when they are all the models chosen, { $max } the cap in millions, { $format } the format ("OBJ").
model-export-beyond = { $n ->
    [one] { $all ->
        [yes] The model has
       *[no] One of the models has
    }
   *[other] { $n } of the models have
} more than { $max } million triangles: EyeCanSee will not be able to open the new { $format } { $n ->
    [one] file. You can still export it.
   *[other] files. You can still export them.
}
model-export-beyond-heading = { $n ->
    [one] 1 model exported to a file that EyeCanSee will not be able to open:
   *[other] { $n } models exported to files that EyeCanSee will not be able to open:
}
model-export-beyond-why = { $n ->
    [one] It has more triangles than EyeCanSee can open in that format.
   *[other] They have more triangles than EyeCanSee can open in that format.
}
hint-model = Arrows or drag: rotate | Shift + drag, middle button drag: move | Wheel, + and −: zoom | 0: starting view | M: measure | S: section on and off | X, Y, Z: axis of the section | . , or Alt + wheel: move the section | F: the other side of the section | Space, Page Down: next | Backspace, Page Up: previous | Home, End: first, last | Ctrl + Shift + S: save as an image | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
hint-edit-select = Click: select an object | Drag: move it | Drag a corner or an end: resize (Shift: a box keeps its proportions; a pasted piece keeps them, Shift frees them) | Drag the round handle above the object: rotate it (Shift: 15° at a time) | Double click on a text or bubble: change its letters | I, Shift + I: eyedropper, on an object with a colour | Del: delete | Esc: let go of the object, then close the editor
hint-edit-marquee = Drag: choose a part | Shift + drag: a square | Ctrl + C: copy the part | Ctrl + X: cut it | Ctrl + V: paste as an object | Click or Esc: let go of the part (Esc again: close the editor)
hint-edit-arrow = Drag: draw | Drag the middle point: bend | Drag an end: move it | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-line = Drag: draw | Drag the middle point: bend | Drag an end: move it | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-pen = Drag: draw | Drag a corner: stretch | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-rect = Drag: draw | Shift + drag: a square | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-ellipse = Drag: draw | Shift + drag: a circle | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-highlight = Drag: highlight | Shift + drag: a square | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-spotlight = Drag: light a part | Shift + drag: a square or a circle | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | Del: delete | Esc: let go, then close the editor
hint-edit-text = Click: put a text, then type it | Enter: new line | Double click on a text: change it | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete (outside the field) | Esc: end the text, then close the editor
hint-edit-bubble = Drag: draw a bubble, then type its text | Enter: new line | Drag the end of the tail: point it | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | Double click: change the text | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: end the text, then close the editor
hint-edit-step = Click: next number | Drag a corner: resize (Shift: same proportions) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-stamp = Click: put the stamp | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-lens = Drag: draw a magnifier | Shift + drag: a square or a circle | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-cover = Drag: cover | Shift + drag: a square | Drag a corner: resize (Shift: same proportions) | Drag the round handle: rotate (Shift: 15° at a time) | I, Shift + I: eyedropper on the image, on the screen | Alt + click: take a colour, the tool stays | Del: delete | Esc: let go, then close the editor
hint-edit-crop = Drag a side or corner: crop | Drag inside: move the crop | Drag outside: move the view | Shift + wheel: rotate by 0.1° | Double click: whole image in view | Enter: apply | Esc: close the editor
hint-edit-common = V, M, A, L, P, R, E, H, S, T, U, N, K, Z, B, C: the tools | Wheel, + and −: zoom | 0: whole image | Right or middle button drag: move the view (with Crop: any button, outside the crop) | Ctrl + Z, Ctrl + Y: undo, redo | Ctrl + R, Ctrl + Shift + R: rotate right, left | Ctrl + Alt + I: resize | Ctrl + C, Ctrl + V: copy the result, paste an image (with Selection: as above) | Ctrl + S, Ctrl + Shift + S: save, save as | Tab, Shift + Tab: the buttons of the bars | F1: guide
hint-image = →, Space, Page Down: next | ←, Backspace, Page Up: previous | Home, End: first, last | Space on an animation: pause and play | Space on a motion photo: play its video | Page Down, Page Up on a TIFF with pages: next, previous page | P on a 360° panorama: 360° view | Wheel, + and −: zoom (Ctrl + wheel when the wheel changes image) | Drag: move a zoomed image | 0: fit the window | 1: 100% | Ctrl + R, Ctrl + Shift + R: rotate right, left | Ctrl + Z, Ctrl + Y: undo, redo an unsaved rotation | Ctrl + E: edit image | Ctrl + S, Ctrl + Shift + S: save, save as | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
hint-pano = Drag: look around | Wheel, + and −: zoom (Ctrl + wheel when the wheel changes image) | 0: starting view | 1: 100% in the middle | P: flat picture | →, Space, Page Down: next | ←, Backspace, Page Up: previous | Home, End: first, last | Ctrl + R, Ctrl + Shift + R: rotate right, left (shown flat) | Ctrl + E: edit image | Ctrl + S, Ctrl + Shift + S: save, save as | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
hint-grid = Arrows: move | Shift + arrows, Shift + Home, Shift + End: extend the selection | Home, End: first, last | Enter: open | Backspace: parent folder | Ctrl + A: select all | Tab, Shift + Tab: next, previous item, then the devices and the folders | ↓ ↑ → ← in the folders on the left: next, previous, open, close | Enter on a device: open it | Ctrl + C, Ctrl + X, Ctrl + V: copy, cut, paste | F2: rename | Ctrl + Shift + N: new folder | Del: delete | Shift + Del: delete for good | Ctrl + R, Ctrl + Shift + R: rotate right, left | Ctrl + E: edit image | Ctrl + B: convert | Ctrl + D: find duplicates | Ctrl + N: new image | Ctrl + Shift + D: bookmarks | F5: read the folder again | Shift + F5: thumbnails again | F6: cancel a job | F11: full screen | F1: guide
hint-dup = ↑ ↓: move between the copies | Space: tick or untick the copy | Click on a picture: open it in the viewer (Esc comes back here, with the ticks) | Tab, Shift + Tab: the buttons of the page | F1: guide | Esc: close (Enter does not delete)
hint-video = Space: pause and play | Ctrl + ← →: 5 seconds back, forward | . ,: one frame forward, back | ↑ ↓, wheel: volume | M: audio off and on | Ctrl + R, Ctrl + Shift + R: rotate right, left | Ctrl + Z, Ctrl + Y: undo, redo an unsaved rotation | Ctrl + S: save the rotation | Ctrl + Shift + S: save the frame shown as a picture | →, Page Down: next | ←, Backspace, Page Up: previous | Home, End: first, last | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
hint-audio = Space: pause and play | Ctrl + ← →: 5 seconds back, forward | ↑ ↓, wheel: volume | M: audio off and on | →, Page Down: next | ←, Backspace, Page Up: previous | Home, End: first, last | Ctrl + C, Ctrl + X: copy, cut the file | F2: rename | Del, Shift + Del: delete, delete for good | F5: read the folder again | Ctrl + Shift + D: bookmarks | Tab, Shift + Tab: the buttons of the bars | F6: cancel a job | F11: full screen | F1: guide | Double click: { $double } | Enter: back to the grid | Esc: back, or close EyeCanSee as the window's ✕
keys-double-grid = back to the grid
keys-double-full = full screen, and back to the window
cad-engine-title = CAD engine
cad-engine-ask = To open STEP, IGES and BREP models of CAD programs EyeCanSee needs the CAD engine (Open CASCADE), downloaded once: { $size }.
cad-engine-download = Download the CAD engine
cad-engine-button = Download the CAD engine ({ $size })
cad-engine-update = Update the CAD engine
cad-engine-update-button = Update the CAD engine ({ $size })
cad-engine-missing = The CAD engine is missing.
cad-engine-missing-brief = the CAD engine is missing
addon-video = Video engine
addon-video-about = Plays videos and audio files.
addon-ffmpeg = FFmpeg
addon-ffmpeg-about = Converts videos and saves the turned ones.
addon-cad = CAD engine
addon-cad-about = Opens the STEP, IGES and BREP models of CAD programs.
addon-download = Download
addon-update = Update
addon-downloaded = Downloaded.
addon-older = A newer version is available.
addon-on-system = Already on this computer: { $version }.
addon-system-only = Not downloaded: Tools > Options… > Video says to use the system's only.
first-start-welcome = Welcome to EyeCanSee
first-start-welcome-text = Choose your language and colors.
first-start-welcome-later = In Tools > Options… you can change them at any time.
first-start-addons = Add-ons
first-start-addons-text = These parts are large and not everybody needs them.
    Tick the ones you want to download now.
first-start-addons-later = The others are offered the first time they are needed.
    They are always in Tools > Options… > Add-ons.
first-start-count = { $n } of { $of }
first-start-back = Back
first-start-next = Next
first-start-skip = Skip
first-start-download = Download
first-start-done = Done
first-start-leave = { $n ->
    [one] You chose 1 add-on but did not download it.
   *[other] You chose { $n } add-ons but did not download them.
}
first-start-leave-download = Download and close
first-start-leave-close = Close without downloading

# The desktop icon on Windows (#210): Tools menu, first-start window, messages.
menu-desktop-icon = Create desktop icon
desktop-icon-title = Desktop icon
desktop-icon-question = Do you want EyeCanSee on the desktop, to start it with a double click?
desktop-icon-yes = Yes
desktop-icon-no = No
desktop-icon-choose = Choose Yes or No.
desktop-icon-done = The icon is on the desktop.
desktop-icon-later = With Tools > Create desktop icon you can make one at any time.
desktop-icon-failed = The icon could not be created.
desktop-icon-not-removed = The icon could not be removed.
desktop-icon-made = The icon **{ $name }** is on the desktop.
desktop-icon-there = An icon that starts EyeCanSee is already on the desktop.
convert-file-size = { $size } on disk
convert-ratio = { $percent }% of the original
convert-format = Format
convert-quality = Quality
convert-speed = Speed
convert-speed-hint = Slower makes a smaller file of the same quality.
convert-lossless = Lossless: the same pixels as the original.
convert-gif = At most 256 colours.
convert-keep-meta = Keep the metadata (EXIF, XMP, IPTC, colour profile)
meta-keep = Metadata
meta-keep-hint = What the new file keeps of the original's metadata. "All but the place" leaves out the GPS position and the city, state and country, for a photo to share; "None" keeps only the colour profile, which the picture needs to look right.
meta-keep-all = All (EXIF, XMP, IPTC, colour profile)
meta-keep-no-place = All but the place (no GPS position)
meta-keep-nothing = None (only the colour profile)
convert-keep-density = Keep the metadata (resolution only)
convert-ultra-hdr-lost = This photo is Ultra HDR: it keeps its HDR only in JPEG with the metadata.
convert-first-only = This image is animated or has several pages: only the picture shown is saved.
convert-no-meta = The original has no metadata.
convert-meta-unsupported = This format cannot keep metadata.
convert-meta-density-only = This format keeps only the resolution (dpi), when the original has one.
# Between the last two names of a list ("EXIF and XMP").
list-and = and
convert-meta-too-large = The new file will be saved without the original's { $parts } data, because it is too large for a JPEG file.
convert-folder = Folder
convert-name = File name
convert-save = Save
convert-cancel = Cancel
convert-exists = An item with this name already exists: it will be saved as { $name }.
convert-saved-elsewhere-title = Image saved
convert-saved-elsewhere = Saved as **{ $path }**.

    The image shown is the original, as it is on disk.
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
# The copy an edit saves next to the original: "photo (edited).jpg".
edit-copy-word = edited
rc-rotation = Rotation
rc-reset = Back to 0°
rc-straighten = Straighten
rc-straighten-hint = Draw a line along the horizon, or along something upright: the image rotates to make it level.
rc-one-only = Edit image and Rotate and crop work on one image at a time. To rotate several images together use the items above or Convert…
rename-one-only = Rename works on one file at a time.
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
rc-save = Save…
rc-save-hint = Saves a copy next to the original, named as the original with "(edited)"; the original stays as it is.
rc-save-as-hint = Saves the result as a new file, in the format you choose; the original stays as it is.
rc-close = Close
rc-discard = Don't save
rc-leave = The changes to **{ $name }** are not saved.

    { "**" }Save** writes them over the original.
    { "**" }Save as…** makes a new file.
    { "**" }Don't save** loses them.
menu-edit-image = Edit image…
key-edit = Ctrl + E
edit-tool-select = Select
edit-tool-arrow = Arrow
edit-tool-line = Line
edit-tool-pen = Freehand
edit-tool-rect = Rectangle
edit-tool-ellipse = Ellipse
edit-tool-highlight = Highlighter
edit-tool-spotlight = Spotlight
edit-tool-text = Text
edit-tool-stamp = Stamps
edit-tool-lens = Magnifier
edit-tool-bubble = Speech bubble
edit-tool-step = Step numbers
edit-tool-cover = Cover
edit-tool-crop = Crop and rotate
edit-save-copy = Save copy
edit-update-copy = Update copy
edit-update-hint = Writes the copy saved before again, from the original with all the changes; if it is gone or another program changed it, a new copy is made.
edit-update-confirm = **{ $copy }** is written again with the changes.
    The original stays as it is.
edit-overwrite-again = **{ $name }**, already overwritten, is written again with the changes, from the original as it was opened.
    The original is already in the trash: what was written before is replaced.
edit-copy-gone = The copy saved before is gone or was changed by another program: a new copy was made.
edit-off-sending = The last save is still being sent.
edit-save-more = More ways to save
edit-off-unchanged = No changes to save.
edit-off-preparing = The file is being made.
edit-off-undo = Nothing to undo.
edit-off-redo = Nothing to redo.
edit-off-copying = Copying to the clipboard.
edit-zoom-in = Zoom in
edit-zoom-out = Zoom out
edit-col-red = Red
edit-col-orange = Orange
edit-col-yellow = Yellow
edit-col-green = Green
edit-col-blue = Blue
edit-col-purple = Purple
edit-col-black = Black
edit-col-white = White
edit-col-other = Other colour
edit-tip-select = Click an object to select it, drag it to move it.
edit-tip-draw = Drag on the image to draw (with Shift a square or a circle); drag the middle point of a line or an arrow to bend it.
edit-tip-text = Click the image where the text goes.
edit-tip-stamp = Click the image to put the stamp chosen in the bar.
edit-stamp-check = Check
edit-stamp-cross = Cross
edit-stamp-warning = Warning
edit-stamp-info = Information
edit-stamp-question = Question
edit-stamp-star = Star
edit-stamp-thumbs-up = Thumbs up
edit-stamp-thumbs-down = Thumbs down
edit-stamp-heart = Heart
edit-stamp-smile = Smile
edit-stamp-grin = Big smile
edit-stamp-frown = Frown
edit-stamp-circle-check = Check in a circle
edit-stamp-circle-x = Cross in a circle
edit-stamp-exclamation = Exclamation mark
edit-stamp-ban = Not allowed
edit-stamp-plus = Plus
edit-stamp-minus = Minus
edit-stamp-up = Arrow up
edit-stamp-down = Arrow down
edit-stamp-left = Arrow left
edit-stamp-right = Arrow right
edit-stamp-flag = Flag
edit-stamp-pin = Place
edit-stamp-target = Target
edit-stamp-idea = Idea
edit-stamp-bell = Bell
edit-stamp-lock = Locked
edit-stamp-unlock = Open lock
edit-stamp-eye = Look
edit-stamp-clock = Clock
edit-stamp-calendar = Calendar
edit-stamp-mail = Mail
edit-stamp-phone = Phone
edit-stamp-home = Home
edit-stamp-camera = Camera
edit-stamp-person = Person
edit-stamp-people = People
edit-stamp-award = Award
edit-stamp-trophy = Trophy
edit-stamp-crown = Crown
edit-stamp-verified = Verified
edit-stamp-shield = Safe
edit-stamp-bookmark = Bookmark
edit-stamp-gift = Gift
edit-stamp-party = Party
edit-stamp-rocket = Rocket
edit-stamp-fire = Fire
edit-stamp-bolt = Lightning
edit-stamp-sparkles = Sparkles
edit-stamp-sun = Sun
edit-stamp-moon = Moon
edit-stamp-cloud = Cloud
edit-stamp-coffee = Coffee
edit-stamp-car = Car
edit-stamp-plane = Plane
edit-stamp-hand = Hand
edit-stamp-more = More stamps
edit-tip-spotlight = Drag on the image over what must stand out: the rest is darkened (with several, all of them stay bright); with Shift a square or a circle.
edit-intensity = Intensity
edit-tip-lens = Drag on the image to draw a magnifier: it shows the part under it larger; with Shift a square or a circle.
edit-zoom = Zoom:
edit-lens-objects = With the objects
edit-lens-objects-hint = The magnifier shows the objects drawn before it too (those drawn after it stay over it); cover boxes and filters it always shows.
edit-round = Round
edit-square = Square
edit-tip-bubble = Drag on the image to draw a bubble, then type its text; drag the end of its tail to point it.
edit-bubble-rounded = Rounded
edit-bubble-oval = Oval
edit-bubble-square = Square
edit-bubble-cloud = Thought (cloud)
edit-bubble-shout = Shout
edit-letters = Letters:
edit-tip-step = Click the image to put the next number (one more than the highest); its number can be changed in the bar.
edit-step-number = Number
edit-tip-cover = Drag over what must be hidden: nothing of it is left in the saved file.
edit-tip-crop = Drag sides and corners to crop; Shift + wheel rotates.
rc-apply = Apply
rc-apply-hint = Back to the tool before, with the result cropped. Ctrl + Z undoes the crop.
rc-reset-off = The angle is already 0°.
rc-swap-off = With free or 1:1 proportions there is nothing to swap.
rc-color-off = Choose Colour first.
edit-head-filled = Filled head
edit-head-open = Open head
edit-head-double = Head at both ends
edit-head-dot = Dot
edit-font-default = Default
edit-font-hint = Point at a font in the list to see the text in it; click one to choose it.
edit-pick = Eyedropper: from anywhere on the screen with the system's picker (button, Shift + I); I takes it on the image, what is drawn included, with a box showing the colour
edit-pick-image = Eyedropper: take a colour from the image
edit-picked = Colour taken: { $codes } (the hex code is on the clipboard)
edit-pick-size = One pixel, or the average of the pixels around it
edit-pick-screen-busy = Choose the point of the screen in the system's window.
edit-pick-failed = The system did not give the colour: { $detail }
edit-result-failed = The edited image could not be made: { $detail }
edit-result-failed-alone = The edited image could not be made.
edit-tip-pick = Click the image to take its colour; Esc cancels.
edit-props-next = Next object:
edit-props-next-hint = Colour, width and opacity are those of the next object you draw with this tool. To change one already drawn, select it.
edit-props-selected = Selected object:
edit-props-selected-hint = The changes go to the selected object, and stay for the next ones of the same kind.
edit-border = Border:
edit-fill = Fill:
edit-fill-none = No fill
edit-tool-marquee = Selection
edit-tip-marquee = Drag on the image to choose a part; Ctrl + C copies it, Ctrl + X cuts it, Ctrl + V pastes it as an object.
edit-copy-part = Copy the part
edit-cut-part = Cut the part
edit-paste-part = Paste
edit-off-no-marquee = Choose a part first, dragging on the image.
edit-off-no-clip = Nothing to paste: copy or cut a part first.
edit-paste-none = No image in the clipboard.
edit-paste-too-big = Not pasted: the image is { $w } × { $h } pixels, and the editor accepts at most { $max } per side and { $mp } megapixels in all.
edit-crop-max = The crop cannot be larger: at most { $w } × { $h } pixels.
edit-copy-too-large = Too large for other programs: { $size } MB, the limit is 16 MB. Ctrl + V pastes it here.
edit-part-copied = Part copied: Ctrl + V pastes it as an object, also in another program.
edit-filters = Filters
edit-filters-part = On the part chosen:
edit-filters-whole = On the whole image (choose a part with Selection first for a piece only):
edit-filter-blur = Blur
edit-filter-pixelate = Pixelate
edit-filter-sharpen = Sharpen
edit-filter-gray = Black and white
edit-filter-sepia = Sepia
edit-filter-brightness = Brightness
edit-filter-contrast = Contrast
edit-filter-done = Done
edit-filter-done-hint = The filter is already applied: it stays as it is and goes into the file with Save copy. A click on the filter opens it again to change it.
edit-filter-amount = Amount
edit-filter-not-cover = Blur and Pixelate are not enough to hide private text or numbers: at a low amount they can be read, or worked back. For those use Cover (B).
edit-text-curve = On a curve
edit-text-curve-hint = The text follows a line: drag its middle point to bend it, its ends to stretch or rotate it.
edit-outline = Outline
edit-outline-hint = A line round the letters, to read them on any photo.
edit-text-back = Box
edit-text-back-hint = A box behind the letters (on a curve: a band along all its line).
edit-margin = Margin
edit-corners = Corners
menu-new-image = New image…
key-new-image = Ctrl + N
menu-open-clipboard = Open from clipboard
clip-open-name = Pasted image
clip-open-title = Open from clipboard
clip-open-none = The clipboard has no image.
    Copy one in another program first (a screenshot, a picture from a web page), then choose Open from clipboard again.
new-title = New image
new-name = New image
new-size = Size
new-custom = Custom
new-width-height = Width × Height
new-swap = Swap width and height
new-dpi = Resolution
new-dpi-off = The resolution is for sizes in millimetres, centimetres or inches.
new-background = Background
new-white = White
new-transparent = Transparent
new-colour = Colour
new-pixels = { $w } × { $h } pixels
new-too-big = { $w } × { $h } pixels, { $mp } megapixels: a new image can have at most { $max } megapixels.
new-create = Create
new-letter = Letter
new-legal = Legal
new-square = Square
new-story = Upright, for stories
edit-new-off = It is a new image: there is no file yet to write over. Use Save as….
edit-undo = Undo
edit-redo = Redo
edit-resize = Resize
edit-resize-hint = Changes the size of the image in pixels; what you drew follows it. Ctrl + Z undoes it.
key-edit-resize = Ctrl + Alt + I
resize-title = Resize the image
resize-now = Now
resize-size = New size
resize-pixels = { $w } × { $h } pixels
resize-unit-pixels = pixels
resize-percent = percent
resize-keep = Keep the proportions
resize-keep-off = The image is rotated by an angle: with other proportions it would come out slanted.
resize-result = The result will be
resize-same = It is the size the image has now.
resize-too-big = Too large: at most { $max } pixels a side.
resize-apply = Resize
edit-copy = Copy
edit-copied = The result is on the clipboard.
edit-color = Colour
edit-width = Width
edit-size = Size
edit-opacity = Opacity
edit-dash = Line
edit-solid = Solid
edit-dashed = Dashed
edit-dotted = Dotted
edit-delete = Delete
edit-turn-hint = Its angle on the screen. You can also drag the round handle above it; hold Shift to rotate it 15° at a time.
edit-patch-ccw = Rotate the piece 90° anticlockwise
edit-patch-cw = Rotate the piece 90° clockwise
edit-patch-flip-h = Flip the piece horizontally
edit-patch-flip-v = Flip the piece vertically
edit-patch-width = Width
edit-patch-height = Height
edit-patch-keep-hint = Width and height change together, here and when you drag a corner. Hold Shift while dragging a corner to change them apart.
edit-patch-front = Bring to front
edit-patch-back = Send to back
edit-select-hint = Select an object to change it, or choose a tool.
edit-text-hint = Click on the image where the text goes, then type it here.
edit-animated = **{ $name }** cannot be edited: it is animated or has several pages, and only the first picture would be saved.
edit-animated-off = This image is animated or has several pages: only the first picture would be saved.
edit-overwrite = Overwrite the original…
edit-overwrite-short = Overwrite…
edit-overwrite-hint = Writes the edited image over the original, after asking; the original goes to the trash, also when deleting for good is chosen in the Options.
edit-overwrite-yes = Overwrite
edit-save-confirm = A copy of **{ $name }** is saved next to it as **{ $copy }**.
    The original stays as it is.
edit-loss-lossy = The copy is **encoded again** and may lose a little quality.
edit-loss-8bit = The copy has **8 bits per colour**, the original more: the original stays as it is, for what needs them.
edit-loss-hdr = The copy is a 16-bit PNG: what is **brighter than white** in the original is cut.
edit-loss-ultra-hdr = Without metadata the copy is no longer **Ultra HDR**: nothing holds its HDR part.
edit-loss-video = The copy has no **video**: the motion photo stays only in the original.
edit-loss-pictures = The copy is the photo alone: the **depth map** or the other pictures the original carries stay only in the original.
edit-loss-meta = The copy is a JPEG without the original's **{ $parts }** data, because it is too large for a JPEG file.
edit-overwrite-trash = **{ $name }** is replaced by the edited image.

    The original goes **to the trash**.
edit-overwrite-folder = **{ $name }** is replaced by the edited image.

    The original goes to **this folder's Trash**, where **Open the trash** restores it.
edit-over-loss-lossy = The new file is **encoded again** and may lose a little quality.
edit-over-loss-8bit = The new file has **8 bits per colour**, the original more.
edit-over-loss-hdr = The new file has 16 bits per colour: what is **brighter than white** in the original is cut.
edit-over-loss-ultra-hdr = Without metadata the new file is no longer **Ultra HDR**: nothing holds its HDR part.
edit-over-loss-video = The new file has no **video**: the motion photo becomes a still photo.
edit-over-loss-pictures = The new file is the photo alone: the **depth map** or the other pictures the original carries are not kept.
edit-over-loss-meta = The new file is a JPEG without the original's **{ $parts }** data, because it is too large for a JPEG file.
edit-leave = The changes to **{ $name }** are not saved.

    { "**" }Save** makes a copy next to the original.
    { "**" }Save as…** makes a new file where you choose.
    { "**" }Don't save** loses them.
edit-leave-over = The changes to **{ $name }** are not saved.

    { "**" }Save** writes them over the original, which goes to the trash.
    { "**" }Save as…** makes a new file where you choose.
    { "**" }Don't save** loses them.
edit-leave-new = The new image **{ $name }** is not saved yet.

    { "**" }Save** and **Save as…** choose where to save it and in which format.
    { "**" }Don't save** loses it.
edit-temp-copy = This image is a temporary copy Windows made of a file of a phone or another device: a copy next to it could be deleted by Windows. Choose where to save it.
edit-remote-phone = The image is edited in a copy on this computer: Save sends the result to the phone.
edit-remote-server = The image is edited in a copy on this computer: Save sends the result to the server.
edit-open-later = { $name } opens when you close the editor.
key-rotate-cw = Ctrl + R
key-rotate-ccw = Ctrl + Shift + R
menu-save = Save
key-save = Ctrl + S
menu-save-copy = Save as…
menu-save-frame = Save frame as…
key-save-copy = Ctrl + Shift + S
save-confirm-trash = Save the changes over **{ $name }**?

    The original goes **to the trash**.
save-confirm-folder = Save the changes over **{ $name }**?

    The original goes to **this folder's Trash**, where **Open the trash** restores it.
save-confirm-permanent = Save the changes over **{ $name }**?

    The original is deleted **for good**.
save-yes = Save
save-failed = Cannot save: { $detail }
## Errors said plainly (src/fault.rs). $known is "yes" when the file's name is known, then in $name.
fault-denied = { $known ->
    [yes] Access denied to "{ $name }": the folder or the file is protected, and your user cannot write or read it there.
   *[no] Access denied: the folder or the file is protected, and your user cannot write or read it there.
}
fault-read-only = { $known ->
    [yes] The disk or the folder is read-only: "{ $name }" cannot be written there.
   *[no] The disk or the folder is read-only: nothing can be written there.
}
fault-no-space = { $known ->
    [yes] There is no space left on the disk for "{ $name }": free some space and try again.
   *[no] There is no space left on the disk: free some space and try again.
}
fault-no-space-replace = { $known ->
    [yes] There is not enough space on the disk to replace "{ $name }": while it is being replaced, the disk needs room for a second copy of the file.
        Free some space and try again.
   *[no] There is not enough space on the disk to replace the file: while it is being replaced, the disk needs room for a second copy of it.
        Free some space and try again.
}
fault-too-large = { $known ->
    [yes] "{ $name }" is too large for this disk: FAT32 disks take files of at most 4 GB.
   *[no] The file is too large for this disk: FAT32 disks take files of at most 4 GB.
}
fault-not-found = { $known ->
    [yes] "{ $name }" is not there any more: it may have been moved, renamed or deleted.
   *[no] The file or folder is not there any more: it may have been moved, renamed or deleted.
}
fault-exists = { $known ->
    [yes] There is already an item named "{ $name }" there.
   *[no] There is already an item with this name there.
}
fault-in-use = { $known ->
    [yes] "{ $name }" is in use by another program: close it and try again.
   *[no] The file is in use by another program: close it and try again.
}
fault-bad-name = { $known ->
    [yes] The name "{ $name }" is not accepted there: it is too long, or has characters this disk does not allow.
   *[no] The name is not accepted there: it is too long, or has characters this disk does not allow.
}
# As the sentences above, in a few words after a file's name in the list of a message.
fault-denied-brief = access denied
fault-read-only-brief = read-only disk or folder
fault-no-space-brief = no space left on the disk
fault-no-space-replace-brief = not enough space on the disk
fault-too-large-brief = too large for this disk
fault-not-found-brief = no longer there
fault-exists-brief = an item with this name is already there
fault-in-use-brief = in use by another program
fault-bad-name-brief = name not accepted there
## Titles of the message and confirmation windows.
save-title = Save over the original
save-not-possible = Cannot save over the original
leave-title = Changes not saved
kept-title = Image not saved
kept-saved-title = Copy saved
file-removed-title = File removed
phone-stalled-title = The phone does not answer
bm-missing-title = Bookmark not available
taken-title = Name already in use
delete-title-trash = Move to the trash
delete-title-permanent = Delete for good
delete-title-empty = Empty the trash
kiod-title = Phone held by KDE
host-title = New SSH server
cert-title = New FTPS server
sftp-rsa-title = Better change this RSA key
edit-overwrite-title = Overwrite the original
dup-ask-title = Find duplicates
vault-plain-title = Servers saved in plain text
kept-failed-phone = **{ $name }** was not saved on the phone: { $error }

    The original on the phone is unchanged.

    The changed image is kept on this computer until it is saved there, in:

    { $folder }
kept-failed-server = **{ $name }** was not saved on the server: { $error }

    The original on the server is unchanged.

    The changed image is kept on this computer until it is saved there, in:

    { $folder }
kept-retry = Try again
kept-open-folder = Open the folder
kept-saved = Saved as **{ $path }**.

    The copy kept on this computer stays where it was, until the image is saved where it belongs.
status-modified = Changed, not saved
status-saving = Saving…
status-preparing = Preparing to save…
zoom-field-hint = Zoom: click to type a percentage, then Enter (Esc: as it was)
zoom-slider-hint = Zoom: drag to make the picture larger or smaller
batch-turn-confirm = { $what ->
    [videos] { $n ->
        [one] **1 video** will be turned and saved over its original.
       *[other] Up to **{ $n } videos** will be turned and saved over their originals.
    }
    [mixed] Up to **{ $n } images and videos** will be changed and saved over their originals.
   *[other] { $n ->
        [one] **1 image** will be changed and saved over its original.
       *[other] Up to **{ $n } images** will be changed and saved over their originals.
    }
}
batch-originals-trash = The originals go **to the trash**.
batch-originals-folder = The originals go to **this folder's Trash**, where **Open the trash** restores them.
batch-originals-permanent = The originals are deleted **for good**.
batch-lossy = JPEG files are rotated without losing quality when their sides are multiples of 16 pixels.
    The others, and WebP, AVIF and GIF, are encoded again and may lose a little quality.
batch-skipped = { $n ->
    [one] **1 image** is left out: its format cannot be saved back (HEIC, RAW…).
   *[other] **{ $n } images** are left out: their format cannot be saved back (HEIC, RAW…).
}
batch-none = None of these **{ $n } images** can be saved back: their format cannot be written (HEIC, RAW…).
    Open them one at a time and use **Save as…**.
batch-save-as-hint = **Save as…** saves it already rotated to a new file, in the format you choose.
    The original stays as it is.
status-saving-n = Saving: { $n }…
save-lossy = This image is **encoded again** and may lose a little quality.
    A JPEG is rotated without loss only when its sides are multiples of 16 pixels; lossy WebP, AVIF and GIF are always encoded again.
save-skipped = The changes cannot be saved over **{ $detail }**.
    It is animated, has several pages or carries more than one picture (the video of a motion photo, a depth map): those parts would be lost.
save-animated = The changes cannot be saved over **{ $detail }**.
    It is animated: saving over it would keep only the first frame.
save-pages = The changes cannot be saved over **{ $detail }**.
    It has several pages: saving over it would keep only the first one.
save-extra = The changes cannot be saved over **{ $detail }**.
    It also carries the video of a motion photo or a depth map, which would be lost.
save-temp-copy = This image **cannot be saved over itself**: it is a temporary copy that Windows made when it opened the file from a phone or another device, not the file there.
    Saving over it would change only the copy, which Windows can delete.
    Use **Save as…** to keep the changes.
save-gvfs = Images **cannot be saved over themselves** here.
    This folder was opened through the file manager (gvfs), where replacing a file can lose the original.
save-no-rename = Images **cannot be saved over themselves** here.
    This source cannot rename files.
save-video-no-rotation = { $detail }: only MP4, MOV, MKV and WebM videos keep a rotation. To rotate it, use Convert….
batch-turn-videos = **Videos** are not encoded again: only the rotation that players read changes, in seconds and without loss.
off-video-flip = A video can only be rotated here: to flip it, use Convert….
batch-animated = **Animated** and **multi-page** images, **motion photos** and photos with a **depth map** (the portrait photos of some phones) are skipped: saving over them would lose those parts.
    Ultra HDR photos are rotated with their HDR map, and with their video when they are motion photos too.
batch-skipped-heading = { $n ->
    [one] 1 picture left out:
   *[other] { $n } pictures left out:
}
batch-skipped-why = { $n ->
    [one] It is animated, has several pages, is a motion photo or carries a depth map: Save does not write it again.
   *[other] They are animated, have several pages, are motion photos or carry a depth map: Save does not write them again.
}
# $op is the id of the menu item of the change (tf-…).
batch-done-title = { $what ->
    [videos] { $n ->
        [one] Video rotated
       *[other] Videos rotated
    }
    [mixed] Pictures and videos rotated
   *[other] { $op ->
        [tf-flip-h] { $n ->
            [one] Image flipped
           *[other] Images flipped
        }
        [tf-flip-v] { $n ->
            [one] Image flipped
           *[other] Images flipped
        }
       *[other] { $n ->
            [one] Image rotated
           *[other] Images rotated
        }
    }
}
batch-done = { $what ->
    [videos] { $n ->
        [one] **1 video** was { $op ->
            [tf-rotate-cw] rotated 90° clockwise
            [tf-rotate-ccw] rotated 90° anticlockwise
           *[other] rotated 180°
        } and saved over its original.
       *[other] **{ $n } videos** were { $op ->
            [tf-rotate-cw] rotated 90° clockwise
            [tf-rotate-ccw] rotated 90° anticlockwise
           *[other] rotated 180°
        } and saved over their originals.
    }
    [mixed] **{ $n } files** (pictures and videos) were { $op ->
        [tf-rotate-cw] rotated 90° clockwise
        [tf-rotate-ccw] rotated 90° anticlockwise
       *[other] rotated 180°
    } and saved over their originals.
   *[other] { $n ->
        [0] No image was saved.
        [one] **1 image** was { $op ->
            [tf-flip-h] flipped horizontally
            [tf-flip-v] flipped vertically
            [tf-rotate-cw] rotated 90° clockwise
            [tf-rotate-ccw] rotated 90° anticlockwise
           *[other] rotated 180°
        } and saved over its original.
       *[other] **{ $n } images** were { $op ->
            [tf-flip-h] flipped horizontally
            [tf-flip-v] flipped vertically
            [tf-rotate-cw] rotated 90° clockwise
            [tf-rotate-ccw] rotated 90° anticlockwise
           *[other] rotated 180°
        } and saved over their originals.
    }
}
save-psd-flatten = **This PSD will be flattened**: layers, text, effects and masks are lost, only the image as it looks remains.
    To keep them use **Save as…** or edit it in Photoshop.
batch-psd-flatten = PSD files with several layers, text or effects are **flattened**: only the image as it looks remains.
save-format = The changes cannot be saved over **{ $name }**.
    EyeCanSee cannot write this format (HEIC, RAW…).
save-as-hint = **Save as…** saves it to a new file, in the format you choose.
    The original stays as it is.
notice-close = Close
notice-copy = Copy the message

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
addr-save-open = Save and open
addr-save-open-hint = Also saves the server with its password in File > Server manager: a saved one with this login gets the new password.
addr-save-open-local = Only a server can be saved.
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
vault-on = 🔒 Master password on: the list of servers is encrypted.
vault-on-hint = Every change is written at once, encrypted. EyeCanSee asks for the password once per session, at the first server.
vault-off = No master password: passwords and passphrases are saved in plain text.
vault-off-hint =
    Anyone who can read your files can read these passwords too.
    Set a master password to encrypt the list of servers.
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
    The passwords and passphrases of your servers are written **as they are** in the file of the servers.

    { "**" }Anyone who can read your files can read these passwords too.**

    To protect them, set a **master password** at the top of File > Server manager.

    This warning is shown only once.
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
kiod-ask = KDE is holding the phone through its kiod service, the one Dolphin uses for phones.
    To open it, EyeCanSee has to close that service.
    KDE restarts it by itself, but it **forgets the passwords of network folders** (Samba, SFTP…) opened in Dolphin, and will ask for them again next time.

    This **does not depend on EyeCanSee**: KDE keeps phones and those passwords in the same service, and lets nobody release only the phone.
kiod-ok = OK
kiod-ask-flatpak =
    The phone is held by a KDE service, and the Flatpak version of EyeCanSee cannot take it over by itself.

    Close Dolphin and press **Release the phone**: EyeCanSee closes that service, and KDE starts it again when needed.

    - Network folder passwords that were not saved are forgotten.
    - Copies running in Dolphin stop, but they can be resumed with Retry.
kiod-free = Release the phone

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
menu-batch = Convert…
menu-batch-about = All the files selected in the grid at once, as they are on disk: the same format and settings for all (also resizing, videos and drawings), each with its own name.
menu-save-copy-about = One image, the one open or selected, as you see it (with unsaved rotations, the editor's work, of a video the frame shown): you choose the name, the format and the folder, with a preview.
menu-resize = Resize
menu-resize-custom = Custom…
batch-title-resize = Resize
batch-working-resize = Resizing
batch-start-resize = Resize
batch-title-copy-meta = Copy with the metadata
batch-start-copy = Copy
batch-working-copy = Copying
batch-tag-rotated = rotated
batch-tag-flipped = flipped
preset-small = Small
preset-medium = Medium
preset-large = Large
preset-half = Half
preset-desc-percent = { $a }%
preset-desc-fit = inside { $a } × { $b } px
preset-desc-long = long side { $a } px
preset-desc-short = short side { $a } px
preset-desc-width = width { $a } px
preset-desc-height = height { $a } px
preset-desc-crop = exactly { $a } × { $b } px, cropped
preset-desc-stretch = exactly { $a } × { $b } px, stretched
preset-skip-format = { $detail }: EyeCanSee cannot write its format; Resize > Custom… can save it in another one.
menu-export-drawings = Export drawing…
menu-partial = * Only on some of the files chosen
menu-partial-hint = The entries marked with * work only on the files of the kinds they are for (for example Convert… on pictures and videos); the other files chosen are left out.
menu-copy-meta = Copy with the metadata…
menu-strip-audio = Copy without audio…
menu-extract-audio = Save the audio to a file…
batch-title = Convert
batch-title-strip = Copy without audio
batch-title-extract = Save the audio to a file
batch-strip-hint = Makes a new video, the same as the original but without audio. The original is not touched. The video and its other tracks (subtitles, chapters) are copied as they are, without the audio: no re-encoding, no loss, the metadata kept. The new file has the format of the original.
batch-extract-hint = Saves the audio of the video in a separate file (for example .m4a). The video is not touched. Every audio track is copied as it is into a file of its own, in the format of its codec (AAC as .m4a, Opus as .opus, MP3 as .mp3, FLAC as .flac): no re-encoding, no loss, the metadata kept. For another format, use Convert…
batch-audio-not-ready = FFmpeg is not ready yet.
batch-images = { $n ->
    [one] **1 file** in { $folder }.
   *[other] **{ $n } files** in { $folder }.
}
batch-same-format = Same as the original
batch-same-format-hint = Each image in its own format, written as Save writes it (a JPEG at its own quality). Formats EyeCanSee cannot write (HEIC, RAW…) are left out.
batch-ultra-hdr-lost = If some photos are Ultra HDR, they lose their HDR in this format: only JPEG keeps it.
batch-size = Size
batch-size-same = Unchanged
batch-size-percent = Percent of the original
batch-size-fit = Inside
batch-pixels = pixels (proportions kept)
batch-px = pixels
batch-size-long = Long side
batch-size-short = Short side
batch-size-width = Width
batch-size-height = Height
batch-size-crop = Exactly, cropped in the middle
batch-size-stretch = Exactly, stretched
batch-size-images-only = Pictures only: off while videos are among the files chosen.
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
batch-nothing = Choose another format, another size or a rotation: as they are, the images would only lose quality.
batch-bad-folder = Write the name of the subfolder (without / or \).
batch-start = Convert
batch-working = Converting
batch-counts = { $n } of { $total } files
batch-stopping = Stopping…
batch-left-out-kind = { $n ->
    [one] 1 file chosen is left out: this entry does not work on its kind.
   *[other] { $n } files chosen are left out: this entry does not work on their kind.
}
summary-saved = { $n ->
    [one] **1 file** saved in { $folder }.
   *[other] **{ $n } files** saved in { $folder }.
}
summary-left-out = { $n ->
    [one] **1 file** left out.
   *[other] **{ $n } files** left out.
}
summary-not-done = { $n ->
    [one] **1 file** not done: the work was stopped.
   *[other] **{ $n } files** not done: the work was stopped.
}
summary-nothing = No file was saved.
summary-took = It took { $time }.
summary-sizes = Size: { $before } before, { $after } after ({ $percent }%).
summary-kind = { $n ->
    [one] 1 file of a kind this command does not work on:
   *[other] { $n } files of a kind this command does not work on:
}
summary-format = { $n ->
    [one] 1 file in a format EyeCanSee cannot write; choose another format:
   *[other] { $n } files in a format EyeCanSee cannot write; choose another format:
}
summary-format-preset = { $n ->
    [one] 1 file in a format EyeCanSee cannot write; Resize > Custom… can save it in another one:
   *[other] { $n } files in a format EyeCanSee cannot write; Resize > Custom… can save them in another one:
}
summary-same-size = { $n ->
    [one] 1 file already of this size, or smaller and not enlarged:
   *[other] { $n } files already of this size, or smaller and not enlarged:
}
summary-inside = { $n ->
    [one] 1 file animated or with several pages, which the new file would not hold:
   *[other] { $n } files animated or with several pages, which the new files would not hold:
}
summary-no-ffmpeg = { $n ->
    [one] 1 video or audio file, and FFmpeg is missing:
   *[other] { $n } videos or audio files, and FFmpeg is missing:
}
summary-not-autocad = { $n ->
    [one] 1 drawing that is not an AutoCAD drawing (DWG or DXF):
   *[other] { $n } drawings that are not AutoCAD drawings (DWG or DXF):
}
summary-would-lose = { $n ->
    [one] 1 drawing that would lose a part in the other format:
   *[other] { $n } drawings that would lose a part in the other format:
}
summary-too-large = { $n ->
    [one] 1 picture too large for the memory of this computer, left out:
   *[other] { $n } pictures too large for the memory of this computer, left out:
}
summary-meta-left-out = { $n ->
    [one] **1 file** saved without part of its metadata, because it is too large for a JPEG file.
   *[other] **{ $n } files** saved without part of their metadata, because it is too large for a JPEG file.
}
summary-meta-group = { $n ->
    [one] 1 file saved without the metadata in brackets, because it is too large for a JPEG file:
   *[other] { $n } files saved without the metadata in brackets, because it is too large for a JPEG file:
}
summary-errors = { $n ->
    [one] 1 error:
   *[other] { $n } errors:
}
summary-no-preview = { $n ->
    [one] 1 copy saved without the camera preview, because it could not be written:
   *[other] { $n } copies saved without the camera previews, because they could not be written:
}
summary-open-folder = Open the folder
summary-open-covered = Close the page in front first: the thumbnails are under it.
drawing-title = Export drawing
drawing-start = Export
drawing-side = Long side
drawing-pixels = pixels
drawing-side-hint = The drawing is drawn again at this size, so it stays sharp even when large.
drawing-svg-hint = A drawing that stays sharp at any zoom, which browsers and graphics programs open. Layers and blocks are not kept.
drawing-cad-hint = The whole drawing in the other AutoCAD format, in the version it was saved with: layers, blocks, layouts and styles. Each file is read back and compared with the original: if anything would be lost, it is not saved and the summary says so.
batch-skip-format = { $detail }: EyeCanSee cannot write its format; choose another format.
batch-skip-same-size = { $detail }: already of this size, or smaller and not enlarged; no copy made.
batch-skip-too-large = { $detail }: it would need more than half of the memory of this computer; left out.
batch-skip-inside = { $detail }: animated or with several pages; the new file would have only the first one.
batch-failed = { $detail }
batch-turn = Rotate and flip
batch-turn-none = No rotation
batch-turn-cw = 90° clockwise
batch-turn-180 = 180°
batch-turn-ccw = 90° anticlockwise
batch-turn-order = First the flips, then the rotation.
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
opt-tab-editor = Editor
opt-tab-resize = Resize
opt-resize-intro = The sizes of Resize, in the right-click menu of the thumbnails and in Tools. Each one makes copies next to the originals, in their format and with their metadata, the size in the name ("photo (1920 px).jpg").
opt-resize-name = Name
opt-resize-remove = Remove
opt-resize-add = Add
opt-resize-reset = Back to the first ones
opt-edit-after = After saving:
opt-edit-after-stay = Stay in the editor
opt-edit-after-back = Go back to where you were (thumbnails or viewer)
opt-edit-after-grid = Go back to the thumbnails
opt-edit-after-hint = After Save copy, Update copy or Overwrite the original…: the editor stays open to go on editing; or it closes once the file is written and you go back to where you opened it (the thumbnails with the saved image selected, or the viewer showing it), or always to the thumbnails with the saved image selected.
opt-edit-main = Main save button (also Ctrl + S):
opt-edit-main-overwrite = Overwrite the original
opt-edit-main-hint = The other one stays in the menu of the arrow next to it. Where the original cannot be overwritten (a new image, HEIC, RAW…) the main button is Save copy.
opt-engine = Video engine (libmpv) and FFmpeg to use
opt-engine-hint = Videos are played by libmpv and converted by FFmpeg. Both can come from the system (on Linux the distribution's packages, better integrated with the graphics card; on Windows a copy next to the program) or be downloaded once by EyeCanSee. EyeCanSee needs at least libmpv 1.109 (mpv 0.33) and FFmpeg 4.3. Automatic uses the system's when it is recent enough and plays H.264 and HEVC (the phones' videos), otherwise the downloaded one; for FFmpeg also the downloaded one when it leaves more of the work to the graphics card.
opt-engine-system-recent = Automatic: the best for this computer
opt-engine-system = Always the system's, whatever its version
opt-engine-downloaded = Always the downloaded one
opt-engine-restart = A change applies the next time EyeCanSee starts.
opt-engine-mpv = Video engine now: { $what }
opt-engine-ffmpeg = FFmpeg now: { $what }
engine-system = the system's, { $version }
engine-downloaded = the downloaded one, { $version }
engine-missing = none found; it is downloaded when a video needs it
engine-too-old = the system's is too old ({ $found }, at least { $required } is needed); the download is offered when a video needs it
engine-lacks = the system's ({ $found }) cannot read { $missing }, the formats phones record in; the download is offered when a video needs it
engine-system-only = none found, and only the system's is allowed
opt-show-secs = Each image stays for
opt-show-fade = Passage between images
opt-show-loop = Start again after the last image
fade-none = None (straight cut)
fade-cross = Crossfade
fade-black = Fade through black
fade-slide = Slide
fade-zoom = Zoom and fade
show-hint = Esc, double click: end | Space: pause | ← →, Page Up, Page Down: previous and next image
show-paused = Paused (Space to go on)
show-skipped = File skipped: { $name } (it cannot be opened)
show-none-readable = Slideshow stopped: none of its files can be opened.
menu-new-folder = New folder…
menu-new-folder-in = New folder inside…
key-new-folder = Ctrl + Shift + N
key-batch = Ctrl + B
key-duplicates = Ctrl + D
key-bookmarks = Ctrl + Shift + D
menu-bookmarks = Bookmarks
bm-title = Bookmarks
bm-empty = No bookmarks yet.
bm-empty-configure = No bookmarks yet: open a folder and click the star next to the address bar.
bm-add-folder = + Add current folder
bm-add-image = + Add this image
bm-already = Already among the bookmarks.
bm-one-only = Bookmarks are added one folder at a time.
bm-star-folder = Add this folder to the bookmarks
bm-star-image = Add this image to the bookmarks
bm-configure = Configure…
bm-added = "{ $name }" added to the bookmarks.
bm-removed = "{ $name }" removed from the bookmarks.
bm-name-hint = Name
bm-edit = Change name and address
bm-address = Address
bm-done = Done
bm-up = Move up
bm-down = Move down
bm-remove = Remove from the bookmarks
bm-file = Saved in { $path }.
bm-missing-place = **{ $name }** cannot be opened: { $address } does not exist any more, or it is on a disk that is not connected.

    The bookmark stays: connect the disk and choose it again, or remove it in **Bookmarks > Configure…**.
bm-missing-phone = **{ $name }** cannot be opened: the phone of { $address } is not connected.

    The bookmark stays: connect the phone, choose "File transfer" on it and choose the bookmark again, or remove it in **Bookmarks > Configure…**.
tb-more = More operations
tb-off-nothing = Select a file or a folder first.
tb-off-viewer = Not in the viewer: go back to the grid.
tb-off-paste = Nothing to paste: Copy or Cut first.
tb-off-paste-here = Nothing can be pasted here (a trash, the list of a phone's storages, or a copy already under way).
tb-off-folder = No folder can be made here (a trash, the list of a phone's storages).
tb-off-no-picture = Select one or more pictures first.
tb-off-save = Nothing to save: rotate the image in the viewer first.
tb-off-one-picture = Select one picture first.
tb-off-busy = A conversion is already under way.
tb-off-batch = Select pictures, videos or audio files first.
tb-off-no-folder = Open a folder first.
off-no-picture = No picture among the files chosen.
off-no-video = No video among the files chosen.
off-no-model = No 3D model among the files chosen.
off-no-drawing = No CAD drawing among the files chosen.
off-no-played = No picture or video among the files chosen.
off-no-converted = No picture, video or audio file among the files chosen.
off-copy-meta-no-video = This is for videos: photos already have their metadata inside the file.
off-no-kind = None of the files chosen is of a kind this entry works on.
off-edit-picture = Edit image and Rotate and crop work on pictures only.
off-edit-video = Videos are rotated with Rotate 90° and Rotate 180°, not in the editor: Edit image and Rotate and crop work on pictures only.
off-video-no-rotation = Only MP4, MOV, MKV and WebM videos keep a rotation without being encoded again: rotate the others with Convert…
off-video-convert = AVI, MPEG and the other videos that keep no rotation are flipped and rotated with Convert…
off-no-turned = Select one or more pictures or videos first.
off-turn-kind = Only pictures and videos can be rotated.
off-flip-kind = Only pictures can be flipped.
skip-not-picture = { $detail }: not a picture.
skip-not-video = { $detail }: not a video.
skip-not-drawing = { $detail }: not a CAD drawing.
skip-not-played = { $detail }: neither a picture nor a video.
skip-not-converted = { $detail }: not a picture, a video or an audio file.
skip-folder = { $detail }: a folder.
tb-off-trash = Not in the trash.
off-empty = This folder is empty.
off-top = This is the top folder already.
off-not-viewer = Only in the viewer.
off-no-show = No picture or video in this folder.
off-no-file = Select one or more files first.
off-save-model = A 3D model is not saved over its file: Save as… makes a picture of it, Export as a 3D model a copy in another 3D format.
off-save-drawing = A CAD drawing is not saved over its file: Save as… makes a picture of the layout shown, Export drawing… a copy as a picture, SVG, DWG or DXF.
off-save-played = Nothing to save: rotate the video first; audio files are not changed in the viewer.
off-save-as-played = Save as… is for pictures and for the frames of videos: audio files are converted with Convert… in the grid.
off-save-frame-wait = The frame can be saved once the video has opened in the viewer.
off-trash-clip = Items in the trash cannot be cut or copied: restore them first.
off-trash-rename = Items in the trash cannot be renamed: restore them first.
off-trash-change = A file in the trash cannot be changed or saved: restore it first.
off-restore = Select an item of the trash first.
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
batch-video-audio = Keep the audio
batch-video-meta = Keep the metadata (date, place, camera)
batch-video-checking = Checking which encoders work on this computer…
batch-video-need-ffmpeg = Videos are converted by FFmpeg, which is not on this computer. It is downloaded once ({ $size }) into the app's data folder.
batch-video-download = Download FFmpeg
batch-video-update = Update FFmpeg
batch-video-better-ffmpeg = Here the system's FFmpeg leaves part of the graphics card's work to the processor. The one EyeCanSee downloads ({ $size }) keeps it on the card.
batch-video-download-failed = Download failed: { $detail }
batch-video-old-ffmpeg = The FFmpeg on this computer is too old: { $found }, while EyeCanSee needs at least { $required }. EyeCanSee can download its own, once ({ $size }), into the app's data folder.
batch-video-lacks-ffmpeg = The FFmpeg on this computer ({ $found }) cannot read { $missing }, the formats phones record in. EyeCanSee can download its own, which reads them, once ({ $size }), into the app's data folder.
ffmpeg-ask-title = FFmpeg needed
turn-video-need-ffmpeg = A turned video is saved by FFmpeg, which EyeCanSee has not got here yet.
    It is downloaded once ({ $size }) into the app's data folder; then the saving goes on.
batch-video-system-only = FFmpeg is not on this computer, and the Options say to use the system's only.
    Install it, or change the choice under Tools > Options > Video.
batch-video-not-ready = Choose how to convert the videos.
batch-skip-video = { $detail }: a video, and FFmpeg is missing.
batch-skip-audio = { $detail }: an audio file, and FFmpeg is missing.
batch-sound = Audio files
batch-sound-format = Format
batch-sound-keeps-cover = The tags (title, artist, album…) and the cover are kept.
batch-sound-keeps-tags = The tags (title, artist, album…) are kept; this format has no room for the cover.
batch-sound-not-ready = Choose how to convert the audio files.
drawing-skip-not-autocad = { $detail }: not an AutoCAD drawing (DWG or DXF), so it cannot be saved as one.
drawing-skip-would-lose = { $detail }: in the other format part of the drawing would be lost, so it was not saved.
# What a drawing would lose in the other format, in brackets after its name (`cad_convert::said`).
drawing-lost-totals = { $before } → { $after } elements, { $layouts_before } → { $layouts_after } layouts
drawing-lost-layout = layout { $name }: { $before } → { $after } elements
drawing-lost-layers = { $n ->
    [one] layer { $names }
   *[other] layers { $names }
}
batch-no-audio = { $detail }: the video has no audio.

video-codec-h264 = Plays everywhere: phones, TVs, browsers, old computers. Larger files than HEVC and AV1.
video-codec-hevc = About half the size of H.264 at the same quality; plays on recent phones, TVs and computers.
video-codec-av1 = The smallest files, free of royalties; plays on recent browsers, phones and TVs, not on older devices.
video-codec-vp9 = Free of royalties, for the web (YouTube uses it); smaller than H.264, a little larger than AV1.
video-codec-vp8 = Older web format: only for compatibility with old browsers.
video-codec-mpeg4 = Old format (DivX, Xvid era): for old players only. Large files.
video-codec-prores = For editing programs (Final Cut, DaVinci Resolve): best quality, very large files.
video-codec-copy = Video and audio copied as they are, in seconds and without losing quality; the metadata are written (a dashcam's position too). The rotation and the size do not apply.
video-codec-gif = Animated image: no audio, 256 colours, very large files. Only for short clips.
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
frames-frame = Frame { $n } / { $total }
frames-page = Page { $n } / { $total }
frames-page-prev = Previous page (Page Up)
frames-page-next = Next page (Page Down)
frames-too-large = Too large to play here: only the first picture is shown
motion-play = Play motion (Space)

## Tips at startup (issue #113)
opt-tips = Show a tip at startup
opt-tips-hint = Once a day, when EyeCanSee starts, a window shows one thing it can do, with Back and Next to see the others.
tip-off = Don't show at startup
tip-off-hint = The tips will not appear again. To have them back: Tools > Options > General > Show a tip at startup.
tip-back = Back
tip-next = Next
tip-count = Tip { $n } of { $of }
tip-guide = Read more in the guide
tip-welcome-title = Welcome to EyeCanSee
tip-welcome-text =
    EyeCanSee shows the pictures, videos, 3D models and CAD drawings of a folder as thumbnails, and opens them one at a time. It works the same way on this computer, on disks and USB sticks, on Android phones and on servers.

    On the left, the side panel with your disks, phones and servers; in the middle, the folder as thumbnails; under it, the information about the selected file. **Next** shows the other tips.
tip-help-title = F1 opens the guide where you are
tip-help-text =
    Press **F1** at any time: the guide opens on the page about what you are doing, the editor, a video, a phone, a window. Press **F1** again to close it.

    The field at the top left of the guide searches every page.
tip-right-click-title = Right-click anything
tip-right-click-text =
    A right click on a thumbnail, a folder, an empty space of the grid or the picture in the viewer opens a menu with what you can do with it.

    An entry that is greyed out says why when you point at it, as do the toolbar buttons.
tip-phone-title = Photos on an Android phone
tip-phone-text =
    1. Connect the phone with its USB cable and unlock it.
    2. In the phone's USB notification choose **File transfer**.
    3. The phone appears at the top left, under Devices: click it.

    You can view, copy, convert and delete its photos and videos without another program.
tip-servers-title = A NAS or a server
tip-servers-text = EyeCanSee opens folders on FTP, FTPS and SFTP servers like the folders of the computer. Type the address in the address bar, for example `sftp://anna@nas.lan/Photos`, or save the server once in **File > Server manager…**: it then opens with one click from the panel on the left.
tip-viewer-title = Looking at pictures
tip-viewer-text =
    Double-click a thumbnail, or press **Enter**: the picture fills the window.
    - **→** and **←** go to the next and the previous one.
    - The mouse wheel zooms in and out.
    - **Esc** or a double click goes back to the grid.
    - **F11** shows only the picture, at full screen.
tip-video-title = Videos and audio
tip-video-text =
    Videos and audio files play in the viewer.
    - **Space** plays and pauses.
    - **Ctrl + ←** and **Ctrl + →** go 5 seconds back and forward.
    - **↑** and **↓** change the volume, **M** turns the audio off and on.
tip-editor-title = Draw on a picture
tip-editor-text =
    { "**" }Image > Edit image…** (**Ctrl + E**) opens the editor: arrows, text, shapes, highlighter, stamps, filters, crop and rotate. Every addition stays movable until you save.

    { "**" }Save copy** writes a new file beside the original, which stays as it was.
tip-cover-title = Hide a number plate or an address
tip-cover-text =
    In the editor, choose **Cover** (key **B**) and drag over what must be hidden. In the saved file nothing of what was under the box is left.

    Blur and Pixelate are not enough for private text or numbers: they can still be read or worked back.
tip-resize-title = Smaller copies in one click
tip-resize-text =
    Select the photos, right-click them and choose **Resize**: Small, Medium, Large or Half. The copies go beside the originals, with their metadata, the size in their names.

    { "**" }Convert…** (**Ctrl + B**) changes the format of many pictures at once.
tip-convert-video-title = Convert videos
tip-convert-video-text = Videos go through the same **Convert…** window as pictures: another format, a smaller size, rotated, or a short clip as a GIF. The right-click menu also has **Copy without audio** and **Save the audio to a file**. The work is done by FFmpeg: if the computer does not have it, the window offers to download it.
tip-bookmarks-title = Bookmarks
tip-bookmarks-text = The **star** next to the address bar turns the folder open into a bookmark: a folder of the computer, of a phone or of a server. The **Bookmarks** drop-down beside it, or **Ctrl + Shift + D**, takes you back there with one click.
tip-metadata-title = Where and when a photo was taken
tip-metadata-text =
    Under the grid, the metadata panel shows the camera, the date the photo was taken, the exposure and the place. **Open on the map** shows the place on OpenStreetMap.

    Each box closes with its ✕ and comes back from **View > Metadata boxes**.
tip-models-title = 3D models and CAD drawings
tip-models-text =
    3D models (STL, OBJ, glTF, FBX, IFC, STEP and more) and CAD drawings (DWG, DXF) appear in the grid with a picture of them, and open in the viewer.

    Drag or use the arrows to rotate a model, the mouse wheel to zoom; a drawing moves with a drag, and **Ctrl + L** turns its layers on and off. **M** measures both.
tip-duplicates-title = Find duplicates
tip-duplicates-text = **Tools > Find duplicates…** (**Ctrl + D**) finds the files that are exactly the same, also under another name or in another folder, on the computer, a disk, a phone or a server. You choose the copies to delete; one always stays.
tip-trash-title = Deleted by mistake?
tip-trash-text = Files of this computer go to the system trash. Phones and servers have no trash EyeCanSee can reach, so EyeCanSee keeps one of its own in each folder: a strip at the top of the folder says so, with **Open the trash**, where you can restore them.
tip-details-title = More under the thumbnails
tip-details-text = Under each thumbnail EyeCanSee writes the name, the size in pixels, the weight of the file, the date and, for videos, the duration. Choose what to see in **Tools > Options… > Thumbnails > Under the thumbnails**.
tip-slideshow-title = Slideshow
tip-slideshow-text =
    { "**" }View > Slideshow** shows the pictures and videos of the folder one after the other at full screen; with several files selected, only those.

    { "**" }Space** pauses, the arrows move at once, **Esc** ends it. The time and the passage between pictures are in **Tools > Options… > Slideshow**.
tip-addons-title = Add-ons
tip-addons-text =
    { "**" }Tools > Options… > Add-ons** lists the parts of EyeCanSee that are downloaded only if you want them: the video engine, FFmpeg to convert videos, the CAD engine.

    Each one says what it is for, its size and whether it is already on this computer; **Download** or **Update** gets it.
pano-flat = Flat picture
pano-flat-hint = Shows the panorama flat, like any other photo (P).
pano-sphere = 360° view
pano-sphere-hint = Shows the panorama from the inside: drag to look around (P).
tip-pano-title = 360° panoramas
tip-pano-text =
    A 360° panorama, from a drone or a 360° camera, opens in the viewer seen from the inside: drag to look around, zoom with the wheel or **+** and **−**.

    { "**" }P**, or **Flat picture** at the bottom left, shows it flat; **P** again brings back the 360° view.
