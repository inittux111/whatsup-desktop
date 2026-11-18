//! The application: one instance per session, one main window.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::accounts::Accounts;
use crate::settings::{self, Settings};
use crate::{background, clipboard, downloads, notifications, paths, session, window};

pub const APP_ID: &str = "app.whatsup_desktop.WhatsUp";
pub const APP_NAME: &str = "WhatsUp";

/// How long quitting waits for the pages to close; well inside the 10 s a
/// session manager gives apps as the session ends.
const UNLOAD_LIMIT: Duration = Duration::from_secs(4);

pub fn run() -> glib::ExitCode {
    glib::set_application_name(APP_NAME);
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.add_main_option(
        settings::BACKGROUND_FLAG,
        glib::Char::from(0),
        glib::OptionFlags::NONE,
        glib::OptionArg::None,
        "Start hidden in the tray",
        None,
    );
    app.connect_handle_local_options(|app, options| {
        let hidden = options.contains(settings::BACKGROUND_FLAG);
        START_HIDDEN.set(hidden);
        // Already running, so already in the tray: activating the running
        // instance would bring its window up, which a hidden start mustn't.
        if hidden && app.register(gio::Cancellable::NONE).is_ok() && app.is_remote() {
            return std::ops::ControlFlow::Break(glib::ExitCode::SUCCESS);
        }
        std::ops::ControlFlow::Continue(())
    });
    app.connect_startup(|app| {
        add_dev_icons();
        session::register(app);
    });
    app.connect_activate(activate);
    app.connect_shutdown(shutdown);

    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    app.add_action_entries([quit]);

    app.run()
}

thread_local! {
    /// Started with `--background`, as starting at login does: the first
    /// window stays hidden in the tray.
    static START_HIDDEN: Cell<bool> = const { Cell::new(false) };
    /// What quitting needs from the main window: its size and settings to
    /// save, and the pages to close.
    static MAIN: RefCell<Option<Quitting>> = const { RefCell::new(None) };
}

/// Shows the existing window, or builds it on first launch.
fn activate(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let main = window::build(app, Settings::load());
    MAIN.with(|m| {
        m.replace(Some(Quitting {
            window: main.window.clone(),
            settings: main.settings.clone(),
            accounts: main.accounts.clone(),
        }))
    });
    background::attach(app, &main);
    notifications::attach(app, &main);
    downloads::attach(&main);
    clipboard::attach(&main);
    if !START_HIDDEN.replace(false) {
        main.window.present();
    }
}

struct Quitting {
    window: adw::ApplicationWindow,
    settings: Settings,
    accounts: Accounts,
}

/// Runs last on every way of quitting (Quit, closing the window without
/// Run in Background, the session ending): saves the window's size and the
/// settings right away, as a background save wouldn't finish, then closes
/// the pages so WhatsApp Web finishes saving before WebKit's processes stop.
fn shutdown(_app: &adw::Application) {
    if let Some(main) = MAIN.with(|m| m.take()) {
        window::remember_size(&main.window, &main.settings);
        main.settings.save_now();
        main.window.set_visible(false);
        main.accounts.unload_all(UNLOAD_LIMIT);
    }
    session::finish();
}

/// In development builds, finds the app's icons in the source tree.
fn add_dev_icons() {
    if let (Some(dir), Some(display)) = (paths::dev_icon_dir(), gtk::gdk::Display::default()) {
        gtk::IconTheme::for_display(&display).add_search_path(dir);
    }
}
