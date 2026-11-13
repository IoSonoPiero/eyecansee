//! Screenshots of the illustrated guide (`docs/help/img/`), only with the Cargo feature `shots`
//! (`tools/shots.sh` builds and runs it). A scene (`tools/shots/<name>.txt`, path in
//! `EYECANSEE_SHOTS_SCENE`) drives the app with real input events (pointer, keys, text) put in
//! `raw_input_hook`, and with menu actions; each `shot` saves the window, or a part of it, as a
//! lossy WebP in the folder `EYECANSEE_SHOTS_OUT`. Lines of a scene, after `#` a comment:
//! - `<seconds> <step>`: a step, at that time from the start (see `parse_step`);
//! - `width <pixels>`: width of the next pictures (default: the part taken, one pixel per point);
//! - anything else (`open`, `set`, `server`, `copy`…) is for `tools/shots.sh`.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, Rect, pos2, vec2};

use crate::actions::Act;

/// WebP quality of the pictures (lossy, 0-100).
const QUALITY: f32 = 80.0;

#[derive(Clone, Debug)]
enum Step {
    /// Pressed and released, with these modifiers.
    Key(Key, Modifiers),
    Text(String),
    /// The pointer glides there in that many seconds.
    Move(Pos2, f64),
    /// Pressed or released.
    Button(PointerButton, bool),
    /// Held from now on (`mods ctrl`, `mods none`), as for a Ctrl+click.
    Mods(Modifiers),
    Wheel(f32),
    /// An item of the folder open, by the end of its name: selected, then opened or marked.
    Select(String),
    Open(String),
    Mark(String),
    Act(String),
    /// A file made during the scene gets the made-up date of the others (`stage.sh`).
    Date(String),
    /// Picture name and the part taken, in points (`None` = the whole window).
    Shot(String, Option<Rect>, Option<u32>),
    End,
}

#[derive(Default)]
struct Run {
    steps: Vec<(f64, Step)>,
    next: usize,
    queue: Vec<Event>,
    mods: Modifiers,
    pointer: Option<Pos2>,
    glide: Option<(f64, f64, Pos2, Pos2)>,
    /// Pictures asked for and not written yet: the window closes after the last one.
    pending: Arc<AtomicUsize>,
    end: bool,
}

thread_local! {
    static RUN: std::cell::RefCell<Option<Run>> = const { std::cell::RefCell::new(None) };
}

/// On while the screenshots are taken: the menu bar leaves out the version.
pub(crate) fn active() -> bool {
    std::env::var_os("EYECANSEE_SHOTS_SCENE").is_some()
}

/// The demo home of `tools/shots.sh`, as the tree names it (no leading `/`).
const HOME: &str = "home/demo/";

/// In the tree of "This computer", the folders above the demo home show only the way down to it:
/// not `bin`, `etc`, `proc`… `true` for a `child` of `dir` left out.
pub(crate) fn tree_hides(dir: &str, child: &str) -> bool {
    active() && off_the_way(dir, child)
}

fn off_the_way(dir: &str, child: &str) -> bool {
    HOME.starts_with(dir) && dir.len() < HOME.len() && !HOME.starts_with(child)
}

fn key(name: &str) -> Key {
    Key::from_name(name).unwrap_or_else(|| panic!("shots: unknown key {name}"))
}

fn mods(name: &str) -> Modifiers {
    name.split('+').fold(Modifiers::NONE, |m, part| match part {
        "ctrl" => m | Modifiers::COMMAND,
        "shift" => m | Modifiers::SHIFT,
        "alt" => m | Modifiers::ALT,
        "none" => m,
        x => panic!("shots: unknown modifier {x}"),
    })
}

fn num<T: std::str::FromStr>(w: &[&str], i: usize) -> T {
    w.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| panic!("shots: number expected in {w:?}"))
}

/// The steps of a scene, sorted by time. `name` is the scene's (the picture of a plain `shot`).
fn parse(text: &str, name: &str) -> Vec<(f64, Step)> {
    let mut steps = Vec::new();
    let mut width = None;
    for line in text.lines() {
        let w: Vec<&str> = line.split('#').next().unwrap_or("").split_whitespace().collect();
        if w.first() == Some(&"width") {
            width = Some(num(&w, 1));
        }
        let Some(t) = w.first().and_then(|t| t.parse::<f64>().ok()) else { continue };
        let rest = line.trim_start().split_once(char::is_whitespace).map_or("", |(_, r)| r.trim());
        parse_step(&mut steps, t, &w[1..], rest, name, width);
    }
    steps.sort_by(|a, b| a.0.total_cmp(&b.0));
    steps
}

/// One line: `key <name>` (egui's names: `Escape`, `Plus`, `E`, `F1`…), `key <mods> <name>`
/// (`ctrl`, `shift`, `ctrl+shift`), `type <text>`, `move <x> <y> [seconds]`, `down`, `up`,
/// `click <x> <y>`, `dclick <x> <y>`, `rclick <x> <y>` (the right button), `drag <x> <y> <x2> <y2> [seconds]`, `mods <mods>`,
/// `wheel <points>`, `select|open|mark <end of a name>`, `act <action>` (`action`),
/// `shot [name] [x y w h]`, `end`.
fn parse_step(steps: &mut Vec<(f64, Step)>, t: f64, w: &[&str], rest: &str, name: &str, width: Option<u32>) {
    let arg = rest.split_once(char::is_whitespace).map_or("", |(_, r)| r.trim()).to_string();
    let at = |i: usize| pos2(num(w, i), num(w, i + 1));
    let mut push = |dt: f64, s: Step| steps.push((t + dt, s));
    match w.first().copied().unwrap_or("") {
        "key" if w.len() > 2 => push(0.0, Step::Key(key(w[2]), mods(w[1]))),
        "key" => push(0.0, Step::Key(key(w[1]), Modifiers::NONE)),
        "type" => arg.chars().enumerate().for_each(|(i, c)| push(i as f64 * 0.05, Step::Text(c.to_string()))),
        "move" => push(0.0, Step::Move(at(1), w.get(3).map_or(0.4, |_| num(w, 3)))),
        "down" => push(0.0, Step::Button(PointerButton::Primary, true)),
        "up" => push(0.0, Step::Button(PointerButton::Primary, false)),
        "click" | "dclick" | "rclick" => {
            push(0.0, Step::Move(at(1), 0.0));
            let n = if w[0] == "dclick" { 2 } else { 1 };
            let button = if w[0] == "rclick" { PointerButton::Secondary } else { PointerButton::Primary };
            for i in 0..n {
                push(0.05 + i as f64 * 0.1, Step::Button(button, true));
                push(0.08 + i as f64 * 0.1, Step::Button(button, false));
            }
        }
        "drag" => {
            push(0.0, Step::Move(at(1), 0.0));
            push(0.05, Step::Button(PointerButton::Primary, true));
            let secs = w.get(5).map_or(0.6, |_| num(w, 5));
            push(0.1, Step::Move(at(3), secs));
            push(0.2 + secs, Step::Button(PointerButton::Primary, false));
        }
        "mods" => push(0.0, Step::Mods(mods(w[1]))),
        "wheel" => push(0.0, Step::Wheel(num(w, 1))),
        "select" => push(0.0, Step::Select(arg)),
        "open" => push(0.0, Step::Open(arg)),
        "mark" => push(0.0, Step::Mark(arg)),
        "act" => push(0.0, Step::Act(arg)),
        "date" => push(0.0, Step::Date(arg)),
        "shot" => {
            let named = w.get(1).is_some_and(|s| s.parse::<f32>().is_err());
            let (pic, k) = if named { (w[1].to_string(), 2) } else { (name.to_string(), 1) };
            let part = (w.len() >= k + 4).then(|| Rect::from_min_size(at(k), vec2(num(w, k + 2), num(w, k + 3))));
            push(0.0, Step::Shot(pic, part, width));
        }
        "end" => push(0.0, Step::End),
        x => panic!("shots: unknown step {x}"),
    }
}

/// `act <name>`: the menu items a scene can choose, as `Act`.
fn act(name: &str) -> Act {
    match name {
        "edit" => Act::Edit(None),
        "batch" => Act::Batch(None),
        "duplicates" => Act::Duplicates(None),
        "slideshow" => Act::Slideshow(None),
        "options" => Act::Options,
        "servers" => Act::Servers,
        "bookmarks" => Act::Bookmarks,
        "convert" => Act::Convert(None),
        "select-all" => Act::SelectAll,
        "address" => Act::Address,
        "new-image" => Act::NewImage,
        "rotate-crop" => Act::RotateCrop(None),
        "drawings" => Act::ExportDrawings(None),
        "manual" => Act::Manual,
        "about" => Act::About,
        x => panic!("shots: unknown action {x}"),
    }
}

fn ease(x: f64) -> f32 {
    (x * x * (3.0 - 2.0 * x)) as f32
}

impl crate::App {
    /// The place in the grid (folders first) of the file or folder whose name ends so.
    fn shots_index(&self, name: &str) -> Option<usize> {
        let Some(Ok(l)) = self.listings.get(&self.dir) else { return None };
        let end = |p: &str| p.trim_end_matches('/').ends_with(name);
        l.dirs.iter().position(|d| end(d)).or_else(|| Some(l.dirs.len() + l.files.iter().position(|f| end(&f.path))?))
    }

    /// In `raw_input_hook`, before anything else: the events of the steps due.
    pub(crate) fn shots_input(&mut self, raw: &mut egui::RawInput) {
        RUN.with_borrow_mut(|run| {
            let Some(run) = run else { return };
            raw.events.append(&mut run.queue);
            raw.events.push(Event::ModifiersChanged(run.mods));
        });
    }

    /// Every frame, before the app's: runs the steps due and closes the window at the end.
    pub(crate) fn shots_tick(&mut self, ctx: &egui::Context) {
        let Ok(scene) = std::env::var("EYECANSEE_SHOTS_SCENE") else { return };
        let mut run = RUN.with_borrow_mut(Option::take).unwrap_or_else(|| {
            let text = std::fs::read_to_string(&scene).unwrap_or_else(|e| panic!("shots: {scene}: {e}"));
            let name = std::path::Path::new(&scene).file_stem().map_or("shot".into(), |s| s.to_string_lossy().into_owned());
            Run { steps: parse(&text, &name), ..Run::default() }
        });
        let t = ctx.input(|i| i.time);
        while run.next < run.steps.len() && t >= run.steps[run.next].0 {
            let step = run.steps[run.next].1.clone();
            run.next += 1;
            log!("shots: {t:.2} {step:?}");
            self.shots_step(ctx, &mut run, t, step);
        }
        if let Some((t0, t1, a, b)) = run.glide {
            let k = if t1 > t0 { ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
            let p = a + (b - a) * ease(k);
            run.pointer = Some(p);
            run.queue.push(Event::PointerMoved(p));
            if k >= 1.0 {
                run.glide = None;
            }
        }
        // Not a close of the window: with an edited image it would ask first.
        if run.end && run.pending.load(Ordering::Relaxed) == 0 {
            log!("shots: done");
            quit();
        }
        RUN.with_borrow_mut(|r| *r = Some(run));
        ctx.request_repaint();
    }

    fn shots_step(&mut self, ctx: &egui::Context, run: &mut Run, t: f64, step: Step) {
        let mods = run.mods;
        match step {
            Step::Key(key, m) => {
                for pressed in [true, false] {
                    run.queue.push(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers: m | mods });
                }
            }
            Step::Text(x) => run.queue.push(Event::Text(x)),
            Step::Move(to, secs) => run.glide = Some((t, t + secs, run.pointer.unwrap_or(to), to)),
            Step::Button(button, pressed) => {
                let pos = run.pointer.unwrap_or_default();
                run.queue.push(Event::PointerButton { pos, button, pressed, modifiers: mods });
            }
            Step::Mods(m) => run.mods = m,
            Step::Wheel(y) => {
                let (unit, phase, source) = (egui::MouseWheelUnit::Point, egui::TouchPhase::Move, egui::MouseWheelSource::Unknown);
                run.queue.push(Event::MouseWheel { unit, delta: vec2(0.0, y), phase, modifiers: mods, source });
            }
            Step::Select(name) | Step::Open(name) | Step::Mark(name) if self.shots_index(&name).is_none() => panic!("shots: no {name} in {}", self.dir),
            Step::Select(name) => (self.selected, self.cursor, self.scroll_to_selected) = (self.shots_index(&name).unwrap_or(0), true, true),
            Step::Open(name) => {
                let i = self.shots_index(&name).unwrap_or(0);
                (self.selected, self.cursor, self.scroll_to_selected) = (i, true, true);
                self.activate(i);
            }
            Step::Mark(name) => {
                let i = self.shots_index(&name).unwrap_or(0);
                if let Some(path) = self.item_path(i) {
                    self.marked.insert(path);
                }
            }
            Step::Act(name) => self.act = Some(act(&name)),
            // The folder's address has no leading slash on Linux.
            Step::Date(name) => fake_date(&std::path::Path::new("/").join(&self.dir).join(name)),
            Step::Shot(name, part, width) => shot(ctx, run, name, part, width),
            Step::End => run.end = true,
        }
    }
}

/// The date `stage.sh` gives to every file, so that no picture shows when it was taken.
fn fake_date(path: &std::path::Path) {
    let when = jiff::civil::datetime(2025, 7, 14, 18, 30, 0, 0).to_zoned(jiff::tz::TimeZone::system()).expect("shots: date");
    let file = std::fs::File::options().write(true).open(path).unwrap_or_else(|e| panic!("shots: date {}: {e}", path.display()));
    file.set_modified(when.timestamp().into()).expect("shots: date");
}

/// Ends the process at once. On Linux without the exit handlers: libmpv's, with a video open,
/// crashed there.
fn quit() -> ! {
    #[cfg(unix)]
    // SAFETY: the pictures are written; nothing else needs to be finished.
    unsafe {
        libc::_exit(0)
    }
    #[cfg(not(unix))]
    std::process::exit(0)
}

/// Asks egui for the window's pixels; the picture is written when they arrive.
fn shot(ctx: &egui::Context, run: &Run, name: String, part: Option<Rect>, width: Option<u32>) {
    let dir = std::env::var("EYECANSEE_SHOTS_OUT").unwrap_or_else(|_| ".".into());
    let to = std::path::Path::new(&dir).join(format!("{name}.webp"));
    let ppp = ctx.pixels_per_point();
    let pending = run.pending.clone();
    pending.fetch_add(1, Ordering::Relaxed);
    ctx.request_screenshot(move |img| {
        match save(&img, ppp, part, width, &to) {
            Ok((w, h, bytes)) => log!("shots: {} {w}x{h} {bytes} B", to.display()),
            Err(e) => panic!("shots: {}: {e}", to.display()),
        }
        pending.fetch_sub(1, Ordering::Relaxed);
    });
}

/// The part (in points) of the window's pixels, made `width` pixels wide (default: one pixel per
/// point), as a lossy WebP. Returns its size and bytes.
fn save(img: &egui::ColorImage, ppp: f32, part: Option<Rect>, width: Option<u32>, to: &std::path::Path) -> Result<(u32, u32, usize), String> {
    let [iw, ih] = img.size;
    let rgb: Vec<u8> = img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b()]).collect();
    let full = image::RgbImage::from_raw(iw as u32, ih as u32, rgb).ok_or("size")?;
    let part = part.unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(iw as f32, ih as f32) / ppp));
    let px = |v: f32, max: usize| ((v * ppp).round().max(0.0) as u32).min(max as u32);
    let (x, y) = (px(part.min.x, iw), px(part.min.y, ih));
    let (w, h) = (px(part.width(), iw - x as usize), px(part.height(), ih - y as usize));
    let cut = image::imageops::crop_imm(&full, x, y, w, h).to_image();
    let out_w = width.unwrap_or(part.width().round() as u32).min(w.max(1));
    let out_h = ((h as f32) * out_w as f32 / w.max(1) as f32).round() as u32;
    let small = if out_w == w { cut } else { image::imageops::resize(&cut, out_w, out_h, image::imageops::FilterType::Lanczos3) };
    let webp = webp::Encoder::from_rgb(small.as_raw(), small.width(), small.height()).encode(QUALITY);
    std::fs::write(to, &*webp).map_err(|e| e.to_string())?;
    Ok((small.width(), small.height(), webp.len()))
}

/// The folders of a real phone that a scene may show, from its line `phone-folders <name>…`
/// (the other folders of the user's phone stay out of the pictures). No line: all of them.
pub(crate) fn phone_folders(dirs: &mut Vec<String>) {
    static SHOWN: std::sync::OnceLock<Option<Vec<String>>> = std::sync::OnceLock::new();
    let shown = SHOWN.get_or_init(|| {
        let text = std::fs::read_to_string(std::env::var_os("EYECANSEE_SHOTS_SCENE")?).ok()?;
        let line = text.lines().find_map(|l| l.strip_prefix("phone-folders "))?;
        Some(line.split_whitespace().map(String::from).collect())
    });
    if let Some(shown) = shown {
        dirs.retain(|d| shown.iter().any(|s| d.trim_end_matches('/').rsplit('/').next() == Some(s.as_str())));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_way_home() {
        let hidden: Vec<bool> = [("", "home/"), ("", "etc/"), ("home/", "home/demo/"), ("home/", "home/other/"), ("home/demo/", "home/demo/Music/")]
            .iter()
            .map(|(dir, child)| off_the_way(dir, child))
            .collect();
        assert_eq!(hidden, [false, true, false, true, false]);
    }

    #[test]
    fn scene_steps() {
        let text = "open /home/demo\nwidth 1000\n# a comment\n2 key ctrl E\n1 click 10 20\n3 type ab\n4 shot\n5 shot part 1 2 30 40\n6 end";
        let steps = parse(text, "grid");
        let kinds: Vec<String> = steps.iter().map(|(t, s)| format!("{t:.2} {s:?}")).collect();
        assert_eq!(steps.len(), 9, "{kinds:#?}");
        assert!(matches!(steps[0].1, Step::Move(p, _) if p == pos2(10.0, 20.0)));
        assert!(matches!(&steps[3].1, Step::Key(Key::E, m) if m.command));
        assert!(matches!(&steps[6].1, Step::Shot(n, None, Some(1000)) if n == "grid"));
        assert!(matches!(&steps[7].1, Step::Shot(n, Some(r), _) if n == "part" && r.width() == 30.0));
    }
}
