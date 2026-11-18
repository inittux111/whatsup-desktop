//! Ending with the desktop session. WhatsUp registers with the session
//! manager (GNOME's, or MATE's, which serves the same D-Bus interface), so a
//! logout or shutdown asks it to quit first and waits until it answers that
//! it's done. Without that, systemd stops WhatsUp and WebKit's processes all
//! at once, which can cut WhatsApp Web off halfway through saving and leave
//! it signed out on the next start.
//!
//! GTK passes on the session's end only through the desktop portal, which
//! doesn't wait for the app to finish (and isn't told at all on MATE).

use std::cell::RefCell;

use adw::prelude::*;
use gtk::{gio, glib};

const MANAGER: &str = "org.gnome.SessionManager";
const MANAGER_PATH: &str = "/org/gnome/SessionManager";
const CLIENT: &str = "org.gnome.SessionManager.ClientPrivate";
/// How long quitting waits for the session manager to take the answer.
const ANSWER_TIMEOUT_MS: i32 = 1000;

/// What a signal from the session manager asks WhatsUp to do.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    /// Whether WhatsUp may quit: it always may, and answers right away.
    MayQuit,
    /// Quit now, and answer once done; the session waits for the answer.
    EndSession,
    /// Quit now, no answer expected.
    Stop,
}

fn request(signal: &str) -> Option<Request> {
    match signal {
        "QueryEndSession" => Some(Request::MayQuit),
        "EndSession" => Some(Request::EndSession),
        "Stop" => Some(Request::Stop),
        // CancelEndSession needs nothing: WhatsUp never held the session up.
        _ => None,
    }
}

#[derive(Default)]
struct Client {
    connection: Option<gio::DBusConnection>,
    path: Option<String>,
    _subscription: Option<gio::SignalSubscription>,
    /// The session is ending and waits for WhatsUp to say it's done.
    owes_answer: bool,
}

thread_local! {
    static CLIENT_STATE: RefCell<Client> = RefCell::default();
}

/// Registers with the session manager, if there is one.
pub fn register(app: &adw::Application) {
    let (Some(connection), Some(app_id)) = (app.dbus_connection(), app.application_id()) else {
        return;
    };
    let app = app.downgrade();
    glib::spawn_future_local(async move {
        let reply = connection
            .call_future(
                Some(MANAGER),
                MANAGER_PATH,
                MANAGER,
                "RegisterClient",
                Some(&(app_id.as_str(), "").to_variant()),
                Some(glib::VariantTy::new("(o)").expect("valid type")),
                gio::DBusCallFlags::NO_AUTO_START,
                -1,
            )
            .await;
        let path = match reply {
            Ok(reply) => reply.child_value(0).str().map(str::to_owned),
            Err(err) => {
                // Not every desktop has a session manager; nothing to do then.
                glib::g_debug!("whatsup-desktop", "no session manager: {}", err.message());
                None
            }
        };
        let Some(path) = path else {
            return;
        };
        glib::g_debug!("whatsup-desktop", "registered with the session manager");
        let subscription = connection.subscribe_to_signal(
            Some(MANAGER),
            Some(CLIENT),
            None,
            Some(&path),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                if let Some(app) = app.upgrade() {
                    handle(&app, signal.signal_name);
                }
            },
        );
        CLIENT_STATE.with(|client| {
            *client.borrow_mut() = Client {
                connection: Some(connection),
                path: Some(path),
                _subscription: Some(subscription),
                owes_answer: false,
            };
        });
    });
}

fn handle(app: &adw::Application, signal: &str) {
    match request(signal) {
        Some(Request::MayQuit) => answer(false),
        Some(Request::EndSession) => {
            CLIENT_STATE.with(|client| client.borrow_mut().owes_answer = true);
            // Shutting down closes the pages and then calls `finish`.
            app.quit();
        }
        Some(Request::Stop) => app.quit(),
        None => {}
    }
}

/// Tells the session manager WhatsUp is done, if the session is ending and
/// waiting for it. Runs last as WhatsUp quits, after the pages are closed.
pub fn finish() {
    if CLIENT_STATE.with(|client| client.borrow().owes_answer) {
        answer(true);
    }
}

/// Answers that WhatsUp may quit, or (`blocking`, as it quits) that it's
/// done; a non-blocking answer wouldn't be sent before the app exits.
fn answer(blocking: bool) {
    let (connection, path) = CLIENT_STATE.with(|client| {
        let client = client.borrow();
        (client.connection.clone(), client.path.clone())
    });
    let (Some(connection), Some(path)) = (connection, path) else {
        return;
    };
    let args = (true, "").to_variant();
    if blocking {
        let result = connection.call_sync(
            Some(MANAGER),
            &path,
            CLIENT,
            "EndSessionResponse",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            ANSWER_TIMEOUT_MS,
            gio::Cancellable::NONE,
        );
        if let Err(err) = result {
            glib::g_warning!(
                "whatsup-desktop",
                "cannot answer the session manager: {}",
                err.message()
            );
        }
    } else {
        connection.call(
            Some(MANAGER),
            &path,
            CLIENT,
            "EndSessionResponse",
            Some(&args),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_from_the_session_manager() {
        assert_eq!(request("QueryEndSession"), Some(Request::MayQuit));
        assert_eq!(request("EndSession"), Some(Request::EndSession));
        assert_eq!(request("Stop"), Some(Request::Stop));
        assert_eq!(request("CancelEndSession"), None);
        assert_eq!(request("Unknown"), None);
    }
}
