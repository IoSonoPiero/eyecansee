//! How long pictures take to read on a phone and a server, the same before and after a change of
//! the reading (#177): what the viewer reads (`view_memory::read`) and what the metadata panel
//! reads (`panel`), time and bytes for each picture, median of three. Read only on the phone; on
//! a server the files are sent to a new folder, deleted at the end.
//! Phone (EyeCanSee closed): `EYECANSEE_MTP=<serial> EYECANSEE_DIR=Pictures/EyeCanSee-prova/`
//! (in its first storage). Server: `EYECANSEE_SFTP` or `EYECANSEE_FTP` as `kept::tests::edit_server_live`,
//! with `EYECANSEE_FILES=<local folder>` sent first.
//! `cargo test --release read_live::reading_times_live -- --exact --ignored --nocapture`

use std::time::{Duration, Instant};

use crate::source::{self, Store};

/// What the metadata panel reads of a picture: its sidecar and the file whole.
async fn panel(op: &Store, path: &str) -> u64 {
    let _ = crate::sidecar::read(op, path).await;
    op.read(path).await.map_or(0, |b| crate::meta::read_file(&b, path).is_some() as u64)
}

fn is_picture(path: &str) -> bool {
    !crate::decode::is_played(path) && !crate::model::is_model(path) && !crate::cad::is_cad(path) && crate::decode::is_image(path)
}

/// The median time of three runs of `f`, the bytes and the reads of the last.
async fn timed<F: std::future::Future>(mut f: impl FnMut() -> F) -> (Duration, u64, u64) {
    let mut times = Vec::new();
    let (mut bytes, mut reads) = (0, 0);
    for _ in 0..3 {
        source::BYTES_READ.with(|n| n.set(0));
        source::READS.with(|n| n.set(0));
        let t = Instant::now();
        f().await;
        times.push(t.elapsed());
        bytes = source::BYTES_READ.with(std::cell::Cell::get);
        reads = source::READS.with(std::cell::Cell::get);
    }
    times.sort();
    (times[1], bytes, reads)
}

/// Every picture under `dir`, with its size.
async fn pictures(op: &Store, dir: &str) -> Vec<(String, u64)> {
    let (mut dirs, mut found) = (vec![dir.to_string()], Vec::new());
    while let Some(d) = dirs.pop() {
        let l = op.list(&d).await.unwrap();
        dirs.extend(l.dirs);
        found.extend(l.files.into_iter().filter(|f| is_picture(&f.path)).map(|f| (f.path, f.size)));
    }
    found.sort();
    found
}

#[test]
#[ignore]
fn reading_times_live() {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        let (op, dir, sent) = match std::env::var("EYECANSEE_MTP") {
            Ok(serial) => {
                let op = Store::Mtp(std::sync::Arc::new(crate::mtp::Phone::new(&serial)));
                let first = op.list("").await.unwrap().dirs[0].clone();
                (op, format!("{first}{}", std::env::var("EYECANSEE_DIR").unwrap()), false)
            }
            Err(_) => {
                let (op, dir, _) = crate::kept::tests::server_dir(secs).await;
                op.create_dir(&dir).await.unwrap();
                let local = std::env::var("EYECANSEE_FILES").expect("EYECANSEE_FILES");
                for e in std::fs::read_dir(local).unwrap().flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if is_picture(&name) {
                        crate::replace::write_whole(&op, &format!("{dir}{name}"), std::fs::read(e.path()).unwrap()).await.unwrap();
                    }
                }
                (op, dir, true)
            }
        };
        println!("file\tMB\tviewer ms\tviewer MB\tviewer reads\tpanel ms\tpanel MB\tpanel reads");
        for (path, size) in pictures(&op, &dir).await {
            let v = timed(|| async { let _ = crate::view_memory::read(&op, &path, Some(size), crate::view_memory::Use::View, false).await; }).await;
            let p = timed(|| async { panel(&op, &path).await; }).await;
            let mb = |b: u64| format!("{:.2}", b as f64 / 1e6);
            println!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}", path.rsplit('/').next().unwrap(), mb(size), v.0.as_millis(), mb(v.1), v.2, p.0.as_millis(), mb(p.1), p.2);
        }
        if sent {
            op.delete(&dir, source::Deletion::Permanent).await.unwrap();
        }
    });
}
