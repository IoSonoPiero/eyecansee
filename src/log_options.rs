//! Options > Diagnostics (#189): the switch of the diagnostic log (`logging`), from the next start,
//! and a button that opens the folder of the logs, to attach one to a report. With `EYECANSEE_LOG`
//! set the variable decides, and the page says so.

use eframe::egui;

use crate::i18n::I18n;
use crate::settings::Settings;

/// What the page says about the log of this start.
#[derive(Debug, PartialEq)]
pub(crate) enum Now {
    /// `EYECANSEE_LOG` is set: the switch is not read.
    Variable,
    /// This start writes in that file (the switch was on when it started).
    File(String),
    /// Nothing written by this start (or not in a file of the switch).
    Nothing,
}

pub(crate) fn now() -> Now {
    if crate::logging::by_variable() {
        return Now::Variable;
    }
    crate::logging::file_now().map_or(Now::Nothing, |p| Now::File(p.to_string_lossy().into_owned()))
}

pub(crate) fn options_ui(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n) {
    draw(ui, s, i18n, &now());
}

fn draw(ui: &mut egui::Ui, s: &mut Settings, i18n: &I18n, now: &Now) {
    let t = |id| i18n.t(id);
    let variable = *now == Now::Variable;
    ui.add_enabled_ui(!variable, |ui| {
        ui.checkbox(&mut s.diagnostic_log, t("opt-log")).on_hover_text(t("opt-log-hint"));
    });
    ui.indent("log-note", |ui| {
        ui.weak(t("opt-engine-restart"));
        ui.weak(t("opt-log-private"));
    });
    ui.add_space(6.0);
    match now {
        Now::Variable => {
            ui.label(t("opt-log-variable"));
        }
        Now::File(file) => {
            ui.label(i18n.t_args("opt-log-now", &[("file", file)]));
        }
        Now::Nothing => {}
    }
    ui.add_space(6.0);
    let folder = crate::logging::folder();
    if ui.add_enabled(folder.is_some(), egui::Button::new(t("opt-log-folder"))).clicked()
        && let Some(folder) = folder
    {
        // Made if needed: the button works also before the first log.
        let made = std::fs::create_dir_all(&folder);
        log!("options: folder of the logs {folder:?}: {made:?}");
        crate::kept::open_path(&folder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons_ui::tests::{click, headless};

    /// A click on the switch turns the log on and off for the next start; with the variable set
    /// the switch is grey and a click changes nothing. In both languages the texts are there.
    #[test]
    fn the_switch_grey_with_the_variable() {
        let (ctx, area) = headless();
        let at = area.min + egui::vec2(8.0, 8.0);
        for lang in ["en", "it"] {
            let i18n = I18n::new(lang);
            let mut s = Settings::default();
            for (now, on) in [(Now::Nothing, true), (Now::File("/data/logs/eyecansee-2026-11-22_143005.log".into()), false), (Now::Variable, false)] {
                click(&ctx, at, |ui| draw(ui, &mut s, &i18n, &now));
                assert_eq!(s.diagnostic_log, on, "{lang} {now:?}");
            }
            assert!(i18n.t("opt-log-variable").contains("EYECANSEE_LOG"), "{lang}");
            assert!(i18n.t_args("opt-log-now", &[("file", "/x.log")]).contains("/x.log"), "{lang}");
        }
    }
}
