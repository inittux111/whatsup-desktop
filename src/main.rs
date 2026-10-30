//! WhatsUp: WhatsApp Web in a native GNOME window.

mod accounts;
mod app;
mod appearance;
mod background;
mod badge;
mod clipboard;
mod downloads;
mod notifications;
mod paths;
mod preferences;
mod settings;
mod shortcuts;
mod theme;
mod tray;
mod unread;
mod web;
mod window;

use gtk::glib;

fn main() -> glib::ExitCode {
    web::prepare_environment();
    app::run()
}
