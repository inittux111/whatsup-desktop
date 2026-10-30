//! Which pages may load inside the app. The main page is only ever WhatsApp
//! Web itself; WhatsApp's own frames inside it (such as WA Flows on
//! `flows.whatsapp.net`) may load too. Links the user follows to anywhere
//! else open in their default browser.

use gtk::glib;
use webkit::prelude::*;

const WHATSAPP_HOST: &str = "web.whatsapp.com";
const WHATSAPP_DOMAINS: [&str; 2] = ["whatsapp.com", "whatsapp.net"];

pub fn decide(
    view: &webkit::WebView,
    decision: &webkit::PolicyDecision,
    kind: webkit::PolicyDecisionType,
) {
    if let Some(response) = decision.downcast_ref::<webkit::ResponsePolicyDecision>() {
        decide_response(view, response);
    } else if let Some(navigation) = decision.downcast_ref::<webkit::NavigationPolicyDecision>() {
        decide_navigation(view, navigation, kind);
    } else {
        decision.ignore();
    }
}

/// Navigations of any frame. WebKit doesn't say here whether it's the main
/// frame, so WhatsApp's own domains pass, and `decide_response` keeps the
/// main frame on WhatsApp Web.
fn decide_navigation(
    view: &webkit::WebView,
    decision: &webkit::NavigationPolicyDecision,
    kind: webkit::PolicyDecisionType,
) {
    let Some(action) = decision.navigation_action() else {
        decision.ignore();
        return;
    };
    let uri = action.request().and_then(|request| request.uri());
    let Some(uri) = uri else {
        decision.ignore();
        return;
    };
    let new_window = kind == webkit::PolicyDecisionType::NewWindowAction;
    let whatsapp = may_load_inside(&uri) || is_whatsapp_owned(&uri) || is_whatsapp_blob(&uri);
    if !new_window && whatsapp {
        decision.use_();
    } else {
        decision.ignore();
        if new_window || action.is_user_gesture() {
            super::open_externally(view, &uri);
        }
    }
}

/// The main frame only ever shows WhatsApp Web; a link that would replace
/// it opens in the browser instead.
/// Files WebKit can't show, and WhatsApp's own file links (`blob:`), are
/// downloaded instead.
fn decide_response(view: &webkit::WebView, decision: &webkit::ResponsePolicyDecision) {
    let Some(uri) = decision.request().and_then(|request| request.uri()) else {
        decision.ignore();
        return;
    };
    let main_frame = decision.is_main_frame_main_resource();
    if !decision.is_mime_type_supported() || (main_frame && is_whatsapp_blob(&uri)) {
        decision.download();
    } else if main_frame && !may_load_inside(&uri) {
        decision.ignore();
        super::open_externally(view, &uri);
    } else {
        decision.use_();
    }
}

/// Whether `uri` may be the app's main page: WhatsApp Web over HTTPS.
pub fn may_load_inside(uri: &str) -> bool {
    uri == "about:blank" || is_whatsapp_web(uri)
}

/// Whether `uri` is WhatsApp Web itself over HTTPS, the only page given
/// permissions (an empty `about:blank` page isn't).
pub fn is_whatsapp_web(uri: &str) -> bool {
    https_host(uri).is_some_and(|host| host == WHATSAPP_HOST)
}

/// Whether `uri` is a file WhatsApp Web made in the page, such as a photo
/// the user saves: `blob:https://web.whatsapp.com/<id>`.
pub fn is_whatsapp_blob(uri: &str) -> bool {
    uri.strip_prefix("blob:")
        .is_some_and(|inner| !inner.starts_with("blob:") && is_whatsapp_web(inner))
}

/// Whether `uri` is on one of WhatsApp's own domains, over HTTPS.
fn is_whatsapp_owned(uri: &str) -> bool {
    https_host(uri).is_some_and(|host| {
        WHATSAPP_DOMAINS
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    })
}

/// The lowercase host of an `https` URI without user info, if it is one.
fn https_host(uri: &str) -> Option<String> {
    let parsed = glib::Uri::parse(uri, glib::UriFlags::NONE).ok()?;
    if parsed.scheme() != "https" || parsed.userinfo().is_some() {
        return None;
    }
    parsed.host().map(|host| host.to_ascii_lowercase())
}

/// Whether `uri` is safe to hand to the desktop: a web or mail link only,
/// never a local file or a scheme that could start some other program.
pub fn is_openable_externally(uri: &str) -> bool {
    match glib::Uri::parse(uri, glib::UriFlags::NONE) {
        Ok(parsed) => matches!(parsed.scheme().as_str(), "https" | "http" | "mailto"),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_whatsapp_web_is_the_main_page() {
        assert!(may_load_inside("https://web.whatsapp.com/"));
        assert!(may_load_inside("https://WEB.whatsapp.com/path?x=1"));
        assert!(may_load_inside("about:blank"));
        assert!(!may_load_inside("http://web.whatsapp.com/"));
        assert!(!may_load_inside("https://web.whatsapp.com.evil.example/"));
        assert!(!may_load_inside("https://evil.example/?web.whatsapp.com"));
        assert!(!may_load_inside("https://user@web.whatsapp.com/"));
        assert!(!may_load_inside("https://www.whatsapp.com/"));
        assert!(!may_load_inside(
            "https://flows.whatsapp.net/flows/cache_management"
        ));
        assert!(!may_load_inside("file:///etc/passwd"));
        assert!(!may_load_inside("not a uri"));
    }

    #[test]
    fn whatsapp_web_is_only_the_https_site() {
        assert!(is_whatsapp_web("https://web.whatsapp.com/"));
        assert!(!is_whatsapp_web("about:blank"));
        assert!(!is_whatsapp_web("http://web.whatsapp.com/"));
        assert!(!is_whatsapp_web("https://user@web.whatsapp.com/"));
        assert!(!is_whatsapp_web("https://flows.whatsapp.net/"));
    }

    #[test]
    fn whatsapp_frames_are_whatsapp_owned() {
        assert!(is_whatsapp_owned(
            "https://flows.whatsapp.net/flows/cache_management"
        ));
        assert!(is_whatsapp_owned("https://static.whatsapp.net/x.js"));
        assert!(is_whatsapp_owned("https://www.whatsapp.com/"));
        assert!(is_whatsapp_owned("https://whatsapp.net/"));
        assert!(!is_whatsapp_owned("http://flows.whatsapp.net/"));
        assert!(!is_whatsapp_owned("https://notwhatsapp.net/"));
        assert!(!is_whatsapp_owned("https://whatsapp.net.evil.example/"));
        assert!(!is_whatsapp_owned("https://user@flows.whatsapp.net/"));
        assert!(!is_whatsapp_owned("https://example.com/"));
    }

    #[test]
    fn only_whatsapp_blobs_download() {
        assert!(is_whatsapp_blob("blob:https://web.whatsapp.com/1b2c-33"));
        assert!(!is_whatsapp_blob("blob:https://evil.example/1b2c"));
        assert!(!is_whatsapp_blob("blob:http://web.whatsapp.com/1b2c"));
        assert!(!is_whatsapp_blob("blob:about:blank"));
        assert!(!is_whatsapp_blob("blob:blob:https://web.whatsapp.com/x"));
        assert!(!is_whatsapp_blob("https://web.whatsapp.com/"));
    }

    #[test]
    fn only_web_and_mail_links_open_externally() {
        assert!(is_openable_externally("https://example.com/"));
        assert!(is_openable_externally("http://example.com/"));
        assert!(is_openable_externally("mailto:someone@example.com"));
        assert!(!is_openable_externally("file:///home/user/.bashrc"));
        assert!(!is_openable_externally("javascript:alert(1)"));
        assert!(!is_openable_externally("ssh://host"));
        assert!(!is_openable_externally("garbage"));
    }
}
