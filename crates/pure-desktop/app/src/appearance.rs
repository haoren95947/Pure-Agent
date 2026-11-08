//! The desktop's light or dark preference on Linux, read from the XDG
//! settings portal, since winit reports no system theme there. Like the
//! reference, only an explicit dark preference is dark, and a desktop
//! with no portal reads as light.

use std::sync::{Arc, OnceLock, mpsc};
use std::time::Duration;
use zbus::zvariant::{OwnedValue, Value};

const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";
/// How long the window waits for the first answer before it draws.
const FIRST_WAIT: Duration = Duration::from_millis(250);

/// The portal watch, started before the window opens.
pub(crate) struct Watch {
    first: mpsc::Receiver<egui::Theme>,
    ctx: Arc<OnceLock<egui::Context>>,
}

/// Starts reading the preference on its own thread, then follows its
/// changes for as long as the app runs.
pub(crate) fn watch() -> Watch {
    let (tx, first) = mpsc::channel();
    let ctx = Arc::new(OnceLock::<egui::Context>::new());
    let follow = Arc::clone(&ctx);
    let _ = std::thread::Builder::new()
        .name("pure-appearance".to_owned())
        .spawn(move || {
            let Ok(conn) = zbus::blocking::Connection::session() else {
                return;
            };
            let Ok(proxy) = zbus::blocking::Proxy::new(
                &conn,
                "org.freedesktop.portal.Desktop",
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Settings",
            ) else {
                return;
            };
            let read = proxy
                .call::<_, _, OwnedValue>("ReadOne", &(NAMESPACE, KEY))
                .or_else(|_| proxy.call::<_, _, OwnedValue>("Read", &(NAMESPACE, KEY)));
            let Ok(value) = read else {
                return;
            };
            let _ = tx.send(theme_of(&value));
            let Ok(changes) = proxy.receive_signal("SettingChanged") else {
                return;
            };
            for message in changes {
                let Ok((namespace, key, value)) =
                    message.body().deserialize::<(String, String, OwnedValue)>()
                else {
                    continue;
                };
                if namespace == NAMESPACE
                    && key == KEY
                    && let Some(ctx) = follow.get()
                {
                    apply(ctx, theme_of(&value));
                }
            }
        });
    Watch { first, ctx }
}

impl Watch {
    /// Sets `ctx`'s system theme from the first answer, light when none
    /// comes, and hands it the changes that follow.
    pub(crate) fn attach(self, ctx: &egui::Context) {
        let theme = self
            .first
            .recv_timeout(FIRST_WAIT)
            .unwrap_or(egui::Theme::Light);
        apply(ctx, theme);
        let _ = self.ctx.set(ctx.clone());
    }
}

/// egui falls back to this theme under a System preference when the
/// platform reports none, which winit never does here.
fn apply(ctx: &egui::Context, theme: egui::Theme) {
    ctx.options_mut(|options| options.fallback_theme = theme);
    ctx.request_repaint();
}

fn theme_of(value: &Value<'_>) -> egui::Theme {
    if scheme(value) == Some(1) {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    }
}

/// The scheme number, unwrapping the extra variant the older `Read`
/// answers with.
fn scheme(value: &Value<'_>) -> Option<u32> {
    match value {
        Value::U32(scheme) => Some(*scheme),
        Value::Value(inner) => scheme(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::theme_of;
    use zbus::zvariant::Value;

    #[test]
    fn only_a_dark_preference_is_dark() {
        assert_eq!(theme_of(&Value::U32(1)), egui::Theme::Dark);
        assert_eq!(theme_of(&Value::U32(0)), egui::Theme::Light);
        assert_eq!(theme_of(&Value::U32(2)), egui::Theme::Light);
        assert_eq!(
            theme_of(&Value::Value(Box::new(Value::U32(1)))),
            egui::Theme::Dark
        );
        assert_eq!(theme_of(&Value::from("dark")), egui::Theme::Light);
    }
}
