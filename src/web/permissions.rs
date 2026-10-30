//! Permission requests from web content: WhatsApp Web may show notifications
//! and use the camera and microphone for calls; everything else is refused.

use gtk::glib;
use webkit::prelude::*;

/// The kinds of permission requests WhatsUp tells apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Notifications,
    /// Camera and microphone.
    UserMedia,
    /// Anything else (location, pointer lock, storage, ...).
    Other,
}

pub fn decide(view: &webkit::WebView, request: &webkit::PermissionRequest) -> bool {
    let kind = if request.is::<webkit::NotificationPermissionRequest>() {
        Kind::Notifications
    } else if request.is::<webkit::UserMediaPermissionRequest>() {
        Kind::UserMedia
    } else {
        Kind::Other
    };
    if allows(view.uri().as_deref(), kind) {
        request.allow();
    } else {
        request.deny();
        glib::g_debug!(
            "whatsup-desktop",
            "refused a {} permission request",
            request.type_().name()
        );
    }
    true
}

/// Whether a request of `kind` from a page showing `page_uri` is allowed:
/// only notifications and camera/microphone, and only on WhatsApp Web.
fn allows(page_uri: Option<&str>, kind: Kind) -> bool {
    let from_whatsapp = page_uri.is_some_and(super::policy::is_whatsapp_web);
    from_whatsapp && matches!(kind, Kind::Notifications | Kind::UserMedia)
}

/// Tells WebKit up front that WhatsApp Web may show notifications, so
/// WhatsApp doesn't ask for them again on every start.
pub fn grant_notifications(context: &webkit::WebContext) {
    context.connect_initialize_notification_permissions(|context| {
        let whatsapp = webkit::SecurityOrigin::for_uri(super::WHATSAPP_URL);
        context.initialize_notification_permissions(&[&whatsapp], &[]);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHATSAPP: Option<&str> = Some("https://web.whatsapp.com/");

    #[test]
    fn whatsapp_gets_notifications_and_calls() {
        assert!(allows(WHATSAPP, Kind::Notifications));
        assert!(allows(WHATSAPP, Kind::UserMedia));
    }

    #[test]
    fn whatsapp_gets_nothing_else() {
        assert!(!allows(WHATSAPP, Kind::Other));
    }

    #[test]
    fn other_pages_get_nothing() {
        for uri in [
            None,
            Some("about:blank"),
            Some("http://web.whatsapp.com/"),
            Some("https://web.whatsapp.com.evil.example/"),
            Some("https://evil.example/?web.whatsapp.com"),
            Some("https://flows.whatsapp.net/"),
            Some("file:///home/user/page.html"),
        ] {
            for kind in [Kind::Notifications, Kind::UserMedia, Kind::Other] {
                assert!(!allows(uri, kind), "{uri:?} got {kind:?}");
            }
        }
    }
}
