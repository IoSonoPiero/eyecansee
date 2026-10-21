//! Print MTP devices as they're plugged in and unplugged.
//!
//! Demonstrates `mtp::watch_devices`: one stream that reports the devices already
//! connected and then every arrival and departure, with non-MTP USB traffic (mice,
//! hubs, chargers) filtered out. Plug a phone in and out while this runs.
//!
//! Needs real hardware, since hotplug events come from the OS:
//!
//! ```text
//! cargo run --example watch_devices
//! ```

use futures::StreamExt;
use mtp_rs::mtp::{watch_devices, HotplugEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut watch = watch_devices()?;

    println!("Watching for MTP devices. Plug one in or out; Ctrl-C to stop.\n");

    // Devices already connected arrive first, so there's no separate enumeration
    // step and nothing plugged in during startup can slip past.
    while let Some(event) = watch.next().await {
        match event {
            HotplugEvent::Arrived(info) => {
                println!("+ {}", info.display());
                println!("    matched by: {}", info.match_reason.as_str());
                if let Some(speed) = info.speed {
                    println!("    link speed: {speed:?}");
                }
            }
            // The OS reports only an opaque id on disconnect; the watch remembers
            // what it last saw, so the departing device is still fully identified.
            HotplugEvent::Left(info) => println!("- {}", info.display()),
        }
    }

    Ok(())
}
