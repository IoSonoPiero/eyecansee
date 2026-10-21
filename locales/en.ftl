# EyeCanSee, English (reference language).
# Every other locale must contain the same message ids: `cargo test` checks it.

# Name of the language written in the language itself, for the choice in Options.
language-name = English

counter = { $n } of { $total }
folder-empty = This folder contains no images or subfolders
cannot-load = Cannot open { $file }: { $error }
error-unsupported = Unsupported protocol: { $detail }
error-no-host = The address has no host: { $detail }
error-path = Cannot open { $detail }
error-source = Cannot connect to the source: { $detail }
error-detail = { $detail }
error-not-found = Not found: { $detail }
error-device-busy = The device is in use by another application (for example the file manager). Close it, or disconnect and reconnect the device. ({ $detail })
error-no-storage = The device shows no storage: unlock it and choose "File transfer" on it. EyeCanSee retries by itself every 2 seconds. ({ $detail })

menu-file = File
menu-open-location = Open location…
menu-options = Options…
menu-quit = Quit
menu-view = View
menu-refresh = Refresh
menu-back-to-grid = Back to the grid
menu-help = Help
menu-manual = Manual
menu-open-selected = Open
menu-go = Go
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
decimal-separator = .
date-format = %Y-%m-%d %H:%M
date-day-format = %Y-%m-%d

options-title = Options
opt-remember-location = Reopen the last folder at startup
opt-remember-scroll = Remember the scroll position of each folder
opt-remember-scroll-hint = When you go back to a folder you already viewed, the grid starts where you left it instead of from the first thumbnail. It lasts until EyeCanSee is closed.
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
this-computer = This computer
device-mount-hint = Not mounted ({ $device }): click to mount and open it
error-mount = Cannot mount the partition: { $detail }
opt-purge = Empty the thumbnail cache…
opt-purge-confirm = Delete { $n } thumbnails ({ $mb } MB)? Your images are not touched: thumbnails are created again when needed.
opt-purge-yes = Empty
opt-purge-no = Cancel

meta-title = Metadata
meta-collapse = Collapse the panel
meta-expand = Open the panel
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
meta-color-uncalibrated = Uncalibrated
meta-colorspace = Color space
meta-altitude = Altitude
meta-gps-date = GPS date
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
sort-asc = Ascending order
