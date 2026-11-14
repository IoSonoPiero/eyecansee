//! The rows of the window of Convert (`batch_page`): format, drawings, turn, size,
//! destination, and the buttons. Moved from `batch_page.rs` on 2026-11-14, unchanged.

use super::*;

/// The choices of the window; `Some(true)` Convert, `Some(false)` Cancel.
pub(super) fn form_ui(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) -> Option<bool> {
    let t = |id| i18n.t(id);
    ui.spacing_mut().item_spacing.y = 6.0;
    // On a low screen the choices scroll, the buttons stay in view.
    let max = (ui.ctx().content_rect().height() - 170.0).max(200.0);
    // Where it opens (`Form::part`): the top, or the size; not in the pass that measures the window.
    let scroll = f.scroll && !ui.is_sizing_pass();
    f.scroll &= !scroll;
    let mut area = egui::ScrollArea::vertical().max_height(max);
    if scroll && f.part != Part::Resize {
        area = area.vertical_scroll_offset(0.0);
    }
    area.show(ui, |ui| {
        if let Some(job) = f.audio {
            section(ui, t("batch-video"), |ui| audio_rows(ui, &mut f.video, job, i18n));
            section(ui, t("batch-where"), |ui| where_rows(ui, f, i18n));
            return;
        }
        if f.drawing {
            section(ui, t("convert-format"), |ui| drawing_rows(ui, f, i18n));
            section(ui, t("batch-where"), |ui| where_rows(ui, f, i18n));
            return;
        }
        if f.images {
            section(ui, t("convert-format"), |ui| format_rows(ui, f, i18n));
        }
        if f.videos {
            section(ui, t("batch-video"), |ui| video_rows(ui, &mut f.video, i18n));
        }
        if f.audios {
            section(ui, t("batch-sound"), |ui| sound_rows(ui, &mut f.sound, &mut f.video, i18n));
        }
        // Audio files have no picture to turn or resize.
        if !f.images && !f.videos {
            section(ui, t("batch-where"), |ui| where_rows(ui, f, i18n));
            return;
        }
        // Copied streams keep their turn and size: with videos only there is nothing to choose.
        let fixed = f.videos && !f.images && f.video.codec == Codec::Copy;
        ui.add_enabled_ui(!fixed, |ui| {
            // The size, with the turn above it (a picture's shape, after the format).
            if scroll && f.part == Part::Resize {
                ui.scroll_to_cursor(Some(egui::Align::TOP));
            }
            section(ui, t("batch-turn"), |ui| turn_rows(ui, f, i18n));
            section(ui, t("batch-size"), |ui| size_rows(ui, f, i18n));
        });
        section(ui, t("batch-where"), |ui| where_rows(ui, f, i18n));
    });
    ui.add_space(14.0);
    let why_not = f.why_not();
    if let Some(id) = why_not {
        ui.label(crate::browser::error_text(ui, t(id)));
        ui.add_space(4.0);
    }
    let mut answer = None;
    ui.horizontal(|ui| {
        let start = crate::theme::main_button(ui, t(f.titles().1));
        if ui.add_enabled(why_not.is_none(), start).clicked() {
            answer = Some(true);
        }
        if ui.button(t("delete-no")).clicked() {
            answer = Some(false);
        }
    });
    answer
}

/// A part of the window: its title, then what is in it, set in a little.
fn section(ui: &mut egui::Ui, title: String, add: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(14.0);
    ui.label(egui::RichText::new(title).strong().size(15.0));
    ui.add_space(4.0);
    ui.indent(ui.next_auto_id(), add);
}

fn format_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    let label = |format: Option<Format>| format.map_or_else(|| t("batch-same-format"), |f| f.label().to_string());
    egui::ComboBox::from_id_salt("batch-format").selected_text(label(f.format)).show_ui(ui, |ui| {
        ui.selectable_value(&mut f.format, None, t("batch-same-format"));
        for format in Format::ALL {
            ui.selectable_value(&mut f.format, Some(format), format.label());
        }
    });
    match f.format {
        None => {
            ui.weak(t("batch-same-format-hint"));
        }
        Some(format) => {
            if format != Format::Jpeg && f.jpegs {
                ui.label(crate::theme::alert(t("batch-ultra-hdr-lost")));
            }
            format_options(ui, f, format, i18n);
        }
    }
}

/// The quality and the other options of `format`, closed at first: the window stays short
/// enough to show every part.
fn format_options(ui: &mut egui::Ui, f: &mut Form, format: Format, i18n: &I18n) {
    f.options.format = format;
    let title = i18n.t_args("batch-format-options", &[("format", format.label())]);
    egui::CollapsingHeader::new(title).id_salt("batch-format-options").show(ui, |ui| {
        // Several images: the options shown as for one with transparency and larger than an icon.
        crate::convert::format_options(ui, &mut f.options, true, true, i18n);
        if format.holds_meta() {
            ui.checkbox(&mut f.options.keep_meta, i18n.t("convert-keep-meta"));
        }
    });
}

/// "Export drawing": a picture format and its width, SVG, or the other AutoCAD format.
fn drawing_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    // "Same as the original" stays chosen for Convert and resize: PNG here (`batch::drawing`).
    let format = f.format.unwrap_or(Format::Png);
    let others = [(Drawn::Svg, "SVG"), (Drawn::Cad(Target::Dwg), "DWG"), (Drawn::Cad(Target::Dxf), "DXF")];
    let label = others.iter().find(|(d, _)| f.kept == Some(*d)).map_or(format.label(), |(_, l)| l);
    egui::ComboBox::from_id_salt("drawing-format").selected_text(label).show_ui(ui, |ui| {
        for k in Format::ALL {
            if ui.selectable_label(f.kept.is_none() && k == format, k.label()).clicked() {
                (f.kept, f.format) = (None, Some(k));
            }
        }
        for (d, label) in others {
            ui.selectable_value(&mut f.kept, Some(d), label);
        }
    });
    if let Some(kept) = f.kept {
        ui.weak(t(if kept == Drawn::Svg { "drawing-svg-hint" } else { "drawing-cad-hint" }));
        return;
    }
    ui.horizontal(|ui| {
        ui.label(t("drawing-side"));
        ui.add(egui::DragValue::new(&mut f.side).range(MIN_SIDE..=MAX_SIDE));
        ui.label(t("drawing-pixels"));
    });
    ui.weak(t("drawing-side-hint"));
    format_options(ui, f, format, i18n);
}

/// The long side of an exported drawing: 16384² pixels in RGBA are 1 GB.
const MIN_SIDE: u32 = 256;
const MAX_SIDE: u32 = 16384;

fn turn_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.horizontal_wrapped(|ui| {
        for (q, id) in [(0, "batch-turn-none"), (1, "batch-turn-cw"), (2, "batch-turn-180"), (3, "batch-turn-ccw")] {
            ui.radio_value(&mut f.quarter, q, t(id));
        }
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut f.flip_h, t("tf-flip-h"));
        ui.checkbox(&mut f.flip_v, t("tf-flip-v"));
    });
    if f.quarter != 0 && (f.flip_h || f.flip_v) {
        ui.weak(t("batch-turn-order"));
    }
}

fn size_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    for i in 0..f.sizes.len() {
        // Videos are resized only by percent or into a box (`Size::for_videos`).
        let on = !f.videos || f.sizes[i].for_videos();
        let row = ui.add_enabled_ui(on, |ui| {
            ui.horizontal(|ui| {
                ui.radio_value(&mut f.size, i, t(f.sizes[i].label()));
                size_fields(ui, &mut f.sizes[i], f.size == i, i18n);
            })
        });
        row.response.on_disabled_hover_text(t("batch-size-images-only"));
    }
    ui.add_space(4.0);
    ui.add_enabled_ui(f.size != 0, |ui| {
        ui.checkbox(&mut f.enlarge, t("batch-enlarge"));
        if !f.images {
            return;
        }
        egui::ComboBox::from_label(t("batch-filter")).selected_text(t(f.filter.label())).show_ui(ui, |ui| {
            for filter in Filter::ALL {
                ui.selectable_value(&mut f.filter, filter, t(filter.label()));
            }
        });
    });
}

/// The measures of `size`, to change (`on`): a percent, one side or a box, in pixels. Also in
/// the Options, for the presets of Resize.
pub(crate) fn size_fields(ui: &mut egui::Ui, size: &mut Size, on: bool, i18n: &I18n) {
    let side = |ui: &mut egui::Ui, v: &mut u32| ui.add_enabled(on, egui::DragValue::new(v).range(1..=65535));
    // Only a box keeps the proportions.
    let unit = i18n.t(if matches!(size, Size::Fit(..)) { "batch-pixels" } else { "batch-px" });
    match size {
        Size::Same => {}
        Size::Percent(p) => {
            ui.add_enabled(on, egui::DragValue::new(p).range(1..=1000).suffix(" %"));
        }
        Size::Long(n) | Size::Short(n) | Size::Width(n) | Size::Height(n) => {
            side(ui, n);
            ui.label(unit);
        }
        Size::Fit(w, h) | Size::Crop(w, h) | Size::Stretch(w, h) => {
            side(ui, w);
            ui.label("×");
            side(ui, h);
            ui.label(unit);
        }
    }
}

fn where_rows(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.radio_value(&mut f.place, Place::Next, t("batch-same-folder"));
    ui.horizontal(|ui| {
        ui.radio_value(&mut f.place, Place::Sub, t("batch-subfolder"));
        ui.add_enabled(f.place == Place::Sub, egui::TextEdit::singleline(&mut f.sub_name).desired_width(200.0));
    });
    ui.radio_value(&mut f.place, Place::Other, t("batch-other"));
    ui.add_enabled_ui(f.place == Place::Other, |ui| {
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut f.other).desired_width(360.0));
            if ui.button(t("batch-browse")).clicked() {
                f.browse = true;
            }
        });
        ui.weak(t("batch-other-hint"));
    });
    ui.add_space(4.0);
    ui.weak(t("batch-keep"));
}
