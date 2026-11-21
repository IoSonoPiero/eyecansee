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

/// The window closed (Fine, Skip, Esc or its X): the welcome is seen, and every add-on it showed
/// is offered.
pub(crate) fn closed(s: &mut Settings, plan: &Plan) {
    if plan.pages.contains(&Page::Welcome) {
        s.welcome_seen = true;
    }
    remember(s, plan.addons.iter().copied());
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

    /// Started with an address: nothing, not even remembered; the next normal start shows it.
    #[test]
    fn started_with_an_address_waits() {
        let mut s = Settings::default();
        assert_eq!(at_start(&mut s, ALL, true, DAY), None);
        assert_eq!((s.welcome_seen, s.addons_offered.len(), s.tips_day), (false, 0, None));
        assert!(at_start(&mut s, ALL, false, DAY).is_some());
    }
}
