//! Audit of 2026-11-17, family 1, the editor's points (5.1, 5.2, 5.11): tests that reproduce them
//! on the whole app without a screen (`crate::audit_family_1::Harness`). Ignored until fixed; run
//! one at a time with `run.sh` (settings, cache and data of the run, never the user's).

use crate::audit_family_1::{Harness, enter, folder};
use crate::edit::doc::{Area, Obj, Pt, Shape, Style};
use eframe::egui;

fn cover(x: f32, color: [u8; 3]) -> Obj {
    Obj { shape: Shape::Cover(Area { centre: Pt::new(x, 0.0), w: 2.0, h: 2.0, rot: 0.0 }), style: Style { color, ..Style::default() } }
}

/// The editor open on `name` of a folder of this run, the image read.
fn editor(h: &mut Harness, name: &str) {
    let path = h.app.files().iter().find(|f| f.path.ends_with(name)).unwrap().path.clone();
    h.app.open_editor(Some(path), crate::edit_page::Tool::Select);
    h.until(|app| app.editor.as_ref().is_some_and(|w| matches!(w.original, Some(Ok(_))) && w.history.is_some()), "the image in the editor");
}

/// Audit 5.2 (#199): a change made while the save is being prepared (seconds on a phone or a
/// server) is not in the file, and until 0.77.0 counted as saved: Save greyed out, closing asked
/// nothing. Now it stays to save.
#[test]
fn audit_5_2_change_during_the_preparation_counts_as_saved() {
    if crate::audit_family_1::own_folder("edit_page::save::audit::audit_5_2_change_during_the_preparation_counts_as_saved") {
        return;
    }
    let dir = folder("five-two", &["a.png"]);
    let mut h = Harness::new(&dir);
    editor(&mut h, "a.png");
    h.app.editor.as_mut().unwrap().doc.objects.push(cover(-2.0, [255, 0, 0]));
    // Ctrl+S: the preparation starts with the document as it is now.
    h.key_with(egui::Key::S, egui::Modifiers::COMMAND);
    assert!(h.app.editor.as_ref().unwrap().preparing, "the save is being prepared");
    // Meanwhile, before the confirmation: one more object.
    h.app.editor.as_mut().unwrap().doc.objects.push(cover(2.0, [0, 0, 255]));
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.ready.is_some()), "the confirmation");
    let written = h.app.editor.as_ref().unwrap().ready.as_ref().unwrap().0.doc.objects.len();
    h.key(enter());
    h.until(|app| app.saving.is_empty() && app.editor.as_ref().is_some_and(|w| w.ready.is_none()), "the copy written");
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let w = h.app.editor.as_ref().unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    eprintln!("objects on the page {}, in the file {written}; something to save: {}; folder {names:?}", w.doc.objects.len(), w.changed());
    assert!(w.doc.objects.len() > written, "the change made during the preparation is not in the file");
    assert!(w.changed(), "the change not in the file counts as saved");
}

/// The other side of 5.2 (#199): nothing changed while the save is prepared, nothing left to
/// save once it is confirmed.
#[test]
fn audit_5_2_no_change_during_the_preparation_leaves_nothing_to_save() {
    if crate::audit_family_1::own_folder("edit_page::save::audit::audit_5_2_no_change_during_the_preparation_leaves_nothing_to_save") {
        return;
    }
    let dir = folder("five-two-still", &["a.png"]);
    let mut h = Harness::new(&dir);
    editor(&mut h, "a.png");
    h.app.editor.as_mut().unwrap().doc.objects.push(cover(-2.0, [255, 0, 0]));
    h.key_with(egui::Key::S, egui::Modifiers::COMMAND);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.ready.is_some()), "the confirmation");
    h.key(enter());
    h.until(|app| app.saving.is_empty() && app.editor.as_ref().is_some_and(|w| w.ready.is_none()), "the copy written");
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let w = h.app.editor.as_ref().unwrap();
    assert!(!w.changed(), "the document written counts as saved");
    let copies = std::fs::read_dir(&dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("a (")).count();
    assert_eq!(copies, 1, "the copy written");
}

/// Audit 5.1: Save as… on the editor's image, cancelled with Esc before its result is made. The
/// result, which carries no path nor number, goes to the editor (`Msg::Original` with Save in
/// another format closed) and takes the place of the photo read: objects already merged in, so
/// that every following save applies them twice.
#[test]
#[ignore = "audit 5.1: not fixed yet"]
fn audit_5_1_save_as_cancelled_becomes_the_editors_photo() {
    let dir = crate::audit_family_1::guard().join("five-one");
    std::fs::create_dir_all(&dir).unwrap();
    // Large enough that the result of Save as… takes a moment to make.
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(4000, 3000, image::Rgb([128, 128, 128]))).save(dir.join("a.png")).unwrap();
    let mut h = Harness::new(&dir);
    editor(&mut h, "a.png");
    let w = h.app.editor.as_mut().unwrap();
    let read = w.full.clone().unwrap();
    let side = w.doc.objects.len();
    w.doc.objects.push(Obj { shape: Shape::Cover(Area { centre: Pt::new(0.0, 0.0), w: 4000.0, h: 3000.0, rot: 0.0 }), style: Style { color: [255, 0, 0], ..Style::default() } });
    // Ctrl+Shift+S, then Esc at once.
    h.key_with(egui::Key::S, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT);
    assert!(h.app.convert.is_some(), "Save as… open");
    h.key(egui::Key::Escape);
    let closed_before = h.app.convert.is_none() && h.app.editor.as_ref().unwrap().full.as_ref().is_some_and(|f| std::sync::Arc::ptr_eq(f, &read));
    eprintln!("Save as… closed with Esc before its result came: {closed_before}");
    assert!(closed_before, "race lost (the result came first): run again");
    let start = std::time::Instant::now();
    while start.elapsed().as_secs() < 10 && h.app.editor.as_ref().unwrap().full.as_ref().is_some_and(|f| std::sync::Arc::ptr_eq(f, &read)) {
        h.frame(Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let w = h.app.editor.as_ref().unwrap();
    let now = w.full.clone().unwrap();
    let px = now.image.to_rgb8().get_pixel(2000, 1500).0;
    eprintln!("after {:.1?}: the editor's photo replaced {}, its middle pixel {px:?} (read: [128, 128, 128]); objects on the page {} (before the cover {side})", start.elapsed(), !std::sync::Arc::ptr_eq(&now, &read), w.doc.objects.len());
    assert!(std::sync::Arc::ptr_eq(&now, &read), "the photo of the editor is now the result of Save as… (middle pixel {px:?})");
}

/// Audit 5.11 (#201): Overwrite on a Motion Photo (a photo with its video after the picture). The
/// file written is the picture alone, as the guide says; until 0.77.0 the confirmation listed only
/// what the picture loses (encoded again…), nothing on the video, which the viewer's Save refuses
/// to lose (`save-extra`). Now it has a line for the video. The original goes to the trash.
#[test]
fn audit_5_11_overwrite_drops_the_video_without_a_word() {
    if crate::audit_family_1::own_folder("edit_page::save::audit::audit_5_11_overwrite_drops_the_video_without_a_word") {
        return;
    }
    let dir = crate::audit_family_1::guard().join("five-eleven");
    std::fs::create_dir_all(&dir).unwrap();
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(64, 48, |x, y| image::Rgb([(x * 4) as u8, (y * 5) as u8, 90])));
    let mut thumb = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(16, 12, image::Rgb([9, 9, 9]))).write_to(&mut std::io::Cursor::new(&mut thumb), image::ImageFormat::Jpeg).unwrap();
    let bytes = crate::edit::save::tests::motion_photo(&img, &thumb);
    std::fs::write(dir.join("a.jpg"), &bytes).unwrap();
    let mut h = Harness::new(&dir);
    h.app.settings.edit_main_overwrite = true;
    editor(&mut h, "a.jpg");
    h.app.editor.as_mut().unwrap().doc.objects.push(cover(0.0, [255, 0, 0]));
    // Ctrl+S, the main save chosen as Overwrite.
    h.key_with(egui::Key::S, egui::Modifiers::COMMAND);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.ready.is_some()), "the confirmation");
    let p = h.app.editor.as_ref().unwrap().ready.as_ref().unwrap().0.clone();
    assert!(p.over, "Overwrite");
    assert!(p.plan.beyond.video, "the video found");
    let losses = h.app.losses(&p.plan, true);
    let said = ["video", "motion", "depth"].iter().any(|w| losses.to_lowercase().contains(w));
    h.key(enter());
    h.until(|app| app.saving.is_empty() && app.editor.as_ref().is_some_and(|w| w.ready.is_none()), "the file written");
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let now = std::fs::read(dir.join("a.jpg")).unwrap();
    let video_kept = now.windows(8).any(|w| w == b"ftypmp42");
    eprintln!("confirmation lines on what is lost: {losses:?}; the video still in a.jpg: {video_kept} ({} bytes, before {})", now.len(), bytes.len());
    assert!(video_kept || said, "the video went without a word in the confirmation");
}
