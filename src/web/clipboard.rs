//! Images and files between WhatsApp Web and the desktop clipboard, where
//! WebKitGTK falls short: its Copy Image puts no image on the clipboard, and
//! pasting files copied in a file manager gives the page only their paths.
//!
//! Both run in an isolated script world, so the page's own scripts can't
//! change what they do, and what comes back from the page is checked like
//! any other page content.

use gtk::glib;
use webkit::prelude::*;

/// The script world WhatsUp's clipboard scripts run in, apart from the page's.
const WORLD: &str = "whatsup-clipboard";

/// The largest image Copy Image puts on the clipboard, and the most a paste
/// hands to the page at once.
pub const MAX_BYTES: usize = 100 * 1024 * 1024;

const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Reads the image at `uri`, decodes it and returns it as a base64 PNG, so
/// any format WebKit shows (JPEG, WebP stickers, ...) copies the same way.
const READ_IMAGE: &str = r#"
const response = await fetch(uri);
if (!response.ok) throw new Error("not loaded");
const bitmap = await createImageBitmap(await response.blob());
const canvas = document.createElement("canvas");
canvas.width = bitmap.width;
canvas.height = bitmap.height;
canvas.getContext("2d").drawImage(bitmap, 0, 0);
bitmap.close();
const png = await new Promise((resolve, reject) =>
    canvas.toBlob(blob => blob ? resolve(blob) : reject(new Error("not encoded")), "image/png"));
const url = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(png);
});
return url.slice(url.indexOf(",") + 1);
"#;

/// Pastes `files` ({name, type, data: base64}) into the focused part of the
/// page, as the browser does when the clipboard holds files.
const PASTE_FILES: &str = r#"
const transfer = new DataTransfer();
for (const file of JSON.parse(files)) {
    const text = atob(file.data);
    const bytes = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i++) bytes[i] = text.charCodeAt(i);
    transfer.items.add(new File([bytes], file.name, { type: file.type }));
}
const target = document.activeElement ?? document.body;
target.dispatchEvent(new ClipboardEvent("paste",
    { clipboardData: transfer, bubbles: true, cancelable: true, composed: true }));
return transfer.files.length;
"#;

/// A file to paste into the page.
pub struct Pasted {
    pub name: String,
    pub mime_type: String,
    pub contents: glib::Bytes,
}

/// Whether Copy Image may read `uri`: an image WhatsApp Web made in the page
/// (`blob:https://web.whatsapp.com/...`) or one written into it (`data:image/...`).
pub fn is_copyable_image(uri: &str) -> bool {
    super::policy::is_whatsapp_blob(uri)
        || uri
            .get(..11)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:image/"))
}

/// The image at `uri` in `view`'s page, as PNG.
pub async fn image_png(view: &webkit::WebView, uri: &str) -> Result<glib::Bytes, String> {
    if !is_copyable_image(uri)
        || !view
            .uri()
            .is_some_and(|page| super::policy::is_whatsapp_web(&page))
    {
        return Err("not an image from WhatsApp".to_owned());
    }
    let arguments = glib::VariantDict::new(None);
    arguments.insert("uri", uri);
    let value = view
        .call_async_javascript_function_future(
            READ_IMAGE,
            Some(&arguments.end()),
            Some(WORLD),
            None,
        )
        .await
        .map_err(|err| err.message().to_owned())?;
    if !value.is_string() {
        return Err("the page returned no image".to_owned());
    }
    decode_png(&value.to_str()).ok_or_else(|| "the page returned no PNG image".to_owned())
}

/// The PNG in `base64`, if that's what it holds and it isn't too large.
fn decode_png(base64: &str) -> Option<glib::Bytes> {
    if base64.len() > MAX_BYTES / 3 * 4 + 4 || base64.contains(|c: char| c.is_ascii_whitespace()) {
        return None;
    }
    let bytes = glib::base64_decode(base64);
    (bytes.len() <= MAX_BYTES && bytes.starts_with(PNG_SIGNATURE))
        .then(|| glib::Bytes::from_owned(bytes))
}

/// Hands `files` to the page as a paste, as if the clipboard held them.
pub async fn paste_files(view: &webkit::WebView, files: Vec<Pasted>) -> Result<(), String> {
    if !view
        .uri()
        .is_some_and(|page| super::policy::is_whatsapp_web(&page))
    {
        return Err("not on WhatsApp".to_owned());
    }
    // Encoding a large file takes a while; keep it off the main thread.
    let arguments = gtk::gio::spawn_blocking(move || files_argument(&files))
        .await
        .map_err(|_| "encoding failed".to_owned())?;
    view.call_async_javascript_function_future(PASTE_FILES, Some(&arguments), Some(WORLD), None)
        .await
        .map(drop)
        .map_err(|err| err.message().to_owned())
}

/// `files` as the script's `files` argument: a JSON list of {name, type,
/// data}. WebKit passes only plain values to scripts, not lists.
fn files_argument(files: &[Pasted]) -> glib::Variant {
    let entries: Vec<String> = files
        .iter()
        .map(|file| {
            format!(
                r#"{{"name":{},"type":{},"data":"{}"}}"#,
                json_string(&file.name),
                json_string(&file.mime_type),
                glib::base64_encode(&file.contents)
            )
        })
        .collect();
    let arguments = glib::VariantDict::new(None);
    arguments.insert("files", format!("[{}]", entries.join(",")));
    arguments.end()
}

/// `text` as a JSON string, quotes included.
fn json_string(text: &str) -> String {
    let mut json = String::with_capacity(text.len() + 2);
    json.push('"');
    for c in text.chars() {
        match c {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                json.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => json.push(c),
        }
    }
    json.push('"');
    json
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_whatsapp_images() {
        assert!(is_copyable_image("blob:https://web.whatsapp.com/1b2c-33"));
        assert!(is_copyable_image("data:image/jpeg;base64,/9j/4AAQ"));
        assert!(is_copyable_image("DATA:IMAGE/png;base64,iVBOR"));
        assert!(!is_copyable_image("blob:https://evil.example/1b2c"));
        assert!(!is_copyable_image(
            "https://pps.whatsapp.net/v/t61/photo.jpg"
        ));
        assert!(!is_copyable_image("file:///home/user/photo.jpg"));
        assert!(!is_copyable_image("data:text/html,<script>"));
        assert!(!is_copyable_image("data:"));
    }

    #[test]
    fn accepts_only_png_from_the_page() {
        let png = glib::base64_encode(b"\x89PNG\r\n\x1a\nrest");
        assert_eq!(&*decode_png(&png).unwrap(), b"\x89PNG\r\n\x1a\nrest");
        let jpeg = glib::base64_encode(b"\xff\xd8\xff\xe0JFIF");
        assert!(decode_png(&jpeg).is_none());
        assert!(decode_png("").is_none());
        assert!(decode_png("iVBORw0K GgoA").is_none());
    }

    #[test]
    fn refuses_oversized_images() {
        let too_long = "A".repeat(MAX_BYTES / 3 * 4 + 8);
        assert!(decode_png(&too_long).is_none());
    }

    #[test]
    fn passes_files_as_json() {
        let files = [Pasted {
            name: "photo \"1\".jpg".to_owned(),
            mime_type: "image/jpeg".to_owned(),
            contents: glib::Bytes::from_static(b"abc"),
        }];
        let arguments = glib::VariantDict::new(Some(&files_argument(&files)));
        let json = arguments.lookup::<String>("files").unwrap().unwrap();
        assert_eq!(
            json,
            r#"[{"name":"photo \"1\".jpg","type":"image/jpeg","data":"YWJj"}]"#
        );
    }

    #[test]
    fn escapes_json_strings() {
        assert_eq!(json_string("plain é"), r#""plain é""#);
        assert_eq!(json_string(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(
            json_string("line\nbreak\u{7}"),
            r#""line\u000abreak\u0007""#
        );
    }
}
