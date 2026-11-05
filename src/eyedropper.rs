//! A colour from anywhere on the screen, for the editor's eyedropper. On Linux a program cannot
//! read the screen by itself under Wayland: it asks the desktop (xdg-desktop-portal,
//! `org.freedesktop.portal.Screenshot.PickColor`), which shows its own picker and answers with
//! the colour; the same works under X11 with a portal. Elsewhere not yet: the eyedropper takes
//! colours from the image only.

/// The colour the user picked on the screen, 0-255; `Err` with why not (cancelled, no portal).
#[cfg(target_os = "linux")]
pub async fn pick() -> Result<[u8; 3], String> {
    use std::collections::HashMap;

    use futures_util::StreamExt;
    use zbus::zvariant::{OwnedValue, Structure, Value};

    let err = |e: zbus::Error| e.to_string();
    let conn = zbus::Connection::session().await.map_err(err)?;
    // The portal answers on a request object named after us and a token of ours: listened to
    // before asking, or the answer could come first.
    let token = format!("eyecansee{}", std::process::id());
    let sender = conn.unique_name().ok_or("no D-Bus name")?.trim_start_matches(':').replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", path, "org.freedesktop.portal.Request").await.map_err(err)?;
    let mut answers = request.receive_signal("Response").await.map_err(err)?;
    let screenshot = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Screenshot").await.map_err(err)?;
    let options: HashMap<&str, Value> = HashMap::from([("handle_token", Value::from(token.as_str()))]);
    let _: zbus::zvariant::OwnedObjectPath = screenshot.call("PickColor", &("", options)).await.map_err(err)?;
    let answer = answers.next().await.ok_or("no answer from the desktop")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = answer.body().deserialize().map_err(err)?;
    if code != 0 {
        return Err("cancelled".into());
    }
    let colour = results.get("color").ok_or("no colour in the answer")?;
    let fields = Structure::try_from(Value::try_from(colour).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let channel = |i: usize| fields.fields().get(i).and_then(|v| f64::try_from(v).ok()).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8).ok_or("not a colour");
    Ok([channel(0)?, channel(1)?, channel(2)?])
}

#[cfg(not(target_os = "linux"))]
pub async fn pick() -> Result<[u8; 3], String> {
    Err("not on this system".into())
}

/// Can the eyedropper take colours outside the image here?
pub fn on_screen() -> bool {
    cfg!(target_os = "linux")
}
