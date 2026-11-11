//! The desktop's light or dark preference from the XDG desktop portal (Linux):
//! `org.freedesktop.portal.Settings`, key `org.freedesktop.appearance` / `color-scheme`.
//! Read on a thread of its own as the program starts, before the window (about 10 ms), then
//! followed through the portal's `SettingChanged` signal.
//! No portal, or no preference: light, as the AppImage catalog asks.

use eframe::egui::Theme;
use futures_util::StreamExt;
use zbus::zvariant::{OwnedValue, Value};

const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// The theme for the portal's `color-scheme` (`None`: no portal or no such setting). The portal
/// says 1 for dark, 2 for light, 0 for no preference; other values count as no preference.
pub fn from_portal(value: Option<u32>) -> Theme {
    match value {
        Some(1) => Theme::Dark,
        _ => Theme::Light,
    }
}

/// A number inside a D-Bus value, also wrapped in variants: the old `Read` call answers with a
/// variant inside a variant, `ReadOne` and `SettingChanged` with one.
fn number(value: &Value) -> Option<u32> {
    match value {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => number(inner),
        _ => None,
    }
}

/// Starts reading and following the preference, in the background.
pub(super) fn start() {
    let spawned = std::thread::Builder::new().name("system-theme".into()).spawn(|| {
        let Ok(rt) = tokio::runtime::Builder::new_current_thread().build() else {
            store(None);
            return;
        };
        if let Err(e) = rt.block_on(follow()) {
            log!("system theme: {e}");
            store(None);
        }
    });
    if spawned.is_err() {
        store(None);
    }
}

fn store(value: Option<u32>) {
    let theme = from_portal(value);
    log!("system theme: color-scheme {value:?}, {theme:?}");
    super::store(theme);
}

/// The first value, then every change of it, until the bus goes away.
async fn follow() -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let settings = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Settings").await?;
    // Listened to before reading: a change in between is not lost.
    let changes = settings.receive_signal("SettingChanged").await;
    store(read(&settings).await);
    // Without the signal the value read stays: an error here must not turn it into light.
    let mut changes = match changes {
        Ok(changes) => changes,
        Err(e) => {
            log!("system theme: changes not followed: {e}");
            return Ok(());
        }
    };
    while let Some(signal) = changes.next().await {
        let Ok((namespace, key, value)) = signal.body().deserialize::<(String, String, OwnedValue)>() else {
            continue;
        };
        if namespace == NAMESPACE && key == KEY {
            store(number(&value));
        }
    }
    Ok(())
}

/// `ReadOne` (portal version 2 and later), else the older `Read`; `None` without the setting.
async fn read(settings: &zbus::Proxy<'_>) -> Option<u32> {
    if let Ok(value) = settings.call::<_, _, OwnedValue>("ReadOne", &(NAMESPACE, KEY)).await {
        return number(&value);
    }
    let value: OwnedValue = settings.call("Read", &(NAMESPACE, KEY)).await.ok()?;
    number(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_values() {
        assert_eq!(from_portal(Some(1)), Theme::Dark);
        assert_eq!(from_portal(Some(2)), Theme::Light);
        // No preference, a value from a newer portal, no portal at all: light.
        assert_eq!(from_portal(Some(0)), Theme::Light);
        assert_eq!(from_portal(Some(7)), Theme::Light);
        assert_eq!(from_portal(None), Theme::Light);
    }

    #[test]
    fn numbers_inside_variants() {
        assert_eq!(number(&Value::U32(1)), Some(1));
        // `Read` answers with a variant inside a variant.
        assert_eq!(number(&Value::Value(Box::new(Value::Value(Box::new(Value::U32(2)))))), Some(2));
        assert_eq!(number(&Value::from("dark")), None);
    }
}
