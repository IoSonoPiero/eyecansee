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
    assert_eq!(h.app.files().len(), 1, "album/ listed");
    assert!(server.logins().iter().all(|l| l == "PASS ok"), "a login without the password typed: {:?}", server.logins());
}
