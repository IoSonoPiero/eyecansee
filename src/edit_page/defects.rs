//! Defects of the editor found for 0.78.0 (audit 5.6, 5.8, 5.15, #174, #310), reproduced on the
//! whole app without a screen (`crate::audit_family_1::Harness`): mouse and keys as the window
//! gives them, each test in a folder of its own (`own_folder`).

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};

use super::Tool;
use crate::edit::geom::Kind;
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
    editor(&mut h, "a.png", Tool::Draw(Kind::Text));
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

/// A drag of the left button from `from` to `to`, let go there.
fn drag(h: &mut Harness, from: Pos2, to: Pos2) {
    press_and_move(h, from, to - from);
    h.frame(vec![button(to, false)]);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
}

/// The middle pixel of the preview of the first lens, if made.
fn lens_middle(h: &Harness) -> Option<[u8; 4]> {
    let w = h.app.editor.as_ref().unwrap();
    w.doc.objects.iter().find_map(|o| match &o.shape {
        crate::edit::doc::Shape::Lens { preview, .. } => preview.0.as_ref().map(|(_, _, p)| {
            let img = p.image();
            img.get_pixel(img.width() / 2, img.height() / 2).0
        }),
        _ => None,
    })
}

/// Audit 5.8 (L3 of 23/11): a lens, a cover in the middle of it (the lens shows it enlarged), an
/// arrow elsewhere, Ctrl+Z (the arrow goes). The step of the cover was kept in the frame the
/// mouse let go of it, before the lens was made again with it: after Undo the lens showed what
/// the cover hides, until a cover or a filter changed. The file saved was right.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_8_lens_after_undo_shows_what_a_cover_hides() {
    if own_folder("edit_page::defects::audit_5_8_lens_after_undo_shows_what_a_cover_hides") {
        return;
    }
    let mut h = Harness::new(&white("five-eight"));
    editor(&mut h, "a.png", Tool::Draw(Kind::Lens));
    drag(&mut h, MIDDLE - vec2(80.0, 80.0), MIDDLE + vec2(80.0, 80.0));
    assert_eq!(lens_middle(&h).map(|p| p[0]), Some(250), "the lens made, on the white photo");
    h.app.editor.as_mut().unwrap().choose(Tool::Draw(Kind::Cover));
    drag(&mut h, MIDDLE - vec2(15.0, 15.0), MIDDLE + vec2(15.0, 15.0));
    assert_eq!(lens_middle(&h).map(|p| p[0]), Some(0), "the cover enlarged in the lens");
    h.app.editor.as_mut().unwrap().choose(Tool::Draw(Kind::Arrow));
    drag(&mut h, MIDDLE + vec2(250.0, 150.0), MIDDLE + vec2(320.0, 200.0));
    h.key_with(Key::Z, Modifiers::COMMAND);
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let w = h.app.editor.as_ref().unwrap();
    assert_eq!(w.doc.objects.len(), 2, "the arrow undone, the lens and the cover left");
    assert_eq!(lens_middle(&h).map(|p| p[0]), Some(0), "after Undo the lens shows what the cover hides");
}

/// A cover box in the middle of the picture, red.
fn red_cover(h: &mut Harness) {
    use crate::edit::doc::{Area, Obj, Pt, Shape, Style};
    let w = h.app.editor.as_mut().unwrap();
    w.doc.objects.push(Obj { shape: Shape::Cover(Area { centre: Pt::new(0.0, 0.0), w: 40.0, h: 30.0, rot: 0.0 }), style: Style { color: [255, 0, 0], ..Style::default() } });
}

/// The buttons of the bottom row of the window on top (a question), from the left.
fn question_buttons(h: &Harness) -> Vec<egui::Rect> {
    // The buttons, not the shade over the window behind it nor the lines of text.
    let rects: Vec<egui::Rect> = h.ctx.viewport(|v| {
        let over = v.prev_pass.widgets.layers().filter(|(l, _)| l.order == egui::Order::Foreground).flat_map(|(_, w)| w.iter());
        over.filter(|w| w.sense.is_focusable() && w.rect.width() < 600.0).map(|w| w.rect).collect()
    });
    let bottom = rects.iter().map(|r| r.bottom()).fold(f32::NEG_INFINITY, f32::max);
    let mut row: Vec<egui::Rect> = rects.into_iter().filter(|r| (r.bottom() - bottom).abs() < 2.0).collect();
    row.sort_by(|a, b| a.left().total_cmp(&b.left()));
    row
}

/// Audit 5.15 (a) (L4 of 23/11): closing with changes not saved while the save before is still
/// being sent (a phone, a server). Save… in the question did nothing: the question closed, the
/// page stayed, nothing said why. Now Save… is greyed out there while the save before is sent,
/// with the reason, as the main button.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_15_a_save_on_closing_while_the_last_save_is_sent() {
    if own_folder("edit_page::defects::audit_5_15_a_save_on_closing_while_the_last_save_is_sent") {
        return;
    }
    let mut h = Harness::new(&white("five-fifteen-a"));
    editor(&mut h, "a.png", Tool::Select);
    red_cover(&mut h);
    h.key_with(Key::S, Modifiers::COMMAND);
    h.until(|app| app.editor.as_ref().is_some_and(|w| w.ready.is_some()), "the confirmation");
    h.key(crate::audit_family_1::enter());
    h.until(|app| app.saving.is_empty() && app.editor.as_ref().is_some_and(|w| w.ready.is_none() && w.copy.done.is_some()), "the copy written");
    // The copy written is still being sent, as `kept::send` keeps it in `saving` until it is there.
    let copy = h.app.editor.as_ref().unwrap().copy.done.as_ref().unwrap().path.clone();
    h.app.saving.insert(copy);
    red_cover(&mut h);
    h.key(Key::Escape);
    h.frame(Vec::new());
    h.frame(Vec::new());
    assert!(h.app.editor.as_ref().unwrap().leaving, "the question on closing");
    let buttons = question_buttons(&h);
    assert_eq!(buttons.len(), 4, "Save…, Save as…, Don't save, Cancel: {buttons:?}");
    click(&mut h, buttons[0].center());
    let w = h.app.editor.as_ref().unwrap();
    eprintln!("after Save…: question open {}, closing {}, preparing {}, note {:?}", w.leaving, w.closing, w.preparing, w.note);
    assert!(w.leaving || w.closing || w.preparing || w.note.is_some(), "Save… did nothing and nothing says why");
}

/// The error bar says the edited image could not be made (`RESULT_FAILED`), in the app's language.
fn said_failed(h: &Harness) -> bool {
    let sentence = h.app.i18n.t_args(super::RESULT_FAILED, &[("detail", "")]);
    let head = sentence.trim_end().trim_end_matches(':');
    h.app.error.as_ref().is_some_and(|e| e.starts_with(head))
}

/// Audit 5.15 (c): a fault while the result is made for Copy (a panic in its thread) left
/// `copying` on: the Copy button greyed out with its wheel for ever, nothing said. Now the error
/// bar says it, and Copy works again.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_15_c_copy_that_fails_says_it() {
    if own_folder("edit_page::defects::audit_5_15_c_copy_that_fails_says_it") {
        return;
    }
    let mut h = Harness::new(&white("five-fifteen-c"));
    editor(&mut h, "a.png", Tool::Select);
    red_cover(&mut h);
    crate::edit::save::FAULT.store(true, std::sync::atomic::Ordering::Relaxed);
    h.frame(vec![Event::Copy]);
    assert!(h.app.editor.as_ref().unwrap().copying, "the copy being made");
    h.until(|app| app.editor.as_ref().is_some_and(|w| !w.copying), "the copy over (Copy greyed out for ever)");
    assert!(said_failed(&h), "nothing says the copy failed: {:?}", h.app.error);
    assert!(!h.app.editor.as_ref().unwrap().copying, "Copy works again");
}

/// Audit 5.15 (b): a fault while the result is made for Save as… (a panic in its job) left the
/// page of Save as… loading for ever, nothing said. Now that page closes, the error bar says
/// why, and the editor is back with its changes.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_5_15_b_save_as_that_fails_says_it() {
    if own_folder("edit_page::defects::audit_5_15_b_save_as_that_fails_says_it") {
        return;
    }
    let mut h = Harness::new(&white("five-fifteen-b"));
    editor(&mut h, "a.png", Tool::Select);
    red_cover(&mut h);
    crate::edit::save::FAULT.store(true, std::sync::atomic::Ordering::Relaxed);
    h.key_with(Key::S, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(h.app.convert.is_some(), "Save as… open");
    h.until(|app| app.convert.is_none() || app.convert.as_ref().is_some_and(|c| c.read().is_some()), "Save as… left loading for ever");
    assert!(h.app.convert.is_none() && said_failed(&h), "the page closed, the error said: {:?}", h.app.error);
    let w = h.app.editor.as_ref().expect("the editor back");
    assert_eq!(w.doc.objects.len(), 1, "its changes kept");
}

/// The field where the letters are typed on the image, if drawn in the last frame.
fn typing_field(h: &Harness) -> Option<egui::Rect> {
    h.ctx.read_response(egui::Id::unique(super::typing::ON_IMAGE)).map(|r| r.rect)
}

/// A font with fixed widths installed here, if any: its "i" as wide as its "M", unlike the
/// interface's font.
fn fixed_width_font() -> Option<String> {
    crate::decode::font_families().iter().find(|f| f.contains("Mono")).cloned()
}

/// How much room the letters of `text` in `font` take as drawn (their advances, as
/// `edit::render` lays them out), for letters of height 1.
fn drawn_width(text: &str, font: &str) -> f32 {
    use resvg::usvg;
    let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><text font-family="'{font}', sans-serif" font-size="100">{text}</text></svg>"#);
    let tree = usvg::Tree::from_str(&svg, &crate::decode::usvg_options()).unwrap();
    tree.root().abs_bounding_box().width() / 100.0
}

/// #310, first part: the letters being typed are in the text's font, as wide as they will be
/// drawn. Until 0.77.0 the field showed them in the interface's font: in a font with fixed
/// widths, "iiiiiiii" typed took a third of the room it takes once drawn.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn typed_letters_in_the_texts_font() {
    if own_folder("edit_page::defects::typed_letters_in_the_texts_font") {
        return;
    }
    let Some(font) = fixed_width_font() else {
        eprintln!("no font with fixed widths on this system: nothing to compare");
        return;
    };
    let mut h = Harness::new(&white("three-ten-font"));
    editor(&mut h, "a.png", Tool::Draw(Kind::Text));
    h.app.editor.as_mut().unwrap().text_font = Some(font.clone());
    click(&mut h, MIDDLE);
    typed(&mut h, "iiiiiiii");
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let w = h.app.editor.as_ref().unwrap();
    assert_eq!(w.editing, Some(0), "the text being typed on the image");
    let crate::edit::doc::Shape::Text { size, .. } = &w.doc.objects[0].shape else { panic!("a text") };
    let pts = size * w.shown.1;
    // The field is as wide as its letters with a space after them, and one more height of them
    // (`typing::overlay`); in a font with fixed widths the space is a ninth of the nine.
    let field = typing_field(&h).expect("the field on the image").width() - pts;
    let drawn = drawn_width("iiiiiiii", &font) * pts * 9.0 / 8.0;
    eprintln!("{font}: letters in the field {field:.1} points wide, drawn {drawn:.1}");
    assert!((field / drawn - 1.0).abs() < 0.03, "the letters typed are {field:.0} points wide, drawn {drawn:.0}: another font");
}

/// The bar's widget of `typ` with `label`, in the last frame, if any (widget infos: debug only).
#[cfg(debug_assertions)]
fn widget(h: &Harness, typ: egui::Role, label: Option<&str>) -> Option<egui::Rect> {
    h.ctx.viewport(|v| {
        let w = &v.prev_pass.widgets;
        w.layers().flat_map(|(_, ws)| ws.iter()).find(|r| w.info(r.id).is_some_and(|i| i.role == typ && label.is_none_or(|l| i.label.as_deref() == Some(l)))).map(|r| r.rect)
    })
}

/// #310, second part: a font chosen from the list while a text is typed leaves the typing on,
/// its cursor where it was: the next letter goes on. Until 0.77.0 the list took the keys from
/// the field, the typing ended, and the text had to be clicked again.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
// Only in debug builds: the widgets are found by their infos (`Style::debug`), not there in release.
#[cfg(debug_assertions)]
fn choosing_a_font_keeps_the_typing() {
    if own_folder("edit_page::defects::choosing_a_font_keeps_the_typing") {
        return;
    }
    let Some(font) = crate::decode::font_families().first().cloned() else { return };
    let mut h = Harness::new(&white("three-ten-list"));
    // The infos of the widgets, to find the list of fonts and its lines.
    h.ctx.global_style_mut(|s| s.debug.show_interactive_widgets = true);
    editor(&mut h, "a.png", Tool::Draw(Kind::Text));
    click(&mut h, MIDDLE);
    typed(&mut h, "ab");
    let list = widget(&h, egui::Role::ComboBox, None).expect("the list of fonts in the bar");
    click(&mut h, list.center());
    let line = widget(&h, egui::Role::Button, Some(&font)).expect("the font in the list");
    click(&mut h, line.center());
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let w = h.app.editor.as_ref().unwrap();
    assert_eq!(w.doc.objects[0].shape.clone().font_mut().cloned(), Some(Some(font.clone())), "the font chosen");
    assert_eq!(w.editing, Some(0), "choosing a font ended the typing");
    typed(&mut h, "c");
    assert_eq!(texts(&h), ["abc"], "the next letter goes on");
}

/// #310: a text typed, the field of the bar clicked (it takes the keys: the typing on the image
/// waits), another tool and Text again, a new text placed and typed: its letters go into it. The
/// pause of the first typing does not end the second one.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn a_pause_does_not_outlive_its_typing() {
    if own_folder("edit_page::defects::a_pause_does_not_outlive_its_typing") {
        return;
    }
    let mut h = Harness::new(&white("three-ten-pause"));
    editor(&mut h, "a.png", Tool::Draw(Kind::Text));
    click(&mut h, MIDDLE);
    typed(&mut h, "ab");
    let bar = h.ctx.read_response(egui::Id::unique(super::typing::IN_BAR)).expect("the field of the bar").rect;
    click(&mut h, bar.center());
    assert!(h.ctx.text_edit_focused(), "the field of the bar has the keys");
    h.app.editor.as_mut().unwrap().choose(Tool::Draw(Kind::Rect));
    h.frame(Vec::new());
    h.app.editor.as_mut().unwrap().choose(Tool::Draw(Kind::Text));
    h.frame(Vec::new());
    click(&mut h, MIDDLE + vec2(0.0, 120.0));
    typed(&mut h, "cd");
    assert_eq!(texts(&h), ["ab", "cd"], "the second text typed");
}

/// #332: a part chosen with Selection, then a second one being drawn, Esc with the button still
/// held: the part chosen before is back, as in the photo editors (Esc took both away).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn esc_while_a_selection_is_drawn_puts_back_the_one_before() {
    if own_folder("edit_page::defects::esc_while_a_selection_is_drawn_puts_back_the_one_before") {
        return;
    }
    let mut h = Harness::new(&white("three-three-two"));
    editor(&mut h, "a.png", Tool::Marquee);
    drag(&mut h, MIDDLE - vec2(100.0, 80.0), MIDDLE - vec2(20.0, 20.0));
    let before = h.app.editor.as_ref().unwrap().marquee.expect("the part chosen");
    press_and_move(&mut h, MIDDLE + vec2(20.0, 20.0), vec2(90.0, 70.0));
    assert_ne!(h.app.editor.as_ref().unwrap().marquee, Some(before), "the second part being drawn");
    h.key(Key::Escape);
    h.frame(vec![button(MIDDLE + vec2(110.0, 90.0), false)]);
    h.frame(Vec::new());
    assert_eq!(h.app.editor.as_ref().unwrap().marquee, Some(before), "Esc during the drawing: the part chosen before");
}

/// #334: a result that could not be made with no detail of the fault (a panic without a
/// message): the sentence in the error bar ends without a colon, for Copy and for Save as….
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn result_failed_without_a_detail_has_no_colon() {
    if own_folder("edit_page::defects::result_failed_without_a_detail_has_no_colon") {
        return;
    }
    let mut h = Harness::new(&white("three-three-four"));
    editor(&mut h, "a.png", Tool::Select);
    let ctx = h.ctx.clone();
    h.app.on_edit_copy(&ctx, Err(String::new()));
    let copy = h.app.error.take().expect("Copy failed, said");
    h.key_with(Key::S, Modifiers::COMMAND | Modifiers::SHIFT);
    assert!(h.app.convert.is_some(), "Save as… open");
    h.app.convert_original(&ctx, Err(format!("{}\u{1}", super::RESULT_FAILED)));
    h.frame(Vec::new());
    let save_as = h.app.error.take().expect("Save as… failed, said");
    for said in [&copy, &save_as] {
        assert!(!said.trim_end().ends_with(':'), "a colon with nothing after it: {said:?}");
    }
}
