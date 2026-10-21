//! Print the MTP devices the USB layer (nusb) enumerates: VID/PID, serial, location, product.
//! Useful for correlating a USB location/serial to a WPD device on Windows.
//! Run: `cargo run -p mtp-rs --example list_devices`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = mtp_rs::MtpDevice::list_devices()?;
    if devices.is_empty() {
        println!("(no MTP devices enumerated by the USB layer)");
    }
    for d in &devices {
        println!(
            "vid={:04x} pid={:04x} serial={:?} location={:#018x} product={:?}",
            d.vendor_id, d.product_id, d.serial_number, d.location_id, d.product
        );
    }
    Ok(())
}
