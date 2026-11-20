//! The error bar at the bottom of the window: the last error, with the warning triangle and a ✕.
//! It speaks as the message windows (Piero, 20/11, of the yellow band before: "brutto da leggere
//! come colore e come testo, non si capisce nulla"): the theme's background and text, the start
//! of the first sentence in bold, one sentence a line, the detail in brackets fainter and smaller
//! under it. The triangle and an amber band at the left say it is a warning.

use eframe::egui::{self, FontFamily, FontId, text::LayoutJob};

use crate::{App, BOLD, theme};

/// The bar's text split for drawing (`bar_parts`).
#[derive(Debug, PartialEq)]
pub(crate) struct BarParts<'a> {
    /// The start of the first line, in bold: up to its first ": ".
    pub lead: &'a str,
    /// The rest of the first line, after the bold start, in the same paragraph.
    pub rest: &'a str,
    /// The lines after the first, before the detail.
    pub lines: Vec<&'a str>,
    /// The detail in brackets at the end, on one line or more (brackets kept).
    pub detail: Vec<&'a str>,
}

/// A bold start longer than this is no start: the whole line goes plain.
const LEAD_MAX: usize = 90;

/// Splits an error's text for the bar. The empty lines go. The detail is the last lines from the
/// last one starting with "(" (never the first), when the text ends with ")": a detail said plainly
/// can take two lines (`fault-no-space-replace` inside `error-trash-failed`). The bold start runs
/// to the first ": " of the first line (a colon and a space: `C:\` and `sftp://` are not one); a
/// line without one is bold whole, and a start over 90 characters is not bold.
pub(crate) fn bar_parts(text: &str) -> BarParts<'_> {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let Some((&first, others)) = lines.split_first() else { return BarParts { lead: "", rest: "", lines: Vec::new(), detail: Vec::new() } };
    let detail_at = match others.last() {
        Some(last) if last.ends_with(')') => others.iter().rposition(|l| l.starts_with('(')),
        _ => None,
    };
    let (others, detail) = others.split_at(detail_at.unwrap_or(others.len()));
    let (lead, rest) = match first.find(": ") {
        Some(at) => first.split_at(at),
        None => (first, ""),
    };
    let (lead, rest) = if lead.chars().count() > LEAD_MAX { ("", first) } else { (lead, rest) };
    BarParts { lead, rest, lines: others.to_vec(), detail: detail.to_vec() }
}

/// The text's size and line height, and the detail's.
const TEXT: (f32, f32) = (15.0, 20.0);
const DETAIL: (f32, f32) = (14.0, 19.0);

impl App {
    /// The last error, at the bottom of the window with a ✕ to close it. Over the pages that
    /// cover the window too (editor, Save in another format, duplicates, slideshow): drawn only
    /// in the grid and the viewer, a save that failed in the editor was told once it was closed.
    /// The slideshow goes on above it: the error is seen, and its ✕ closes it.
    /// It stays at the bottom, where the status bar is: at the top it would push the toolbar and
    /// the address bar down every time an error comes and goes.
    pub(crate) fn error_bar(&mut self, ui: &mut egui::Ui) {
        let Some(e) = self.error.clone() else { return };
        let c = theme::error_bar(ui.visuals());
        let frame = egui::Frame::new().fill(c.fill).inner_margin(egui::Margin { left: 20, right: 12, top: 10, bottom: 10 });
        let close = self.i18n.t("notice-close");
        let panel = egui::Panel::bottom("error").frame(frame).resizable(false).show_separator_line(false).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                theme::warning_icon(ui, 28.0);
                ui.add_space(12.0);
                // All the width but the ✕: a cap of 720 points broke sentences in the middle on a wide
                // window ("…known_hosts, / quindi…", Piero 20/11); the lines break where the message does.
                let width = ui.available_width() - 44.0;
                ui.vertical(|ui| {
                    ui.set_max_width(width);
                    bar_text(ui, &e, &c);
                });
                // At the right, level with the first line of the text (3 points under the top).
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.add_space(3.0);
                    ui.add(crate::icons::small(ui, crate::icons::Line::Close)).on_hover_text(close).clicked()
                })
                .inner
            })
            .inner
        });
        if panel.inner {
            self.error = None;
        }
        // The band at the left and the line along the top, over the panel's whole rectangle.
        let r = panel.response.rect;
        let p = ui.painter();
        p.rect_filled(egui::Rect::from_min_max(r.min, egui::pos2(r.min.x + 6.0, r.max.y)), 0.0, c.band);
        p.hline(r.x_range(), r.min.y + 0.5, egui::Stroke::new(1.0, c.line));
    }
}

/// The text of the bar (`bar_parts`): the first line with its start in bold, the next lines one a
/// line, the detail fainter and smaller.
fn bar_text(ui: &mut egui::Ui, text: &str, c: &theme::BarColours) {
    let parts = bar_parts(text);
    let format = |(size, height): (f32, f32), family: FontFamily, color| egui::TextFormat { font_id: FontId::new(size, family), color, line_height: Some(height), ..Default::default() };
    let line = |ui: &mut egui::Ui, job: LayoutJob| ui.add(egui::Label::new(job).wrap());
    let single = |text: &str, size, color| LayoutJob::single_section(text.to_string(), format(size, FontFamily::Proportional, color));
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.add_space(3.0);
    let mut first = LayoutJob::default();
    first.append(parts.lead, 0.0, format(TEXT, FontFamily::Name(BOLD.into()), c.lead));
    first.append(parts.rest, 0.0, format(TEXT, FontFamily::Proportional, c.text));
    line(ui, first);
    for l in parts.lines {
        line(ui, single(l, TEXT, c.text));
    }
    if !parts.detail.is_empty() {
        ui.add_space(3.0);
        line(ui, single(&parts.detail.join("\n"), DETAIL, c.detail));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::I18n;

    fn parts<'a>(lead: &'a str, rest: &'a str, lines: &[&'a str], detail: &[&'a str]) -> BarParts<'a> {
        BarParts { lead, rest, lines: lines.to_vec(), detail: detail.to_vec() }
    }

    /// One line: bold up to the first ": ", or whole without one.
    #[test]
    fn one_line() {
        assert_eq!(bar_parts("Cannot save: the disk is full."), parts("Cannot save", ": the disk is full.", &[], &[]));
        assert_eq!(bar_parts("The folder is empty."), parts("The folder is empty.", "", &[], &[]));
        assert_eq!(bar_parts(""), parts("", "", &[], &[]));
        // A single line ending with ")" has no detail apart.
        assert_eq!(bar_parts("Cannot open it (busy)"), parts("Cannot open it (busy)", "", &[], &[]));
    }

    /// `C:\` and `sftp://` are not the end of the start: a colon and a space are.
    #[test]
    fn colons_in_addresses() {
        assert_eq!(bar_parts(r"C:\Photos cannot be read: access denied."), parts(r"C:\Photos cannot be read", ": access denied.", &[], &[]));
        assert_eq!(bar_parts("sftp://nas/x cannot be read: timeout"), parts("sftp://nas/x cannot be read", ": timeout", &[], &[]));
        assert_eq!(bar_parts(r"C:\Photos\x.jpg"), parts(r"C:\Photos\x.jpg", "", &[], &[]));
    }

    /// A start over 90 characters is not bold (counted in characters, not bytes).
    #[test]
    fn long_start_plain() {
        let long = format!("{}: rest", "è".repeat(91));
        assert_eq!(bar_parts(&long), parts("", &long, &[], &[]));
        let ninety = format!("{}: rest", "è".repeat(90));
        assert_eq!(bar_parts(&ninety).lead.chars().count(), 90);
    }

    /// The detail on one line, on two, and the empty lines left out.
    #[test]
    fn detail() {
        let one = "Not found: unlock it.\n\nIt retries by itself.\n(USB 3-2: no MTP device)";
        assert_eq!(bar_parts(one), parts("Not found", ": unlock it.", &["It retries by itself."], &["(USB 3-2: no MTP device)"]));
        let two = "Not moved: unchanged.\n(No space for \"x\".\nFree some space.)";
        assert_eq!(bar_parts(two), parts("Not moved", ": unchanged.", &[], &["(No space for \"x\".", "Free some space.)"]));
        // A last line ending with ")" and no line starting with "(": no detail.
        assert_eq!(bar_parts("A: b.\nc (d)"), parts("A", ": b.", &["c (d)"], &[]));
        // The first line is never the detail.
        assert_eq!(bar_parts("(a): b\nc)"), parts("(a)", ": b", &["c)"], &[]));
        // A line starting with "(" before the end, with the last not ending with ")".
        assert_eq!(bar_parts("A: b.\n(c)\nd"), parts("A", ": b.", &["(c)", "d"], &[]));
    }

    /// The real messages of 0.77.0, in Italian, still on one line each (they go to a new line between
    /// sentences from 0.78.0): the start before ": " in bold, nothing else apart.
    #[test]
    fn real_messages() {
        let it = I18n::new("it");
        let fault = it.fault(crate::fault::Kind::NoSpaceToReplace, Some("x.jpg"));
        let trash = it.t_args("error-trash-failed", &[("detail", &fault)]);
        assert_eq!(bar_parts(&trash).lead, "Non è stato possibile spostare l'originale nel cestino", "{trash}");
        let device = it.t_args("error-no-device", &[("detail", "USB 3-2: no MTP device")]);
        assert_eq!(bar_parts(&device).lead, "Il dispositivo non è stato trovato");
    }
}
