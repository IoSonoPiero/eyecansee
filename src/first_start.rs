//! The first-start window (#212): which pages it shows at this start, and what closing it
//! remembers. Everyone sees the welcome page and the add-ons once, on Windows with the desktop
//! icon page between them (#210); a later version brings the window back, with the add-ons page
//! alone, only for a new add-on that has something to offer. The window itself is in
//! `first_start_ui.rs`, the desktop icon page in `first_start_desktop.rs`.

use crate::settings::Settings;

/// A page of the window, in the order shown. A later page (the CJK font of the PDF engine) is
/// one more here, with its rule in `at_start`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    /// Logo, language, colors.
    Welcome,
    /// The desktop icon question of Windows (#210): only after the welcome page, and only where
    /// it is asked (`desktop_icon::ask_at_start`).
    Desktop,
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
/// `desktop` tells whether the desktop icon is asked: it reads the desktop, so it is called only
/// on the first start, where its page follows the welcome (Piero, 22/11: a page of its own).
pub(crate) fn at_start(s: &mut Settings, addons: Offers, desktop: impl FnOnce() -> bool, opened_with_address: bool, today: u64) -> Option<Plan> {
    if opened_with_address {
        return None;
    }
    let plan = if s.welcome_seen {
        let new: Vec<_> = addons.iter().filter(|(kind, offers)| *offers && !offered(s, kind)).map(|a| a.0).collect();
        (!new.is_empty()).then(|| Plan { pages: vec![Page::Addons], addons: new })
    } else {
        let pages = if desktop() { vec![Page::Welcome, Page::Desktop, Page::Addons] } else { vec![Page::Welcome, Page::Addons] };
        Some(Plan { pages, addons: addons.iter().map(|a| a.0).collect() })
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

/// Where the desktop icon page is. Next waits for an answer (`waits`); Skip and Esc close the
/// window without one and make no icon, and the page does not come back (`welcome_seen`):
/// Tools > Create desktop icon stays.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Desktop {
    /// Nothing chosen yet.
    Ask,
    /// Yes made the icon, with this name.
    Made(String),
    /// Yes found an icon that starts this copy already.
    There,
    No,
    /// Yes could not make it: the system's reason, on hover and in the log.
    Failed(String),
    /// No, after Yes made the icon with this name, could not take it away: the reason, on hover.
    Kept(String, String),
}

/// A choice of the desktop icon page: Yes makes the icon at once (`create`), also after No or a
/// failure; No makes none, and after Yes takes away the icon Yes made (`remove`, that one only:
/// Piero, 22/11, to change one's mind while on the page). An icon that was there already
/// (`There`) is never touched: the answer stays.
pub(crate) fn chosen(now: Desktop, yes: bool, create: impl FnOnce() -> Result<crate::desktop_icon::Made, String>, remove: impl FnOnce(&str) -> Result<(), String>) -> Desktop {
    use crate::desktop_icon::Made;
    match (now, yes) {
        (Desktop::There, _) => Desktop::There,
        (Desktop::Made(name) | Desktop::Kept(name, _), true) => Desktop::Made(name),
        (Desktop::Made(name) | Desktop::Kept(name, _), false) => match remove(&name) {
            Ok(()) => Desktop::No,
            Err(why) => Desktop::Kept(name, why),
        },
        (_, false) => Desktop::No,
        (_, true) => match create() {
            Ok(Made::Made(name)) => Desktop::Made(name),
            Ok(Made::There) => Desktop::There,
            Err(why) => Desktop::Failed(why),
        },
    }
}

/// What a page waits for before Next: the id of the sentence the page shows when Next is pressed
/// before an answer (D9, Piero, 24/11). The desktop icon page waits for Yes or No (Piero, 22/11);
/// a Yes that failed is an answer, with its error on the page. Back, Skip and Esc never wait.
pub(crate) fn waits(page: Page, desktop: &Desktop) -> Option<&'static str> {
    (page == Page::Desktop && *desktop == Desktop::Ask).then_some("desktop-icon-choose")
}

/// How the window closed, for the tip of the day: with Done (or Download and close, once its
/// downloads have arrived), or another way (Skip, Esc, Close without downloading).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum How {
    Done,
    Other,
}

/// The window closed: remembered as `closed` does, and the tip to show at once, marked as seen
/// today, so the next day brings the one after it. Only on the first start (the welcome page),
/// only with Done and only with the tips on in the Options (Piero, 22/11, over the decision of
/// 21/11 that kept the first tip for the next day): closed another way, the tip comes the next
/// day from the first, as `at_start` set.
pub(crate) fn closed_with_tip(s: &mut Settings, plan: &Plan, how: How, today: u64) -> Option<usize> {
    closed(s, plan);
    let i = (how == How::Done && s.tips && plan.pages.contains(&Page::Welcome)).then(|| s.tip_next % crate::tips::TIPS.len())?;
    crate::tips::seen(s, i, today);
    Some(i)
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

    /// `super::at_start` where the desktop icon is not asked (Linux, the installer's copy, an
    /// icon already there).
    fn at_start(s: &mut Settings, addons: Offers, opened_with_address: bool, today: u64) -> Option<Plan> {
        super::at_start(s, addons, || false, opened_with_address, today)
    }

    /// Windows, the first start: three pages, the desktop icon between the welcome and the
    /// add-ons. Closing remembers as with two: no setting of its own, the page does not come back.
    #[test]
    fn first_start_on_windows_three_pages() {
        let mut s = Settings::default();
        let plan = super::at_start(&mut s, ALL, || true, false, DAY).unwrap();
        assert_eq!(plan.pages, [Page::Welcome, Page::Desktop, Page::Addons]);
        assert_eq!(plan.addons, ["libmpv", "ffmpeg", "occt"]);
        assert_eq!(s.tips_day, Some(DAY), "the tip waits for tomorrow");
        let before = s.clone();
        closed(&mut s, &plan);
        assert_eq!(s, Settings { welcome_seen: true, addons_offered: vec!["libmpv".into(), "ffmpeg".into(), "occt".into()], ..before });
        assert_eq!(super::at_start(&mut s, ALL, || true, false, DAY + 1), None);
    }

    /// The desktop icon page never comes alone or with a new add-on, and never with an address:
    /// on those starts the desktop is not even read.
    #[test]
    fn desktop_page_only_after_the_welcome() {
        let never = || -> bool { panic!("the desktop is read only on the first start") };
        let mut s = Settings { welcome_seen: true, addons_offered: vec!["libmpv".into(), "ffmpeg".into(), "occt".into()], ..Settings::default() };
        let with_pdf: Offers = &[("libmpv", true), ("ffmpeg", false), ("occt", true), ("pdf", true)];
        assert_eq!(super::at_start(&mut s, with_pdf, never, false, DAY), Some(Plan { pages: vec![Page::Addons], addons: vec!["pdf"] }));
        assert_eq!(super::at_start(&mut s, ALL, never, false, DAY), None, "nothing new");
        assert_eq!(super::at_start(&mut Settings::default(), ALL, never, true, DAY), None, "started with an address");
    }

    /// Next waits only on the desktop icon page, and only until Yes or No is chosen; whatever Yes
    /// did, the failure too, is an answer.
    #[test]
    fn next_waits_for_the_desktop_answer() {
        assert_eq!(waits(Page::Desktop, &Desktop::Ask), Some("desktop-icon-choose"));
        for answered in [Desktop::No, Desktop::Made("EyeCanSee".into()), Desktop::There, Desktop::Failed("x".into())] {
            assert_eq!(waits(Page::Desktop, &answered), None, "{answered:?}");
        }
        for page in [Page::Welcome, Page::Addons] {
            assert_eq!(waits(page, &Desktop::Ask), None, "{page:?}");
        }
    }

    /// Yes makes the icon at once and says what happened; No makes none; after No or a failure
    /// Yes still makes it; No after Yes takes away the icon Yes made, and only that one; an icon
    /// that was there already stays whatever is answered.
    #[test]
    fn desktop_answers() {
        use crate::desktop_icon::Made;
        let made = || Ok(Made::Made("EyeCanSee (2)".into()));
        let never = || -> Result<Made, String> { panic!("nothing to make") };
        let keep = |_: &str| -> Result<(), String> { panic!("nothing to take away") };
        let chosen = |now, yes, create: &dyn Fn() -> Result<Made, String>| chosen(now, yes, create, keep);
        assert_eq!(chosen(Desktop::Ask, true, &made), Desktop::Made("EyeCanSee (2)".into()));
        assert_eq!(chosen(Desktop::Ask, true, &|| Ok(Made::There)), Desktop::There);
        assert_eq!(chosen(Desktop::Ask, true, &|| Err("access denied".into())), Desktop::Failed("access denied".into()));
        assert_eq!(chosen(Desktop::Ask, false, &never), Desktop::No);
        assert_eq!(chosen(Desktop::No, true, &made), Desktop::Made("EyeCanSee (2)".into()));
        assert_eq!(chosen(Desktop::No, false, &never), Desktop::No);
        assert_eq!(chosen(Desktop::Failed("x".into()), true, &made), Desktop::Made("EyeCanSee (2)".into()), "Yes again tries again");
        assert_eq!(chosen(Desktop::Failed("x".into()), false, &never), Desktop::No);
        assert_eq!(chosen(Desktop::There, false, &never), Desktop::There, "an icon there before is never taken away");
        assert_eq!(chosen(Desktop::There, true, &never), Desktop::There);
        assert_eq!(chosen(Desktop::Made("EyeCanSee".into()), true, &never), Desktop::Made("EyeCanSee".into()), "Yes makes no second one");
    }

    /// No after Yes takes away the icon Yes made, by its name; when that fails the icon stays and
    /// the page says so; Yes then keeps it, No tries again.
    #[test]
    fn no_after_yes_takes_the_icon_away() {
        use crate::desktop_icon::Made;
        let never = || -> Result<Made, String> { panic!("nothing to make") };
        let gone = std::cell::RefCell::new(Vec::new());
        let removed = |name: &str| -> Result<(), String> { gone.borrow_mut().push(name.to_string()); Ok(()) };
        assert_eq!(chosen(Desktop::Made("EyeCanSee (2)".into()), false, never, removed), Desktop::No);
        assert_eq!(*gone.borrow(), ["EyeCanSee (2)"], "that icon, by its name");
        let stuck = Desktop::Kept("EyeCanSee".into(), "in use".into());
        assert_eq!(chosen(Desktop::Made("EyeCanSee".into()), false, never, |_| Err("in use".into())), stuck);
        assert_eq!(chosen(stuck.clone(), true, never, |_| unreachable!()), Desktop::Made("EyeCanSee".into()));
        assert_eq!(chosen(stuck, false, never, |_| Ok(())), Desktop::No);
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

    /// A start of the app at `day` with `s`: the window (closed `how`) and the tip shown, if any.
    /// With `address`, started with a file.
    fn start(s: &mut Settings, day: u64, how: How, address: bool) -> (bool, Option<usize>) {
        match at_start(s, ALL, address, day) {
            Some(plan) => (true, closed_with_tip(s, &plan, how, day)),
            None => (false, crate::tips::at_start(s, day, address).inspect(|&i| crate::tips::seen(s, i, day))),
        }
    }

    /// The cases of Piero's prompt of 22/11 (a-h): Done on the first start brings the first tip
    /// at once, and the next day the second; closed another way, no tip that day and the first
    /// the next day; started with a file, no window and no tip; tips off, no tip after Done.
    #[test]
    fn the_tip_after_the_window() {
        let mut s = Settings::default();
        assert_eq!(start(&mut s, DAY, How::Done, false), (true, Some(0)), "a: the window, then tip 1");
        assert!(s.welcome_seen, "saved as seen before the tip");
        assert_eq!(start(&mut s, DAY, How::Done, false), (false, None), "b: the same day, nothing");
        assert_eq!(start(&mut s, DAY + 1, How::Done, false), (false, Some(1)), "c: the next day, tip 2");

        let mut s = Settings::default();
        assert_eq!(start(&mut s, DAY, How::Other, false), (true, None), "d: Skip, Esc, Close without downloading");
        assert_eq!(start(&mut s, DAY, How::Other, false), (false, None), "e: the same day, nothing");
        assert_eq!(start(&mut s, DAY + 1, How::Other, false), (false, Some(0)), "f: the next day, tip 1");

        let mut s = Settings::default();
        assert_eq!(start(&mut s, DAY, How::Done, true), (false, None), "g: with a file, no window and no tip");
        assert!(!s.welcome_seen, "the window waits for a normal start");

        let mut s = Settings { tips: false, ..Settings::default() };
        assert_eq!(start(&mut s, DAY, How::Done, false), (true, None), "h: tips off in the Options");
    }

    /// The add-ons page alone, for a new add-on of a later version: Done brings no tip, the day
    /// goes on as before.
    #[test]
    fn no_tip_after_the_addons_page_alone() {
        let mut s = Settings { welcome_seen: true, ..Settings::default() };
        let alone = Plan { pages: vec![Page::Addons], addons: vec!["pdf"] };
        assert_eq!(closed_with_tip(&mut s, &alone, How::Done, DAY), None);
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
