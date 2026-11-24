//! Audit of 2026-11-17, phones and servers (wave 7): tests of the whole app without a screen, on
//! a server of their own on 127.0.0.1 (`fake_ftps`). Each runs in a folder of its own
//! (`audit_family_1::own_folder`): settings, cache and `known_certs` are never the user's.

mod fake_ftps;

use crate::App;
use crate::audit_family_1::{Harness, own_folder};

/// The address of the folder `album/` of `server`, with the user and `password` in it.
fn album(server: &fake_ftps::Server, password: &str) -> String {
    format!("ftps://tester:{password}@127.0.0.1:{}/album/", server.port)
}

/// Started on `address`, until the question about the server's certificate is open (drawn once).
fn asked(address: &str) -> Harness {
    let mut h = Harness::start(address);
    h.until(|a| a.host_ask, "the question about the certificate");
    h.frame(Vec::new());
    h
}

/// The question answered as a key would: Trust (`true`) or Cancel.
fn answer(h: &mut Harness, trust: bool) {
    h.app.key_answer = Some(trust);
    h.frame(Vec::new());
    assert!(!h.app.host_ask, "the question still open after the answer");
}

fn known_certs() -> String {
    std::fs::read_to_string(crate::settings::config_path("known_certs").unwrap()).unwrap_or_default()
}

/// Audit 2.14: an FTPS server never seen, opened with the password in the address (as a link or
/// File > Open location gives it, the server not saved). After Trust the address opened again
/// from the bar, which never holds the password: the login went without it, was refused, and the
/// password had to be typed again. The folder must open with the password typed the first time.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_2_14_password_kept_after_trust() {
    if own_folder("audit_servers::audit_2_14_password_kept_after_trust") {
        return;
    }
    let server = fake_ftps::start("secret");
    let mut h = asked(&album(&server, "secret"));
    answer(&mut h, true);
    h.until(|a: &App| !a.files().is_empty() || a.open_address.is_some(), "the folder, or the form asking for the password");
    eprintln!("logins: {:?}", server.logins());
    assert!(h.app.open_address.is_none(), "the password was asked again");
    assert!(h.app.error.is_none(), "an error left in the bar: {:?}", h.app.error);
    assert_eq!(h.app.files().len(), 1, "album/ listed");
    assert!(server.logins().iter().all(|l| l == "PASS ok"), "a login without the password typed: {:?}", server.logins());
}

/// Audit 2.10: the certificate of an FTPS server refused (Cancel) stayed waiting in memory, and
/// the next question about a server (an SFTP server never seen) asked about that certificate
/// again, Trust adding it to the known ones. And Trust saved the certificate waiting at the
/// moment of the click, not the one shown: another connection failing meanwhile (a second server
/// with a certificate of its own) put its own there.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn audit_2_10_refused_certificate_forgotten() {
    if own_folder("audit_servers::audit_2_10_refused_certificate_forgotten") {
        return;
    }
    let (first, second) = (fake_ftps::start("secret"), fake_ftps::start("secret"));
    let mut h = asked(&album(&first, "secret"));
    answer(&mut h, false);
    assert!(crate::ftp::unknown().is_none(), "the certificate refused still waits: it would be asked about with the next server");
    assert!(known_certs().is_empty(), "a certificate refused was trusted: {}", known_certs());
    // Asked again, and meanwhile a connection to the second server fails on its certificate.
    h.app.refresh();
    h.until(|a| a.host_ask, "the question about the first server again");
    h.frame(Vec::new());
    let target = crate::ftp::Target { host: "127.0.0.1".into(), port: second.port, user: "tester".into(), password: "secret".into(), secure: true };
    let other = h.app.rt.block_on(crate::ftp::Server::shared(target).home());
    assert!(matches!(other, Err(("error-cert-unknown", _))), "the second server: {other:?}");
    // An SFTP server never seen, refused meanwhile, would be asked about its key, not this.
    assert!(crate::host_question::Question::of("error-host-unknown").is_none(), "an SFTP question would show a certificate");
    answer(&mut h, true);
    let (shown, other) = (format!("127.0.0.1:{} ", first.port), format!("127.0.0.1:{} ", second.port));
    assert!(known_certs().contains(&shown) && !known_certs().contains(&other), "trusted: {:?}, the one asked about: {shown:?}", known_certs());
}

/// The folder shown is listed, on a place whose address starts with `base`.
fn listed_on(a: &App, base: &str) -> bool {
    a.base.starts_with(base) && a.listings.get(&a.dir).is_some_and(Result::is_ok)
}

/// #206: away from a phone or a server and back by its row in the left panel, it opened again at
/// its first folder, not at the folder that was open there. A server stands in for the phone
/// (`open_device`, the same way): the phone itself is tried by hand. A folder gone meanwhile:
/// its first folder, as before.
#[test]
#[cfg_attr(windows, ignore = "the app without a screen not tried on Windows yet")]
fn back_on_a_device_reopens_its_last_folder() {
    if own_folder("audit_servers::back_on_a_device_reopens_its_last_folder") {
        return;
    }
    use crate::audit_family_1::{fake_ftp, folder};
    let local = folder("206-local", &["a.png"]);
    let png = std::fs::read(local.join("a.png")).unwrap();
    let mut tree = fake_ftp::Tree::default();
    tree.folder("/album", &[("a.png", &png)]);
    tree.folder("/album/2024", &[("b.png", &png)]);
    let (target, tree) = fake_ftp::start(tree);
    let device = crate::devices::Device { name: "Server".into(), path: format!("ftp://tester:x@127.0.0.1:{}/", target.port), phone: true, serial: None, block: None, internal: false };
    let mut h = Harness::new(&local);
    let ctx = h.ctx.clone();
    h.app.open_device(&ctx, &device);
    h.until(|a| listed_on(a, "ftp://"), "the server's first folder");
    h.app.set_dir("album/2024/".into());
    h.until(|a| !a.files().is_empty(), "album/2024/ listed");
    let back_home = |h: &mut Harness| {
        h.app.open_source(&ctx, &format!("{}/", local.display()));
        h.until(|a| listed_on(a, "/"), "the computer's folder");
    };
    back_home(&mut h);
    h.app.open_device(&ctx, &device);
    h.until(|a| listed_on(a, "ftp://"), "the server again");
    assert_eq!(h.app.dir, "album/2024/", "back on the server: not the folder open there before");
    // The folder gone meanwhile: the first folder.
    back_home(&mut h);
    tree.lock().unwrap().folders.remove("/album/2024");
    h.app.open_device(&ctx, &device);
    h.until(|a| listed_on(a, "ftp://") && a.dir.is_empty(), "the server's first folder, album/2024/ being gone");
}
