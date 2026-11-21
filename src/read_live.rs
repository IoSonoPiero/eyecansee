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

/// What the metadata panel reads of a picture, as `Loader::load_meta`: its sidecar and the
/// parts of the file with the metadata (`meta_read`).
async fn panel(op: &Store, path: &str, size: u64) {
    let sidecar = crate::sidecar::read(op, path).await;
    let _ = crate::meta_read::read(op, path, size).await.map(|b| crate::meta::read_file_with(&b, path, sidecar.as_ref()));
}

fn is_picture(path: &str) -> bool {
    !crate::decode::is_played(path) && !crate::model::is_model(path) && !crate::cad::is_cad(path) && crate::decode::is_image(path)
}

/// The median time of three runs of `f`, the bytes and the reads of the last; `None` when it
/// panicked (on the runtime's only thread, as the counters).
async fn timed<F: std::future::Future<Output = ()> + Send + 'static>(mut f: impl FnMut() -> F) -> Option<(Duration, u64, u64)> {
    let mut times = Vec::new();
    let (mut bytes, mut reads) = (0, 0);
    for _ in 0..3 {
        source::BYTES_READ.with(|n| n.set(0));
        source::READS.with(|n| n.set(0));
        let t = Instant::now();
        tokio::spawn(f()).await.ok()?;
        times.push(t.elapsed());
        bytes = source::BYTES_READ.with(std::cell::Cell::get);
        reads = source::READS.with(std::cell::Cell::get);
    }
    times.sort();
    Some((times[1], bytes, reads))
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
            let (o, p) = (op.clone(), path.clone());
            let v = timed(move || {
                let (op, path) = (o.clone(), p.clone());
                async move {
                    let _ = crate::view_memory::read(&op, &path, Some(size), crate::view_memory::Use::View, false).await;
                }
            })
            .await;
            let (o, p) = (op.clone(), path.clone());
            let m = timed(move || {
                let (op, path) = (o.clone(), p.clone());
                async move {
                    panel(&op, &path, size).await;
                }
            })
            .await;
            let row = |r: Option<(Duration, u64, u64)>| r.map_or("panic\t\t".into(), |(t, b, n)| format!("{}\t{:.2}\t{n}", t.as_millis(), b as f64 / 1e6));
            println!("{}\t{:.2}\t{}\t{}", path.rsplit('/').next().unwrap(), size as f64 / 1e6, row(v), row(m));
        }
        if sent {
            op.delete(&dir, source::Deletion::Permanent).await.unwrap();
        }
    });
}
