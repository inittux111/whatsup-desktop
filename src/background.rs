//! Running in the background: closing the window hides it, the tray icon
//! brings it back, and the unread count shows on the tray and the dock.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::window::Main;
use crate::{badge, paths, tray};

pub fn attach(app: &adw::Application, main: &Main) {
    let window = &main.window;
    let settings = main.settings.clone();
    window.connect_close_request(move |window| {
        if !settings.get().run_in_background {
            // The app quits with its window; `app::shutdown` saves.
            return glib::Propagation::Proceed;
        }
        window.set_visible(false);
        glib::Propagation::Stop
    });

    let Some(connection) = app.dbus_connection() else {
        return;
    };
    let tray = new_tray(app, window, &connection).map(Rc::new);

    if let Some(tray) = &tray {
        tray.set_window_visible(window.is_visible());
        tray.set_icon_visible(main.settings.get().show_tray_icon);
        tray.set_colored(main.settings.get().colored_tray_icon);
        let for_window = tray.clone();
        window.connect_visible_notify(move |window| {
            for_window.set_window_visible(window.is_visible());
        });
    }

    let for_dock = connection.clone();
    let count = Rc::new(std::cell::Cell::new(0));
    let settings = main.settings.clone();
    let update = glib::clone!(
        #[strong]
        tray,
        #[strong]
        count,
        #[strong]
        settings,
        move || {
            let values = settings.get();
            if let Some(tray) = &tray {
                tray.set_icon_visible(values.show_tray_icon);
                tray.set_colored(values.colored_tray_icon);
                tray.set_unread(count.get());
            }
            badge::set_count(&connection, if values.dock_badge { count.get() } else { 0 });
        }
    );
    badge::connect_dock_appeared(&for_dock, update.clone());
    let on_change = update.clone();
    main.settings.connect_changed(move |_| on_change());
    main.accounts.connect_unread(move |total| {
        count.set(total);
        update();
    });
}

fn new_tray(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    connection: &gtk::gio::DBusConnection,
) -> Option<tray::Tray> {
    let icon_theme_path = paths::dev_icon_dir()
        .map(|dir| dir.to_string_lossy().into_owned())
        .unwrap_or_default();
    let actions = tray::Actions {
        toggle: Box::new(glib::clone!(
            #[weak]
            window,
            move || toggle(&window)
        )),
        show: Box::new(glib::clone!(
            #[weak]
            window,
            move |show| {
                if show {
                    window.present();
                } else {
                    window.set_visible(false);
                }
            }
        )),
        settings: Box::new(glib::clone!(
            #[weak]
            window,
            move || {
                window.present();
                let _ = WidgetExt::activate_action(&window, "win.preferences", None);
            }
        )),
        quit: Box::new(glib::clone!(
            #[weak]
            app,
            move || app.activate_action("quit", None)
        )),
    };
    match tray::Tray::new(connection, icon_theme_path, actions) {
        Ok(tray) => Some(tray),
        Err(err) => {
            glib::g_warning!("whatsup-desktop", "no tray icon: {}", err.message());
            None
        }
    }
}

/// Hides the window when it's in front, and brings it to the front otherwise.
fn toggle(window: &adw::ApplicationWindow) {
    if window.is_visible() && window.is_active() {
        window.set_visible(false);
    } else {
        window.present();
    }
}
