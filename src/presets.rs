//! Resize: sizes kept in the settings (a name and a `batch::Size`), chosen from the right-click
//! menu of the grid and from Tools; each one works at once on the pictures chosen
//! (`App::resize_now`), Custom… opens Convert and resize. The Options tab that edits them.

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::App;
use crate::actions::Act;
use crate::batch::Size;
use crate::browser::MenuItem;
use crate::i18n::I18n;
use crate::settings::Settings;

/// A size of Resize; the name may be empty (the menu then shows only the size).
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Preset {
    pub name: String,
    pub size: Size,
}

/// The first presets, named in the language of `i18n`.
pub fn defaults(i18n: &I18n) -> Vec<Preset> {
    [("preset-small", Size::Long(1280)), ("preset-medium", Size::Long(1920)), ("preset-large", Size::Long(2560)), ("preset-half", Size::Percent(50))]
        .map(|(id, size)| Preset { name: i18n.t(id), size })
        .to_vec()
}

/// The presets of the settings; the first ones until some are changed (`None` in the file), so
/// their names follow the language.
pub fn of(s: &Settings, i18n: &I18n) -> Vec<Preset> {
    s.resize_presets.clone().unwrap_or_else(|| defaults(i18n))
}

/// The size in words, as "long side 1920 px".
pub fn describe(size: Size, i18n: &I18n) -> String {
    let (a, b) = match size {
        Size::Same => return i18n.t("batch-size-same"),
        Size::Percent(n) | Size::Long(n) | Size::Short(n) | Size::Width(n) | Size::Height(n) => (n, 0),
        Size::Fit(w, h) | Size::Crop(w, h) | Size::Stretch(w, h) => (w, h),
    };
    let key = size.label().trim_start_matches("batch-size-");
    i18n.t_args(&format!("preset-desc-{key}"), &[("a", &a.to_string()), ("b", &b.to_string())])
}

impl Preset {
    /// Its menu entry: "Medium (long side 1920 px)", or the size alone.
    pub fn label(&self, i18n: &I18n) -> String {
        let size = describe(self.size, i18n);
        match self.name.trim() {
            "" => size,
            name => format!("{name} ({size})"),
        }
    }
}

impl App {
    /// Resize's submenu: the presets, then Custom….
    pub(crate) fn resize_items(&self, path: Option<String>) -> Vec<MenuItem> {
        let presets = of(&self.settings, &self.i18n);
        let mut items: Vec<MenuItem> = presets.iter().map(|p| Some((p.label(&self.i18n), String::new(), Act::Resize(p.size, path.clone())))).collect();
        items.extend([None, Some((self.i18n.t("menu-resize-custom"), String::new(), Act::ResizeCustom(path)))]);
        items
    }
}

/// The Resize tab: one row per preset (name, kind, measures, remove), then add and the first ones
/// back. A change is kept only when the list differs from what was there.
pub fn options_ui(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    let t = |id| i18n.t(id);
    ui.label(t("opt-resize-intro"));
    ui.add_space(6.0);
    let before = of(s, i18n);
    let mut list = before.clone();
    let mut remove = None;
    egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
        for (i, p) in list.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut p.name).id_salt(("preset-name", i)).desired_width(130.0).hint_text(t("opt-resize-name")));
                kind_combo(ui, &mut p.size, i, i18n);
                crate::batch_page::size_fields(ui, &mut p.size, true, i18n);
                if ui.add(crate::icons::small(ui, crate::icons::Line::Close)).on_hover_text(t("opt-resize-remove")).clicked() {
                    remove = Some(i);
                }
            });
        }
    });
    if let Some(i) = remove {
        list.remove(i);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if ui.button(t("opt-resize-add")).clicked() {
            list.push(Preset { name: String::new(), size: Size::Long(1920) });
        }
        if ui.add_enabled(s.resize_presets.is_some(), egui::Button::new(t("opt-resize-reset"))).clicked() {
            s.resize_presets = None;
            list = before.clone();
        }
    });
    if list != before {
        s.resize_presets = Some(list);
    }
}

/// The kind of a preset's size; another kind starts from its first measures (`Size::KINDS`).
fn kind_combo(ui: &mut egui::Ui, size: &mut Size, i: usize, i18n: &I18n) {
    egui::ComboBox::from_id_salt(("preset-kind", i)).width(170.0).selected_text(i18n.t(size.label())).show_ui(ui, |ui| {
        for kind in &Size::KINDS[1..] {
            if ui.selectable_label(size.same_kind(*kind), i18n.t(kind.label())).clicked() && !size.same_kind(*kind) {
                *size = *kind;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind is said in words in both languages, and the menu shows the name with it.
    #[test]
    fn presets_in_words() {
        for code in ["en", "it"] {
            let i18n = I18n::new(code);
            for kind in Size::KINDS {
                let words = describe(kind, &i18n);
                assert!(!words.contains("preset-desc") && !words.is_empty(), "{code}: {kind:?}");
            }
        }
        let en = I18n::new("en");
        let first = defaults(&en);
        assert_eq!(first.iter().map(|p| p.label(&en)).collect::<Vec<_>>(), ["Small (long side 1280 px)", "Medium (long side 1920 px)", "Large (long side 2560 px)", "Half (50%)"]);
        let it = I18n::new("it");
        assert_eq!(defaults(&it)[1].label(&it), "Media (lato lungo 1920 px)");
        assert_eq!(defaults(&it)[3].label(&it), "Metà (50 %)");
        assert_eq!(Preset { name: " ".into(), size: Size::Crop(800, 600) }.label(&en), "exactly 800 × 600 px, cropped");
    }

    /// settings.json without presets (written before Resize): the first ones, in the language;
    /// presets changed are read back as written.
    #[test]
    fn presets_kept_in_the_settings() {
        let it = I18n::new("it");
        let old: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50}"#).unwrap();
        assert_eq!(old.resize_presets, None);
        assert_eq!(of(&old, &it), defaults(&it));
        let mine = Settings { resize_presets: Some(vec![Preset { name: "Web".into(), size: Size::Width(1200) }, Preset { name: String::new(), size: Size::Stretch(64, 64) }]), ..Settings::default() };
        let text = serde_json::to_string(&mine).unwrap();
        assert!(text.contains(r#"{"name":"Web","size":{"width":1200}}"#), "{text}");
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(of(&back, &it), mine.resize_presets.unwrap());
    }
}
