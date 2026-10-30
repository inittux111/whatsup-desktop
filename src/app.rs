//! The application: one instance per session, one main window.

use std::cell::Cell;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::settings::{self, Settings};
use crate::{background, clipboard, downloads, notifications, paths, window};

pub const APP_ID: &str = "app.whatsup_desktop.WhatsUp";
pub const APP_NAME: &str = "WhatsUp";

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
    app.connect_startup(|_| add_dev_icons());
    app.connect_activate(activate);

    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| quit(app))
        .build();
    app.add_action_entries([quit]);

    app.run()
}

thread_local! {
    /// Started with `--background`, as starting at login does: the first
    /// window stays hidden in the tray.
    static START_HIDDEN: Cell<bool> = const { Cell::new(false) };
    /// The main window's settings, for saving them as the app quits.
    static MAIN_SETTINGS: std::cell::RefCell<Option<Settings>> = const { std::cell::RefCell::new(None) };
}

/// Shows the existing window, or builds it on first launch.
fn activate(app: &adw::Application) {
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let main = window::build(app, Settings::load());
    MAIN_SETTINGS.with(|s| s.replace(Some(main.settings.clone())));
    background::attach(app, &main);
    notifications::attach(app, &main);
    downloads::attach(&main);
    clipboard::attach(&main);
    if !START_HIDDEN.replace(false) {
        main.window.present();
    }
}

/// Quits, after saving the window's size; the settings are written right
/// away, as a background save wouldn't finish.
fn quit(app: &adw::Application) {
    if let Some(window) = app.active_window().and_downcast::<adw::ApplicationWindow>() {
        let settings = MAIN_SETTINGS.with(|s| s.borrow().clone());
        if let Some(settings) = settings {
            window::remember_size(&window, &settings);
            settings.save_now();
        }
    }
    app.quit();
}

/// In development builds, finds the app's icons in the source tree.
fn add_dev_icons() {
    if let (Some(dir), Some(display)) = (paths::dev_icon_dir(), gtk::gdk::Display::default()) {
        gtk::IconTheme::for_display(&display).add_search_path(dir);
    }
}
