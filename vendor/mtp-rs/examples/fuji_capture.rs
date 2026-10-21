//! Fuji-specific capture sequence.
//!
//! Run with: cargo run --example fuji_capture

use mtp_rs::ptp::{DevicePropertyCode, ObjectFormatCode, PropertyValue, PtpDevice, StorageId};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Fuji X-T4 capture sequence ===\n");

    let device = PtpDevice::open_first().await?;
    let session = device.open_session().await?;

    // Fuji-specific property codes
    let prop_0xd207 = DevicePropertyCode::Unknown(0xD207); // PriorityMode
    let prop_0xd208 = DevicePropertyCode::Unknown(0xD208); // Capture control
    let prop_0xd209 = DevicePropertyCode::Unknown(0xD209); // AFStatus

    // Step 1: Set priority mode to USB control (0x0002)
    println!("Step 1: Setting priority mode to USB control (0xD207 = 2)...");
    match session
        .set_device_prop_value_typed(prop_0xd207, &PropertyValue::Uint16(2))
        .await
    {
        Ok(()) => println!("  OK"),
        Err(e) => println!("  Error: {} (continuing anyway)", e),
    }

    // Check current 0xD208 value
    println!("\nCurrent 0xD208 value:");
    match session.get_device_prop_desc(prop_0xd208).await {
        Ok(desc) => println!("  {:?} (writable: {})", desc.current_value, desc.writable),
        Err(e) => println!("  Error: {}", e),
    }

    // Step 2: Set 0xD208 to 0x0200 (focus)
    println!("\nStep 2: Setting capture control to FOCUS (0xD208 = 0x0200)...");
    match session
        .set_device_prop_value_typed(prop_0xd208, &PropertyValue::Uint16(0x0200))
        .await
    {
        Ok(()) => println!("  OK"),
        Err(e) => {
            println!("  Error: {}", e);
            // Try continuing anyway
        }
    }

    // Step 3: InitiateCapture (triggers focus)
    println!("\nStep 3: InitiateCapture (focus phase)...");
    match session
        .initiate_capture(StorageId(0), ObjectFormatCode::Undefined)
        .await
    {
        Ok(()) => println!("  OK - focus initiated"),
        Err(e) => println!("  Error: {:?}", e),
    }

    // Step 4: Poll AFStatus until ready
    println!("\nStep 4: Polling AFStatus (0xD209)...");
    for i in 0..50 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        match session.get_device_prop_desc(prop_0xd209).await {
            Ok(desc) => {
                println!("  Attempt {}: {:?}", i + 1, desc.current_value);
                if let PropertyValue::Uint16(status) = desc.current_value {
                    if status != 0x0001 {
                        if status == 2 {
                            println!("  -> Focus OK!");
                        } else if status == 3 {
                            println!("  -> Focus FAILED (out of focus?)");
                        }
                        break;
                    }
                }
            }
            Err(e) => {
                println!("  Error polling: {}", e);
                break;
            }
        }
    }

    // Step 5: Set 0xD208 to 0x0304 (shoot)
    println!("\nStep 5: Setting capture control to SHOOT (0xD208 = 0x0304)...");
    match session
        .set_device_prop_value_typed(prop_0xd208, &PropertyValue::Uint16(0x0304))
        .await
    {
        Ok(()) => println!("  OK"),
        Err(e) => println!("  Error: {}", e),
    }

    // Step 6: InitiateCapture (triggers shutter)
    println!("\nStep 6: InitiateCapture (shoot phase)...");
    match session
        .initiate_capture(StorageId(0), ObjectFormatCode::Undefined)
        .await
    {
        Ok(()) => {
            println!("  SUCCESS! Shutter should fire now.");
            // Wait for capture to complete
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
        Err(e) => println!("  Error: {:?}", e),
    }

    // Reset priority mode
    println!("\nResetting priority mode (0xD207 = 1)...");
    let _ = session
        .set_device_prop_value_typed(prop_0xd207, &PropertyValue::Uint16(1))
        .await;

    session.close().await?;
    println!("\n=== Done ===");
    Ok(())
}
