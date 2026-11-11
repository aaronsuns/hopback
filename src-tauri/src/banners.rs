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
