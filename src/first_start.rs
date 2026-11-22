//! The first-start window (#212): which pages it shows at this start, and what closing it
//! remembers. Everyone sees the welcome page and the add-ons once; a later version brings the
//! window back, with the add-ons page alone, only for a new add-on that has something to offer.
//! The window itself is in `first_start_ui.rs`.

use crate::settings::Settings;

/// A page of the window, in the order shown. A later page (the CJK font of the PDF engine) is
/// one more here, with its rule in `at_start`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    /// Logo, language, colors (and on Windows the desktop icon, #210).
    Welcome,
    Addons,
}

/// What this start shows: the pages, and the add-ons (`Component::kind`) of the add-ons page.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct Plan {
    pub(crate) pages: Vec<Page>,
    pub(crate) addons: Vec<&'static str>,
}

/// The add-ons of this system as the rule sees them: the kind, and whether it has something to
/// offer here (neither downloaded nor on the system: `component::Have::Missing` or `Older`).
pub(crate) type Offers<'a> = &'a [(&'static str, bool)];

/// The pages to show at this start; `None`: no window. Started with an address (a double click on
/// a picture) it waits for the next normal start. A start that shows it shows no tip of the day:
/// today counts as the tip's day, the tip comes tomorrow (as #210 decided for its question).
/// With nothing to show, a new add-on that offers nothing here (already downloaded or on the
/// system) is remembered as offered at once: it never brings the window back (Piero, 21/11).
pub(crate) fn at_start(s: &mut Settings, addons: Offers, opened_with_address: bool, today: u64) -> Option<Plan> {
    if opened_with_address {
        return None;
    }
    let plan = if s.welcome_seen {
        let new: Vec<_> = addons.iter().filter(|(kind, offers)| *offers && !offered(s, kind)).map(|a| a.0).collect();
        (!new.is_empty()).then(|| Plan { pages: vec![Page::Addons], addons: new })
    } else {
        Some(Plan { pages: vec![Page::Welcome, Page::Addons], addons: addons.iter().map(|a| a.0).collect() })
    };
    match &plan {
        Some(_) => s.tips_day = Some(today),
        None => remember(s, addons.iter().map(|a| a.0)),
    }
    plan
}

/// The window closed (Done, Skip or Esc, and either answer that closes of the question asked
/// when ticked cards are left behind, `answered`): the welcome is seen, and every add-on it showed
/// is offered.
pub(crate) fn closed(s: &mut Settings, plan: &Plan) {
    if plan.pages.contains(&Page::Welcome) {
        s.welcome_seen = true;
    }
    remember(s, plan.addons.iter().copied());
}

/// A card of the add-ons page as the rule on closing sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Card {
    pub(crate) kind: &'static str,
    pub(crate) ticked: bool,
    /// Something to download here: neither downloaded nor on the system.
    pub(crate) offers: bool,
    /// Its download is under way or has arrived (one that failed has not: it can start again).
    pub(crate) started: bool,
}

/// The ticked cards left behind when the add-ons page closes: ticked, with something to download
/// and no download started. With any of them, Done, Skip and Esc ask before closing, from either page (Piero, 22/11: who
/// ticks add-ons and closes without pressing Download must be told).
pub(crate) fn waiting(cards: &[Card]) -> Vec<&'static str> {
    cards.iter().filter(|c| c.ticked && c.offers && !c.started).map(|c| c.kind).collect()
}

/// The answers to that question.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Answer {
    /// Download and close.
    Download,
    /// Close without downloading.
    Close,
    /// Back to the page, the ticks as they were.
    Back,
}

/// What follows an answer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum After {
    /// Back: the window stays open as it was.
    Stay,
    /// Close without downloading: closed as Done closes it.
    Closed,
    /// Download and close: the window stays open on the add-ons page, these downloads start
    /// there with their bars, and it closes by itself once they have all arrived (`ended`).
    Downloading(Vec<&'static str>),
}

/// What an answer does (Piero, 22/11, after trying the copy: Download and close does not close
/// at once and download behind the program, it downloads in the window and then closes).
pub(crate) fn answered(s: &mut Settings, plan: &Plan, waiting: Vec<&'static str>, answer: Answer) -> After {
    match answer {
        Answer::Back => After::Stay,
        Answer::Download => After::Downloading(waiting),
        Answer::Close => {
            closed(s, plan);
            After::Closed
        }
    }
}

/// Where a download started by Download and close is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Download {
    Running,
    Arrived,
    Failed,
}

/// The downloads of Download and close: `Some(true)` all arrived, the window closes by itself;
/// `Some(false)` one failed, the window stays open with its error and closes with Done, as after
/// Download; `None` still under way.
pub(crate) fn ended(downloads: &[Download]) -> Option<bool> {
    if downloads.contains(&Download::Failed) {
        Some(false)
    } else if downloads.contains(&Download::Running) {
        None
    } else {
        Some(true)
    }
}

fn offered(s: &Settings, kind: &str) -> bool {
    s.addons_offered.iter().any(|k| k == kind)
}

fn remember(s: &mut Settings, kinds: impl Iterator<Item = &'static str>) {
    for kind in kinds {
        if !offered(s, kind) {
            s.addons_offered.push(kind.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 20_000;
    const ALL: Offers = &[("libmpv", true), ("ffmpeg", false), ("occt", true)];

    fn both(addons: &[&'static str]) -> Option<Plan> {
        Some(Plan { pages: vec![Page::Welcome, Page::Addons], addons: addons.to_vec() })
    }

    /// The first start: both pages, every add-on (also one already on the system, which says so),
    /// and no tip of the day.
    #[test]
    fn first_start_both_pages_every_addon() {
        let mut s = Settings::default();
        assert_eq!(at_start(&mut s, ALL, false, DAY), both(&["libmpv", "ffmpeg", "occt"]));
        assert_eq!(s.tips_day, Some(DAY), "the tip waits for tomorrow");
        assert_eq!(crate::tips::at_start(&s, DAY, false), None);
        assert_eq!(crate::tips::at_start(&s, DAY + 1, false), Some(0), "the first tip, tomorrow");
    }

    /// Closing remembers: the next start shows nothing, and the tip comes as usual.
    #[test]
    fn closed_once_then_nothing() {
        let mut s = Settings::default();
        let plan = at_start(&mut s, ALL, false, DAY).unwrap();
        closed(&mut s, &plan);
        assert!(s.welcome_seen);
        assert_eq!(s.addons_offered, ["libmpv", "ffmpeg", "occt"]);
        assert_eq!(at_start(&mut s, ALL, false, DAY + 1), None);
        assert_eq!(s.tips_day, Some(DAY), "untouched on a start without the window");
    }

    /// A later version brings a new add-on: only the add-ons page, only the new one.
    #[test]
    fn new_addon_only_its_page() {
        let mut s = Settings { welcome_seen: true, addons_offered: vec!["libmpv".into(), "ffmpeg".into(), "occt".into()], ..Settings::default() };
        let with_pdf: Offers = &[("libmpv", true), ("ffmpeg", false), ("occt", true), ("pdf", true)];
        let plan = at_start(&mut s, with_pdf, false, DAY).unwrap();
        assert_eq!(plan, Plan { pages: vec![Page::Addons], addons: vec!["pdf"] });
        closed(&mut s, &plan);
        assert_eq!(at_start(&mut s, with_pdf, false, DAY), None);
    }

    /// A new add-on with nothing to offer here (on the system, downloaded) brings no window and is
    /// offered at once: it does not come back when the system loses its copy (Piero, 21/11).
    #[test]
    fn new_addon_already_here_no_window() {
        let mut s = Settings { welcome_seen: true, addons_offered: vec!["libmpv".into()], ..Settings::default() };
        assert_eq!(at_start(&mut s, &[("libmpv", true), ("ffmpeg", false)], false, DAY), None);
        assert_eq!(s.addons_offered, ["libmpv", "ffmpeg"]);
        assert_eq!(at_start(&mut s, &[("libmpv", true), ("ffmpeg", true)], false, DAY), None);
    }

    /// An add-on that does not exist for this system is not in the list (`component::addons`), so
    /// it never brings the window back; one offered on another system does not count here either.
    #[test]
    fn addon_of_another_system_no_window() {
        let mut s = Settings { welcome_seen: true, addons_offered: vec!["libmpv".into(), "ffmpeg".into(), "occt".into(), "other".into()], ..Settings::default() };
        assert_eq!(at_start(&mut s, ALL, false, DAY), None);
    }

    /// Who updates from a version without the window (its settings.json has neither key): both
    /// pages once.
    #[test]
    fn update_from_a_version_without_the_window() {
        let mut old: Settings = serde_json::from_str(r#"{"thumb_cache_mb": 50, "tips_day": 19999}"#).unwrap();
        assert!(!old.welcome_seen && old.addons_offered.is_empty());
        assert_eq!(at_start(&mut old, ALL, false, DAY), both(&["libmpv", "ffmpeg", "occt"]));
        let plan = both(&["libmpv", "ffmpeg", "occt"]).unwrap();
        closed(&mut old, &plan);
        let back: Settings = serde_json::from_str(&serde_json::to_string(&old).unwrap()).unwrap();
        assert!(back.welcome_seen);
        assert_eq!(back.addons_offered, ["libmpv", "ffmpeg", "occt"]);
    }

    fn card(kind: &'static str, ticked: bool, offers: bool, started: bool) -> Card {
        Card { kind, ticked, offers, started }
    }

    /// Nothing ticked: closing asks nothing.
    #[test]
    fn no_ticks_nothing_waits() {
        assert!(waiting(&[]).is_empty());
        assert!(waiting(&[card("libmpv", false, true, false), card("ffmpeg", false, false, false), card("occt", false, true, true)]).is_empty());
    }

    /// A tick on a card whose download is under way or has arrived does not wait.
    #[test]
    fn ticks_with_the_download_started_do_not_wait() {
        assert!(waiting(&[card("libmpv", true, true, true), card("occt", true, false, true)]).is_empty());
    }

    /// A tick on an add-on already downloaded or on the system does not wait.
    #[test]
    fn ticks_on_addons_already_here_do_not_wait() {
        assert!(waiting(&[card("libmpv", true, false, false), card("ffmpeg", true, false, false)]).is_empty());
    }

    /// Ticked, something to download, no download started: these wait, and only these.
    #[test]
    fn ticks_not_downloaded_wait() {
        let cards = [card("libmpv", true, true, false), card("ffmpeg", true, false, false), card("occt", true, true, false), card("pdf", true, true, true), card("font", false, true, false)];
        assert_eq!(waiting(&cards), ["libmpv", "occt"]);
        assert_eq!(waiting(&cards[..1]), ["libmpv"]);
    }

    /// Download and close: the waiting ones start and the window stays open, nothing remembered
    /// yet; it closes by itself, as Done, once they have all arrived.
    #[test]
    fn download_and_close_starts_the_waiting_ones() {
        let mut s = Settings::default();
        let plan = at_start(&mut s, ALL, false, DAY).unwrap();
        assert_eq!(answered(&mut s, &plan, vec!["libmpv", "occt"], Answer::Download), After::Downloading(vec!["libmpv", "occt"]));
        assert!(!s.welcome_seen && s.addons_offered.is_empty(), "still open");
    }

    /// All arrived: closes by itself. One failed: stays open, whatever the others do. One under
    /// way and none failed: waits.
    #[test]
    fn closes_by_itself_only_when_all_arrived() {
        use Download::{Arrived, Failed, Running};
        assert_eq!(ended(&[Arrived, Arrived]), Some(true));
        assert_eq!(ended(&[Arrived]), Some(true));
        assert_eq!(ended(&[Arrived, Failed]), Some(false));
        assert_eq!(ended(&[Running, Failed]), Some(false), "a failure ends the wait at once");
        assert_eq!(ended(&[Arrived, Running]), None);
        assert_eq!(ended(&[Running]), None);
    }

    /// Close without downloading: nothing starts, and it counts as closing, exactly as Done.
    #[test]
    fn close_without_downloading_starts_nothing() {
        let (mut s, mut done) = (Settings::default(), Settings::default());
        let plan = at_start(&mut s, ALL, false, DAY).unwrap();
        at_start(&mut done, ALL, false, DAY);
        closed(&mut done, &plan);
        assert_eq!(answered(&mut s, &plan, vec!["libmpv", "occt"], Answer::Close), After::Closed);
        assert_eq!(s, done, "the same settings as after Done");
        assert!(s.welcome_seen);
        assert_eq!(s.addons_offered, ["libmpv", "ffmpeg", "occt"]);
    }

    /// Back: the window stays open, nothing starts and nothing is remembered; the add-ons page
    /// shown alone (a new add-on) the same, and closing it does not touch the welcome.
    #[test]
    fn back_keeps_the_window_open() {
        let mut s = Settings::default();
        let plan = at_start(&mut s, ALL, false, DAY).unwrap();
        assert_eq!(answered(&mut s, &plan, vec!["libmpv"], Answer::Back), After::Stay);
        assert!(!s.welcome_seen && s.addons_offered.is_empty());
        assert!(at_start(&mut s, ALL, false, DAY).is_some(), "not closed: it comes back");

        let mut s = Settings { addons_offered: vec!["libmpv".into()], ..Settings::default() };
        let alone = Plan { pages: vec![Page::Addons], addons: vec!["pdf"] };
        assert_eq!(answered(&mut s, &alone, vec!["pdf"], Answer::Back), After::Stay);
        assert_eq!(s.addons_offered, ["libmpv"]);
        assert_eq!(answered(&mut s, &alone, vec!["pdf"], Answer::Close), After::Closed);
        assert_eq!(s.addons_offered, ["libmpv", "pdf"]);
        assert!(!s.welcome_seen, "the add-ons page alone does not show the welcome");
    }

    /// Started with an address: nothing, not even remembered; the next normal start shows it.
    #[test]
    fn started_with_an_address_waits() {
        let mut s = Settings::default();
        assert_eq!(at_start(&mut s, ALL, true, DAY), None);
        assert_eq!((s.welcome_seen, s.addons_offered.len(), s.tips_day), (false, 0, None));
        assert!(at_start(&mut s, ALL, false, DAY).is_some());
    }
}
