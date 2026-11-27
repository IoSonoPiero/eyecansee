//! The wait for what a phone goes on making after its call was given up (`across`): the
//! destination is looked at every few seconds until the item is there. Apart from the phone, so
//! that it runs in the tests with a destination that is not one.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How the wait goes.
pub(super) struct Pace<'a> {
    /// How often the destination is looked at.
    pub look: Duration,
    /// The longest the wait lasts.
    pub most: Duration,
    /// Raised by who no longer wants to wait (`across::leave`).
    pub leave: &'a AtomicBool,
}

/// How the wait ended.
#[derive(Debug)]
pub(super) enum End<T> {
    /// What was waited for is there: what the look found.
    Arrived(T),
    /// The phone went meanwhile: the error that said so.
    Gone(mtp_rs::Error),
    /// Left before the item was there (`Pace::leave`): the phone may still be at it.
    Left,
    /// Not there after `Pace::most`.
    Late,
}

/// Looks at the destination with `look` (given the time waited so far) every `Pace::look` until
/// it finds what is waited for. A look that fails is the phone still at work, which answers
/// nothing (the reading was given up too): the wait goes on, unless the failure says the phone is
/// gone (`gone`, in the words of Windows too). The wait is left (`Pace::leave`) between two looks,
/// never during one: a call of mtp-rs dropped half way can leave the phone's session stuck
/// (mtp-rs, AGENTS.md, "dropped future"), so leaving takes as long as the look under way.
pub(super) async fn until_arrived<T, F, Fut>(pace: Pace<'_>, mut look: F) -> End<T>
where
    F: FnMut(Duration) -> Fut,
    Fut: Future<Output = Result<Option<T>, mtp_rs::Error>>,
{
    let start = Instant::now();
    while start.elapsed() < pace.most {
        if left(&pace).await {
            return End::Left;
        }
        match look(start.elapsed()).await {
            Ok(Some(h)) => return End::Arrived(h),
            Ok(None) => {}
            Err(e) if super::gone(&e) => return End::Gone(e),
            Err(_) => {}
        }
    }
    End::Late
}

/// The pause before a look, in short steps: `true` as soon as the wait is left.
async fn left(pace: &Pace<'_>) -> bool {
    const STEP: Duration = Duration::from_millis(100);
    let until = Instant::now() + pace.look;
    loop {
        if pace.leave.load(Ordering::Relaxed) {
            return true;
        }
        let rest = until.saturating_duration_since(Instant::now());
        if rest.is_zero() {
            return false;
        }
        tokio::time::sleep(rest.min(STEP)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use mtp_rs::ObjectHandle;

    use super::*;

    /// Nobody leaves the wait.
    static STAY: AtomicBool = AtomicBool::new(false);

    fn run<T>(wait: impl Future<Output = T>) -> T {
        tokio::runtime::Runtime::new().unwrap().block_on(wait)
    }

    /// A wait that looks every 5 ms, for `most` ms.
    fn pace(most: u64) -> Pace<'static> {
        Pace { look: Duration::from_millis(5), most: Duration::from_millis(most), leave: &STAY }
    }

    /// The wait with a destination that always answers `answer`: how it ended, after how many looks.
    fn ended(most: u64, answer: impl Fn() -> Result<Option<ObjectHandle>, mtp_rs::Error>) -> (End<ObjectHandle>, usize) {
        let looks = AtomicUsize::new(0);
        let end = run(until_arrived(pace(most), |_| {
            looks.fetch_add(1, Ordering::Relaxed);
            let r = answer();
            async move { r }
        }));
        (end, looks.load(Ordering::Relaxed))
    }

    /// What WPD answers to every reading once the phone is unplugged (prova W5 on Windows,
    /// 27/11/2026): mtp-rs leaves it as `Other`.
    fn wpd_unplugged() -> mtp_rs::Error {
        mtp_rs::Error::Other { detail: "HRESULT 0x802a0002: Chiamata di arresto già eseguita su questo oggetto o sull'oggetto proprietario.".into() }
    }

    /// #354 on Windows: the phone unplugged while EyeCanSee waited for its copy. Every reading of
    /// the destination said so (`0x802a0002`), and the wait went on, reading every 3 s, for up to
    /// 1 h 47 min for 6 GB. It ends at the first reading that says the phone is gone, also in the
    /// words of Windows. A reading given up while the phone still copies (`Timeout`) is not that.
    #[test]
    fn an_unplugged_phone_ends_the_wait_at_once() {
        let aborted = || mtp_rs::Error::Other { detail: "HRESULT 0x800703e3: Operazione di I/O terminata".into() };
        for (what, gone) in [("Disconnected", (|| mtp_rs::Error::Disconnected) as fn() -> mtp_rs::Error), ("0x802a0002", wpd_unplugged), ("0x800703e3", aborted)] {
            let (end, looks) = ended(300, || Err(gone()));
            assert!(matches!(end, End::Gone(_)) && looks == 1, "{what}: {end:?} after {looks} looks");
        }
        let (end, looks) = ended(60, || Err(mtp_rs::Error::Timeout));
        assert!(matches!(end, End::Late) && looks > 1, "the phone still copying: {end:?} after {looks} looks");
    }

    #[test]
    fn the_wait_ends_when_the_item_is_there() {
        let seen = AtomicUsize::new(0);
        let (end, looks) = ended(300, || Ok((seen.fetch_add(1, Ordering::Relaxed) == 2).then_some(ObjectHandle(7))));
        assert!(matches!(end, End::Arrived(ObjectHandle(7))) && looks == 3, "{end:?} after {looks} looks");
        assert!(matches!(ended(30, || Ok(None)).0, End::Late));
    }

    /// #354 (prova W5 on Windows, 27/11): the wait for a copy the phone goes on making ended only
    /// when the item was there or at its deadline (1 h 47 min for 6 GB): Cancel did nothing until
    /// then, and the ✕ of the window, which waits for the copy to end, never closed EyeCanSee. The
    /// wait can be left: asked while it runs it ends before the next look, and asked before it
    /// starts (during the call that was given up) it ends without one.
    #[test]
    fn the_wait_can_be_left() {
        let (leave, looks) = (AtomicBool::new(false), AtomicUsize::new(0));
        let pace = Pace { look: Duration::from_millis(5), most: Duration::from_millis(400), leave: &leave };
        let end: End<ObjectHandle> = run(until_arrived(pace, |_| {
            leave.store(looks.fetch_add(1, Ordering::Relaxed) == 2, Ordering::Relaxed);
            async { Ok(None) }
        }));
        assert!(matches!(end, End::Left) && looks.load(Ordering::Relaxed) == 3, "{end:?} after {} looks", looks.load(Ordering::Relaxed));
        let pace = Pace { look: Duration::from_secs(60), most: Duration::from_secs(600), leave: &leave };
        let start = Instant::now();
        let end: End<ObjectHandle> = run(until_arrived(pace, |_| async { Ok(None) }));
        assert!(matches!(end, End::Left) && start.elapsed() < Duration::from_secs(5), "asked before the wait: {end:?} after {:?}", start.elapsed());
    }
}
