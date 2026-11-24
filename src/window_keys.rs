//! #311: every window, page and question of EyeCanSee goes through with the keys (Piero, 24/11:
//! "Assicurati che TUTTE le finestre mi facciano navigare con i tasti"). Tests only: with nothing
//! focused Tab and Shift+Tab reach the window's controls and go on through them, the arrows move
//! between them, and none of these keys, nor a copy, reaches the grid or the viewer under it.

use eframe::egui::{self, Key, Modifiers};

use crate::App;
use crate::audit_family_1::{Harness, folder, own_folder};

/// Where the keyboard is: the widget and its layer.
fn focused(h: &Harness) -> Option<(egui::Id, egui::LayerId)> {
    let id = h.ctx.memory(|m| m.focused())?;
    Some((id, h.ctx.read_response(id).map(|r| r.layer_id)?))
}

/// The layer of the modal window `id` (`egui::Modal::new(egui::Id::unique(id))`).
pub(crate) fn modal(id: &str) -> impl Fn(egui::LayerId) -> bool {
    let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique(id));
    move |l| l == layer
}

/// What would move under a window: the grid, the folder, the viewer, what is copied.
fn under(a: &App) -> String {
    format!("{:?}", (a.cursor, a.selected, a.focus, a.viewing, &a.dir, a.clip.is_some()))
}

/// #311: the window `open` (its controls on a layer `on` accepts), with nothing focused (as it
/// opens, or after a click on its body): Tab gives the keyboard to one of its controls and goes on
/// through `least` of them at least, Shift+Tab the same the other way; then, from a control that
/// is not a text field, the arrows move the keyboard (`arrows`: the window has two such controls
/// or more). A copy (Ctrl+C) with nothing focused copies nothing of the grid under it. The window
/// stays open and nothing under it moves.
pub(crate) fn tab_goes_through(h: &mut Harness, on: impl Fn(egui::LayerId) -> bool, open: impl Fn(&App) -> bool, least: usize, arrows: bool) {
    let before = under(&h.app);
    tab_and_arrows(h, on, &open, least, arrows);
    h.blur();
    h.frame(vec![egui::Event::Copy]);
    h.frame(Vec::new());
    assert!(open(&h.app), "the window closed");
    assert_eq!(under(&h.app), before, "the keys or the copy reached what is under the window");
}

/// `tab_goes_through` without the copy: for the guide, a window beside the app, where Ctrl+C
/// with nothing focused still copies the files of the grid.
fn tab_and_arrows(h: &mut Harness, on: impl Fn(egui::LayerId) -> bool, open: &impl Fn(&App) -> bool, least: usize, arrows: bool) {
    for _ in 0..3 {
        h.frame(Vec::new());
    }
    let before = under(&h.app);
    for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
        h.blur();
        h.frame(Vec::new());
        let mut seen = std::collections::HashSet::new();
        for i in 1..=6 {
            h.key_with(Key::Tab, modifiers);
            h.frame(Vec::new());
            let now = focused(h);
            assert!(now.is_some_and(|(_, l)| on(l)), "Tab {modifiers:?} pressed {i} times with nothing focused: the keyboard is not in the window ({now:?})");
            seen.extend(now.map(|(id, _)| id));
            assert!(open(&h.app), "Tab {modifiers:?} closed the window");
        }
        assert!(seen.len() >= least, "Tab {modifiers:?} went through {} controls, not {least}", seen.len());
    }
    if arrows {
        // From a control that is not a text field: there the arrows move the cursor in the text.
        for _ in 0..10 {
            if !h.ctx.text_edit_focused() {
                break;
            }
            h.key(Key::Tab);
            h.frame(Vec::new());
        }
        assert!(!h.ctx.text_edit_focused(), "Tab reached no control that is not a text field");
        let start = focused(h);
        let mut moved = false;
        for key in [Key::ArrowDown, Key::ArrowUp, Key::ArrowRight, Key::ArrowLeft] {
            let was = focused(h);
            h.key(key);
            h.frame(Vec::new());
            let now = focused(h);
            assert!(now.is_some_and(|(_, l)| on(l)), "{key:?}: the keyboard left the window ({now:?}, from {start:?})");
            moved |= now != was;
        }
        assert!(moved, "the arrows moved the keyboard nowhere");
    }
    assert!(open(&h.app), "the window closed");
    assert_eq!(under(&h.app), before, "the keys reached what is under the window");
}

/// The folder of a test, with three small pictures, the app open on it and the grid on the first.
fn on_grid(name: &str) -> Harness {
    let mut h = Harness::new(&folder(name, &["a.png", "b.png", "c.png"]));
    (h.app.cursor, h.app.selected, h.app.focus) = (true, 0, crate::Focus::Grid);
    h
}

/// The editor open on a.png, its picture read.
fn in_editor(name: &str) -> Harness {
    let mut h = on_grid(name);
    let a = h.app.files()[0].path.clone();
    h.app.open_editor(Some(a), crate::edit_page::Tool::Select);
    h.until(|a| a.editor.as_ref().is_some_and(crate::edit_page::Window::read), "the editor's picture read");
    h.frame(Vec::new());
    h
}

/// #311: the windows with fields (Open location, Bookmarks > Configure, the master password,
/// Convert, the editor's Resize): with nothing focused Tab did nothing, its keys went to a row of
/// buttons these windows have not. Tab goes through their fields, lists and buttons, as in Server
/// manager.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_through_the_windows_with_fields() {
    if own_folder("window_keys::tab_goes_through_the_windows_with_fields") {
        return;
    }
    let mut h = on_grid("keys-address");
    h.app.open_address = Some(crate::address::AddressForm::new());
    tab_goes_through(&mut h, modal("open-address"), |a| a.open_address.is_some(), 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.open_address.is_none(), "Esc did not close Open location");

    let mut h = on_grid("keys-bookmarks");
    let mark = |name: &str| crate::bookmarks::Bookmark { name: name.into(), address: "/".into() };
    h.app.settings.bookmarks = vec![mark("x"), mark("y")];
    h.app.bookmarks.configuring = true;
    tab_goes_through(&mut h, modal("bookmarks"), |a| a.bookmarks.configuring, 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!h.app.bookmarks.configuring, "Esc did not close Bookmarks");

    let mut h = on_grid("keys-unlock");
    h.app.ask_master(crate::vault_page::Then::Manager);
    tab_goes_through(&mut h, modal("unlock"), |a| a.unlock.is_some(), 2, false);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.unlock.is_none(), "Esc did not close the master password");

    let mut h = on_grid("keys-convert");
    let a = h.app.files()[0].path.clone();
    h.app.open_batch(Some(a));
    tab_goes_through(&mut h, modal("batch-ask"), |a| a.batch.is_some(), 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.batch.is_none(), "Esc did not close Convert");

    let mut h = in_editor("keys-resize");
    h.key_with(Key::I, Modifiers::COMMAND | Modifiers::ALT);
    let resizing = |a: &App| a.editor.as_ref().is_some_and(crate::edit_page::Window::resizing);
    assert!(resizing(&h.app), "Ctrl+Alt+I did not open Resize");
    tab_goes_through(&mut h, modal("edit-resize"), resizing, 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!resizing(&h.app) && h.app.editor.is_some(), "Esc did not close Resize only");
}

/// A page covering the window (Save as, Find duplicates): its controls are on the bottom layer.
fn page(l: egui::LayerId) -> bool {
    l.order == egui::Order::Background
}

/// #311: the pages that cover the window, Save as (opened from the grid, the viewer and the
/// editor) and the results of Find duplicates: with nothing focused Tab went to the grid hidden
/// under them, and Ctrl+C copied its files. Tab goes through the page's controls, the arrows too.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_through_the_pages() {
    if own_folder("window_keys::tab_goes_through_the_pages") {
        return;
    }
    let saving = |a: &App| a.convert.is_some();
    let mut h = on_grid("keys-save-as");
    let a = h.app.files()[0].path.clone();
    h.app.open_convert(Some(a));
    tab_goes_through(&mut h, page, saving, 3, true);

    let mut h = on_grid("keys-save-as-viewer");
    h.app.activate(1);
    h.until(|a| a.zoom_of == Some(1), "b.png shown");
    let b = h.app.files()[1].path.clone();
    h.app.open_convert(Some(b));
    tab_goes_through(&mut h, page, saving, 3, true);

    let mut h = in_editor("keys-save-as-editor");
    h.key_with(Key::S, Modifiers::COMMAND | Modifiers::SHIFT);
    h.frame(Vec::new());
    assert!(saving(&h.app), "Ctrl+Shift+S did not open Save as from the editor");
    tab_goes_through(&mut h, page, saving, 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!saving(&h.app) && h.app.editor.is_some(), "Esc did not go back to the editor");

    // a.png, b.png and c.png are the same picture: one group of three copies.
    let mut h = on_grid("keys-duplicates");
    h.app.open_duplicates(None);
    h.frame(Vec::new());
    h.key(Key::Enter);
    h.until(App::dup_page_shown, "the results of Find duplicates");
    tab_goes_through(&mut h, page, App::dup_page_shown, 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.duplicates.is_none(), "Esc did not close Find duplicates");
}

/// #311: the questions with a control besides their buttons: Tab and the arrows went round the
/// buttons only, and the control could not be reached from the keyboard ("Also in subfolders" of
/// Find duplicates, "Apply to all" of a name taken while pasting, the list of formats of Export
/// as a 3D model, the link of a phone that stalled). Tab goes through it with the buttons, Space
/// or Enter on it ticks the box, opens the list or the link, not the highlighted button. The
/// search under way of Find duplicates: its Cancel is highlighted, Enter presses it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_reaches_the_choices_of_the_questions() {
    if own_folder("window_keys::tab_reaches_the_choices_of_the_questions") {
        return;
    }
    let mut h = on_grid("keys-dup-ask");
    h.app.open_duplicates(None);
    let asked = |a: &App| a.duplicates.as_ref().is_some_and(crate::dup_page::Window::asking);
    tab_goes_through(&mut h, modal("dup-ask"), asked, 3, true);
    // Search highlighted when it opens; the box is above it.
    h.app.duplicates = None;
    h.frame(Vec::new());
    h.app.open_duplicates(None);
    h.frame(Vec::new());
    h.frame(Vec::new());
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.frame(Vec::new());
    h.key(Key::Space);
    h.frame(Vec::new());
    let sub = |a: &App| a.duplicates.as_ref().is_some_and(crate::dup_page::Window::sub);
    assert!(asked(&h.app) && !sub(&h.app), "Space on \"Also in subfolders\" did not untick it, or searched");
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(asked(&h.app) && sub(&h.app), "Enter on \"Also in subfolders\" did not tick it, or searched");

    let mut h = on_grid("keys-dup-progress");
    h.app.open_duplicates(None);
    h.app.duplicates.as_mut().unwrap().search_forever();
    tab_goes_through(&mut h, modal("dup-progress"), |a| a.duplicates.is_some(), 1, false);
    h.blur();
    h.frame(Vec::new());
    h.key(Key::Tab);
    h.key(Key::Enter);
    assert!(h.app.duplicates.is_none(), "Enter on Cancel did not stop the search");

    let mut h = on_grid("keys-paste");
    let d = h.app.dir.clone();
    let conflicts = Some(vec![format!("{d}a.png"), format!("{d}b.png")]);
    h.app.paste_ask = Some(crate::PasteAsk { dir: d.clone(), conflicts: conflicts.clone(), choices: Default::default(), all: false });
    tab_goes_through(&mut h, modal("paste"), |a| a.paste_ask.is_some(), 5, true);
    // Keep both highlighted when it opens; "Apply to all" is above the buttons.
    h.app.paste_ask = None;
    h.frame(Vec::new());
    h.app.paste_ask = Some(crate::PasteAsk { dir: d, conflicts, choices: Default::default(), all: false });
    h.frame(Vec::new());
    h.frame(Vec::new());
    for _ in 0..3 {
        h.key_with(Key::Tab, Modifiers::SHIFT);
    }
    h.key(Key::Space);
    h.frame(Vec::new());
    let ask = h.app.paste_ask.as_ref().expect("Space on \"Apply to all\" answered the question");
    assert!(ask.all && ask.choices.is_empty(), "Space on \"Apply to all\" did not tick it");

    let mut h = Harness::new(&folder("keys-model", &["a.png"]));
    std::fs::write(crate::audit_family_1::guard().join("keys-model/m.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    h.app.refresh();
    h.until(|a| a.files().len() == 2, "m.obj listed");
    let m = h.app.files().iter().find(|f| f.path.ends_with("m.obj")).unwrap().path.clone();
    h.app.open_model_export(Some(m));
    assert!(h.app.model_export.is_some(), "Export as a 3D model did not open");
    tab_goes_through(&mut h, |l| l.order == egui::Order::Middle, |a| a.model_export.is_some(), 3, true);
    // Export highlighted when it opens; the list of formats is above it.
    h.app.model_export = None;
    h.frame(Vec::new());
    let m = h.app.files().iter().find(|f| f.path.ends_with("m.obj")).unwrap().path.clone();
    h.app.open_model_export(Some(m));
    h.frame(Vec::new());
    h.frame(Vec::new());
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Space);
    h.frame(Vec::new());
    assert!(h.app.model_export.is_some() && egui::Popup::is_any_open(&h.ctx), "Space on the list of formats did not open it, or exported");

    let mut h = on_grid("keys-stalled");
    let notice = crate::save_dialogs::Notice::new("t".into(), "x".into());
    h.app.stall_notice = Some(notice.text.clone());
    h.app.notice = Some(notice);
    tab_goes_through(&mut h, modal("notice"), |a| a.notice.is_some(), 2, true);
}

/// #311: the editor's question before saving a copy of a photo with metadata has a list of what
/// to keep: Tab went round Save and Cancel only. It reaches the list, Space opens it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_reaches_the_list_of_the_editor_save() {
    if own_folder("window_keys::tab_reaches_the_list_of_the_editor_save") {
        return;
    }
    let dir = folder("keys-edit-save", &["a.png"]);
    let exif = crate::encode::tests::large_exif(10, 0);
    let photo = crate::encode::Original::with_exif(image::DynamicImage::new_rgb8(8, 6), Some(exif));
    std::fs::write(dir.join("p.jpg"), photo.encode(&crate::encode::Options::default()).unwrap()).unwrap();
    let mut h = Harness::new(&dir);
    h.until(|a| a.files().len() == 2, "p.jpg listed");
    let p = h.app.files().iter().find(|f| f.path.ends_with("p.jpg")).unwrap().path.clone();
    h.app.open_editor(Some(p), crate::edit_page::Tool::Select);
    h.until(|a| a.editor.as_ref().is_some_and(crate::edit_page::Window::read), "the editor's picture read");
    h.key_with(Key::R, Modifiers::COMMAND);
    h.key_with(Key::S, Modifiers::COMMAND);
    let asked = |a: &App| a.editor.as_ref().is_some_and(crate::edit_page::Window::asking);
    h.until(asked, "the question before saving");
    tab_goes_through(&mut h, modal("edit-save"), asked, 3, true);
    // Save highlighted when it opens; the list is above it.
    h.blur();
    h.frame(Vec::new());
    for _ in 0..3 {
        h.key_with(Key::Tab, Modifiers::SHIFT);
        if !egui::Popup::is_any_open(&h.ctx) {
            h.key(Key::Space);
        }
        if egui::Popup::is_any_open(&h.ctx) {
            break;
        }
    }
    assert!(asked(&h.app) && egui::Popup::is_any_open(&h.ctx), "Space did not open the list of what to keep, or answered the question");
}

/// #311: the small Copy of the message windows (the end of a check for updates, a message, the
/// summary of Convert) could not be reached from the keyboard: Tab goes to it after the buttons
/// (Piero, 24/11), and Enter on it copies, the window stays.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_reaches_the_copy_of_the_messages() {
    if own_folder("window_keys::tab_reaches_the_copy_of_the_messages") {
        return;
    }
    let mut h = on_grid("keys-update");
    h.app.update_note = Some("x".into());
    tab_goes_through(&mut h, modal("update"), |a| a.update_note.is_some(), 2, true);
    // Close highlighted when it opens, Copy after it.
    h.app.update_note = None;
    h.frame(Vec::new());
    h.app.update_note = Some("x".into());
    h.frame(Vec::new());
    h.frame(Vec::new());
    h.key(Key::Tab);
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(h.app.update_note.is_some(), "Enter on Copy closed the window");

    let mut h = on_grid("keys-notice");
    h.app.notice = Some(crate::save_dialogs::Notice::new("t".into(), "x".into()));
    tab_goes_through(&mut h, modal("notice"), |a| a.notice.is_some(), 2, true);

    let mut h = on_grid("keys-summary");
    let dir = h.app.dir.clone();
    h.app.batch_summary = Some(crate::batch_summary::Summary { title: "t".into(), text: "x".into(), groups: Vec::new(), open: Some(dir), copy: "x".into() });
    tab_goes_through(&mut h, modal("batch-summary"), |a| a.batch_summary.is_some(), 3, true);
}

/// #311: Help > About had no control reachable from the keyboard: Tab goes through its links
/// (and the copy of the graphics card), Enter on a link opens it and the window stays; with
/// nothing focused Enter closes it, as before, and Esc too (Piero, 24/11).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_through_about() {
    if own_folder("window_keys::tab_goes_through_about") {
        return;
    }
    let mut h = on_grid("keys-about");
    h.app.about = true;
    tab_goes_through(&mut h, |l| l.order == egui::Order::Middle, |a| a.about, 2, true);
    h.blur();
    h.frame(Vec::new());
    h.key(Key::Tab);
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(h.app.about, "Enter on a link closed About");
    h.blur();
    h.frame(Vec::new());
    h.key(Key::Enter);
    assert!(!h.app.about, "Enter with nothing focused did not close About");
    h.app.about = true;
    h.frame(Vec::new());
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!h.app.about, "Esc did not close About");
}

/// #311: the guide (F1) open over the grid or the viewer, with nothing focused: Tab moved the grid
/// under it. It goes into the guide, its search field, contents and page (Piero, 24/11).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_into_the_guide() {
    if own_folder("window_keys::tab_goes_into_the_guide") {
        return;
    }
    // The guide's window is on the layer of its id, over the others (`help::window_id`).
    let guide = modal("help-window");
    let mut h = on_grid("keys-guide");
    h.key(Key::F1);
    assert!(h.app.help.open, "F1 did not open the guide");
    tab_and_arrows(&mut h, &guide, &|a: &App| a.help.open, 3, true);

    let mut h = on_grid("keys-guide-viewer");
    h.app.activate(1);
    h.until(|a| a.zoom_of == Some(1), "b.png shown");
    h.key(Key::F1);
    assert!(h.app.help.open, "F1 did not open the guide");
    tab_and_arrows(&mut h, &guide, &|a: &App| a.help.open && a.viewing == Some(1), 3, true);
}

/// #311: the box of a copy under way (and of Convert under way), in a corner while the program
/// is used: its Cancel could not be reached from the keyboard. It comes in the round of Tab
/// after the folders of the tree, before the grid (Piero, 24/11): Shift+Tab on the first item
/// of the grid goes to it, Tab on it to the grid, Shift+Tab on it to the tree; Enter presses it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn the_box_of_a_copy_is_in_the_round_of_tab() {
    use crate::Focus;
    if own_folder("window_keys::the_box_of_a_copy_is_in_the_round_of_tab") {
        return;
    }
    let mut h = on_grid("keys-copy-box");
    let p = std::sync::Arc::new(crate::transfer::Progress::default());
    h.app.transfer = Some((p.clone(), false, h.app.base.clone(), h.app.dir.clone()));
    h.frame(Vec::new());
    let in_box = |h: &Harness| focused(h).is_some_and(|(_, l)| l.order == egui::Order::Middle);
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.frame(Vec::new());
    assert!(in_box(&h), "Shift+Tab on the first item of the grid did not reach the box");
    h.key(Key::Tab);
    h.frame(Vec::new());
    assert!(!in_box(&h) && h.app.focus == Focus::Grid && (h.app.cursor, h.app.selected) == (true, 0), "Tab on the box did not go to the first item of the grid");
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.frame(Vec::new());
    assert!(in_box(&h), "Shift+Tab did not come back to the box");
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.frame(Vec::new());
    assert!(!in_box(&h) && h.app.focus == Focus::Tree, "Shift+Tab on the box did not go to the tree");
    h.app.focus = Focus::Grid;
    (h.app.cursor, h.app.selected) = (true, 0);
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.frame(Vec::new());
    assert!(in_box(&h), "Shift+Tab did not come back to the box");
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(p.cancel.load(std::sync::atomic::Ordering::Relaxed), "Enter on Cancel did not stop the copy");
}

/// #311: the questions with a row of buttons (`confirm::choose`): deleting, turning pictures of
/// the grid, leaving a turn of the viewer or the changes of the editor not saved, Rename, New
/// folder, saving a turn of the viewer, KDE holding a phone. Tab with nothing focused goes through their buttons, the arrows too, Esc cancels.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_through_the_rows_of_buttons() {
    if own_folder("window_keys::tab_goes_through_the_rows_of_buttons") {
        return;
    }
    let mut h = on_grid("keys-delete");
    h.key(Key::Delete);
    assert!(h.app.delete_confirm.is_some(), "Del did not ask");
    tab_goes_through(&mut h, modal("delete"), |a| a.delete_confirm.is_some(), 2, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.delete_confirm.is_none(), "Esc did not cancel the deletion");

    let mut h = on_grid("keys-batch-turn");
    h.key_with(Key::R, Modifiers::COMMAND);
    assert!(h.app.batch_turn.is_some(), "Ctrl+R on the grid did not ask");
    tab_goes_through(&mut h, modal("batch-turn"), |a| a.batch_turn.is_some(), 2, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.batch_turn.is_none(), "Esc did not cancel the turn");

    let mut h = crate::audit_family_1::turned_middle("keys-turn-leave");
    h.key(Key::Escape);
    h.frame(Vec::new());
    assert!(h.app.after_turn.is_some(), "Esc on a turn not saved did not ask");
    tab_goes_through(&mut h, modal("turn-leave"), |a| a.after_turn.is_some(), 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.after_turn.is_none() && h.app.viewing.is_some(), "Esc did not stay in the viewer");

    let mut h = in_editor("keys-edit-leave");
    h.key_with(Key::R, Modifiers::COMMAND);
    h.key(Key::Escape);
    h.frame(Vec::new());
    let asked = |a: &App| a.editor.as_ref().is_some_and(crate::edit_page::Window::asking);
    assert!(asked(&h.app), "Esc on changes not saved did not ask");
    tab_goes_through(&mut h, modal("rotate-leave"), asked, 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!asked(&h.app) && h.app.editor.is_some(), "Esc did not go back to the editor");

    let mut h = on_grid("keys-rename");
    h.app.ask_rename(None);
    tab_goes_through(&mut h, modal("rename"), |a| a.rename.is_some(), 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.rename.is_none(), "Esc did not cancel Rename");

    let mut h = on_grid("keys-new-folder");
    h.app.ask_new_folder(None);
    tab_goes_through(&mut h, modal("new-folder"), |a| a.new_folder.is_some(), 3, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.new_folder.is_none(), "Esc did not cancel New folder");

    let mut h = crate::audit_family_1::turned_middle("keys-save");
    h.key_with(Key::S, Modifiers::COMMAND);
    h.until(|a| a.save_ready.is_some(), "the question before saving the turn");
    tab_goes_through(&mut h, modal("save"), |a| a.save_ready.is_some(), 2, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.save_ready.is_none() && h.app.turn_unsaved(), "Esc did not cancel the save");

    let mut h = on_grid("keys-kiod");
    h.app.kiod_ask = true;
    tab_goes_through(&mut h, modal("kiod"), |a| a.kiod_ask, 2, true);
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(!h.app.kiod_ask, "Esc did not cancel");
}

/// #311: the editor's page and the viewer, with nothing focused: Tab and Shift+Tab go through
/// their bars and do not stop. The zoom slider of the status bar let the keyboard go when it
/// had it, so Tab was lost there and started again from the first button: what comes after it
/// (the image of the editor, its Keys) could not be reached, and Shift+Tab went round three
/// controls only.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_through_the_editor_and_the_viewer() {
    if own_folder("window_keys::tab_goes_through_the_editor_and_the_viewer") {
        return;
    }
    let mut h = in_editor("keys-editor");
    tab_round(&mut h);
    tab_goes_through(&mut h, page, |a| a.editor.is_some() && a.convert.is_none(), 3, true);

    let mut h = on_grid("keys-viewer");
    h.app.activate(1);
    h.until(|a| a.zoom_of == Some(1), "b.png shown");
    tab_round(&mut h);
    tab_and_arrows(&mut h, page, &|a: &App| a.viewing == Some(1), 3, false);
}

/// Tab, then Shift+Tab, pressed many times from nothing focused: the keyboard is never lost.
fn tab_round(h: &mut Harness) {
    for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
        h.blur();
        h.frame(Vec::new());
        for i in 1..=60 {
            h.key_with(Key::Tab, modifiers);
            h.frame(Vec::new());
            assert!(focused(h).is_some(), "Tab {modifiers:?} pressed {i} times: the keyboard was lost");
        }
    }
}

/// The app on a folder with a small 3D model, m.obj, and Export as a 3D model open on it.
fn exporting(name: &str) -> Harness {
    let mut h = Harness::new(&folder(name, &["a.png"]));
    std::fs::write(crate::audit_family_1::guard().join(name).join("m.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    h.app.refresh();
    h.until(|a| a.files().len() == 2, "m.obj listed");
    let m = h.app.files().iter().find(|f| f.path.ends_with("m.obj")).unwrap().path.clone();
    h.app.open_model_export(Some(m));
    h.frame(Vec::new());
    h.frame(Vec::new());
    h
}

/// #311: a list of a question opened from the keyboard (the formats of Export as a 3D model): its
/// keys are the list's. ↓ and Enter chose a button of the window and pressed it (Export), Esc
/// cancelled the window instead of closing the list.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn a_list_of_a_question_takes_its_keys() {
    if own_folder("window_keys::a_list_of_a_question_takes_its_keys") {
        return;
    }
    let mut h = exporting("keys-model-list");
    let kind = |h: &Harness| h.app.model_export.as_ref().map(|f| f.kind);
    let before = kind(&h);
    // Export highlighted when it opens; the list of formats is above it.
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Space);
    h.frame(Vec::new());
    assert!(egui::Popup::is_any_open(&h.ctx), "Space did not open the list");
    h.key(Key::Escape);
    h.frame(Vec::new());
    assert!(!egui::Popup::is_any_open(&h.ctx) && kind(&h) == before, "Esc did not close the list only");
    h.key(Key::Space);
    h.frame(Vec::new());
    // The first ↓ goes to the format chosen, the first of the list; the second to the next one.
    h.key(Key::ArrowDown);
    h.key(Key::ArrowDown);
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(kind(&h).is_some() && kind(&h) != before, "↓ and Enter in the list did not choose another format, or exported");
}

/// #311: the box of a copy under way over a page (the viewer, the editor): Tab goes through its
/// Cancel as egui gives it, and on from there; the round of the grid (`box_keys`) moved the grid
/// and the folder hidden under the page.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn the_box_of_a_copy_over_a_page_leaves_the_grid() {
    if own_folder("window_keys::the_box_of_a_copy_over_a_page_leaves_the_grid") {
        return;
    }
    for editor in [false, true] {
        let mut h = if editor { in_editor("keys-box-editor") } else { on_grid("keys-box-viewer") };
        if !editor {
            h.app.activate(1);
            h.until(|a| a.zoom_of == Some(1), "b.png shown");
        }
        let p = std::sync::Arc::new(crate::transfer::Progress::default());
        h.app.transfer = Some((p, false, h.app.base.clone(), h.app.dir.clone()));
        h.frame(Vec::new());
        let state = |a: &App| format!("{:?}", (a.viewing, a.selected, a.cursor, a.focus, &a.dir, a.editor.is_some()));
        let before = state(&h.app);
        let mut reached = false;
        for modifiers in [Modifiers::NONE, Modifiers::SHIFT] {
            h.blur();
            for _ in 0..80 {
                h.key_with(Key::Tab, modifiers);
                h.frame(Vec::new());
                reached |= focused(&h).is_some_and(|(_, l)| l.order == egui::Order::Middle);
                assert_eq!(state(&h.app), before, "Tab {modifiers:?} moved what is under the page (editor {editor})");
            }
        }
        assert!(reached, "Tab never reached the box of the copy (editor {editor})");
    }
}

/// #311: a question asked while Convert works in its box (About, a deletion): the keys are the
/// question's, as at any other time. The work under way counted as Convert's window open: Tab in
/// About went to the menus under it, and with a button of the deletion focused Esc did nothing.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn questions_during_convert_keep_their_keys() {
    if own_folder("window_keys::questions_during_convert_keep_their_keys") {
        return;
    }
    let mut h = on_grid("keys-during-convert");
    let a = h.app.files()[0].path.clone();
    h.app.open_batch(Some(a));
    h.frame(Vec::new());
    h.app.batch.as_mut().unwrap().work_forever();
    h.frame(Vec::new());
    h.app.about = true;
    // The layer of a window is its title's (`egui::Window::new`).
    let title = h.app.i18n.t("menu-about");
    let about = egui::LayerId::new(egui::Order::Middle, egui::Id::unique(Some(title.as_str())));
    tab_goes_through(&mut h, |l| l == about, |a| a.about, 2, true);
    h.key(Key::Escape);
    assert!(!h.app.about, "Esc did not close About");
    h.key(Key::Delete);
    assert!(h.app.delete_confirm.is_some(), "Del did not ask");
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert!(h.app.delete_confirm.is_none(), "Esc with a button focused did not cancel the deletion");
}

/// #311: Esc with a control focused (reached with Tab) closes as with nothing focused: the results
/// of Find duplicates and Convert kept it.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn esc_closes_with_a_control_focused() {
    if own_folder("window_keys::esc_closes_with_a_control_focused") {
        return;
    }
    let mut h = on_grid("keys-esc-duplicates");
    h.app.open_duplicates(None);
    h.frame(Vec::new());
    h.key(Key::Enter);
    h.until(App::dup_page_shown, "the results of Find duplicates");
    h.key(Key::Tab);
    assert!(focused(&h).is_some(), "Tab reached nothing");
    h.key(Key::Escape);
    h.frame(Vec::new());
    assert!(h.app.duplicates.is_none(), "Esc with a control focused did not close Find duplicates");

    let mut h = on_grid("keys-esc-convert");
    let a = h.app.files()[0].path.clone();
    h.app.open_batch(Some(a));
    h.frame(Vec::new());
    h.key(Key::Tab);
    assert!(focused(&h).is_some(), "Tab reached nothing");
    h.key(Key::Escape);
    h.frame(Vec::new());
    assert!(h.app.batch.is_none(), "Esc with a control focused did not close Convert");
}

/// #311: in the windows with fields, Enter on Cancel (the last control, reached with Shift+Tab)
/// cancels: the window does not also read it as Open, Unlock or Resize.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn enter_on_cancel_cancels() {
    if own_folder("window_keys::enter_on_cancel_cancels") {
        return;
    }
    let mut h = on_grid("keys-cancel-address");
    let dir = h.app.dir.clone();
    h.app.open_address = Some(crate::address::AddressForm { url: "/".into(), ..crate::address::AddressForm::new() });
    h.frame(Vec::new());
    h.blur();
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(h.app.open_address.is_none() && h.app.dir == dir, "Enter on Cancel did not cancel Open location, or opened the address");

    let mut h = in_editor("keys-cancel-resize");
    h.key_with(Key::I, Modifiers::COMMAND | Modifiers::ALT);
    h.frame(Vec::new());
    h.blur();
    h.key_with(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Enter);
    h.frame(Vec::new());
    let w = h.app.editor.as_ref().expect("the editor closed");
    assert!(!w.resizing() && !crate::edit_page::Window::asking(w), "Enter on Cancel did not close Resize");
}

/// #311 (Piero, 24/11): in the viewer, with a button of the bars holding the keyboard (reached
/// with Tab), the keys of the viewer work all the same: → goes to the next picture, Esc goes back
/// to the grid. Before, with any button focused the viewer read none of its keys.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn the_viewer_keys_work_with_a_bar_button_focused() {
    if own_folder("window_keys::the_viewer_keys_work_with_a_bar_button_focused") {
        return;
    }
    let mut h = Harness::new(&folder("viewer-bar-keys", &["a.png", "b.png", "c.png"]));
    h.app.activate(0);
    h.until(|app| app.zoom_of == Some(0), "a.png shown");
    let tab = |h: &mut Harness| {
        h.key(Key::Tab);
        h.frame(Vec::new());
        assert!(focused(h).is_some(), "Tab gave the keyboard to no button of the bars");
    };
    tab(&mut h);
    h.key(Key::ArrowRight);
    h.until(|app| app.viewing == Some(1), "→ with a button focused went to the next picture");
    tab(&mut h);
    h.key(Key::Escape);
    h.until(|app| app.viewing.is_none(), "Esc with a button focused went back to the grid");
}

/// #311: with the keyboard inside the guide (reached with Tab) and the pointer elsewhere, two Esc
/// close it, as everywhere; they were lost (Piero's test L1.5, 24/11).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn two_esc_close_the_guide_with_the_keyboard_inside() {
    if own_folder("window_keys::two_esc_close_the_guide_with_the_keyboard_inside") {
        return;
    }
    let guide = modal("help-window");
    let mut h = on_grid("keys-guide-esc");
    h.key(Key::F1);
    assert!(h.app.help.open, "F1 did not open the guide");
    for _ in 0..3 {
        h.key(Key::Tab);
        h.frame(Vec::new());
    }
    assert!(focused(&h).is_some_and(|(_, l)| guide(l)), "Tab did not go into the guide");
    let esc = egui::Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
    h.frame(vec![esc.clone()]);
    h.frame(vec![esc]);
    h.frame(Vec::new());
    assert!(!h.app.help.open, "two Esc with the keyboard in the guide did not close it");
}

/// #311: the guide opened with F1 over a window that asks (About): Tab goes into the guide, as
/// over the grid; it was lost (Piero's test W1.4 on Windows, 24/11, the same on Linux).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn tab_goes_into_the_guide_over_about() {
    if own_folder("window_keys::tab_goes_into_the_guide_over_about") {
        return;
    }
    let guide = modal("help-window");
    let mut h = on_grid("keys-guide-over-about");
    h.app.about = true;
    h.frame(Vec::new());
    // As in the test: the keyboard first on a control of About (Tab), then F1.
    h.key(Key::Tab);
    h.frame(Vec::new());
    assert!(focused(&h).is_some(), "Tab in About reached nothing");
    h.key(Key::F1);
    assert!(h.app.help.open, "F1 did not open the guide over About");
    h.frame(Vec::new());
    // The guide opens with its search field focused: Tab must move on from it, inside the guide.
    let start = focused(&h).map(|(id, _)| id);
    let mut seen = std::collections::HashSet::new();
    for _ in 0..3 {
        h.key(Key::Tab);
        h.frame(Vec::new());
        assert!(focused(&h).is_some_and(|(_, l)| guide(l)), "Tab over About left the guide");
        seen.extend(focused(&h).map(|(id, _)| id));
    }
    assert!(seen.iter().any(|id| Some(*id) != start), "Tab over About did not move inside the guide (Piero's test W1.4)");
    assert!(h.app.about, "About closed");
}
