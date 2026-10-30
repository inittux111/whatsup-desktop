//! The web view that runs WhatsApp Web, locked down to WhatsApp's own site.

mod audio;
pub mod clipboard;
mod nvidia;
mod permissions;
mod policy;
mod style;

pub use style::Extras;

use std::cell::Cell;
use std::ffi::OsString;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;

use crate::paths::AccountDirs;

pub const WHATSAPP_URL: &str = "https://web.whatsapp.com/";

/// A page that ran at least this long before its web process ended is treated
/// as a fresh crash, not part of a crash loop.
const CRASH_HEALTHY: Duration = Duration::from_secs(60);
/// The longest WhatsUp waits before reloading after repeated quick crashes.
const CRASH_BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Sets up the environment the web processes inherit, working around
/// problems in the graphics driver and the sound system. Must run first in
/// `main`, before any thread starts.
pub fn prepare_environment() {
    let _ = STARTED_WITH.set(
        std::env::vars_os()
            .map(|(key, value)| {
                let mut entry = key;
                entry.push("=");
                entry.push(value);
                entry
            })
            .collect(),
    );
    nvidia::prepare();
    audio::prepare();
}

/// The environment WhatsUp started with, as `KEY=value`, before
/// `prepare_environment` changed it for the web processes.
static STARTED_WITH: OnceLock<Vec<OsString>> = OnceLock::new();

/// The environment for programs WhatsUp starts that outlive it, such as a
/// restarted panel: without WhatsUp's own changes, which would otherwise
/// reach every app started from there.
pub fn original_environment() -> &'static [OsString] {
    STARTED_WITH.get().map_or(&[], Vec::as_slice)
}

/// Builds a web view for one account and starts loading WhatsApp Web.
/// `extras` are the theme colours and user scripts to add.
pub fn build(dirs: &AccountDirs, extras: Extras) -> webkit::WebView {
    let session = webkit::NetworkSession::new(dirs.data.to_str(), dirs.cache.to_str());
    let content = webkit::UserContentManager::new();
    style::load(&content, extras);

    let view = webkit::WebView::builder()
        .network_session(&session)
        .user_content_manager(&content)
        .settings(&settings())
        .hexpand(true)
        .vexpand(true)
        .build();

    view.connect_decide_policy(|view, decision, kind| {
        policy::decide(view, decision, kind);
        true
    });
    view.connect_create(|view, action| {
        if let Some(uri) = action.request().and_then(|r| r.uri()) {
            open_externally(view, &uri);
        }
        None
    });
    view.connect_permission_request(permissions::decide);
    // A crashed page keeps showing its last frame but no longer reacts to
    // anything, so load WhatsApp again. Repeated quick crashes back off, so a
    // page that keeps dying (e.g. a recurring driver fault) can't spin in a
    // tight reload loop. Removing an account terminates its process on purpose
    // (`TerminatedByApi`); that one stays closed.
    let rapid_crashes = Rc::new(Cell::new(0u32));
    let last_start = Rc::new(Cell::new(Instant::now()));
    view.connect_web_process_terminated(move |view, reason| {
        if reason == webkit::WebProcessTerminationReason::TerminatedByApi {
            return;
        }
        let count = if last_start.get().elapsed() >= CRASH_HEALTHY {
            0
        } else {
            rapid_crashes.get().saturating_add(1)
        };
        rapid_crashes.set(count);
        let delay = reload_delay(count);
        glib::g_warning!(
            "whatsup-desktop",
            "web process ended ({reason:?}), reloading in {}s",
            delay.as_secs()
        );
        glib::timeout_add_local_once(
            delay,
            glib::clone!(
                #[weak]
                view,
                #[strong]
                last_start,
                move || {
                    last_start.set(Instant::now());
                    view.load_uri(WHATSAPP_URL);
                }
            ),
        );
    });
    if let Some(context) = view.context() {
        permissions::grant_notifications(&context);
    }

    view.load_uri(WHATSAPP_URL);
    view
}

/// Loads the stylesheets and scripts again and reloads the page, so changes
/// to them show.
pub fn reload_customizations(view: &webkit::WebView, extras: Extras) {
    let Some(content) = view.user_content_manager() else {
        return;
    };
    let view = view.clone();
    glib::spawn_future_local(async move {
        style::apply(content, extras).await;
        view.reload();
    });
}

/// Swaps in new stylesheets (such as new theme colours) without reloading
/// the page; WebKit restyles it right away.
pub fn restyle(view: &webkit::WebView, extras: Extras) {
    if let Some(content) = view.user_content_manager() {
        style::load(&content, extras);
    }
}

/// Applies the zoom and spell checking settings now and whenever they change.
pub fn follow_settings(view: &webkit::WebView, settings: &crate::settings::Settings) {
    let apply = glib::clone!(
        #[weak]
        view,
        move |values: &crate::settings::Values| {
            view.set_zoom_level(values.zoom);
            if let Some(context) = view.context() {
                context.set_spell_checking_enabled(values.spell_check);
                let languages = if values.spell_languages.is_empty() {
                    spell_languages(&glib::language_names())
                } else {
                    values.spell_languages.clone()
                };
                let languages: Vec<&str> = languages.iter().map(String::as_str).collect();
                context.set_spell_checking_languages(&languages);
            }
        }
    );
    apply(&settings.get());
    settings.connect_changed(apply);
}

/// The user's languages as spell checking dictionaries ("nl_NL", "en"),
/// from GLib's list, which also holds variants such as "nl_NL.UTF-8" and "C".
fn spell_languages(names: &[glib::GString]) -> Vec<String> {
    let mut languages: Vec<String> = Vec::new();
    for name in names {
        let valid = !name.contains(['.', '@']) && !matches!(name.as_str(), "C" | "POSIX");
        if valid && !languages.iter().any(|l| l == name.as_str()) {
            languages.push(name.to_string());
        }
    }
    languages
}

/// How long to wait before reloading after the web process ended, given how
/// many times it has already crashed in quick succession. A one-off crash
/// reloads at once; repeated quick crashes back off (1s, 2s, 4s, …, capped at
/// `CRASH_BACKOFF_MAX`), so a page that keeps crashing can't spin in a tight
/// reload loop, yet WhatsUp never gives up and stops retrying.
fn reload_delay(rapid_crashes: u32) -> Duration {
    match rapid_crashes {
        0 => Duration::ZERO,
        n => {
            let secs = 1u64.checked_shl(n - 1).unwrap_or(u64::MAX);
            CRASH_BACKOFF_MAX.min(Duration::from_secs(secs))
        }
    }
}

fn settings() -> webkit::Settings {
    let settings = webkit::Settings::new();
    settings.set_enable_developer_extras(cfg!(debug_assertions));
    settings.set_enable_media_stream(true);
    settings.set_enable_webrtc(true);
    settings.set_enable_back_forward_navigation_gestures(false);
    settings.set_javascript_can_open_windows_automatically(false);
    settings
}

/// Opens `uri` in the user's default app for it, such as their browser.
pub fn open_externally(view: &webkit::WebView, uri: &str) {
    if !policy::is_openable_externally(uri) {
        return;
    }
    let window = view.root().and_downcast::<gtk::Window>();
    gtk::UriLauncher::new(uri).launch(window.as_ref(), gio::Cancellable::NONE, |result| {
        if let Err(err) = result {
            glib::g_warning!("whatsup-desktop", "cannot open a link: {}", err.message());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_backs_off_then_caps() {
        assert_eq!(reload_delay(0), Duration::ZERO);
        assert_eq!(reload_delay(1), Duration::from_secs(1));
        assert_eq!(reload_delay(2), Duration::from_secs(2));
        assert_eq!(reload_delay(3), Duration::from_secs(4));
        assert_eq!(reload_delay(5), Duration::from_secs(16));
        assert_eq!(reload_delay(6), CRASH_BACKOFF_MAX); // 32s capped to 30s
        assert_eq!(reload_delay(100), CRASH_BACKOFF_MAX); // no overflow
    }

    #[test]
    fn spell_languages_are_plain_locales() {
        let names: Vec<glib::GString> =
            ["nl_NL.UTF-8", "nl_NL", "nl.UTF-8", "nl", "en_US@euro", "C"]
                .into_iter()
                .map(glib::GString::from)
                .collect();
        assert_eq!(spell_languages(&names), ["nl_NL", "nl"]);
    }
}
