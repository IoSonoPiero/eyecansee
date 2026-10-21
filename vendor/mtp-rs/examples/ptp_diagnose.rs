//! PTP camera diagnostic script.
//!
//! Run with: cargo run --example ptp_diagnose

use mtp_rs::ptp::{DevicePropertyCode, PropertyDataType, PtpDevice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== PTP camera diagnostic ===\n");

    // Connect via low-level PTP
    let device = PtpDevice::open_first().await?;
    let session = device.open_session().await?;

    // Get device info
    let info = session.get_device_info().await?;
    println!("Manufacturer: {}", info.manufacturer);
    println!("Model: {}", info.model);
    println!("Serial: {}", info.serial_number);
    println!("Device version: {}", info.device_version);
    println!("Vendor extension: {}", info.vendor_extension_desc);
    println!();

    // Show supported operations
    println!(
        "=== Supported operations ({}) ===",
        info.operations_supported.len()
    );
    for op in &info.operations_supported {
        println!("  {:?}", op);
    }
    println!();

    // Show supported device properties
    println!(
        "=== Supported device properties ({}) ===",
        info.device_properties_supported.len()
    );
    for prop_code in &info.device_properties_supported {
        let prop = DevicePropertyCode::from(*prop_code);
        print!("  0x{:04X} {:?}", prop_code, prop);

        // Try to read the property descriptor
        match session.get_device_prop_desc(prop).await {
            Ok(desc) => {
                let rw = if desc.writable { "RW" } else { "RO" };
                println!(" [{}] = {:?}", rw, desc.current_value);
            }
            Err(e) => {
                println!(" - Error: {}", e);
            }
        }
    }
    println!();

    // Test some standard properties
    println!("=== Standard property tests ===");

    let test_props = [
        (DevicePropertyCode::BatteryLevel, PropertyDataType::Uint8),
        (DevicePropertyCode::FNumber, PropertyDataType::Uint16),
        (DevicePropertyCode::ExposureTime, PropertyDataType::Uint32),
        (DevicePropertyCode::ExposureIndex, PropertyDataType::Uint16),
        (
            DevicePropertyCode::ExposureProgramMode,
            PropertyDataType::Uint16,
        ),
        (DevicePropertyCode::WhiteBalance, PropertyDataType::Uint16),
        (DevicePropertyCode::FocusMode, PropertyDataType::Uint16),
        (DevicePropertyCode::DateTime, PropertyDataType::String),
    ];

    for (prop, _expected_type) in test_props {
        print!("{:?}: ", prop);
        match session.get_device_prop_desc(prop).await {
            Ok(desc) => {
                println!(
                    "{:?} ({})",
                    desc.current_value,
                    if desc.writable {
                        "writable"
                    } else {
                        "read-only"
                    }
                );
                if let Some(ref range) = desc.range {
                    println!(
                        "    Range: {:?} to {:?}, step {:?}",
                        range.min, range.max, range.step
                    );
                }
                if let Some(vals) = &desc.enum_values {
                    let count = vals.len();
                    if count <= 10 {
                        println!("    Allowed: {:?}", vals);
                    } else {
                        println!("    Allowed: {} values", count);
                    }
                }
            }
            Err(e) => println!("Not supported ({})", e),
        }
    }

    println!("\n=== Diagnostic complete ===");
    Ok(())
}
