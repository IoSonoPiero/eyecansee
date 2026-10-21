//! Manual hardware probe for the Windows WPD **streaming upload**: (1) determines what a cancelled
//! upload leaves on the device (partial object or nothing), and (2) proves the streaming path holds
//! only a few chunks in memory, not the whole file.
//!
//! Everything happens inside a fresh `mtp-rs-mem/` folder under `Download`, deleted on the way out.
//! The large payload is generated lazily (one chunk at a time), so the *source* never holds the file
//! in memory either — peak process working set reflects the upload path, not a giant input buffer.
//!
//! Windows-only; phone in MTP mode, unlocked.
//! Run: `cargo run -p mtp-rs --example wpd_upload_mem [--release] -- [SIZE_MB]`
//! Pair with the PowerShell poller in the task notes to capture PeakWorkingSet64.

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mtp_rs::{Backend, MtpDevice};

    let size_mb: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    println!("PID={}", std::process::id());

    let device = MtpDevice::builder()
        .backend(Backend::Wpd)
        .open_first()
        .await?;
    println!(
        "Opened {} {}",
        device.device_info().manufacturer,
        device.device_info().model
    );

    let storage = device
        .storages()
        .await?
        .into_iter()
        .next()
        .ok_or("no storage")?;
    let root = storage.list_objects(None).await?;
    let download = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Download"))
        .ok_or("no Download folder")?
        .handle;
    let dir = storage.create_folder(Some(download), "mtp-rs-mem").await?;

    let result = run(&storage, dir, size_mb).await;

    print!("\nCleanup: deleting Download/mtp-rs-mem ... ");
    match storage.delete(dir).await {
        Ok(()) => println!("ok"),
        Err(e) => println!("FAILED ({e}) — remove Download/mtp-rs-mem manually"),
    }
    result?;
    device.close().await?;
    Ok(())
}

#[cfg(windows)]
async fn run(
    storage: &mtp_rs::Storage,
    dir: mtp_rs::ObjectHandle,
    size_mb: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    use bytes::Bytes;
    use mtp_rs::NewObjectInfo;
    use std::ops::ControlFlow;

    // (1) Partial-handle determination — mirror the conformance cancel exactly: a 5-byte upload,
    // Break on the FIRST progress call (before any data reaches the device).
    println!("\n== Partial-handle determination (cancel before any data) ==");
    let info = NewObjectInfo::file("cancelled.bin", 5);
    let src = futures::stream::once(futures::future::ok(Bytes::from_static(b"hello")));
    let err = storage
        .upload_with_progress(Some(dir), info, Box::pin(src), |_p| ControlFlow::Break(()))
        .await
        .expect_err("a cancelled upload must fail");
    println!("  upload_with_progress error: {:?}", err.source);
    println!(
        "  UploadError.partial reported by library: {:?}",
        err.partial
    );

    // Ground truth: independently list the folder for the filename.
    let listed = storage.list_objects(Some(dir)).await?;
    let present = listed.iter().find(|o| o.filename == "cancelled.bin");
    match present {
        Some(o) => println!(
            "  PARTIAL_FINDING: PRESENT (listing shows cancelled.bin, size={})",
            o.size
        ),
        None => println!("  PARTIAL_FINDING: ABSENT (cancelled.bin not in listing)"),
    }
    println!(
        "  library/ground-truth agree: {}",
        err.partial.is_some() == present.is_some()
    );
    if let Some(h) = err.partial {
        let _ = storage.delete(h).await; // tidy up any partial the device kept
    }

    // (1b) Partial-handle determination — MID-STREAM abort: a multi-chunk upload, Break after a few
    // chunks have actually been written to the device (the case the buffered path never exercised).
    println!("\n== Partial-handle determination (cancel MID-STREAM, after some bytes written) ==");
    let declared = 64 * 256 * 1024u64; // 16 MiB across 64 chunks
    let info = NewObjectInfo::file("midstream.bin", declared);
    let mut seen = 0u32;
    let src = futures::stream::iter(
        (0..64).map(|_| Ok::<Bytes, std::io::Error>(Bytes::from(vec![1u8; 256 * 1024]))),
    );
    let err = storage
        .upload_with_progress(Some(dir), info, Box::pin(src), move |_p| {
            seen += 1;
            if seen > 8 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await
        .expect_err("a mid-stream cancelled upload must fail");
    println!("  upload_with_progress error: {:?}", err.source);
    println!(
        "  UploadError.partial reported by library: {:?}",
        err.partial
    );
    let listed = storage.list_objects(Some(dir)).await?;
    match listed.iter().find(|o| o.filename == "midstream.bin") {
        Some(o) => println!(
            "  PARTIAL_FINDING (mid-stream): PRESENT (size={}, declared={declared})",
            o.size
        ),
        None => println!("  PARTIAL_FINDING (mid-stream): ABSENT"),
    }
    if let Some(h) = err.partial {
        let _ = storage.delete(h).await;
    }

    // (2) Memory proof — stream a large file, generated one chunk at a time.
    const CHUNK: usize = 256 * 1024;
    let total = (size_mb * 1024 * 1024) as usize;
    let num_chunks = total.div_ceil(CHUNK);
    let exact = num_chunks * CHUNK;
    println!(
        "\n== Memory proof: streaming {} MiB ({} chunks of {} KiB) ==",
        size_mb,
        num_chunks,
        CHUNK / 1024
    );
    // Lazy source: each chunk is allocated only when polled, then forwarded and dropped. The old
    // buffering path held ~size_mb MiB; this holds ~DATA_BOUND chunks (~1 MiB).
    let src = futures::stream::iter(
        (0..num_chunks).map(move |_| Ok::<Bytes, std::io::Error>(Bytes::from(vec![0u8; CHUNK]))),
    );
    let info = NewObjectInfo::file("big.bin", exact as u64);
    let t0 = std::time::Instant::now();
    let handle = storage.upload(Some(dir), info, Box::pin(src)).await?;
    let secs = t0.elapsed().as_secs_f64();
    println!(
        "  uploaded big.bin in {:.1}s ({:.1} MiB/s)",
        secs,
        exact as f64 / 1024.0 / 1024.0 / secs
    );

    let got = storage.get_object_info(handle).await?;
    println!("  object_info.size = {} (expected {exact})", got.size);
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("wpd_upload_mem is Windows-only.");
}
