//! Manual event monitor for the Windows WPD backend against a **real device**.
//!
//! Opens the first WPD device, registers happen automatically at open (the backend `Advise`s a WPD
//! event callback), and prints every [`DeviceEvent`] for ~30 seconds. Change the device by hand
//! while it runs — add/remove a file from the phone, take a photo, etc. — to see which events the
//! device actually emits.
//!
//! Windows-only; needs a phone connected in MTP/File-transfer mode, screen unlocked.
//! Run: `cargo run -p mtp-rs --example wpd_events`

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mtp_rs::{Backend, MtpDevice};
    use std::time::{Duration, Instant};
    use tokio::time::timeout;

    let device = MtpDevice::builder()
        .backend(Backend::Wpd)
        .open_first()
        .await?;

    let info = device.device_info();
    println!(
        "Device: {} {} (serial {:?})",
        info.manufacturer, info.model, info.serial_number
    );
    println!(
        "supports_events = {}",
        device.capabilities().supports_events
    );
    println!("\nWatching for events for 30s — change the device by hand now...\n");

    let deadline = Instant::now() + Duration::from_secs(30);
    let mut count = 0u32;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match timeout(remaining.min(Duration::from_secs(5)), device.next_event()).await {
            Ok(Ok(event)) => {
                count += 1;
                println!("  event #{count}: {event:?}");
            }
            Ok(Err(e)) => {
                println!("  next_event error: {e} (stopping)");
                break;
            }
            Err(_) => { /* poll window elapsed with no event; loop until the 30s deadline */ }
        }
    }

    println!("\nDone. Observed {count} event(s).");
    device.close().await?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("wpd_events is Windows-only.");
}
