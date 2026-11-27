//! The wait for what a phone goes on making after its call was given up (`across`): the
//! destination is looked at every few seconds until the item is there. Apart from the phone, so
//! that it runs in the tests with a destination that is not one.

use std::time::{Duration, Instant};

use mtp_rs::ObjectHandle;

/// How the wait goes.
pub(super) struct Pace {
    /// How often the destination is looked at.
    pub look: Duration,
    /// The longest the wait lasts.
    pub most: Duration,
}

/// How the wait ended.
#[derive(Debug)]
pub(super) enum End {
    /// What was waited for is there: its handle.
    Arrived(ObjectHandle),
    /// The phone went meanwhile: the error that said so.
    Gone(mtp_rs::Error),
    /// Not there after `Pace::most`.
    Late,
}

/// Looks at the destination with `look` (given the time waited so far) every `Pace::look` until
/// it finds what is waited for. A look that fails is the phone still at work, which answers
/// nothing (the reading was given up too): the wait goes on.
pub(super) async fn until_arrived<F, Fut>(pace: Pace, mut look: F) -> End
where
    F: FnMut(Duration) -> Fut,
    Fut: Future<Output = Result<Option<ObjectHandle>, mtp_rs::Error>>,
{
    let start = Instant::now();
    while start.elapsed() < pace.most {
        tokio::time::sleep(pace.look).await;
        match look(start.elapsed()).await {
            Ok(Some(h)) => return End::Arrived(h),
            Ok(None) => {}
            Err(e) if e.is_disconnected() => return End::Gone(e),
            Err(_) => {}
        }
    }
    End::Late
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn run<T>(wait: impl Future<Output = T>) -> T {
        tokio::runtime::Runtime::new().unwrap().block_on(wait)
    }

    /// A wait that looks every 5 ms, for `most` ms.
    fn pace(most: u64) -> Pace {
        Pace { look: Duration::from_millis(5), most: Duration::from_millis(most) }
    }

    /// The wait with a destination that always answers `answer`: how it ended, after how many looks.
    fn ended(most: u64, answer: impl Fn() -> Result<Option<ObjectHandle>, mtp_rs::Error>) -> (End, usize) {
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
}
