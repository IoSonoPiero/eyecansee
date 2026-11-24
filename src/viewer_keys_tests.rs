//! Keys of the viewer that did something else than their list says (#342, #343, #345), tried on
//! the app without a screen, each test in a folder of its own (`own_folder`).

use eframe::egui::{self, Key, Modifiers};

use crate::audit_family_1::{Harness, folder, own_folder};

/// The viewer open on `name` in a folder of this run with a picture, a small 3D model (one
/// triangle, OBJ) and a drawing (one line, DXF).
fn showing(dir: &str, name: &str) -> Harness {
    let dir = folder(dir, &["b.png"]);
    std::fs::write(dir.join("a.obj"), "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let line = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n30\n0\n11\n100\n21\n50\n31\n0\n0\nENDSEC\n0\nEOF\n";
    std::fs::write(dir.join("c.dxf"), line).unwrap();
    let mut h = Harness::new(&dir);
    open(&mut h, name);
    h
}

/// The viewer on `name`, once it is shown (a model read, or a picture or drawing drawn).
fn open(h: &mut Harness, name: &str) {
    let i = h.app.files().iter().position(|f| f.path.ends_with(name)).unwrap();
    h.app.activate(i);
    h.until(|a| a.viewed_mesh().is_some() || a.zoom_of == a.viewing, "the file shown");
}

/// M, and Measure is on.
fn measure(h: &mut Harness) {
    h.key(Key::M);
    h.frame(Vec::new());
    assert!(h.app.model.measure.on, "M did not start Measure");
}

/// #342: during the measure of a 3D model, which has no modes, D, A and G do nothing: the two
/// points clicked stay.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn d_a_g_keep_the_points_of_a_model() {
    if own_folder("viewer_keys_tests::d_a_g_keep_the_points_of_a_model") {
        return;
    }
    let mut h = showing("keys-342", "a.obj");
    measure(&mut h);
    let mode = h.app.model.measure.mode;
    for key in [Key::A, Key::G, Key::D] {
        h.app.model.measure.pts3 = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        h.key(key);
        h.frame(Vec::new());
        assert_eq!(h.app.model.measure.pts3.len(), 2, "{key:?} cleared the points measured on the model");
        assert!(h.app.model.measure.mode == mode, "{key:?} changed the mode on a model");
    }
}

/// #345: during the measure of a 3D model, where there is no area to close, Enter goes back to
/// the grid as without Measure; on a drawing with no area to close it does nothing, and
/// Measure stays on.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn enter_while_measuring() {
    if own_folder("viewer_keys_tests::enter_while_measuring") {
        return;
    }
    let mut h = showing("keys-345", "a.obj");
    measure(&mut h);
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(h.app.viewing.is_none(), "Enter did not go back to the grid while measuring the model");

    open(&mut h, "c.dxf");
    h.until(|a| a.measure_off().is_none(), "the drawing read");
    if !h.app.model.measure.on {
        measure(&mut h);
    }
    h.key(Key::Enter);
    h.frame(Vec::new());
    assert!(h.app.viewing.is_some() && h.app.model.measure.on, "Enter on a drawing without an area to close left Measure or the viewer");
}

/// #343: in the viewer Shift+F5 does what F5 does, reads the folder again: it does not make the
/// thumbnails of the whole folder again (the menu's item is greyed out there).
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn shift_f5_in_the_viewer() {
    if own_folder("viewer_keys_tests::shift_f5_in_the_viewer") {
        return;
    }
    let mut h = Harness::new(&folder("keys-343", &["a.png", "b.png"]));
    h.until(|_| crate::thumbcache::stats().0 == 2, "the two thumbnails in the cache");
    // No frames after the key: the grid would make the thumbnails again. The cache's writer does
    // its jobs in order: once an empty forget is done (as `purge` waits), what the key sent is done.
    // The modifiers held come as their own event: `global_keys` reads Shift from them.
    let after = |h: &mut Harness, key: Key, modifiers: Modifiers| {
        let ev = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        h.frame(vec![egui::Event::ModifiersChanged(modifiers), ev(true), ev(false)]);
        let (tx, rx) = std::sync::mpsc::channel();
        crate::thumbcache::forget("", Vec::new(), move || drop(tx.send(())));
        rx.recv_timeout(std::time::Duration::from_secs(10)).expect("the cache's writer done");
        let n = crate::thumbcache::stats().0;
        h.frame(vec![egui::Event::ModifiersChanged(Modifiers::NONE)]);
        n
    };
    // In the grid Shift+F5 takes them from the cache, as the menu's item.
    assert_eq!(after(&mut h, Key::F5, Modifiers::SHIFT), 0, "Shift+F5 in the grid kept the thumbnails in the cache");
    h.until(|_| crate::thumbcache::stats().0 == 2, "the two thumbnails made again");
    h.app.activate(1);
    h.until(|a| a.zoom_of == Some(1), "b.png shown");
    assert_eq!(after(&mut h, Key::F5, Modifiers::NONE), 2, "F5 alone took thumbnails from the cache");
    assert_eq!(after(&mut h, Key::F5, Modifiers::SHIFT), 2, "Shift+F5 in the viewer took the thumbnails of the folder from the cache");
    h.frame(Vec::new());
    assert_eq!(h.app.viewing, Some(1), "Shift+F5 in the viewer left the picture open");
}
