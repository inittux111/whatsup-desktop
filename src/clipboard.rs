//! Copying images out of chats and pasting files into them: Copy Image in
//! the page's right-click menu puts the picture itself on the clipboard, and
//! pasting files copied in a file manager attaches them, as in a browser.
//! Pasting image data (a screenshot, an image copied in another app) needs
//! nothing extra: WebKit already hands it to the page.

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use webkit::prelude::*;

use crate::accounts::Session;
use crate::web::clipboard::{self, MAX_BYTES, Pasted};
use crate::window::Main;

pub fn attach(main: &Main) {
    main.accounts.on_session(glib::clone!(
        #[weak(rename_to = toasts)]
        main.toasts,
        move |session| {
            offer_copy_image(session, &toasts);
            paste_files_on_ctrl_v(session, &toasts);
        }
    ));
}

/// Swaps WebKit's Copy Image, which copies only the image's address, for
/// one that copies the image.
fn offer_copy_image(session: &Rc<Session>, toasts: &adw::ToastOverlay) {
    let view = &session.view;
    let copy = gio::SimpleAction::new("copy-image", Some(glib::VariantTy::STRING));
    copy.connect_activate(glib::clone!(
        #[weak]
        view,
        #[weak]
        toasts,
        move |_, uri| {
            let Some(uri) = uri.and_then(|uri| uri.str().map(str::to_owned)) else {
                return;
            };
            glib::spawn_future_local(async move {
                let message = match clipboard::image_png(&view, &uri).await {
                    Ok(png) => {
                        let provider = gdk::ContentProvider::for_bytes("image/png", &png);
                        match view.clipboard().set_content(Some(&provider)) {
                            Ok(()) => "Image copied",
                            Err(_) => "Couldn’t copy the image",
                        }
                    }
                    Err(err) => {
                        glib::g_warning!("whatsup-desktop", "cannot copy an image: {err}");
                        "Couldn’t copy the image"
                    }
                };
                toasts.add_toast(adw::Toast::new(message));
            });
        }
    ));
    view.connect_context_menu(move |_, menu, hit| {
        let items = menu.items();
        let stock = webkit::ContextMenuAction::CopyImageToClipboard;
        let Some(index) = items.iter().position(|item| item.stock_action() == stock) else {
            return false;
        };
        menu.remove(&items[index]);
        if let Some(uri) = hit
            .image_uri()
            .filter(|uri| clipboard::is_copyable_image(uri))
        {
            let target = uri.as_str().to_variant();
            let ours = webkit::ContextMenuItem::from_gaction(&copy, "Copy Image", Some(&target));
            menu.insert(&ours, index as i32);
        }
        false
    });
}

/// Ctrl+V (or Shift+Insert) with files on the clipboard attaches them: WebKit
/// would paste only their paths. Anything else pastes as usual.
fn paste_files_on_ctrl_v(session: &Rc<Session>, toasts: &adw::ToastOverlay) {
    let view = &session.view;
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[weak]
        view,
        #[weak]
        toasts,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, state| {
            let clipboard = view.clipboard();
            let mime_types = clipboard.formats().mime_types();
            if !is_paste(key, state) || !holds_files_only(&mime_types) {
                return glib::Propagation::Proceed;
            }
            glib::spawn_future_local(paste_files(view, clipboard, toasts));
            glib::Propagation::Stop
        }
    ));
    view.add_controller(keys);
}

fn is_paste(key: gdk::Key, state: gdk::ModifierType) -> bool {
    let modifiers = state
        & (gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::ALT_MASK
            | gdk::ModifierType::SUPER_MASK);
    (modifiers == gdk::ModifierType::CONTROL_MASK && key.to_lower() == gdk::Key::v)
        || (modifiers == gdk::ModifierType::SHIFT_MASK && key == gdk::Key::Insert)
}

/// Whether the clipboard holds a list of files and no image data, which
/// WebKit pastes well by itself.
fn holds_files_only(mime_types: &[glib::GString]) -> bool {
    let files = mime_types.iter().any(|m| m == "text/uri-list");
    let image = mime_types.iter().any(|m| m.starts_with("image/"));
    files && !image
}

async fn paste_files(view: webkit::WebView, clipboard: gdk::Clipboard, toasts: adw::ToastOverlay) {
    let uris = copied_uris(&clipboard).await;
    // A copied web link is a list of addresses too: paste those as text.
    let Some(files) = local_files(&uris) else {
        view.execute_editing_command(webkit::EDITING_COMMAND_PASTE);
        return;
    };
    let message = match read_files(&files).await {
        Ok(pasted) if pasted.is_empty() => Some("Folders can’t be pasted".to_owned()),
        Ok(pasted) => match clipboard::paste_files(&view, pasted).await {
            Ok(()) => None,
            Err(err) => {
                glib::g_warning!("whatsup-desktop", "cannot paste files: {err}");
                Some("Couldn’t paste the files".to_owned())
            }
        },
        Err(message) => Some(message),
    };
    if let Some(message) = message {
        toasts.add_toast(adw::Toast::new(&message));
    }
}

/// The clipboard's `text/uri-list`, read as text: GTK's own file list goes
/// through the desktop portal, which isn't there on every desktop.
async fn copied_uris(clipboard: &gdk::Clipboard) -> String {
    const MAX_LIST: usize = 1024 * 1024;
    let Ok((stream, _)) = clipboard
        .read_future(&["text/uri-list"], glib::Priority::DEFAULT)
        .await
    else {
        return String::new();
    };
    let mut list = Vec::new();
    while list.len() <= MAX_LIST {
        match stream
            .read_bytes_future(64 * 1024, glib::Priority::DEFAULT)
            .await
        {
            Ok(chunk) if !chunk.is_empty() => list.extend_from_slice(&chunk),
            _ => break,
        }
    }
    String::from_utf8(list).unwrap_or_default()
}

/// The files in a `text/uri-list` (RFC 2483: one URI a line, `#` starts a
/// comment), if it names only local files.
fn local_files(list: &str) -> Option<Vec<gio::File>> {
    let uris: Vec<&str> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let local = |uri: &&str| glib::Uri::peek_scheme(uri).is_some_and(|scheme| scheme == "file");
    (!uris.is_empty() && uris.iter().all(local))
        .then(|| uris.iter().map(|uri| gio::File::for_uri(uri)).collect())
}

/// The regular files among `files`, read in full; folders are left out.
/// The error is a message for the user.
async fn read_files(files: &[gio::File]) -> Result<Vec<Pasted>, String> {
    let attributes = "standard::type,standard::size,standard::content-type,standard::display-name";
    let mut infos = Vec::new();
    for file in files {
        let info = file
            .query_info_future(
                attributes,
                gio::FileQueryInfoFlags::NONE,
                glib::Priority::DEFAULT,
            )
            .await
            .map_err(|_| couldnt_read(file))?;
        if info.file_type() == gio::FileType::Regular {
            infos.push((file, info));
        }
    }
    let sizes: Vec<i64> = infos.iter().map(|(_, info)| info.size()).collect();
    if !fits(&sizes) {
        return Err("Too large to paste: attach files over 100 MB instead".to_owned());
    }
    let mut pasted = Vec::new();
    for (file, info) in infos {
        let (contents, _) = file
            .load_bytes_future()
            .await
            .map_err(|_| couldnt_read(file))?;
        let mime_type = info
            .content_type()
            .and_then(|kind| gio::content_type_get_mime_type(&kind))
            .map_or_else(|| "application/octet-stream".to_owned(), |m| m.to_string());
        pasted.push(Pasted {
            name: info.display_name().to_string(),
            mime_type,
            contents,
        });
    }
    // A file may have grown since its size was read.
    let total: usize = pasted.iter().map(|p| p.contents.len()).sum();
    if total > MAX_BYTES {
        return Err("Too large to paste: attach files over 100 MB instead".to_owned());
    }
    Ok(pasted)
}

/// Whether files of these `sizes` may be pasted together.
fn fits(sizes: &[i64]) -> bool {
    let total = sizes.iter().try_fold(0u64, |sum, &size| {
        sum.checked_add(u64::try_from(size).ok()?)
    });
    total.is_some_and(|total| total <= MAX_BYTES as u64)
}

fn couldnt_read(file: &gio::File) -> String {
    let name = file
        .basename()
        .map_or_else(|| "a file".to_owned(), |n| format!("“{}”", n.display()));
    format!("Couldn’t read {name}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn ctrl_v_and_shift_insert_paste() {
        use gdk::ModifierType as M;
        assert!(is_paste(gdk::Key::v, M::CONTROL_MASK));
        assert!(is_paste(gdk::Key::V, M::CONTROL_MASK | M::LOCK_MASK));
        assert!(is_paste(gdk::Key::Insert, M::SHIFT_MASK));
        assert!(!is_paste(gdk::Key::v, M::empty()));
        assert!(!is_paste(gdk::Key::V, M::CONTROL_MASK | M::SHIFT_MASK));
        assert!(!is_paste(gdk::Key::v, M::CONTROL_MASK | M::ALT_MASK));
        assert!(!is_paste(gdk::Key::c, M::CONTROL_MASK));
    }

    #[test]
    fn only_file_lists_without_images_are_handled() {
        let types = |list: &[&str]| -> Vec<glib::GString> {
            list.iter().copied().map(glib::GString::from).collect()
        };
        // Files copied in GNOME Files.
        assert!(holds_files_only(&types(&[
            "x-special/gnome-copied-files",
            "text/uri-list",
            "text/plain;charset=utf-8",
        ])));
        // A screenshot, or an image copied in a browser: WebKit pastes these.
        assert!(!holds_files_only(&types(&["image/png"])));
        assert!(!holds_files_only(&types(&[
            "text/html",
            "text/uri-list",
            "image/png",
        ])));
        // Plain text.
        assert!(!holds_files_only(&types(&["text/plain;charset=utf-8"])));
    }

    #[test]
    fn pastes_only_lists_of_local_files() {
        let files = local_files("file:///home/me/a.png\r\nfile:///home/me/b%20c.txt\r\n").unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path().unwrap()).collect();
        assert_eq!(
            paths,
            [Path::new("/home/me/a.png"), Path::new("/home/me/b c.txt")]
        );
        // Comments are allowed in a uri-list.
        assert_eq!(local_files("# copied\r\nfile:///a\r\n").unwrap().len(), 1);
        assert!(local_files("https://example.com/\r\n").is_none());
        assert!(local_files("file:///a\r\nhttps://example.com/\r\n").is_none());
        assert!(local_files("").is_none());
        assert!(local_files("not a uri").is_none());
    }

    #[test]
    fn pastes_up_to_the_limit() {
        let max = MAX_BYTES as i64;
        assert!(fits(&[]));
        assert!(fits(&[max]));
        assert!(fits(&[max / 2, max / 2]));
        assert!(!fits(&[max, 1]));
        assert!(!fits(&[-1]));
        assert!(!fits(&[i64::MAX, i64::MAX]));
    }
}
