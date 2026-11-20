//! Memory of the thumbnails made at the same time (#177). On a local disk there are as many
//! workers as logical processors, and every thumbnail but a JPEG's decodes its picture whole: 16
//! to 32 large photos (RAW, HEIC, PSD, 16-bit TIFF) at once took more memory than a PC with 16 GB
//! has. So, before decoding, each thumbnail reserves the memory it will use, estimated from the
//! file's header with factors measured one file at a time (`notes/rapporti/miniature-177-*`), out
//! of a quarter of the computer's RAM (Piero, 20/11/2026), and waits while there is no room. A
//! picture larger than the whole budget waits until no other is being made, then is made alone.
//! No photo is refused: the ceiling on the size of a picture is `pixel_limit`'s.
//!
//! The small ones pass a large one that waits, as long as there is room for them (Piero,
//! 20/11/2026): in a folder of JPEGs and RAWs the JPEGs no longer wait behind every RAW. After
//! `MAX_WAIT` or `MAX_PASSES`, whichever comes first, nobody passes it any more: it is made as soon
//! as the memory it needs is free, so it never waits for ever.

use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;

const MIB: u64 = 1024 * 1024;

/// The RAM assumed when it cannot be read: the smallest PC EyeCanSee is made for (8 GiB), so the
/// budget is never larger than on that PC.
const FALLBACK_RAM: u64 = 8 * 1024 * MIB;

/// How long a thumbnail may be passed by those that fit when it does not. A large photo's own
/// thumbnail takes 1-2 s to make (a RAW, a HEIC of 45 MP), so 3 s at most adds about as much
/// again to it, while the small ones that pass it in the meantime are the screen the user is
/// looking at: a JPEG takes about 40 ms, so a whole screen of them (24-60 cells) is made well
/// within it by 16 workers.
const MAX_WAIT: Duration = Duration::from_secs(3);
/// How many thumbnails may pass one that waits: a screen of cells and the rows next to it (the
/// order of the queue, `grid::thumb_order`), so on a slow disk, where 3 s pass before a screen is
/// made, the large one is not left behind the whole folder.
const MAX_PASSES: u32 = 64;

/// A thumbnail waiting for its memory, in the order they asked.
#[derive(Debug)]
struct Waiter {
    ticket: u64,
    places: u32,
    since: Instant,
    /// Thumbnails given their memory while this one waited.
    passed: u32,
    /// Its memory is set aside for it: it takes it when it wakes up.
    granted: bool,
}

impl Waiter {
    /// Waited long enough: nobody passes it any more.
    fn held(&self, now: Instant) -> bool {
        !self.granted && (self.passed >= MAX_PASSES || now.duration_since(self.since) >= MAX_WAIT)
    }
}

/// The places (MiB) free and the thumbnails waiting, without the waiting itself: what `Budget`
/// decides, so that it can be tested with a clock of its own.
#[derive(Debug)]
struct Ledger {
    free: u32,
    next: u64,
    queue: VecDeque<Waiter>,
}

impl Ledger {
    /// `places` now, if they fit and no thumbnail waiting was passed long enough; every one waiting
    /// counts it as passing.
    fn try_take(&mut self, places: u32, now: Instant) -> bool {
        if places > self.free || self.queue.iter().any(|w| w.held(now)) {
            return false;
        }
        self.free -= places;
        self.queue.iter_mut().filter(|w| !w.granted).for_each(|w| w.passed += 1);
        true
    }

    /// In the queue, after the others.
    fn wait(&mut self, places: u32, now: Instant) -> u64 {
        self.next += 1;
        self.queue.push_back(Waiter { ticket: self.next, places, since: now, passed: 0, granted: false });
        self.next
    }

    /// The memory set aside for `ticket`, once it was: it leaves the queue.
    fn take(&mut self, ticket: u64) -> Option<u32> {
        let at = self.queue.iter().position(|w| w.ticket == ticket && w.granted)?;
        self.queue.remove(at).map(|w| w.places)
    }

    /// `ticket` gives up waiting (its future dropped): out of the queue, and the memory set aside
    /// for it, if any, back.
    fn leave(&mut self, ticket: u64, now: Instant) {
        if let Some(at) = self.queue.iter().position(|w| w.ticket == ticket)
            && let Some(w) = self.queue.remove(at)
        {
            self.free += if w.granted { w.places } else { 0 };
            self.hand_out(now);
        }
    }

    fn give_back(&mut self, places: u32, now: Instant) {
        self.free += places;
        self.hand_out(now);
    }

    /// Sets the free memory aside for the ones waiting, first come first: each one that fits,
    /// passing those before it that do not, until one that was passed long enough, after which
    /// nobody behind it is served.
    fn hand_out(&mut self, now: Instant) {
        for i in 0..self.queue.len() {
            let w = &self.queue[i];
            if w.granted {
                continue;
            }
            if w.places <= self.free {
                self.free -= w.places;
                self.queue[i].granted = true;
                self.queue.iter_mut().take(i).filter(|w| !w.granted).for_each(|w| w.passed += 1);
            } else if w.held(now) {
                return;
            }
        }
    }
}

/// The memory of the thumbnails made at the same time, in MiB: one place per MiB.
pub struct Budget {
    shared: Arc<Shared>,
    total: u32,
    /// The RAM it is a quarter of: a picture `over_half` of it gets no thumbnail.
    ram: u64,
}

#[derive(Debug)]
struct Shared {
    ledger: Mutex<Ledger>,
    /// Woken when memory is set aside for someone waiting.
    wake: Notify,
}

/// The memory a thumbnail reserved: given back when dropped, also on an error or a panic.
#[derive(Debug)]
pub struct Reservation {
    shared: Arc<Shared>,
    places: u32,
}

impl Reservation {
    #[cfg(test)]
    fn places(&self) -> u32 {
        self.places
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.shared.ledger.lock().unwrap().give_back(self.places, Instant::now());
        self.shared.wake.notify_waiters();
    }
}

/// A place in the queue: leaves it if the wait is given up (the future dropped), and gives back
/// the memory set aside for it meanwhile. Once taken, it is no longer in the queue: nothing to do.
struct Ticket<'a> {
    shared: &'a Arc<Shared>,
    ticket: u64,
}

impl Drop for Ticket<'_> {
    fn drop(&mut self) {
        self.shared.ledger.lock().unwrap().leave(self.ticket, Instant::now());
        self.shared.wake.notify_waiters();
    }
}

impl Budget {
    /// `bytes` to share, out of four times as much RAM.
    #[cfg(test)]
    pub fn new(bytes: u64) -> Self {
        Self::of(bytes, bytes.saturating_mul(4))
    }

    fn of(bytes: u64, ram: u64) -> Self {
        let total = u32::try_from(bytes / MIB).unwrap_or(u32::MAX).max(1);
        let ledger = Ledger { free: total, next: 0, queue: VecDeque::new() };
        Budget { shared: Arc::new(Shared { ledger: Mutex::new(ledger), wake: Notify::new() }), total, ram }
    }

    /// The RAM it is a quarter of.
    pub fn ram(&self) -> u64 {
        self.ram
    }

    /// No thumbnail for a picture that needs `need`: `over_half` the RAM.
    pub fn too_large(&self, need: u64) -> bool {
        over_half(need, self.ram)
    }

    /// The places for `bytes`: at least one, at most all (a picture larger than the budget).
    fn places(&self, bytes: u64) -> u32 {
        u32::try_from(bytes.div_ceil(MIB)).unwrap_or(u32::MAX).clamp(1, self.total)
    }

    fn reservation(&self, places: u32) -> Reservation {
        Reservation { shared: self.shared.clone(), places }
    }

    /// `bytes` if there is room now and no thumbnail waiting was passed long enough.
    pub fn try_reserve(&self, bytes: u64) -> Option<Reservation> {
        let places = self.places(bytes);
        let taken = self.shared.ledger.lock().unwrap().try_take(places, Instant::now());
        taken.then(|| self.reservation(places))
    }

    /// `bytes`, once there is room: passing the larger ones that wait, until one of them waited
    /// `MAX_WAIT` or was passed `MAX_PASSES` times.
    pub async fn reserve(&self, bytes: u64) -> Reservation {
        let places = self.places(bytes);
        let ticket = {
            let mut ledger = self.shared.ledger.lock().unwrap();
            let now = Instant::now();
            if ledger.try_take(places, now) {
                return self.reservation(places);
            }
            ledger.wait(places, now)
        };
        let ticket = Ticket { shared: &self.shared, ticket };
        loop {
            // Registered before looking: memory set aside meanwhile wakes it all the same.
            let woken = self.shared.wake.notified();
            tokio::pin!(woken);
            woken.as_mut().enable();
            let taken = self.shared.ledger.lock().unwrap().take(ticket.ticket);
            if let Some(places) = taken {
                return self.reservation(places);
            }
            woken.await;
        }
    }
}

/// The budget of all the thumbnails: a quarter of the RAM.
pub fn budget() -> &'static Budget {
    static BUDGET: LazyLock<Budget> = LazyLock::new(|| Budget::of(quarter(ram()), ram()));
    &BUDGET
}

/// A quarter of `ram` (written in the log).
fn quarter(ram: u64) -> u64 {
    log!("thumbnails: at most {} MiB of memory at the same time (RAM {} MiB)", ram / 4 / MIB, ram / MIB);
    ram / 4
}

/// The computer's RAM, read once; `FALLBACK_RAM` when it cannot be read.
pub fn ram() -> u64 {
    static RAM: LazyLock<u64> = LazyLock::new(|| known(read_ram()));
    *RAM
}

/// `ram`, or `FALLBACK_RAM` when it could not be read (written in the log).
fn known(ram: Option<u64>) -> u64 {
    ram.unwrap_or_else(|| {
        log!("RAM not read: {} MiB assumed", FALLBACK_RAM / MIB);
        FALLBACK_RAM
    })
}

/// A picture that needs more than half of `ram` (Piero, 20/11/2026): no thumbnail, which on a PC
/// with 8 GB would take as much as the whole computer has (HEIC, 16-bit TIFF, PSD, JPEG XL or
/// OpenEXR near 512 megapixels: 5 to 17 GB), and the viewer asks before opening it.
pub fn over_half(need: u64, ram: u64) -> bool {
    need > ram / 2
}

/// The error of a thumbnail not made because its picture is `over_half` the RAM: the grid shows a
/// sign and a line in its place (`grid::paint_thumb`).
pub const TOO_LARGE: &str = "thumbnail: picture too large for the memory";

/// The computer's physical memory.
#[cfg(target_os = "linux")]
fn read_ram() -> Option<u64> {
    let info = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo_total(&info)
}

/// "MemTotal" of /proc/meminfo, in bytes.
#[cfg(any(target_os = "linux", test))]
fn meminfo_total(info: &str) -> Option<u64> {
    let kib = info.lines().find_map(|l| l.strip_prefix("MemTotal:"))?.trim().strip_suffix("kB")?.trim().parse::<u64>().ok()?;
    (kib > 0).then_some(kib * 1024)
}

#[cfg(windows)]
fn read_ram() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut status = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: `status` is valid and writable for the whole call.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) } != 0;
    (ok && status.ullTotalPhys > 0).then_some(status.ullTotalPhys)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn read_ram() -> Option<u64> {
    None
}

/// The memory of the thumbnail of `bytes`, waited for on a thread that may block (inside
/// `spawn_blocking`): for a picture decoded outside the workers (`thumb_queue::file_info`).
/// `Err(TOO_LARGE)` for a picture `over_half` the RAM, not to be decoded; `Ok(None)` outside the
/// runtime, where it cannot wait.
pub fn wait_for(bytes: &[u8], name: &str) -> Result<Option<Reservation>, &'static str> {
    let need = estimate(bytes, name);
    if budget().too_large(need) {
        return Err(TOO_LARGE);
    }
    Ok(budget().try_reserve(need).or_else(|| Some(tokio::runtime::Handle::try_current().ok()?.block_on(budget().reserve(need)))))
}

// The factors below: the peak of one thumbnail (VmHWM, build --release, one file per process)
// against width × height × the bytes per pixel of the decoded picture, on photos of 24 to 512
// megapixels (`notes/rapporti/miniature-177-2026-11-20.md`). Each is the largest measured for
// its decoder plus about 15% (five runs of the same file differ by less than 1%).

/// Besides the pixels, every thumbnail: decoder, its threads, the small copies (24-50 MB measured
/// for pictures under 2 megapixels, part of which is the process's own, once only).
pub(crate) const FIXED: u64 = 32 * MIB;

/// A JPEG decoded already reduced (`decode::jpeg_scaled`), per pixel of the whole picture:
/// 0.29-0.39 measured from 96 megapixels up, at most 1.9 at 24 (with `FIXED`, 0.6 covers them).
const JPEG_SCALED: f64 = 0.6;
/// A progressive JPEG reduced the same: libjpeg-turbo keeps the coefficients of the whole picture
/// (6.0 a pixel measured, 4:4:4).
pub(crate) const JPEG_PROGRESSIVE: f64 = 7.0;
/// A JPEG decoded whole (lossless, CMYK), RGB: 1.2 and 1.7 times the picture.
pub(crate) const JPEG_WHOLE: f64 = 2.0;
/// HEIC and AVIF, always decoded to RGBA at 8 bits (4 bytes): 2.3-2.8 times.
const HEIF: f64 = 3.25;
/// A PSD at 8 bits, RGBA (4 bytes): 3.0-3.3 times; every layer is decoded too, 4 bytes a pixel.
const PSD: f64 = 3.8;
const PSD_LAYER: f64 = 4.6;
/// PSD at 16 bits and PSB go to ImageMagick (16-bit RGBA, 8 bytes): 3.5 times; every layer 8
/// bytes a pixel more.
const PSD_MAGICK: f64 = 4.0;
const PSD_MAGICK_LAYER: f64 = 9.2;
/// JPEG XL: jxl-oxide works in 32-bit floats, 9.2 bytes a pixel for every channel measured.
const JXL_CHANNEL: f64 = 11.0;
/// The other formats `image` decodes (TIFF, PNG, WebP, GIF, EXR…): 1.3-2.0 times the picture.
const IMAGE: f64 = 2.3;
/// A DNG developed, per pixel of its sensor: 29-48.5 measured (iPhone ProRAW the highest).
const DNG: f64 = 56.0;
/// The other RAW formats give their size only with the sensor data: the estimate goes by the
/// file's size instead, 17-61 times measured on 44 files (a Canon CR3 the highest).
const RAW_PER_BYTE: u64 = 72;
/// An SVG, rasterised at `decode::SVG_SIDE` (12.7 bytes a pixel of the raster measured), and its
/// tree, several times the file.
const SVG: f64 = 4.0;
const SVG_PER_BYTE: u64 = 8;
/// A file whose header says no size (formats read only by ImageMagick or the rare decoders, a wrong
/// extension): 2.3-28 times its size measured (JPEG 2000 the highest), at least `UNKNOWN_FLOOR`.
const UNKNOWN_PER_BYTE: u64 = 32;
const UNKNOWN_FLOOR: u64 = 512 * MIB;

/// The memory the thumbnail of `bytes` takes, file included, from its header. Never refused, never
/// zero; more than the whole budget only means alone (`Budget::places`).
pub fn estimate(bytes: &[u8], name: &str) -> u64 {
    estimate_sized(bytes, bytes.len() as u64, name).need
}

/// An estimate, and what it went by: whether the bytes it was made from were enough (`sure`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Estimate {
    pub need: u64,
    basis: Basis,
}

/// What an estimate went by.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Basis {
    /// The size of the picture, read in its header.
    Header,
    /// The size of the file alone, by design (a RAW other than DNG, an SVG, a format whose header
    /// EyeCanSee never reads).
    Size,
    /// The size of the file, because the header was not found: it may be further on.
    Unread,
}

impl Estimate {
    /// Made from the first `read` bytes of a file of `size`, it is the file's own: they were the
    /// whole file, or held its header, or it goes by the size alone. Not so for a header that may
    /// be further on: a TIFF whose block of tags comes after the pixels, a DNG's raw image, a
    /// HEIC or a JPEG with a long start.
    pub fn sure(&self, read: usize, size: u64, name: &str) -> bool {
        read as u64 >= size || self.basis != Basis::Unread || !header_may_follow(name)
    }

    /// `need`, when it is over half of `ram` (`over_half`) and `sure`.
    pub fn over_half(&self, read: usize, size: u64, name: &str, ram: u64) -> Option<u64> {
        (over_half(self.need, ram) && self.sure(read, size, name)).then_some(self.need)
    }

    /// `need`, from a picture's header (a JPEG's, read by its own decoder).
    pub(crate) fn from_header(need: u64) -> Self {
        Estimate { need, basis: Basis::Header }
    }

    pub(crate) fn plus(self, more: u64) -> Self {
        Estimate { need: self.need.saturating_add(more), ..self }
    }
}

/// A format whose header `decode::header` reads, so that it may be further on in the file than
/// the bytes read: the formats of `image`, HEIC and AVIF, JPEG XL, PSD and DNG.
fn header_may_follow(name: &str) -> bool {
    let ext = std::path::Path::new(name).extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase();
    matches!(ext.as_str(), "heic" | "heif" | "hif" | "avif" | "jxl" | "psd" | "psb" | "dng") || image::ImageFormat::from_extension(&ext).is_some()
}

/// `estimate` from `bytes`, the start of a file of `file` bytes (or all of it).
pub fn estimate_sized(bytes: &[u8], file: u64, name: &str) -> Estimate {
    let (picture, basis) = if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        && let Some(jpeg) = jpeg(bytes)
    {
        (jpeg, Basis::Header)
    } else {
        picture(bytes, file, name)
    };
    Estimate { need: file.saturating_add(picture).saturating_add(FIXED), basis }
}

/// The memory of the decoding of a picture that is not a JPEG (decoded whole), and what it went by.
fn picture(bytes: &[u8], file: u64, name: &str) -> (u64, Basis) {
    if crate::decode::is_raw(name) {
        match crate::raw_meta::dng_pixels(bytes) {
            Some(px) => (times(px, DNG), Basis::Header),
            None if header_may_follow(name) => (file.saturating_mul(RAW_PER_BYTE), Basis::Unread),
            None => (file.saturating_mul(RAW_PER_BYTE), Basis::Size),
        }
    } else if crate::decode::is_svg(name) {
        let side = crate::decode::SVG_SIDE as u64;
        (times(side * side * 4, SVG).saturating_add(file.saturating_mul(SVG_PER_BYTE)), Basis::Size)
    } else {
        match crate::decode::header(bytes, name) {
            Some(h) => (decoded(&h, bytes), Basis::Header),
            None => (file.saturating_mul(UNKNOWN_PER_BYTE).max(UNKNOWN_FLOOR), Basis::Unread),
        }
    }
}

/// A JPEG: reduced while decoded, or whole (lossless, CMYK: `decode::jpeg_scaled` fails on them).
fn jpeg(bytes: &[u8]) -> Option<u64> {
    use turbojpeg::Colorspace;
    let h = turbojpeg::Decompressor::new().ok()?.read_header(bytes).ok()?;
    let px = (h.width as u64).saturating_mul(h.height as u64);
    Some(if h.is_lossless || matches!(h.colorspace, Colorspace::CMYK | Colorspace::YCCK) {
        times(px * 3, JPEG_WHOLE)
    } else if h.is_progressive {
        times(px, JPEG_PROGRESSIVE)
    } else {
        times(px, JPEG_SCALED)
    })
}

/// The pictures whose header `decode::header` reads, by their decoder.
fn decoded(h: &crate::decode::Header, bytes: &[u8]) -> u64 {
    let px = u64::from(h.size[0]) * u64::from(h.size[1]);
    let (channels, depth) = match &h.color {
        Some(c) => (if c.rgb { 3 } else { 1 } + u64::from(c.alpha), u64::from(c.bits.div_ceil(8).max(1))),
        // A PSD in CMYK, Lab or indexed colours: four channels at 16 bits, to be safe.
        None => (4, 2),
    };
    match h.format.as_str() {
        "HEIC" | "HEIF" | "HIF" | "AVIF" => times(px * 4, HEIF),
        "JPEG XL" => times(px * channels, JXL_CHANNEL),
        "PSD" | "PSB" => {
            let layers = f64::from(crate::psd::layers(bytes).unwrap_or(0));
            if h.format == "PSD" && depth == 1 && h.color.is_some() {
                times(px, 4.0 * PSD + layers * PSD_LAYER)
            } else {
                times(px, 8.0 * PSD_MAGICK + layers * PSD_MAGICK_LAYER)
            }
        }
        _ => times(px * channels * depth, IMAGE),
    }
}

pub(crate) fn times(bytes: u64, factor: f64) -> u64 {
    (bytes as f64 * factor).min(u64::MAX as f64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    const GIB: u64 = 1024 * MIB;

    impl Budget {
        fn free(&self) -> u32 {
            self.shared.ledger.lock().unwrap().free
        }
    }

    /// A reservation that fits is given at once; one that does not waits, the smaller ones that
    /// fit pass it, and it gets its turn as soon as there is room for it.
    #[test]
    fn small_ones_pass_a_waiting_large_one() {
        let b = Budget::new(100 * MIB);
        let first = b.try_reserve(60 * MIB).unwrap();
        assert!(b.try_reserve(50 * MIB).is_none());
        let mut large = Box::pin(b.reserve(50 * MIB));
        assert!(large.as_mut().now_or_never().is_none());
        let small = b.try_reserve(30 * MIB).expect("a small one passes the large one waiting");
        let mut also = Box::pin(b.reserve(5 * MIB));
        let also = also.as_mut().now_or_never().expect("waits for nothing");
        drop(first);
        let large = large.now_or_never().expect("room for it now");
        assert_eq!(large.places(), 50);
        assert_eq!(b.free(), 100 - 50 - 30 - 5);
        drop((large, small, also));
        assert_eq!(b.free(), 100);
    }

    /// Passed `MAX_PASSES` times, a large one waiting stops the small ones; it starts as soon as
    /// its memory is free, and then the small ones pass again.
    #[test]
    fn passed_too_often_the_large_one_goes_first() {
        let b = Budget::new(100 * MIB);
        let first = b.try_reserve(60 * MIB).unwrap();
        let mut large = Box::pin(b.reserve(50 * MIB));
        assert!(large.as_mut().now_or_never().is_none());
        for _ in 0..MAX_PASSES {
            drop(b.try_reserve(MIB).expect("passes"));
        }
        assert!(b.try_reserve(MIB).is_none(), "passed enough");
        let mut small = Box::pin(b.reserve(MIB));
        assert!(small.as_mut().now_or_never().is_none(), "waits behind it");
        drop(first);
        let large = large.now_or_never().unwrap();
        let small = small.now_or_never().expect("served with it, there is room");
        assert_eq!(b.free(), 100 - 50 - 1);
        assert!(b.try_reserve(MIB).is_some());
        drop((large, small));
        assert_eq!(b.free(), 100);
    }

    /// `MAX_WAIT` after it asked, a large one waiting is passed no more, however few passed it;
    /// the ones that were waiting behind it are served after it, in their order.
    #[test]
    fn waited_too_long_the_large_one_goes_first() {
        let t0 = Instant::now();
        let mut l = Ledger { free: 100, next: 0, queue: VecDeque::new() };
        assert!(l.try_take(60, t0));
        let large = l.wait(50, t0);
        assert!(l.try_take(10, t0 + MAX_WAIT / 2), "passes it");
        let late = t0 + MAX_WAIT;
        assert!(!l.try_take(10, late), "waited too long");
        let small = l.wait(10, late);
        l.give_back(10, late);
        assert_eq!((l.take(large), l.take(small)), (None, None), "40 free, not enough for it");
        l.give_back(60, late);
        assert_eq!((l.take(large), l.take(small)), (Some(50), Some(10)));
        assert_eq!(l.free, 40);
        assert!(l.try_take(10, late));
    }

    /// A wait given up (its future dropped) leaves the queue: a large one that stopped the small
    /// ones no longer stops them, and memory set aside for one that went is given back.
    #[test]
    fn wait_given_up() {
        let b = Budget::new(100 * MIB);
        let first = b.try_reserve(60 * MIB).unwrap();
        let mut large = Box::pin(b.reserve(50 * MIB));
        assert!(large.as_mut().now_or_never().is_none());
        for _ in 0..MAX_PASSES {
            drop(b.try_reserve(MIB).unwrap());
        }
        assert!(b.try_reserve(MIB).is_none());
        drop(large);
        assert!(b.try_reserve(MIB).is_some(), "no longer stopped");
        let mut granted = Box::pin(b.reserve(50 * MIB));
        assert!(granted.as_mut().now_or_never().is_none());
        drop(first);
        assert_eq!(b.free(), 50, "set aside for the one waiting");
        drop(granted);
        assert_eq!(b.free(), 100, "given back: it went before taking it");
    }

    /// A picture larger than the whole budget waits until nothing else is reserved, then takes
    /// it all: alone, never refused. Small ones pass it until it was passed enough, then wait.
    #[test]
    fn larger_than_budget_goes_alone() {
        let b = Budget::new(100 * MIB);
        let small = b.try_reserve(MIB).unwrap();
        assert!(b.try_reserve(10 * GIB).is_none());
        let mut big = Box::pin(b.reserve(10 * GIB));
        assert!(big.as_mut().now_or_never().is_none());
        let mut passing: Vec<Reservation> = (0..MAX_PASSES).map(|_| b.try_reserve(MIB).expect("passes")).collect();
        assert!(b.try_reserve(MIB).is_none());
        passing.pop();
        assert!(big.as_mut().now_or_never().is_none(), "the others still hold memory");
        drop((small, passing));
        let big = big.now_or_never().unwrap();
        assert_eq!(big.places(), 100);
        assert!(b.try_reserve(1).is_none());
        drop(big);
        assert!(b.try_reserve(u64::MAX).is_some());
    }

    /// Across threads, as the workers: one woken when the memory is given back by another.
    #[test]
    fn woken_by_another_thread() {
        let b = Arc::new(Budget::new(100 * MIB));
        let first = b.try_reserve(100 * MIB).unwrap();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).build().unwrap();
        let waiting = rt.spawn({
            let b = b.clone();
            async move { b.reserve(70 * MIB).await.places() }
        });
        std::thread::sleep(Duration::from_millis(50));
        drop(first);
        assert_eq!(rt.block_on(waiting).unwrap(), 70);
        assert_eq!(b.free(), 100);
    }

    /// The memory reserved is given back when the work fails or panics.
    #[test]
    fn given_back_on_error_and_panic() {
        let b = Budget::new(100 * MIB);
        let failing = |r: Reservation| -> Result<(), String> {
            let _r = r;
            Err("broken file".into())
        };
        assert!(failing(b.try_reserve(80 * MIB).unwrap()).is_err());
        assert_eq!(b.free(), 100);
        let r = b.try_reserve(80 * MIB).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let joined = rt.block_on(async {
            tokio::task::spawn_blocking(move || {
                let _r = r;
                panic!("decoder");
            })
            .await
        });
        assert!(joined.is_err());
        assert_eq!(b.free(), 100);
    }

    /// A quarter of the RAM; when it is not read, a quarter of 8 GiB.
    #[test]
    fn budget_from_ram() {
        assert_eq!(quarter(known(Some(16 * GIB))), 4 * GIB);
        assert_eq!(quarter(known(None)), 2 * GIB);
        assert_eq!(meminfo_total("MemTotal:       32529440 kB\nMemFree: 1 kB\n"), Some(32529440 * 1024));
        assert_eq!(meminfo_total("MemFree: 1 kB\n"), None);
        assert_eq!(meminfo_total("MemTotal: x kB\n"), None);
        assert_eq!(Budget::new(0).total, 1);
        assert_eq!(budget().ram, ram());
        #[cfg(target_os = "linux")]
        assert!(read_ram().is_some_and(|r| r > GIB));
        assert!(ram() > GIB);
    }

    /// Over half the RAM, not at half: on 8 GiB a picture of 4 GiB still gets its thumbnail, one
    /// byte more does not; the RAM not read counts as 8 GiB.
    #[test]
    fn too_large_over_half_the_ram() {
        assert!(!over_half(4 * GIB, 8 * GIB));
        assert!(over_half(4 * GIB + 1, 8 * GIB));
        assert!(!over_half(4 * GIB + 1, 16 * GIB));
        assert!(over_half(4 * GIB + 1, known(None)));
        // A 512-megapixel TIFF at 16 bits (32768 × 16384, RGB): about 6.4 GB, over half of 8 GiB
        // and of 12, under half of 16; a JPEG as large, read reduced, under half of 8.
        let tiff = times(32768 * 16384 * 6, IMAGE) + FIXED;
        assert!(over_half(tiff, 8 * GIB) && over_half(tiff, 12 * GIB) && !over_half(tiff, 16 * GIB));
        assert!(!over_half(times(32768 * 16384, JPEG_SCALED) + FIXED, 8 * GIB));
    }

    fn jpeg_bytes(w: u32, h: u32, lossless: bool) -> Vec<u8> {
        let img = image::RgbImage::new(w, h);
        let mut c = turbojpeg::Compressor::new().unwrap();
        c.set_lossless(lossless).unwrap();
        let pixels = turbojpeg::Image { pixels: img.as_raw().as_slice(), width: w as usize, pitch: w as usize * 3, height: h as usize, format: turbojpeg::PixelFormat::RGB };
        c.compress_to_vec(pixels).unwrap()
    }

    /// A JPEG read reduced reserves little; a lossless one, decoded whole, its whole picture.
    #[test]
    fn jpeg_estimates() {
        let (w, h) = (1200, 800);
        let px = u64::from(w * h);
        let small = jpeg_bytes(w, h, false);
        assert_eq!(estimate(&small, "a.jpg"), small.len() as u64 + times(px, JPEG_SCALED) + FIXED);
        let whole = jpeg_bytes(w, h, true);
        assert_eq!(estimate(&whole, "a.jpg"), whole.len() as u64 + times(px * 3, JPEG_WHOLE) + FIXED);
    }

    /// The other formats by the size their header says, and a 16-bit picture twice an 8-bit one.
    #[test]
    fn header_estimates() {
        let png = |img: image::DynamicImage| {
            let mut out = std::io::Cursor::new(Vec::new());
            img.write_to(&mut out, image::ImageFormat::Png).unwrap();
            out.into_inner()
        };
        let p8 = png(image::DynamicImage::new_rgb8(1000, 500));
        let p16 = png(image::DynamicImage::new_rgb16(1000, 500));
        assert_eq!(estimate(&p8, "a.png"), p8.len() as u64 + times(500_000 * 3, IMAGE) + FIXED);
        assert_eq!(estimate(&p16, "a.png"), p16.len() as u64 + times(500_000 * 6, IMAGE) + FIXED);
    }

    /// A PSD of 100 × 50 with `layers` layers, 8 or 16 bits, its sections empty but for the count.
    fn psd_bytes(bits: u16, layers: i16) -> Vec<u8> {
        let mut b = b"8BPS".to_vec();
        b.extend(1u16.to_be_bytes());
        b.extend([0; 6]);
        b.extend(3u16.to_be_bytes());
        b.extend(50u32.to_be_bytes());
        b.extend(100u32.to_be_bytes());
        b.extend(bits.to_be_bytes());
        b.extend(3u16.to_be_bytes());
        b.extend([0; 8]);
        b.extend(6u32.to_be_bytes());
        b.extend(2u32.to_be_bytes());
        b.extend(layers.to_be_bytes());
        b
    }

    /// Each layer of a PSD counts, and a 16-bit one (ImageMagick's) more than an 8-bit one.
    #[test]
    fn psd_estimates() {
        assert_eq!(crate::psd::layers(&psd_bytes(8, 0)), Some(0));
        assert_eq!(crate::psd::layers(&psd_bytes(8, -3)), Some(3));
        assert_eq!(crate::psd::layers(b"8BPS"), None);
        let flat = estimate(&psd_bytes(8, 0), "a.psd");
        let layered = estimate(&psd_bytes(8, 3), "a.psd");
        assert_eq!(layered - flat, times(5000, 4.0 * PSD + 3.0 * PSD_LAYER) - times(5000, 4.0 * PSD));
        assert!(estimate(&psd_bytes(16, 0), "a.psd") > flat);
        // Photoshop's 16-bit files: no layer info, the layers in an "Lr16" block after the mask,
        // behind another block.
        let mut b = psd_bytes(16, 0);
        b.truncate(b.len() - 10);
        b.extend(38u32.to_be_bytes());
        b.extend([0; 8]);
        b.extend(b"8BIMlnsr");
        b.extend(4u32.to_be_bytes());
        b.extend(b"rend");
        b.extend(b"8BIMLr16");
        b.extend(2u32.to_be_bytes());
        b.extend(5i16.to_be_bytes());
        assert_eq!(crate::psd::layers(&b), Some(5));
    }

    /// No size from the header: a RAW by its file, anything else at least `UNKNOWN_FLOOR`.
    #[test]
    fn unknown_sizes() {
        let raw = vec![0u8; 1000];
        assert_eq!(estimate(&raw, "a.cr3"), 1000 + 1000 * RAW_PER_BYTE + FIXED);
        assert_eq!(estimate(&raw, "a.xcf"), 1000 + UNKNOWN_FLOOR + FIXED);
        assert_eq!(estimate(&[], "a.heic"), UNKNOWN_FLOOR + FIXED);
    }

    /// From the start of a file and its size: the size counts as the file's, and the estimate is
    /// sure when the start holds the header or the estimate goes by the size alone; not for a
    /// header that may be further on (a TIFF, a DNG), unless the start is the whole file.
    #[test]
    fn sure_from_the_start() {
        let size = 230 << 20;
        let head = vec![0x5Au8; 1000];
        let raw = estimate_sized(&head, size, "huge.cr3");
        assert_eq!(raw.need, size + size * RAW_PER_BYTE + FIXED);
        assert!(raw.sure(head.len(), size, "huge.cr3"));
        assert_eq!(raw.over_half(head.len(), size, "huge.cr3", 8 * GIB), Some(raw.need));
        assert_eq!(raw.over_half(head.len(), size, "huge.cr3", 64 * GIB), None);
        let xcf = estimate_sized(&head, size, "a.xcf");
        assert_eq!(xcf.need, size + size * UNKNOWN_PER_BYTE + FIXED);
        assert!(xcf.sure(head.len(), size, "a.xcf"), "a rare format: by the size alone");
        for name in ["a.tif", "a.dng", "a.heic", "a.psd", "a.jpg"] {
            let e = estimate_sized(&head, size, name);
            assert!(!e.sure(head.len(), size, name), "{name}");
            assert_eq!(e.over_half(head.len(), size, name, 8 * GIB), None, "{name}");
            assert!(e.sure(1000, 1000, name), "{name}: the whole file");
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1000, 500).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let png = png.into_inner();
        let start = &png[..png.len().min(64)];
        let e = estimate_sized(start, size, "a.png");
        assert_eq!(e.need, size + times(500_000 * 3, IMAGE) + FIXED);
        assert!(e.sure(start.len(), size, "a.png"), "its header in the start");
    }

    /// Peak memory of ONE thumbnail made as `thumb_queue::generate` makes it, for the factors
    /// above: `EYECANSEE_PEAK_FILE=<file> cargo test --release thumb_peak_live -- --ignored
    /// --exact --nocapture`, one file per process, with a temporary `XDG_CACHE_HOME`. Linux only
    /// (VmHWM of /proc/self/status). The file's bytes are read before, so not in the delta.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn thumb_peak_live() {
        let status = |key: &str| -> u64 {
            let s = std::fs::read_to_string("/proc/self/status").unwrap();
            let line = s.lines().find(|l| l.starts_with(key)).unwrap();
            line.split_whitespace().nth(1).unwrap().parse::<u64>().unwrap() * 1024
        };
        let path = std::env::var("EYECANSEE_PEAK_FILE").unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let file = crate::source::File { path: path.clone(), size: bytes.len() as u64, modified: String::new() };
        let need = estimate(&bytes, &path);
        let before = status("VmRSS:");
        let (img, own) = crate::decode::thumbnail_sized(&bytes, &path, crate::loader::THUMB_SIDE).unwrap();
        let info = crate::thumb_details::Info::of(&bytes, &path, (img.width(), img.height()), own);
        crate::thumbcache::save("/", &file, &img);
        let _ = crate::decode::to_color_image(img);
        let delta = status("VmHWM:").saturating_sub(before);
        let header = crate::decode::header(&bytes, &path).map(|h| (h.size, h.color.map(|c| (c.rgb, c.alpha, c.bits))));
        println!("PEAK\t{path}\tfile={}\tdelta={delta}\testimate={need}\tcovered={}\town={own:?}\tpixels={:?}\theader={header:?}", bytes.len(), need >= delta + bytes.len() as u64, info.pixels);
    }
}
