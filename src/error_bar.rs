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

/// The text's size and line height, the detail's, and the space above the text.
#[derive(Clone, Copy)]
struct Look {
    text: (f32, f32),
    detail: (f32, f32),
    top: f32,
}

/// The bar's: the text level with the 28-point triangle.
const BAR: Look = Look { text: (15.0, 20.0), detail: (14.0, 19.0), top: 3.0 };

impl App {
    /// The last error, at the bottom of the window with a ✕ to close it. Over the pages that
    /// cover the window too (editor, Save in another format, duplicates, slideshow): drawn only
    /// in the grid and the viewer, a save that failed in the editor was told once it was closed.
    /// The slideshow goes on above it: the error is seen, and its ✕ closes it.
    /// It stays at the bottom, where the status bar is: at the top it would push the toolbar and
    /// the address bar down every time an error comes and goes.
    pub(crate) fn error_bar(&mut self, ui: &mut egui::Ui) {
        // What the folder or the viewer shows instead of pictures: in the bar while it lasts.
        let lasting = self.folder_error().or_else(|| self.viewer_error()).filter(|e| !e.is_empty());
        let (shown, closed) = bar_choice(self.error.clone(), lasting, self.lasting_closed.take());
        self.lasting_closed = closed;
        let Some(e) = shown else { return };
        let c = theme::error_bar(ui.visuals());
        let frame = egui::Frame::new().fill(c.fill).inner_margin(egui::Margin { left: 20, right: 12, top: 10, bottom: 10 });
        let (close, copy) = (self.i18n.t("notice-close"), self.i18n.t("notice-copy"));
        let panel = egui::Panel::bottom("error").frame(frame).resizable(false).show_separator_line(false).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                theme::warning_icon(ui, 28.0);
                ui.add_space(12.0);
                // All the width but Copy and the ✕: a cap of 720 points broke sentences in the middle on
                // a wide window ("…known_hosts, / quindi…", Piero 20/11); the lines break where the message does.
                let width = ui.available_width() - 76.0;
                ui.vertical(|ui| {
                    ui.set_max_width(width);
                    bar_text(ui, &e, &c, BAR);
                });
                // At the right, level with the first line of the text (3 points under the top): the ✕,
                // and Copy before it, the whole text with the detail (it could not be copied, Piero 20/11).
                ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                    ui.add_space(3.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        let closed = ui.add(crate::icons::small(ui, crate::icons::Line::Close)).on_hover_text(close).clicked();
                        if ui.add(crate::icons::small(ui, crate::icons::Line::Copy)).on_hover_text(copy).clicked() {
                            ui.ctx().copy_text(e.clone());
                        }
                        closed
                    })
                    .inner
                })
                .inner
            })
            .inner
        });
        if panel.inner && self.error.take().is_none() {
            self.lasting_closed = Some(e);
        }
        // The band at the left and the line along the top, over the panel's whole rectangle.
        let r = panel.response.rect;
        let p = ui.painter();
        p.rect_filled(egui::Rect::from_min_max(r.min, egui::pos2(r.min.x + 6.0, r.max.y)), 0.0, c.band);
        p.hline(r.x_range(), r.min.y + 0.5, egui::Stroke::new(1.0, c.line));
    }

    /// Why the open folder shows nothing (a phone or a server that does not open, a folder that
    /// cannot be read, a phone without storage), for the bar: the grid has only its start
    /// (`tree_line`). It was red, on one long line, in the middle of the grid (Piero, 20/11). It
    /// goes when the folder opens. Closed with the ✕ it stays closed while the text is the same:
    /// a phone that is not there is tried again every 2 seconds, with the same message.
    pub(crate) fn folder_error(&self) -> Option<String> {
        let text = match self.listings.get(&self.dir)? {
            Err(e) => e.clone(),
            Ok(l) if l.dirs.is_empty() && l.files.is_empty() && self.dir.is_empty() => {
                self.i18n.t_args("error-no-storage", &[("detail", &self.phone_root(&self.dir)?.name)])
            }
            Ok(_) => return None,
        };
        Some(text)
    }

    /// Why the viewer shows no picture: one that cannot be read, a video that does not play
    /// (not when a card says it: the engine to download, a picture too large, a drawing that needs
    /// the CAD engine). In the bar as the folder's (`folder_error`); it was red in the middle.
    pub(crate) fn viewer_error(&self) -> Option<String> {
        let current = self.viewing?;
        let path = &self.files().get(current)?.path;
        if let Some(video) = self.video_error(path) {
            return video;
        }
        match self.full.get(&current)? {
            Err(e) if !crate::occt::needs_engine(path) && crate::view_memory::need_of(e).is_none() => Some(self.i18n.cannot_load(crate::source::shown(path), e)),
            _ => None,
        }
    }
}

/// What the bar shows: the last error, otherwise the folder's or the viewer's (`lasting`) unless
/// it was closed with the ✕; and what stays closed: the text closed, while that error lasts (the
/// phone tried again every 2 seconds gives the same text back), forgotten once it is gone.
fn bar_choice(error: Option<String>, lasting: Option<String>, closed: Option<String>) -> (Option<String>, Option<String>) {
    let closed = closed.filter(|_| lasting.is_some());
    let lasting = lasting.filter(|l| closed.as_ref() != Some(l));
    (error.or(lasting), closed)
}

/// Whether a window says the open folder's error `id`, and asks what to do: KDE holding the phone,
/// a server's key or certificate to confirm (when its fingerprint is there to show: otherwise no
/// window comes), a login refused (unless File > Open location is open already: then it is not
/// opened again). The bar is silent on it then, and after Cancel too, while the same error lasts
/// (`bar_choice`, as closed with the ✕): it said it twice (Piero, 20/11). The grid keeps its start.
pub(crate) fn said_in_window(id: &str, fingerprint: bool, address_open: bool) -> bool {
    match id {
        "error-device-kde" | "error-device-kde-back" => true,
        "error-host-unknown" | "error-cert-unknown" => fingerprint,
        "error-login" | "error-login-ssh" => !address_open,
        _ => false,
    }
}

/// `message` with `raw`, the library's own text at its end, taken out and put under it as the
/// detail, in brackets on its own line: the bar and `inline` draw it fainter and smaller. The ":"
/// left hanging goes. Unchanged when `raw` is not at the end, is empty or already in brackets.
pub(crate) fn with_detail(message: &str, raw: &str) -> String {
    let raw = raw.trim();
    // On more lines it is a sentence already said, with its own detail.
    if raw.is_empty() || raw.starts_with('(') || raw.contains('\n') {
        return message.to_string();
    }
    match message.trim_end().strip_suffix(raw).map(|head| head.trim_end().trim_end_matches(':').trim_end()) {
        Some(head) if !head.is_empty() => format!("{head}\n({raw})"),
        _ => message.to_string(),
    }
}

/// A message inside a window or a page (an address refused, a picture that does not open, a
/// choice that cannot be made): the bar's small triangle and text, without its background, where
/// the message was red (Piero, 20/11: no red text anywhere).
pub(crate) fn inline(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let c = theme::error_bar(ui.visuals());
    // The bar's sizes (the detail at 14, Piero 20/11), with the smaller triangle level with the
    // first line.
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.add_space(1.0);
            theme::warning_icon(ui, 18.0);
        });
        ui.add_space(8.0);
        ui.vertical(|ui| bar_text(ui, text, &c, Look { top: 0.0, ..BAR }));
    })
    .response
}

/// A hint while something is typed or chosen (a port that is not a number, an empty name, a
/// size too large): no triangle, the theme's text, wrapped. Not an error, so not red either.
pub(crate) fn hint(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Label::new(egui::RichText::new(text).color(ui.visuals().text_color())).wrap())
}

/// `inline` on the viewer's dark cards, in both themes.
pub(crate) fn inline_on_dark(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.scope(|ui| {
        theme::on_dark(ui);
        inline(ui, text)
    })
    .inner
}

/// The error under a phone or a server in the tree, and in the middle of the grid: the start of
/// the message only (`bar_parts`), on one line; the whole message is in the bar. The first line
/// when its start is too long to be one.
pub(crate) fn tree_line(text: &str) -> &str {
    let parts = bar_parts(text);
    if parts.lead.is_empty() { parts.rest } else { parts.lead }
}

/// `tree_line` in the bar's fainter colour, for a label on one line.
pub(crate) fn short_text(ui: &egui::Ui, text: &str) -> egui::RichText {
    egui::RichText::new(tree_line(text)).color(theme::error_bar(ui.visuals()).detail)
}

/// `short_text` in the middle of the viewer, dark in both themes: the message is in the bar.
pub(crate) fn short_on_dark(ui: &mut egui::Ui, text: &str) {
    ui.scope(|ui| {
        theme::on_dark(ui);
        ui.centered_and_justified(|ui| ui.add(egui::Label::new(short_text(ui, text).size(18.0)).truncate().selectable(false)));
    });
}

/// The text of the bar (`bar_parts`): the first line with its start in bold, the next lines one a
/// line, the detail fainter and smaller.
fn bar_text(ui: &mut egui::Ui, text: &str, c: &theme::BarColours, look: Look) {
    let parts = bar_parts(text);
    let format = |(size, height): (f32, f32), family: FontFamily, color| egui::TextFormat { font_id: FontId::new(size, family), color, line_height: Some(height), ..Default::default() };
    let line = |ui: &mut egui::Ui, job: LayoutJob| ui.add(egui::Label::new(job).wrap());
    let single = |text: &str, size, color| LayoutJob::single_section(text.to_string(), format(size, FontFamily::Proportional, color));
    ui.spacing_mut().item_spacing.y = 3.0;
    ui.add_space(look.top);
    let mut first = LayoutJob::default();
    first.append(parts.lead, 0.0, format(look.text, FontFamily::Name(BOLD.into()), c.lead));
    first.append(parts.rest, 0.0, format(look.text, FontFamily::Proportional, c.text));
    line(ui, first);
    for l in parts.lines {
        line(ui, single(l, look.text, c.text));
    }
    if !parts.detail.is_empty() {
        ui.add_space(3.0);
        line(ui, single(&parts.detail.join("\n"), look.detail, c.detail));
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

    /// The tree's line: the bold start, the first line when there is none, nothing for no text.
    #[test]
    fn tree_lines() {
        assert_eq!(tree_line("Cannot save: the disk is full.\n(busy)"), "Cannot save");
        assert_eq!(tree_line("The folder is empty."), "The folder is empty.");
        let long = format!("{}: rest\nmore", "è".repeat(91));
        assert_eq!(tree_line(&long), format!("{}: rest", "è".repeat(91)));
        assert_eq!(tree_line(""), "");
        let it = I18n::new("it");
        let device = it.t_args("error-no-device", &[("detail", "USB 3-2: no MTP device")]);
        assert_eq!(tree_line(&device), "Il dispositivo non è stato trovato");
    }

    /// The ✕ closes the folder's error while the same text comes back; a new text, or the same
    /// one after the folder opened, shows again; an error of now goes first.
    #[test]
    fn closed_while_it_lasts() {
        let s = |t: &str| Some(t.to_string());
        assert_eq!(bar_choice(None, s("no phone"), s("no phone")), (None, s("no phone")));
        assert_eq!(bar_choice(None, s("no storage"), s("no phone")), (s("no storage"), s("no phone")));
        assert_eq!(bar_choice(None, None, s("no phone")), (None, None));
        assert_eq!(bar_choice(None, s("no phone"), None), (s("no phone"), None));
        assert_eq!(bar_choice(s("not saved"), s("no phone"), s("no phone")), (s("not saved"), s("no phone")));
    }

    /// A refused login, a key to confirm, KDE holding the phone: in their window only, also
    /// after Cancel; the bar speaks again for another error. No window, no silence.
    #[test]
    fn said_in_its_window_only() {
        assert!(said_in_window("error-login", false, false) && said_in_window("error-login-ssh", true, false));
        assert!(said_in_window("error-device-kde", false, true) && said_in_window("error-device-kde-back", false, false));
        assert!(said_in_window("error-host-unknown", true, false) && said_in_window("error-cert-unknown", true, true));
        // Open location already open does not open again; a key without its fingerprint asks nothing.
        assert!(!said_in_window("error-login", false, true) && !said_in_window("error-host-unknown", false, false));
        assert!(!said_in_window("error-no-device", true, false) && !said_in_window("error-key-failed", true, false));
        // Marked as closed when the window opens: silent while it lasts, after Cancel too.
        let s = |t: &str| Some(t.to_string());
        let refused = I18n::new("it").error("error-login-ssh", "nas.local");
        assert_eq!(bar_choice(None, s(&refused), s(&refused)), (None, s(&refused)));
        assert_eq!(bar_choice(None, s("no phone"), s(&refused)), (s("no phone"), s(&refused)));
    }

    /// The library's text at the end goes under as the detail, without the ":" left hanging.
    #[test]
    fn details_apart() {
        assert_eq!(with_detail("Cannot open a.jpg\nFormat error: x", "Format error: x"), "Cannot open a.jpg\n(Format error: x)");
        assert_eq!(with_detail("Cannot convert: timeout", "timeout"), "Cannot convert\n(timeout)");
        assert_eq!(with_detail("Cannot convert: timeout", "other"), "Cannot convert: timeout");
        assert_eq!(with_detail("timeout", "timeout"), "timeout");
        assert_eq!(with_detail("Cannot convert: (busy)", "(busy)"), "Cannot convert: (busy)");
        assert_eq!(with_detail("Cannot convert: ", ""), "Cannot convert: ");
        // A sentence already said, on more lines, is not put in brackets again.
        assert_eq!(with_detail("Cannot open a.jpg\nToo large: x\n(y)", "Too large: x\n(y)"), "Cannot open a.jpg\nToo large: x\n(y)");
        let it = I18n::new("it");
        let text = it.cannot_load("rotta.jpg", "Format error decoding Jpeg: I/O errors");
        let p = bar_parts(&text);
        assert_eq!((p.lead, p.detail), ("Impossibile aprire rotta.jpg", vec!["(Format error decoding Jpeg: I/O errors)"]));
        // A detail not recognised by `I18n::error`: apart; a name or an address stays in the sentence.
        let source = it.error("error-source", "boom 42");
        assert_eq!((bar_parts(&source).lead, bar_parts(&source).detail), ("Impossibile collegarsi alla sorgente", vec!["(boom 42)"]));
        assert_eq!(it.error("error-path", "x/y.jpg"), "Impossibile aprire x/y.jpg");
        // Said plainly: under the start, not in brackets.
        let big = it.cannot_load("a.jpg", &format!("error-image-too-large\u{1}{}", "60000 × 60000"));
        assert_eq!(bar_parts(&big).detail.len(), 0, "{big}");
    }

    /// The real messages, in Italian: the trash with a reason said plainly on two lines.
    #[test]
    fn real_messages() {
        let it = I18n::new("it");
        let fault = it.fault(crate::fault::Kind::NoSpaceToReplace, Some("x.jpg"));
        let trash = it.t_args("error-trash-failed", &[("detail", &fault)]);
        let p = bar_parts(&trash);
        assert_eq!((p.lead, p.lines.len(), p.detail.len()), ("Non è stato possibile spostare l'originale nel cestino", 0, 2), "{trash}");
        let device = it.t_args("error-no-device", &[("detail", "USB 3-2: no MTP device")]);
        let device = bar_parts(&device);
        assert_eq!((device.lead, device.lines.len(), device.detail), ("Il dispositivo non è stato trovato", 1, vec!["(USB 3-2: no MTP device)"]));
        let host = it.t_args("error-host-changed", &[("detail", "nas.local (known_hosts, line 12)")]);
        let host = bar_parts(&host);
        assert_eq!((host.lead, host.lines.len(), host.detail.len()), ("La chiave di nas.local (known_hosts, line 12) è cambiata", 2, 0));
    }

    /// In the app (no screen): what failed is said in the error bar, to close and to copy, not in
    /// a note of the status bar that goes by itself. Two drawings that could not be compared (the
    /// note "Comparing…" goes too), and the system's eyedropper that gave no colour, in the editor.
    #[test]
    #[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
    fn failures_in_the_bar_not_in_a_passing_note() {
        use crate::audit_family_1::{Harness, folder, own_folder};
        use crate::loader::Msg;
        if own_folder("error_bar::tests::failures_in_the_bar_not_in_a_passing_note") {
            return;
        }
        let mut h = Harness::new(&folder("failures", &["a.png"]));
        h.app.status_note = Some(h.app.i18n.t("compare-reading"));
        h.app.note_until = Some(1e9);
        h.app.tx.send(Msg::Compared(Err("a.dwg\u{1}not a drawing".into()))).unwrap();
        h.frame(Vec::new());
        assert_eq!(h.app.error, Some(h.app.i18n.t_args("compare-failed", &[("error", &h.app.i18n.cannot_load("a.dwg", "not a drawing"))])));
        assert_eq!(bar_parts(h.app.error.as_deref().unwrap()).detail, ["(not a drawing)"]);
        assert!(h.app.status_note.is_none() && h.app.note_until.is_none(), "the note of the comparison stays");
        h.app.error = None;
        let a = h.app.files()[0].path.clone();
        h.app.open_editor(Some(a), crate::edit_page::Tool::Select);
        h.frame(Vec::new());
        assert!(h.app.editor.is_some(), "the editor did not open");
        h.app.tx.send(Msg::EditPicked(Err("no portal".into()))).unwrap();
        h.frame(Vec::new());
        assert_eq!(h.app.error, Some(h.app.i18n.t_args("edit-pick-failed", &[("detail", "no portal")])));
        h.frame(Vec::new());
        assert!(h.app.error.is_some(), "the error left the editor's page with the next frame");
    }
}
