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

/// Audit 5.14 (#295): the first frame of the editor loaded every font of the system (the fonts
/// of the bubbles, asked for in every frame even with no bubble), the window still meanwhile.
/// Now they are loaded aside as the page opens, and a frame with no letters to draw does not ask.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_14_the_fonts_are_loaded_aside() {
    if own_folder("edit_page::frame_work::audit_5_14_the_fonts_are_loaded_aside") {
        return;
    }
    let mut h = Harness::new(&white("five-fourteen-fonts"));
    crate::decode::FONTS_ASKED.with(|n| n.set(0));
    editor(&mut h, "a.png", Tool::Select);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let asked = crate::decode::FONTS_ASKED.with(|n| n.get());
    eprintln!("fonts asked for by the frames of a page with no letters: {asked}");
    assert_eq!(asked, 0, "the frame asks for the fonts");
}

/// Audit 5.12: the PNG of a piece pasted from another program (26 ms for 3 megapixels, 108 for 12
/// in --release) was made in the first frame that drew it. Now aside, with its pixels.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_12_the_png_of_a_pasted_piece_is_made_aside() {
    if own_folder("edit_page::frame_work::audit_5_12_the_png_of_a_pasted_piece_is_made_aside") {
        return;
    }
    let mut h = Harness::new(&white("five-twelve-png"));
    editor(&mut h, "a.png", Tool::Select);
    crate::edit::doc::PNGS.with(|n| n.set(0));
    let img = std::sync::Arc::new(eframe::egui::ColorImage::filled([600, 400], eframe::egui::Color32::RED));
    h.frame(vec![eframe::egui::Event::PasteImage(img)]);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.doc.objects.iter().any(|o| matches!(o.shape, Shape::Patch { .. }))), "the patch");
    h.frame(Vec::new());
    assert!(h.app.editor.as_ref().unwrap().objects.is_some(), "the patch drawn");
    let made = crate::edit::doc::PNGS.with(|n| n.get());
    eprintln!("PNGs of the piece made in the frames: {made}");
    assert_eq!(made, 0, "the PNG of the piece made in the frame");
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

/// Audit 5.12, a measure: a pan of the view with a piece of 2000 × 1500 pasted draws the texture
/// of the objects again in every frame (the view and the zoom are in its key, `Drawn`): how many frames of
/// twenty, and how long a frame takes. Run by name, with `--release -- --ignored --nocapture`.
#[test]
#[ignore = "a measure: by name, in --release"]
fn audit_5_12_measure_a_pan_with_a_piece() {
    if own_folder("edit_page::frame_work::audit_5_12_measure_a_pan_with_a_piece") {
        return;
    }
    let mut h = Harness::new(&white("five-twelve"));
    editor(&mut h, "a.png", Tool::Select);
    h.app.editor.as_mut().unwrap().pasted_in(crate::edit::render::tests::photo_like(2000, 1500));
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let seen = |h: &Harness| h.app.editor.as_ref().unwrap().objects.as_ref().map(|(d, _)| (d.seen.at, d.seen.k));
    let (mut drawn, start) = (0, std::time::Instant::now());
    for _ in 0..20 {
        let w = h.app.editor.as_mut().unwrap();
        let (at, k) = w.shown;
        w.view = Some((at + eframe::egui::vec2(2.0, 0.0), k));
        let before = seen(&h);
        h.frame(Vec::new());
        drawn += usize::from(seen(&h) != before);
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / 20.0;
    eprintln!("a pan of 20 frames with a piece of 2000 x 1500 pasted: the objects drawn again in {drawn} frames, {ms:.1} ms a frame");
}
