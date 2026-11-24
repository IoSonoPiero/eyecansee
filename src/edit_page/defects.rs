//! Defects of the editor found for 0.78.0 (audit 5.6, 5.8, 5.15, #174, #310), reproduced on the
//! whole app without a screen (`crate::audit_family_1::Harness`): mouse and keys as the window
//! gives them, each test in a folder of its own (`own_folder`).

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};

use super::Tool;
use crate::audit_family_1::{Harness, own_folder};

/// The editor open on `name`, a picture of the folder of this run, with `tool`, the image read.
pub(super) fn editor(h: &mut Harness, name: &str, tool: Tool) {
    let path = h.app.files().iter().find(|f| f.path.ends_with(name)).unwrap().path.clone();
    h.app.open_editor(Some(path), tool);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.read() && w.history.is_some()), "the image in the editor");
    h.frame(Vec::new());
}

/// A folder of this run with `a.png`, 400 × 300 almost white: a text placed on it with its
/// size at first (a 25th of the long side) is small on the screen.
pub(super) fn white(name: &str) -> std::path::PathBuf {
    let dir = crate::audit_family_1::guard().join(name);
    std::fs::create_dir_all(&dir).unwrap();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(400, 300, image::Rgb([250, 250, 250]))).save(dir.join("a.png")).unwrap();
    dir
}

/// The middle of the window: on the canvas, on the picture fitted in it.
pub(super) const MIDDLE: Pos2 = pos2(640.0, 430.0);

fn button(at: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: at, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

/// A click of the left button at `at`.
pub(super) fn click(h: &mut Harness, at: Pos2) {
    h.frame(vec![Event::PointerMoved(at)]);
    h.frame(vec![button(at, true)]);
    h.frame(vec![button(at, false)]);
    h.frame(Vec::new());
}

/// The left button pressed at `from` and moved by `by`, in a few frames, still held.
fn press_and_move(h: &mut Harness, from: Pos2, by: egui::Vec2) {
    h.frame(vec![Event::PointerMoved(from)]);
    h.frame(vec![button(from, true)]);
    for i in 1..=5 {
        h.frame(vec![Event::PointerMoved(from + by * i as f32 / 5.0)]);
    }
}

fn typed(h: &mut Harness, s: &str) {
    h.frame(vec![Event::Text(s.into())]);
    h.frame(Vec::new());
}

/// The letters of the texts of the document shown.
fn texts(h: &Harness) -> Vec<String> {
    let w = h.app.editor.as_ref().unwrap();
    w.doc.objects.iter().filter_map(|o| o.shape.clone().text_mut().cloned()).collect()
}

/// Audit 5.6 (L2 of 23/11, scenario 2): a text just typed, not yet a step of the history (the
/// field had the keys), then a drag that moves the view and Esc with the button still held. Esc
/// put back the last step of the history: the text went, its index stayed selected, and Del then
/// removed an object that was not there: panic, EyeCanSee closed. Esc stops the drag only.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_6_esc_during_a_pan_keeps_the_text() {
    if own_folder("edit_page::defects::audit_5_6_esc_during_a_pan_keeps_the_text") {
        return;
    }
    let mut h = Harness::new(&white("five-six"));
    editor(&mut h, "a.png", Tool::Draw(crate::edit::geom::Kind::Text));
    click(&mut h, MIDDLE);
    typed(&mut h, "xyz");
    assert_eq!(texts(&h), ["xyz"], "the text typed");
    // Away from the text: the drag moves the view.
    press_and_move(&mut h, MIDDLE + vec2(150.0, 100.0), vec2(60.0, 40.0));
    assert!(matches!(h.app.editor.as_ref().unwrap().hold, Some(super::Hold::Pan { .. })), "the view being moved");
    h.key(Key::Escape);
    h.frame(vec![button(MIDDLE + vec2(210.0, 140.0), false)]);
    h.frame(Vec::new());
    assert_eq!(texts(&h), ["xyz"], "Esc during the drag took the text away");
    h.key(Key::Delete);
    h.frame(Vec::new());
    assert!(h.app.editor.is_some(), "the editor open");
}
