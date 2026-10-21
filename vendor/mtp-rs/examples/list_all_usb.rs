//! Diagnostic tool to list all USB devices and their interfaces.
//!
//! Run with: cargo run --example list_all_usb

use nusb::MaybeFuture;

fn main() {
    println!("Listing all USB devices...\n");

    let devices = match nusb::list_devices().wait() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error listing devices: {}", e);
            return;
        }
    };

    let devices: Vec<_> = devices.collect();
    println!("Found {} USB device(s)\n", devices.len());

    for dev in &devices {
        println!(
            "Device: {:04x}:{:04x} at bus {} address {}",
            dev.vendor_id(),
            dev.product_id(),
            dev.bus_id(),
            dev.device_address()
        );
        println!(
            "  Device class/subclass/protocol: {:02x}/{:02x}/{:02x}",
            dev.class(),
            dev.subclass(),
            dev.protocol()
        );
        println!(
            "  Manufacturer: {:?}, Product: {:?}",
            dev.manufacturer_string(),
            dev.product_string()
        );

        // Check if this could be MTP at device level
        let is_mtp_device_level = is_mtp_class(dev.class(), dev.subclass(), dev.protocol());
        let is_composite = dev.class() == 0;

        if is_mtp_device_level {
            println!("  -> MTP device (device-level class)");
        } else if is_composite {
            println!("  -> Composite device (class 0) - might have MTP interface");
        }

        // Try to open and inspect interfaces
        match dev.open().wait() {
            Ok(device) => {
                if let Ok(config) = device.active_configuration() {
                    println!("  Interfaces:");
                    for interface in config.interfaces() {
                        for alt in interface.alt_settings() {
                            let is_mtp = is_mtp_class(alt.class(), alt.subclass(), alt.protocol());
                            let mtp_marker = if is_mtp { " <-- MTP!" } else { "" };
                            println!(
                                "    Interface {}: class/subclass/protocol = {:02x}/{:02x}/{:02x}{}",
                                interface.interface_number(),
                                alt.class(),
                                alt.subclass(),
                                alt.protocol(),
                                mtp_marker
                            );

                            // Show endpoints for MTP interfaces
                            if is_mtp {
                                for ep in alt.endpoints() {
                                    println!(
                                        "      Endpoint 0x{:02x}: {:?} {:?}",
                                        ep.address(),
                                        ep.direction(),
                                        ep.transfer_type()
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("  (Could not open device: {})", e);
            }
        }
        println!();
    }

    // Summary
    let potential_mtp: Vec<_> = devices
        .iter()
        .filter(|d| is_mtp_class(d.class(), d.subclass(), d.protocol()) || d.class() == 0)
        .collect();

    println!("---");
    println!(
        "Potential MTP devices (device-level MTP class or composite): {}",
        potential_mtp.len()
    );
    for dev in potential_mtp {
        println!(
            "  {:04x}:{:04x} - {:?}",
            dev.vendor_id(),
            dev.product_id(),
            dev.product_string()
        );
    }
}

fn is_mtp_class(class: u8, subclass: u8, protocol: u8) -> bool {
    (class == 0x06 || class == 0xFF) && subclass == 0x01 && protocol == 0x01
}
