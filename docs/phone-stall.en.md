# Windows stops talking to the phone

*[Italiano](phone-stall.it.md)*

## What you see

On Windows, while a phone is open in EyeCanSee, every operation on it can stop for about **six minutes**: the folder does not load, a copy does not go on, and EyeCanSee says "Windows has lost the connection with the phone". Other programs that open the phone in those minutes wait too. Then everything goes on by itself.

It happens right after something changes on the phone itself: a photo just taken (the camera saves it in more than one step), a file deleted in the gallery. It was seen with Xiaomi and Samsung phones.

## What to do

- **Unplug the cable and plug it in again**, then choose **File transfer** on the phone: Windows talks to the phone again at once.
- Or wait a few minutes: it recovers by itself.

The phone and its files are fine. Only a file that was being copied **to** the phone when you unplug stays there half written: copy it again.

## Why

On Windows, programs reach a phone through Windows' own driver (Windows Portable Devices, the same one File Explorer uses). It is that driver that stops answering, and nothing a program does without administrator rights can restart it: unplugging the cable is the reset. Microsoft has not documented the cause. Similar stalls of five or six minutes, also in File Explorer, have been reported for years after renaming or deleting files on a phone.

On Linux EyeCanSee talks to the phone directly over USB, without that driver.

## Reports

- [mtp-rs #52](https://github.com/vdavid/mtp-rs/issues/52): the stall after a delete, reported by EyeCanSee to the library it uses for phones.
- [mtp-rs #43](https://github.com/vdavid/mtp-rs/issues/43): a 135 s wait, with File Explorer not responding at the same time.
- [XDA: MTP freeze on renaming](https://xdaforums.com/t/q-mtp-mode-freeze-on-renaming-file-folder.2155438/): File Explorer frozen for 5 min 37 s after a rename.
- [Microsoft Q&A: renaming a file over MTP takes long](https://learn.microsoft.com/en-us/answers/questions/2576612/it-takes-long-time-to-rename-a-file-under-mtp-in-w?forum=windows-all)
