//! Files saved from chats go to the Downloads folder under a cleaned-up,
//! unused name; a toast says when each one is saved and offers to open it.

mod filename;

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};
use webkit::prelude::*;

use crate::paths;
use crate::settings::Settings;
use crate::window::Main;

pub fn attach(main: &Main) {
    let settings = main.settings.clone();
    main.accounts.on_session(glib::clone!(
        #[weak(rename_to = toasts)]
        main.toasts,
        move |session| {
            let Some(network) = session.view.network_session() else {
                return;
            };
            let settings = settings.clone();
            network.connect_download_started(glib::clone!(
                #[weak]
                toasts,
                move |_, download| track(download, &toasts, &settings)
            ));
        }
    ));
}

fn track(download: &webkit::Download, toasts: &adw::ToastOverlay, settings: &Settings) {
    download.set_allow_overwrite(false);
    let values = settings.get();
    let folder = values.download_folder.unwrap_or_else(paths::downloads_dir);
    let ask = values.ask_download_location;
    download.connect_decide_destination(glib::clone!(
        #[weak]
        toasts,
        #[upgrade_or]
        false,
        move |download, suggested| {
            if ask {
                // WebKit waits until the destination is set or the download cancelled.
                let window = toasts.root().and_downcast::<gtk::Window>();
                glib::spawn_future_local(ask_destination(
                    download.clone(),
                    window,
                    folder.clone(),
                    filename::sanitize(suggested),
                ));
            } else {
                choose_destination(download, &folder, suggested);
            }
            true
        }
    ));

    let failed = Rc::new(Cell::new(false));
    download.connect_failed(glib::clone!(
        #[weak]
        toasts,
        #[strong]
        failed,
        move |download, err| {
            failed.set(true);
            if !err.matches(webkit::DownloadError::CancelledByUser) {
                glib::g_warning!("whatsup-desktop", "a download failed: {}", err.message());
                toasts.add_toast(adw::Toast::new(&format!(
                    "Couldn’t save “{}”",
                    name_of(download)
                )));
            }
        }
    ));
    download.connect_finished(glib::clone!(
        #[weak]
        toasts,
        move |download| {
            if !failed.get() {
                announce_saved(download, &toasts);
            }
        }
    ));
}

fn choose_destination(download: &webkit::Download, dir: &Path, suggested: &str) {
    if let Err(err) = std::fs::create_dir_all(dir) {
        glib::g_warning!(
            "whatsup-desktop",
            "cannot create the downloads folder: {err}"
        );
    }
    let path = filename::unique_path(dir, &filename::sanitize(suggested), Path::exists);
    match path.to_str() {
        Some(path) => download.set_destination(path),
        // WebKit takes the destination as UTF-8; a folder it can't name is refused.
        None => download.cancel(),
    }
}

/// Asks where to save, through the desktop's file picker; cancelling it
/// cancels the download.
async fn ask_destination(
    download: webkit::Download,
    window: Option<gtk::Window>,
    folder: std::path::PathBuf,
    name: String,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Save File")
        .modal(true)
        .initial_folder(&gio::File::for_path(&folder))
        .initial_name(name.as_str())
        .build();
    let path = dialog
        .save_future(window.as_ref())
        .await
        .ok()
        .and_then(|file| file.path());
    match path.as_deref().and_then(Path::to_str) {
        Some(path) => {
            // The user chose this file in the picker, which asked before replacing it.
            download.set_allow_overwrite(true);
            download.set_destination(path);
        }
        None => download.cancel(),
    }
}

fn announce_saved(download: &webkit::Download, toasts: &adw::ToastOverlay) {
    let Some(destination) = download.destination() else {
        return;
    };
    let file = gio::File::for_path(destination.as_str());
    let toast = adw::Toast::builder()
        .title(format!("Saved “{}”", name_of(download)))
        .button_label("Open")
        .build();
    toast.connect_button_clicked(glib::clone!(
        #[weak]
        toasts,
        move |_| {
            let window = toasts.root().and_downcast::<gtk::Window>();
            gtk::FileLauncher::new(Some(&file)).launch(
                window.as_ref(),
                gio::Cancellable::NONE,
                |result| {
                    if let Err(err) = result {
                        glib::g_warning!(
                            "whatsup-desktop",
                            "cannot open a download: {}",
                            err.message()
                        );
                    }
                },
            );
        }
    ));
    toasts.add_toast(toast);
}

/// The saved file's name, for messages to the user.
fn name_of(download: &webkit::Download) -> String {
    display_name(download.destination().as_deref())
}

/// The file name in `destination`, or "file" when there's none.
fn display_name(destination: Option<&str>) -> String {
    destination
        .and_then(|path| Path::new(path).file_name())
        .map_or_else(|| "file".to_owned(), |n| n.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_saved_file() {
        assert_eq!(
            display_name(Some("/home/user/Downloads/photo (1).jpg")),
            "photo (1).jpg"
        );
        assert_eq!(display_name(Some("/")), "file");
        assert_eq!(display_name(None), "file");
    }
}
