//! WhatsApp Web's notifications as GNOME notifications, so they follow the
//! desktop's Do Not Disturb and per-app settings. Clicking one brings the
//! window to the front on that account and lets WhatsApp open the chat.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;

use crate::accounts::{Accounts, Session};
use crate::settings::Settings;
use crate::window::Main;

const OPEN_ACTION: &str = "open-notification";

/// The notifications on screen, by their GNotification id, with their account.
type Shown = Rc<RefCell<HashMap<String, (String, webkit::Notification)>>>;

pub fn attach(app: &adw::Application, main: &Main) {
    let shown = Shown::default();

    let open = gio::SimpleAction::new(OPEN_ACTION, Some(glib::VariantTy::STRING));
    open.connect_activate(glib::clone!(
        #[weak(rename_to = window)]
        main.window,
        #[strong(rename_to = accounts)]
        main.accounts,
        #[strong]
        shown,
        move |_, param| {
            window.present();
            let id = param.and_then(|p| p.str().map(str::to_owned));
            let entry = id.and_then(|id| shown.borrow_mut().remove(&id));
            if let Some((account, notification)) = entry {
                accounts.show(&account);
                notification.clicked();
            }
        }
    ));
    app.add_action(&open);

    let settings = main.settings.clone();
    let accounts = main.accounts.clone();
    main.accounts.on_session(glib::clone!(
        #[weak]
        app,
        move |session| {
            let session_ref = Rc::downgrade(session);
            let (shown, settings, accounts) = (shown.clone(), settings.clone(), accounts.clone());
            session.view.connect_show_notification(glib::clone!(
                #[weak]
                app,
                #[upgrade_or]
                false,
                move |_, notification| {
                    if let Some(session) = session_ref.upgrade() {
                        show(&app, &shown, &settings, &accounts, &session, notification);
                    }
                    true
                }
            ));
        }
    ));
}

fn show(
    app: &adw::Application,
    shown: &Shown,
    settings: &Settings,
    accounts: &Accounts,
    session: &Session,
    notification: &webkit::Notification,
) {
    let values = settings.get();
    if !values.notifications {
        return;
    }
    let id = format!("message-{}-{}", session.id, notification.id());
    let account = accounts.several().then(|| session.name());
    let (title, body) = content(
        notification.title().as_deref(),
        notification.body().as_deref(),
        values.notification_previews,
        account.as_deref(),
    );
    let desktop = gio::Notification::new(&title);
    desktop.set_body(body.as_deref());
    desktop.set_category(Some("im.received"));
    desktop
        .set_default_action_and_target_value(&format!("app.{OPEN_ACTION}"), Some(&id.to_variant()));

    notification.connect_closed(glib::clone!(
        #[weak]
        app,
        #[strong]
        shown,
        #[strong]
        id,
        move |_| {
            shown.borrow_mut().remove(&id);
            app.withdraw_notification(&id);
        }
    ));
    shown
        .borrow_mut()
        .insert(id.clone(), (session.id.clone(), notification.clone()));
    app.send_notification(Some(&id), &desktop);
}

/// The desktop notification's title and body for WhatsApp's `title` and
/// `body`. Without `previews`, it doesn't say who wrote or what. `account`
/// names the account when there are several.
fn content(
    title: Option<&str>,
    body: Option<&str>,
    previews: bool,
    account: Option<&str>,
) -> (String, Option<String>) {
    let mut title = match title {
        Some(title) if previews => title.to_owned(),
        _ => "New message".to_owned(),
    };
    if let Some(account) = account {
        title = format!("{title} ({account})");
    }
    let body = body.filter(|body| previews && !body.is_empty());
    (title, body.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_show_sender_and_message() {
        assert_eq!(
            content(Some("Anna"), Some("Hi!"), true, None),
            ("Anna".to_owned(), Some("Hi!".to_owned()))
        );
    }

    #[test]
    fn without_previews_nothing_leaks() {
        assert_eq!(
            content(Some("Anna"), Some("Hi!"), false, None),
            ("New message".to_owned(), None)
        );
    }

    #[test]
    fn several_accounts_name_the_account() {
        assert_eq!(
            content(Some("Anna"), None, true, Some("Work")).0,
            "Anna (Work)"
        );
        assert_eq!(
            content(Some("Anna"), Some("Hi!"), false, Some("Work")),
            ("New message (Work)".to_owned(), None)
        );
    }

    #[test]
    fn missing_or_empty_parts_fall_back() {
        assert_eq!(
            content(None, Some(""), true, None),
            ("New message".to_owned(), None)
        );
    }
}
