//! Manual smoke test for the Windows WPD **write** path against a real device.
//!
//! Scoped and self-cleaning: everything happens inside a fresh `mtp-rs-test/` folder under
//! `Download`, which is deleted (recursively) at the end — on success or failure. Your real files
//! are never touched. Exercises create_folder, upload (+progress), list/verify, download round-trip,
//! rename, copy, move, and delete through the public `mtp::` API with `Backend::Wpd`.
//!
//! Windows-only; phone in MTP mode, unlocked. Run: `cargo run -p mtp-rs --example wpd_write_smoke`

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use mtp_rs::{Backend, MtpDevice};

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

    // Android rejects uploads to the storage root, so work inside Download.
    let root = storage.list_objects(None).await?;
    let download = root
        .iter()
        .find(|o| o.is_folder() && o.filename.eq_ignore_ascii_case("Download"))
        .ok_or("no Download folder")?
        .handle;

    let test_dir = storage.create_folder(Some(download), "mtp-rs-test").await?;
    println!("Created Download/mtp-rs-test ({:?})", test_dir);

    // Run the workflow, then always clean up the scoped folder.
    let result = run_writes(&storage, test_dir).await;

    print!("\nCleanup: deleting Download/mtp-rs-test ... ");
    match storage.delete(test_dir).await {
        Ok(()) => println!("ok"),
        Err(e) => println!("FAILED ({e}) — remove Download/mtp-rs-test manually"),
    }

    result?;
    device.close().await?;
    println!("\nAll write ops verified \u{2713}");
    Ok(())
}

#[cfg(windows)]
async fn run_writes(
    storage: &mtp_rs::Storage,
    dir: mtp_rs::ObjectHandle,
) -> Result<(), Box<dyn std::error::Error>> {
    use bytes::Bytes;
    use mtp_rs::NewObjectInfo;
    use std::ops::ControlFlow;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn stream(data: &[u8]) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Unpin {
        futures::stream::once(futures::future::ok(Bytes::copy_from_slice(data)))
    }
    fn check(cond: bool, msg: &str) -> Result<(), Box<dyn std::error::Error>> {
        if cond {
            Ok(())
        } else {
            Err(msg.into())
        }
    }

    // 1. Upload with progress.
    let content = b"the quick brown fox jumps over the lazy dog\n".repeat(64);
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_cb = Arc::clone(&calls);
    let info = NewObjectInfo::file("hello.txt", content.len() as u64);
    let file = storage
        .upload_with_progress(Some(dir), info, stream(&content), move |_p| {
            calls_cb.fetch_add(1, Ordering::SeqCst);
            ControlFlow::Continue(())
        })
        .await?;
    println!(
        "upload: hello.txt -> {:?} ({} bytes, {} progress calls)",
        file,
        content.len(),
        calls.load(Ordering::SeqCst)
    );

    // 2. List + verify.
    let listed = storage.list_objects(Some(dir)).await?;
    let found = listed
        .iter()
        .find(|o| o.filename == "hello.txt")
        .ok_or("uploaded file not in listing")?;
    check(
        found.is_file() && found.size == content.len() as u64,
        "listed size mismatch",
    )?;
    println!("list: found hello.txt size={} \u{2713}", found.size);

    // 3. Download round-trip.
    let got = storage.download_to_vec(file).await?;
    check(got == content, "download bytes != uploaded bytes")?;
    println!("download: {} bytes match \u{2713}", got.len());

    // 4. Rename.
    storage.rename(file, "renamed.txt").await?;
    let renamed = storage.get_object_info(file).await?;
    check(renamed.filename == "renamed.txt", "rename did not take")?;
    println!("rename: -> {} \u{2713}", renamed.filename);

    // 5. Copy into a subfolder (new handle, original stays).
    let copy_dst = storage.create_folder(Some(dir), "copy_dst").await?;
    let copy = storage.copy_object(file, copy_dst, None).await?;
    let copy_info = storage.get_object_info(copy).await?;
    check(copy != file, "copy handle equals original")?;
    check(copy_info.parent == copy_dst, "copy parent wrong")?;
    check(copy_info.filename == "renamed.txt", "copy name wrong")?;
    println!("copy: -> {:?} parent={:?} \u{2713}", copy, copy_info.parent);

    // 6. Move the original into another subfolder.
    let move_dst = storage.create_folder(Some(dir), "move_dst").await?;
    storage.move_object(file, move_dst, None).await?;
    let moved = storage.get_object_info(file).await?;
    check(moved.parent == move_dst, "move parent wrong")?;
    println!("move: original parent -> {:?} \u{2713}", moved.parent);

    // 7. Delete the copy explicitly (the rest goes with the scoped-folder cleanup).
    storage.delete(copy).await?;
    check(
        storage.get_object_info(copy).await.is_err(),
        "deleted copy still resolves",
    )?;
    println!("delete: copy removed \u{2713}");

    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("wpd_write_smoke is Windows-only.");
}
