//! macOS banners. Clicking one goes to the session.

use crate::state::{lock, App};
use hopback_core::board::{self, Session};
use hopback_core::markers::State;

pub fn init() {
    if let Err(e) = mac_notification_sys::set_application("se.zexa.hopback") {
        log::warn!("notifications: {e}");
    }
}

/// Show a banner on a background thread; `on_click` runs if the user clicks it.
pub fn show(title: String, subtitle: String, message: String, on_click: impl FnOnce() + Send + 'static) {
    std::thread::spawn(move || {
        let resp = mac_notification_sys::Notification::new()
            .title(&title)
            .subtitle(&subtitle)
            .message(&message)
            .wait_for_click(true)
            .send();
        if let Ok(mac_notification_sys::NotificationResponse::Click) = resp {
            on_click();
        }
    });
}

/// One banner per session that just started waiting, if enabled for its kind.
pub fn announce(app: &App, fresh: Vec<Session>) {
    let (approve, question) = {
        let s = lock(&app.settings);
        (s.banners_approve, s.banners_question)
    };
    for s in fresh {
        let wanted = match s.state {
            Some(State::Approve) => approve,
            Some(State::Question) => question,
            _ => false,
        };
        if !wanted {
            continue;
        }
        let app2 = app.clone();
        let id = s.id.clone();
        show(s.folder().to_owned(), board::waiting_for(&s), s.title.clone(), move || {
            crate::actions::go_to_session(&app2, &id)
        });
    }
}

/// Whether macOS allows Hopback's notifications: `Some(true)` allowed,
/// `Some(false)` turned off, `None` unknown (never asked, or not running from
/// Hopback.app, where asking would crash).
pub fn allowed() -> Option<bool> {
    use block2::RcBlock;
    use objc2_foundation::NSBundle;
    use objc2_user_notifications::{UNNotificationSettings, UNUserNotificationCenter};
    use std::ptr::NonNull;
    use std::time::Duration;

    let bundle = NSBundle::mainBundle();
    if bundle.bundleIdentifier().is_none() || !bundle.bundlePath().to_string().ends_with(".app") {
        return None;
    }
    let center = UNUserNotificationCenter::currentNotificationCenter();
    let (tx, rx) = std::sync::mpsc::channel();
    let handler = RcBlock::new(move |settings: NonNull<UNNotificationSettings>| {
        // SAFETY: macOS passes a valid settings object for the block's duration.
        let status = unsafe { settings.as_ref() }.authorizationStatus();
        let _ = tx.send(status.0);
    });
    center.getNotificationSettingsWithCompletionHandler(&handler);
    allowed_from_status(rx.recv_timeout(Duration::from_secs(2)).ok()?)
}

/// `UNAuthorizationStatus` → allowed? Not determined is unknown.
fn allowed_from_status(status: isize) -> Option<bool> {
    match status {
        1 => Some(false),
        2..=4 => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::allowed_from_status;

    #[test]
    fn authorization_status() {
        assert_eq!(allowed_from_status(0), None);
        assert_eq!(allowed_from_status(1), Some(false));
        assert_eq!(allowed_from_status(2), Some(true));
        assert_eq!(allowed_from_status(3), Some(true));
        assert_eq!(allowed_from_status(4), Some(true));
        assert_eq!(allowed_from_status(9), None);
    }
}
