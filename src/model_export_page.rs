//! "Export as a 3D model…": a window, as "Export drawing…" has one: the models chosen, the format
//! to write, then Export; each copy goes next to its model (`App::export_model`).

use eframe::egui;

use crate::App;
use crate::i18n::I18n;
use crate::model_export::Kind;

/// The window open: the models it works on (`None` = those chosen when it opened), the format.
pub(crate) struct Form {
    pub(crate) path: Option<String>,
    pub(crate) models: usize,
    pub(crate) kind: Kind,
}

enum Answer {
    Export,
    Cancel,
}

impl App {
    /// File > Export as a 3D model… and the same entry of the right-click menus: the window, on the
    /// format of the last export (STL at first).
    pub(crate) fn open_model_export(&mut self, path: Option<String>) {
        let models = self.files_chosen(path.clone()).iter().filter(|p| crate::model::is_model(p)).count();
        if models > 0 {
            self.model_export = Some(Form { path, models, kind: self.model_export_kind });
        }
    }

    pub(crate) fn model_export_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.model_export.take() else { return };
        let mut answer = None;
        let window = egui::Window::new(self.i18n.t("model-export-title")).collapsible(false).resizable(false);
        window.anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
            answer = form(ui, &mut f, &self.i18n);
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
