//! Work the editor does in every frame (audit 5.13, 5.14 of 2026-11-17): counted on the whole app
//! without a screen (`crate::audit_family_1::Harness`), each test in a folder of its own
//! (`own_folder`). The frame runs on the test's thread: what the counters of this thread see is
//! what a frame does, not what a thread aside does.

use super::Tool;
use super::defects::{editor, white};
use crate::audit_family_1::{Harness, own_folder};
use crate::edit::doc::{Area, Obj, Pt, Shape, Style};

/// Audit 5.14 (#295): an image pasted from another program (up to 200 megapixels) was turned
/// into the patch's pixels inside the frame: 16 MB for one of 2000 × 2000. The frame asks; the
/// pixels come from a thread aside, and the patch with them.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_14_a_pasted_image_is_made_aside() {
    if own_folder("edit_page::frame_work::audit_5_14_a_pasted_image_is_made_aside") {
        return;
    }
    let mut h = Harness::new(&white("five-fourteen-paste"));
    editor(&mut h, "a.png", Tool::Select);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let img = std::sync::Arc::new(eframe::egui::ColorImage::filled([2000, 2000], eframe::egui::Color32::RED));
    let ((), held) = crate::test_alloc::peak(|| h.frame(vec![eframe::egui::Event::PasteImage(img)]));
    eprintln!("memory the frame of the paste held at most: {} MB", held >> 20);
    assert!(held < 8 << 20, "the pasted image turned into pixels in the frame: {} MB", held >> 20);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.doc.objects.iter().any(|o| matches!(o.shape, Shape::Patch { .. }))), "the patch");
    let w = h.app.editor.as_ref().unwrap();
    assert_eq!((w.selected, w.tool), (Some(w.doc.objects.len() - 1), Tool::Select));
}

/// Audit 5.13: with a document that stays as it is, a frame measured every word of every bubble
/// (`bubble::fit`, an SVG read by `usvg` for each) and copied the whole document twice (the key
/// of the objects' texture, `History::push`), every point of the strokes drawn by hand with it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_13_a_still_document_is_neither_measured_nor_copied() {
    if own_folder("edit_page::frame_work::audit_5_13_a_still_document_is_neither_measured_nor_copied") {
        return;
    }
    let mut h = Harness::new(&white("five-thirteen"));
    editor(&mut h, "a.png", Tool::Select);
    let words: Vec<String> = (0..40).map(|i| format!("word{i}")).collect();
    let style = Style { letters: 12.0, width: 2.0, fill: Some([255; 3]), ..Style::default() };
    let area = Area { centre: Pt::new(0.0, 0.0), w: 200.0, h: 40.0, rot: 0.0 };
    let w = h.app.editor.as_mut().unwrap();
    w.doc.objects.push(Obj { shape: Shape::Bubble { area, tail: Pt::new(0.0, 100.0), text: words.join(" "), font: None }, style });
    w.doc.objects.push(Obj { shape: Shape::Pen((0..10_000).map(|i| Pt::new((i % 300) as f32 - 150.0, (i / 300) as f32 - 15.0)).collect()), style: Style::default() });
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    crate::edit::bubble::MEASURES.with(|n| n.set(0));
    crate::edit::doc::COPIES.with(|n| n.set(0));
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let measures = crate::edit::bubble::MEASURES.with(|n| n.get());
    let copies = crate::edit::doc::COPIES.with(|n| n.get());
    eprintln!("five frames with nothing changed: {measures} lines measured, {copies} copies of the document");
    assert_eq!(measures, 0, "the bubble is measured again in frames where nothing changed");
    assert_eq!(copies, 0, "the document is copied in frames where nothing changed");
}
