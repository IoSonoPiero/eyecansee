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
