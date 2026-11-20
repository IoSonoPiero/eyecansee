//! File > Open from clipboard (issue #57): an image copied in another program (a screenshot, a
//! picture copied from a web page) opened in the editor as a new image, as GIMP's Paste as New
//! Image and Paint.NET's Paste into New Image. Its Save is Save as…, as for File > New image….
//! Ctrl+V in the grid stays for files; no key of its own for now (`Act::OpenClipboard` is ready
//! for one). The clipboard is read with `arboard` itself, away from the window: egui's Ctrl+V
//! hands an image only when the clipboard has no text (egui-winit 0.37), and on X11 the program
//! that copied answers when it can.

use eframe::egui;
use image::RgbaImage;

use crate::App;

/// Why there is no image to open: the Fluent id of the message.
pub const NO_IMAGE: &str = "clip-open-none";

/// The image of the clipboard, `width` × `height` pixels in RGBA as `arboard` gives them; `None`
/// when they do not make one (no pixel, or not as many bytes).
pub fn image(width: usize, height: usize, bytes: Vec<u8>) -> Option<RgbaImage> {
    if width == 0 || height == 0 {
        return None;
    }
    RgbaImage::from_raw(u32::try_from(width).ok()?, u32::try_from(height).ok()?, bytes)
}

/// The clipboard's image, read now (it may wait for the program that copied it).
fn read() -> Result<RgbaImage, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    let data = clipboard.get_image().map_err(|e| e.to_string())?;
    image(data.width, data.height, data.bytes.into_owned()).ok_or_else(|| "not an image".into())
}

impl App {
    /// File > Open from clipboard: on or off with File > New image… (a folder open, where Save
    /// as… proposes the file; not from the viewer).
    pub(crate) fn can_open_clipboard(&self) -> bool {
        self.can_new_image()
    }

    /// File > Open from clipboard: the clipboard read in the background (`Msg::ClipImage`).
    pub(crate) fn open_clipboard(&mut self, ctx: &egui::Context) {
        if !self.can_open_clipboard() {
            return;
        }
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        crate::logging::spawn_named("clipboard-open", move || {
            let r = read();
            if let Err(e) = &r {
                log!("open from clipboard: {e}");
            }
            if tx.send(crate::loader::Msg::ClipImage(r)).is_ok() {
                ctx.request_repaint();
            }
        });
    }

    /// The clipboard read: its image in the editor, named "Pasted image" in the folder open; or a
    /// message saying there is none.
    pub(crate) fn on_clip_image(&mut self, ctx: &egui::Context, r: Result<RgbaImage, String>) {
        match r {
            // Opened from a menu: by the time the clipboard answers, the viewer or another page may be open.
            Ok(img) if self.can_open_clipboard() && self.editor.is_none() => {
                log!("open from clipboard: {} × {}", img.width(), img.height());
                let name = self.i18n.t("clip-open-name");
                self.open_new_image(ctx, image::DynamicImage::ImageRgba8(img), &name);
            }
            Ok(_) => {}
            Err(_) => self.tell("clip-open-title", self.i18n.t(NO_IMAGE)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The pixels arboard gives make the image only when they are as many as its size says.
    #[test]
    fn image_from_its_bytes() {
        let img = image(2, 3, vec![7; 24]).unwrap();
        assert_eq!((img.width(), img.height(), img.get_pixel(1, 2).0), (2, 3, [7; 4]));
        assert!(image(2, 3, vec![7; 23]).is_none());
        assert!(image(0, 3, Vec::new()).is_none());
        assert!(image(3, 0, Vec::new()).is_none());
    }
}
