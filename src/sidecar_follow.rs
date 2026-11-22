//! The sidecar of a RAW photo (`sidecar`, #24) follows the photo when it is deleted, put back from
//! EyeCanSee's trash folder or renamed, as in Lightroom, darktable and digiKam: it is not shown
//! in the grid, and left behind it would be lost to the photo or describe another photo that
//! takes the name later. Copy and move take it along in `transfer`. A sidecar keeps its pattern
//! (`a.CR2.xmp` or `a.xmp`) under the photo's new name (`sidecar::renamed`), and a name is free
//! only when it is free for the photo and its sidecars together (`sidecar::free_pair`).

use std::collections::HashSet;

use crate::sidecar::{among, free_pair, renamed};
use crate::source::{Conflict, Deletion, Error, Store, TRASH_DIR, name, parent};

/// The sidecars that follow the photo `path`, as paths, among `names` (those of its folder): a
/// `foo.xmp` only if it is its own (`sidecar::is_its`, read from the file).
pub async fn following(op: &Store, path: &str, names: &HashSet<String>) -> Vec<String> {
    let dir = parent(path);
    let mut out = Vec::new();
    for side in among(name(path), names) {
        let full = format!("{dir}{side}");
        let its = match crate::sidecar::surely(path, &full) {
            Some(sure) => sure,
            None => match op.read(&full).await {
                Ok(x) => crate::sidecar::is_its(op, path, &full, &x, Some(names)).await,
                Err(_) => false,
            },
        };
        if its {
            out.push(full);
        }
    }
    out
}

/// The sidecars of `path` there are in its folder, with the folder's names: from `cache` if read
/// already (a deletion of many files reads each folder once), otherwise read now.
async fn of(op: &Store, path: &str, cache: &mut Folders) -> (Vec<String>, HashSet<String>) {
    if crate::sidecar::names(path).is_empty() {
        return (Vec::new(), HashSet::new());
    }
    let dir = parent(path);
    if !cache.contains_key(&dir) {
        cache.insert(dir.clone(), op.names(&dir).await.unwrap_or_default());
    }
    let names = cache[&dir].clone();
    (following(op, path, &names).await, names)
}

/// The names of the folders read, by folder.
pub type Folders = std::collections::HashMap<String, HashSet<String>>;

fn names_of(paths: &[String]) -> Vec<String> {
    paths.iter().map(|p| name(p).to_string()).collect()
}

/// Deletes `path` as `how` says, and its sidecars after it. Into a trash folder they go under
/// the same name pattern as the photo, "a (2).CR2" with "a (2).CR2.xmp" when "a.CR2" is there
/// already, so that Restore finds them together. `cache`: the folders read for the deletions
/// before, which it keeps up to date.
pub async fn delete(op: &Store, path: &str, how: Deletion, cache: &mut Folders) -> Result<(), Error> {
    let (sides, names) = of(op, path, cache).await;
    let r = if sides.is_empty() { op.delete(path, how).await } else { delete_with(op, path, how, &sides, names).await };
    // What went is not there for the next deletion, whatever stopped it.
    if let Some(left) = cache.get_mut(&parent(path)) {
        for p in std::iter::once(path).chain(sides.iter().map(String::as_str)) {
            if r.is_ok() || !op.exists(p).await {
                left.remove(name(p));
            }
        }
    }
    r
}

async fn delete_with(op: &Store, path: &str, how: Deletion, sides: &[String], names: HashSet<String>) -> Result<(), Error> {
    let own = name(path);
    let side_names = names_of(sides);
    if how != Deletion::TrashFolder {
        op.delete(path, how).await?;
        for s in sides {
            op.delete(s, how).await?;
        }
        return Ok(());
    }
    let trash = format!("{}{TRASH_DIR}/", parent(path));
    let mut taken = op.names(&trash).await.unwrap_or_default();
    // On a phone an item is renamed in its folder before it moves (`to_trash_folder`).
    if let Store::Mtp(_) = op {
        taken.extend(names.into_iter().filter(|n| n != own && !side_names.contains(n)));
    }
    let chosen = free_pair(|n| taken.contains(n), own, &side_names);
    op.to_trash_folder(path, &chosen).await?;
    for (s, n) in sides.iter().zip(&side_names) {
        op.to_trash_folder(s, &renamed(own, n, &chosen)).await?;
    }
    Ok(())
}

/// Puts `path` back from a trash folder (`Store::restore`), its sidecars in the trash with it,
/// under the name pattern the photo gets there. A conflict counts for the photo and its sidecars
/// together: Replace replaces them all, Keep both gives them all the same new name.
pub async fn restore(op: &Store, path: &str, conflict: Conflict) -> Result<String, Error> {
    if crate::source::android_trashed(name(path)).is_some() {
        return op.restore(path, conflict).await;
    }
    let (sides, in_trash) = of(op, path, &mut Folders::new()).await;
    if sides.is_empty() {
        return op.restore(path, conflict).await;
    }
    let (dest, own) = crate::source::restore_place(path);
    let side_names = names_of(&sides);
    let taken = op.names(&dest).await?;
    let target = match free_pair(|n| taken.contains(n), own, &side_names) {
        free if free == own => own.to_string(),
        _ if conflict == Conflict::Ask => return Err(("error-exists", own.to_string())),
        _ if conflict == Conflict::Replace => {
            for n in std::iter::once(own.to_string()).chain(side_names.iter().map(|s| renamed(own, s, own))) {
                if taken.contains(&n) {
                    op.delete(&format!("{dest}{n}"), Deletion::Permanent).await?;
                }
            }
            own.to_string()
        }
        _ => {
            let mut all = taken;
            all.extend(in_trash.into_iter().filter(|n| n != own && !side_names.contains(n)));
            free_pair(|n| all.contains(n), own, &side_names)
        }
    };
    let to = op.move_into(path, &dest, &target).await?;
    for (s, n) in sides.iter().zip(&side_names) {
        op.move_into(s, &dest, &renamed(own, n, &target)).await?;
    }
    Ok(to)
}

/// Renames `path` to `new_name`, its sidecars with it. A sidecar's new name taken by another file
/// stops it before anything is renamed; a sidecar that cannot be renamed puts back what was.
pub async fn rename(op: &Store, path: &str, new_name: &str) -> Result<String, Error> {
    let (sides, names) = of(op, path, &mut Folders::new()).await;
    if sides.is_empty() {
        return op.rename(path, new_name).await;
    }
    let (dir, own) = (parent(path), name(path));
    let side_names = names_of(&sides);
    let targets: Vec<String> = side_names.iter().map(|s| renamed(own, s, new_name)).collect();
    if let Some(t) = targets.iter().find(|t| names.contains(*t) && !side_names.contains(*t)) {
        return Err(("error-exists", format!("{dir}{t}")));
    }
    let to = op.rename(path, new_name).await?;
    let mut done: Vec<(String, &str)> = Vec::new();
    for (s, t) in sides.iter().zip(&targets) {
        if name(s) == t {
            continue;
        }
        if let Err(e) = op.rename(s, t).await {
            for (t, s) in done.iter().rev() {
                let _ = op.rename(t, s).await;
            }
            let back = op.rename(&to, own).await;
            log!("rename {path:?}: sidecar {s:?} not renamed ({e:?}), put back: {back:?}");
            return Err(e);
        }
        done.push((format!("{dir}{t}"), name(s)));
    }
    Ok(to)
}

/// `paths` to restore all at once, without the sidecars of a photo among them: they go back with
/// it (`restore`).
pub fn without_sidecars(paths: Vec<String>) -> Vec<String> {
    let mut by_dir: std::collections::HashMap<String, HashSet<String>> = std::collections::HashMap::new();
    for p in &paths {
        by_dir.entry(parent(p)).or_default().insert(name(p).to_string());
    }
    let sides: HashSet<String> = paths.iter().flat_map(|p| among(name(p), &by_dir[&parent(p)]).into_iter().map(move |s| format!("{}{s}", parent(p)))).collect();
    paths.into_iter().filter(|p| !sides.contains(p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Folder {
        root: std::path::PathBuf,
        op: Store,
        rt: tokio::runtime::Runtime,
    }

    impl Folder {
        fn new(tag: &str) -> Folder {
            let root = std::env::temp_dir().join(format!("eyecansee-follow-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("d")).unwrap();
            let op = Store::Dal(opendal::Operator::new(opendal::services::Fs::default().root(&root.to_string_lossy())).unwrap());
            Folder { root, op, rt: tokio::runtime::Runtime::new().unwrap() }
        }

        fn put(&self, rel: &str, text: &str) {
            let p = self.root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }

        /// The files below `rel`, sorted, with their content.
        fn files(&self, rel: &str) -> Vec<(String, String)> {
            let mut out: Vec<(String, String)> = std::fs::read_dir(self.root.join(rel)).map_or_else(|_| Vec::new(), |d| d.flatten().filter(|e| e.path().is_file()).map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).unwrap())).collect());
            out.sort();
            out
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    const FOR_JPG: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"><rdf:Description xmlns:photoshop="http://ns.adobe.com/photoshop/1.0/" photoshop:SidecarForExtension="JPG"/></rdf:RDF></x:xmpmeta>"#;

    fn s(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    /// Into the trash folder and back: the sidecars follow under the photo's name pattern, also when
    /// the trash has a photo of that name already; a `foo.xmp` of the JPEG beside stays.
    #[test]
    fn deleted_and_restored() {
        let f = Folder::new("trash");
        f.put("d/a.CR2", "raw");
        f.put("d/a.CR2.xmp", "dt");
        f.put("d/a.JPG", "jpg");
        f.put("d/a.xmp", FOR_JPG);
        f.put(&format!("d/{TRASH_DIR}/a.CR2"), "old raw");
        f.rt.block_on(delete(&f.op, "d/a.CR2", Deletion::TrashFolder, &mut Folders::new())).unwrap();
        assert_eq!(f.files("d"), s(&[("a.JPG", "jpg"), ("a.xmp", FOR_JPG)]));
        assert_eq!(f.files(&format!("d/{TRASH_DIR}")), s(&[("a (2).CR2", "raw"), ("a (2).CR2.xmp", "dt"), ("a.CR2", "old raw")]));
        // A photo of its name in the trash came meanwhile: Ask says so, Keep both moves the pair to
        // a new name.
        f.put("d/a (2).CR2", "new raw");
        let trashed = format!("d/{TRASH_DIR}/a (2).CR2");
        assert_eq!(f.rt.block_on(restore(&f.op, &trashed, Conflict::Ask)).unwrap_err().0, "error-exists");
        assert_eq!(f.rt.block_on(restore(&f.op, &trashed, Conflict::KeepBoth)).unwrap(), "d/a (2) (2).CR2");
        assert_eq!(f.files("d"), s(&[("a (2) (2).CR2", "raw"), ("a (2) (2).CR2.xmp", "dt"), ("a (2).CR2", "new raw"), ("a.JPG", "jpg"), ("a.xmp", FOR_JPG)]));
        // Only its sidecar in the way: Replace replaces it, with the photo.
        f.rt.block_on(delete(&f.op, "d/a (2) (2).CR2", Deletion::TrashFolder, &mut Folders::new())).unwrap();
        f.put("d/a (2) (2).CR2.xmp", "stray");
        let trashed = format!("d/{TRASH_DIR}/a (2) (2).CR2");
        assert_eq!(f.rt.block_on(restore(&f.op, &trashed, Conflict::Ask)).unwrap_err().0, "error-exists");
        f.rt.block_on(restore(&f.op, &trashed, Conflict::Replace)).unwrap();
        assert_eq!(f.files("d")[..2], s(&[("a (2) (2).CR2", "raw"), ("a (2) (2).CR2.xmp", "dt")])[..]);
        assert_eq!(f.files(&format!("d/{TRASH_DIR}")), s(&[("a.CR2", "old raw")]));
    }

    /// Deleted for good: the sidecars too; a photo without one as before.
    #[test]
    fn deleted_for_good() {
        let f = Folder::new("gone");
        f.put("d/a.NEF", "raw");
        f.put("d/a.NEF.xmp", "dt");
        f.put("d/a.xmp", "lr");
        f.put("d/b.NEF", "raw");
        f.put("d/c.jpg", "jpg");
        f.put("d/c.jpg.xmp", "dt of a jpg");
        f.put("d/e.png", "png");
        f.put("d/e.xmp", "digikam");
        // One reading of the folder for them all, kept up to date.
        let mut cache = Folders::new();
        for p in ["d/a.NEF", "d/b.NEF", "d/c.jpg", "d/e.png"] {
            f.rt.block_on(delete(&f.op, p, Deletion::Permanent, &mut cache)).unwrap();
        }
        assert!(f.files("d").is_empty(), "{:?}", f.files("d"));
        assert!(cache["d/"].is_empty(), "{cache:?}");
    }

    /// Renamed with the photo, each sidecar in its pattern; a name taken by another file stops it
    /// before anything changes.
    #[test]
    fn renamed_together() {
        let f = Folder::new("rename");
        f.put("d/a.ARW", "raw");
        f.put("d/a.ARW.xmp", "dt");
        f.put("d/a.xmp", "lr");
        assert_eq!(f.rt.block_on(rename(&f.op, "d/a.ARW", "sea.ARW")).unwrap(), "d/sea.ARW");
        assert_eq!(f.files("d"), s(&[("sea.ARW", "raw"), ("sea.ARW.xmp", "dt"), ("sea.xmp", "lr")]));
        f.put("d/sky.xmp", "other");
        assert_eq!(f.rt.block_on(rename(&f.op, "d/sea.ARW", "sky.ARW")).unwrap_err().0, "error-exists");
        assert_eq!(f.files("d"), s(&[("sea.ARW", "raw"), ("sea.ARW.xmp", "dt"), ("sea.xmp", "lr"), ("sky.xmp", "other")]));
    }

    /// On a server: copied there and moved back with their photos, renamed, into the trash folder
    /// and back. `EYECANSEE_SFTP=sftp://user:password@host:port/folder/ cargo test sidecars_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sidecars_live() {
        use crate::transfer::{Clip, Progress, run};
        let url = std::env::var("EYECANSEE_SFTP").expect("EYECANSEE_SFTP not set");
        let f = Folder::new("live");
        f.put("d/a.CR2", "raw");
        f.put("d/a.CR2.xmp", "dt");
        f.put("d/a.xmp", "lr");
        f.put("d/b.jpg", "jpg");
        f.put("d/b.jpg.xmp", "dk");
        std::fs::create_dir_all(f.root.join("e")).unwrap();
        f.rt.block_on(async {
            let server = crate::source::open(&url).unwrap();
            let op = &server.op;
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let dir = format!("{}eyecansee-xmp-{secs}/", server.dir);
            op.create_dir(&dir).await.unwrap();
            let names = |d: String| async move {
                let mut v: Vec<String> = op.names(&d).await.unwrap_or_default().into_iter().collect();
                v.sort();
                v
            };
            let clip = Clip { op: f.op.clone(), base: "local-test://".into(), paths: vec!["d/a.CR2".into(), "d/b.jpg".into()], cut: false, text: String::new() };
            run(&clip, op, &server.base, &dir, &Default::default(), &Progress::default()).await.1.unwrap();
            assert_eq!(names(dir.clone()).await, ["a.CR2", "a.CR2.xmp", "a.xmp", "b.jpg", "b.jpg.xmp"]);
            rename(op, &format!("{dir}a.CR2"), "sea.CR2").await.unwrap();
            assert_eq!(names(dir.clone()).await, ["b.jpg", "b.jpg.xmp", "sea.CR2", "sea.CR2.xmp", "sea.xmp"]);
            delete(op, &format!("{dir}sea.CR2"), Deletion::TrashFolder, &mut Folders::new()).await.unwrap();
            let trash = format!("{dir}{TRASH_DIR}/");
            assert_eq!(names(trash.clone()).await, ["sea.CR2", "sea.CR2.xmp", "sea.xmp"]);
            restore(op, &format!("{trash}sea.CR2"), Conflict::Ask).await.unwrap();
            assert_eq!(names(dir.clone()).await, [TRASH_DIR, "b.jpg", "b.jpg.xmp", "sea.CR2", "sea.CR2.xmp", "sea.xmp"]);
            let back = Clip { op: op.clone(), base: server.base.clone(), paths: vec![format!("{dir}b.jpg")], cut: true, text: String::new() };
            let (gone, r) = run(&back, &f.op, "local-test://", "e/", &Default::default(), &Progress::default()).await;
            r.unwrap();
            assert_eq!(gone, [format!("{dir}b.jpg")]);
            assert_eq!(f.files("e"), s(&[("b.jpg", "jpg"), ("b.jpg.xmp", "dk")]));
            assert_eq!(names(dir.clone()).await, [TRASH_DIR, "sea.CR2", "sea.CR2.xmp", "sea.xmp"]);
            op.delete(&dir, Deletion::Permanent).await.unwrap();
            println!("server: all as expected");
        });
    }

    #[test]
    fn restore_all_without_sidecars() {
        let t = format!("d/{TRASH_DIR}/");
        let all = ["a.CR2", "a.CR2.xmp", "a.xmp", "b.xmp", "c.jpg"].map(|n| format!("{t}{n}")).to_vec();
        assert_eq!(without_sidecars(all), ["a.CR2", "b.xmp", "c.jpg"].map(|n| format!("{t}{n}")));
    }

    /// Audit 2.23 (b): with its sidecars too, a trash folder that cannot be listed (Linux, no
    /// permission to read it, but to write into it) stops the deletion: the pair of the same name
    /// in the trash is not replaced.
    #[cfg(unix)]
    #[test]
    fn audit_2_23_trash_not_listed_is_not_empty() {
        use std::os::unix::fs::PermissionsExt;
        let f = Folder::new("unread");
        f.put("d/a.CR2", "raw");
        f.put("d/a.CR2.xmp", "dt");
        f.put(&format!("d/{TRASH_DIR}/a.CR2"), "old raw");
        f.put(&format!("d/{TRASH_DIR}/a.CR2.xmp"), "old dt");
        let trash = f.root.join("d").join(TRASH_DIR);
        std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o300)).unwrap();
        let readable = std::fs::read_dir(&trash).is_ok();
        let r = (!readable).then(|| f.rt.block_on(delete(&f.op, "d/a.CR2", Deletion::TrashFolder, &mut Folders::new())));
        std::fs::set_permissions(&trash, std::fs::Permissions::from_mode(0o700)).unwrap();
        if let Some(r) = r {
            assert!(r.is_err(), "the trash taken for empty: {r:?}");
            assert_eq!(f.files(&format!("d/{TRASH_DIR}")), s(&[("a.CR2", "old raw"), ("a.CR2.xmp", "old dt")]));
        }
    }
}
