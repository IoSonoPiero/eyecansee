//! A bar on as many rows as it needs: the menu bar and the viewer's status bar. In a narrow
//! window (400 px) the last menu, Help, was cut at the right edge, and the date at the end of the
//! viewer's status bar went under its Keys button (520 px). Each item now goes whole on the next
//! row when it does not fit on this one, as the menu bar of Windows does; what stands at the right
//! end (the update button, Keys) stays on the row when there is room for it, otherwise it takes
//! the next row. Nothing is cut or left out.

use eframe::egui;

/// `add` on rows that wrap: each item whole, never a text broken in two.
pub(crate) fn rows<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        add(ui)
    })
    .inner
}

/// Whether what stands at the right end, `need` points wide, goes on a row of its own: when the
/// room left on this row is less.
pub(crate) fn own_row(left: f32, need: f32) -> bool {
    need > 0.0 && left + 0.5 < need
}

/// At the end of `rows`, what stands at the right end, drawn right to left by `add`, which says
/// how wide it is (what may shrink, as a text cut to the room left, not counted). The width of the
/// last pass decides whether it goes on a row of its own; when it changes, the pass is drawn again,
/// so it is never shown over the items before it.
pub(crate) fn right_end<R>(ui: &mut egui::Ui, salt: &str, add: impl FnOnce(&mut egui::Ui) -> (R, f32)) -> R {
    let id = ui.scope_id().with(("bar-right-end", salt));
    let last = ui.data(|d| d.get_temp::<f32>(id)).unwrap_or(0.0);
    if own_row(ui.available_size_before_wrap().x, last) {
        ui.end_row();
    }
    let (inner, need) = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner;
    if (need - last).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, need));
        ui.ctx().request_discard("the right end of a bar measured");
    }
    inner
}

/// A text that changes as it goes (a number moving) kept as wide as `widest` with any digits, so
/// what comes after it on the bar stays still instead of shaking.
pub(crate) fn steady(ui: &mut egui::Ui, text: &str, widest: &str) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let width = |s: String| ui.painter().layout_no_wrap(s, font.clone(), egui::Color32::WHITE).size().x;
    let digits = ('0'..='9').map(|d| width(widest.chars().map(|c| if c.is_ascii_digit() { d } else { c }).collect()));
    let wide = digits.fold(width(text.to_string()), f32::max);
    ui.allocate_ui_with_layout(egui::vec2(wide, ui.spacing().interact_size.y), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(wide);
        ui.label(text);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_end_row() {
        assert!(!own_row(200.0, 120.0));
        assert!(!own_row(120.0, 120.0));
        assert!(own_row(100.0, 120.0));
        // Nothing at the right end (the version only, shown where it fits): never a row of its own.
        assert!(!own_row(0.0, 0.0));
    }

    /// Through egui with the app's fonts: the seven menus in Italian and a button at the right
    /// end, in windows from 300 to 1400 px at 100 % and 125 %: nothing past the right edge, the
    /// menus in their order, rows added only when needed.
    #[test]
    fn nothing_past_the_edge() {
        let ctx = egui::Context::default();
        ctx.set_fonts(crate::fonts::fonts());
        let names = ["File", "Modifica", "Immagine", "Visualizza", "Segnalibri", "Strumenti", "Aiuto"];
        for ppp in [1.0, 1.25] {
            ctx.set_pixels_per_point(ppp);
            for px in [300.0, 400.0, 520.0, 700.0, 1000.0, 1400.0] {
                let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(px / ppp, 600.0))), ..Default::default() };
                let mut seen = (Vec::new(), egui::Rect::NOTHING, 0.0);
                for _ in 0..4 {
                    ctx.run_ui(input.clone(), |ui| {
                        egui::Panel::top("menu").show(ui, |ui| {
                            let right = ui.max_rect().right();
                            let (menus, end) = rows(ui, |ui| {
                                let menus: Vec<egui::Rect> = names.iter().map(|n| ui.button(*n).rect).collect();
                                let end = right_end(ui, "t", |ui| {
                                    let b = ui.button("Installa la 0.78.0").rect;
                                    (b, ui.min_rect().width())
                                });
                                (menus, end)
                            });
                            seen = (menus, end, right);
                        });
                    })
                    .textures_delta
                    .clear();
                }
                let (menus, end, right) = seen;
                let what = format!("{px} px at {ppp}");
                for r in menus.iter().chain([&end]) {
                    assert!(r.right() <= right + 0.5, "{what}: {r:?} past {right}");
                }
                // In reading order: each menu after the one before, on its row or on a lower one.
                for w in menus.windows(2) {
                    assert!(w[1].left() > w[0].left() || w[1].top() > w[0].top(), "{what}: {w:?}");
                }
                if px >= 1000.0 {
                    assert!(menus.iter().chain([&end]).all(|r| (r.top() - menus[0].top()).abs() < 0.5), "{what}: more than one row");
                }
            }
        }
    }
}
