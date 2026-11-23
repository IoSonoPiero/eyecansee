//! "Export as a 3D model…": a window, as "Export drawing…" has one: the models chosen, the format
//! to write, then Export; each copy goes next to its model (`App::export_model`).

use eframe::egui;

use crate::App;
use crate::i18n::I18n;
use crate::model_export::Kind;

/// The window open: the models it works on (`None` = those chosen when it opened), the format,
/// and the triangles of those already read (in the viewer, `model.meshes`).
pub(crate) struct Form {
    pub(crate) path: Option<String>,
    pub(crate) models: usize,
    pub(crate) kind: Kind,
    pub(crate) known: Vec<usize>,
}

enum Answer {
    Export,
    Cancel,
}

impl App {
    /// File > Export as a 3D model… and the same entry of the right-click menus: the window, on the
    /// format of the last export (STL at first).
    pub(crate) fn open_model_export(&mut self, path: Option<String>) {
        let chosen: Vec<String> = self.files_chosen(path.clone()).into_iter().filter(|p| crate::model::is_model(p)).collect();
        if !chosen.is_empty() {
            let known = chosen.iter().filter_map(|p| self.model.meshes.get(p)).map(|m| m.tris.len()).collect();
            self.model_export = Some(Form { path, models: chosen.len(), kind: self.model_export_kind, known });
        }
    }

    pub(crate) fn model_export_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.model_export.take() else { return };
        // Enter exports and Esc cancels (`App::asking`), never reaching the viewer under it.
        let mut answer = crate::confirm::answer(ctx, &mut self.key_answer, "model-export").map(|yes| if yes { Answer::Export } else { Answer::Cancel });
        let window = egui::Window::new(self.i18n.t("model-export-title")).collapsible(false).resizable(false);
        window.anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            answer = form(ui, &mut f, &self.i18n).or(answer.take());
        });
        match answer {
            Some(Answer::Export) => {
                self.model_export_kind = f.kind;
                self.export_model(f.path, f.kind);
            }
            Some(Answer::Cancel) => {}
            None => self.model_export = Some(f),
        }
    }
}

/// How many models, the format, where the copies go; Cancel and Export.
fn form(ui: &mut egui::Ui, f: &mut Form, i18n: &I18n) -> Option<Answer> {
    let t = |id| i18n.t(id);
    ui.label(i18n.t_args("model-export-count", &[("n", &f.models.to_string())]));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(t("model-export-format"));
        egui::ComboBox::from_id_salt("model-export-format").width(160.0).selected_text(f.kind.label()).show_ui(ui, |ui| {
            for k in Kind::ALL {
                ui.selectable_value(&mut f.kind, k, k.label());
            }
        });
    });
    if let Some(line) = beyond_line(i18n, f) {
        ui.add_space(6.0);
        ui.colored_label(ui.visuals().warn_fg_color, line);
    }
    ui.add_space(6.0);
    ui.weak(t("model-export-where"));
    ui.add_space(8.0);
    let mut answer = None;
    ui.horizontal(|ui| {
        if ui.button(t("model-export-ok")).clicked() {
            answer = Some(Answer::Export);
        }
        if ui.button(t("delete-no")).clicked() {
            answer = Some(Answer::Cancel);
        }
    });
    answer
}

/// The warning of the window when some of the models already read have more triangles than
/// EyeCanSee opens in the format chosen (`model_limit::of_export`): Export writes them all the
/// same (Piero, 22/11/2026). The others are told at the end (`model_view::export_notice`).
fn beyond_line(i18n: &I18n, f: &Form) -> Option<String> {
    let limit = crate::model_limit::of_export(f.kind);
    let n = f.known.iter().filter(|&&t| limit.check(t).is_err()).count();
    let all = if n == f.models { "yes" } else { "no" };
    let max = (limit.max / 1_000_000).to_string();
    (n > 0).then(|| i18n.t_args("model-export-beyond", &[("n", &n.to_string()), ("all", all), ("max", &max), ("format", f.kind.label())]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Esc on the window of Export as a 3D model, opened from the viewer, closes the window and
    /// leaves the viewer open; Enter exports (Piero, 23/11: Esc closed the viewer and the window
    /// stayed over the grid).
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn esc_closes_the_export_window_not_the_viewer() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        if own_folder("model_export_page::tests::esc_closes_the_export_window_not_the_viewer") {
            return;
        }
        let dir = folder("export-esc", &["a.obj"]);
        let mut h = Harness::new(&dir);
        h.app.viewing = Some(0);
        h.frame(Vec::new());
        h.app.open_model_export(None);
        h.frame(Vec::new());
        assert!(h.app.model_export.is_some(), "the window opened");
        h.key(egui::Key::Escape);
        h.frame(Vec::new());
        assert!(h.app.model_export.is_none(), "Esc should close the export window");
        assert!(h.app.viewing.is_some(), "Esc on the export window closed the viewer");
    }

    /// The model open in the viewer, of 12 million triangles: OBJ (10 million) warned, GLB (20)
    /// not; with three models chosen, two of them read and beyond, said of the two (Piero,
    /// 22/11/2026). In both languages.
    #[test]
    fn beyond_the_cap_of_the_new_format_d1() {
        let (en, it) = (I18n::new("en"), I18n::new("it"));
        let mut f = Form { path: None, models: 1, kind: Kind::Obj, known: vec![12_000_000] };
        assert_eq!(beyond_line(&en, &f).as_deref(), Some("The model has more than 10 million triangles: EyeCanSee will not be able to open the new OBJ file. You can still export it."));
        assert_eq!(beyond_line(&it, &f).as_deref(), Some("Il modello ha più di 10 milioni di triangoli: EyeCanSee non potrà aprire il nuovo file OBJ. Puoi esportarlo lo stesso."));
        f.kind = Kind::Glb;
        assert_eq!(beyond_line(&en, &f), None);
        f.kind = Kind::Ply;
        assert_eq!(beyond_line(&en, &f), None);
        f = Form { path: None, models: 3, kind: Kind::Stl, known: vec![25_000_000, 21_000_000] };
        assert_eq!(beyond_line(&en, &f).as_deref(), Some("2 of the models have more than 20 million triangles: EyeCanSee will not be able to open the new STL files. You can still export them."));
        assert_eq!(beyond_line(&it, &f).as_deref(), Some("2 dei modelli hanno più di 20 milioni di triangoli: EyeCanSee non potrà aprire i nuovi file STL. Puoi esportarli lo stesso."));
        f.known = vec![25_000_000, 1];
        assert_eq!(beyond_line(&en, &f).as_deref(), Some("One of the models has more than 20 million triangles: EyeCanSee will not be able to open the new STL file. You can still export it."));
        assert_eq!(beyond_line(&it, &f).as_deref(), Some("Uno dei modelli ha più di 20 milioni di triangoli: EyeCanSee non potrà aprire il nuovo file STL. Puoi esportarlo lo stesso."));
    }
}
