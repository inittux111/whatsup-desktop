# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

WhatsUp is a WhatsApp client for Linux/GNOME, for personal use only (not distributed), packaged as an RPM for the user's own Fedora system.

- **Approach:** a Rust app that embeds WhatsApp Web in **WebKitGTK** (`webkitgtk6.0`, through the `webkit6` crate), so the account uses WhatsApp's official web client and isn't at risk of a ban. The app around it is native, built with GTK4 + libadwaita (gtk-rs): window, tray icon, notifications, settings, accounts. WebKitGTK was chosen over Chromium (CEF): it's GNOME's GTK4-native engine, already on the system and far easier to package.
- **No unofficial protocol libraries:** an earlier fully native version built on one was abandoned over the account-ban risk. Don't go back to that approach.
- **Design bar:** it should look and feel like a modern app made by a professional designer: follow the GNOME Human Interface Guidelines and libadwaita's patterns rather than custom styling.
- **Why it exists:** WhatsUp stays responsive under GPU load, where a Python-based GUI application can become unresponsive, hang and crash — most of all when attaching and sending a file in a chat. WhatsUp is built to stay performant in exactly those conditions. (ZapZap, a Python-based WhatsApp client, was the inspiration for building it.)
- **Autonomy:** make reasonable choices yourself and list your assumptions at the end. Ask the user only about decisions that are hard to reverse.
- **Workflow:** use the available coding skills while coding, including the Superpowers plugin (enabled in `.claude/settings.json`). Work as a professional software engineer and a professional UI/UX designer would: clean, well-structured, tested code, and an interface that is polished, consistent and easy to use.

## Rules from the user

- **Name:** "WhatsUp" for the app as the user sees it. App ID `app.whatsup_desktop.WhatsUp`. Everything else is lowercase `whatsup-desktop`, as Rust and Linux conventions expect: the crate, binary, packages, data folders, toolbox, repo folder (`~/Git/claude/whatsup-desktop`) and GitHub repo (`inittux111/whatsup-desktop`). Not plain `whatsup`: Fedora already has an unrelated package by that name (a cluster tool that also installs `/usr/bin/whatsup`).
- **Files stay under 500 lines.** Split by responsibility before a file reaches that.
- **Few dependencies.** Add a crate only when it's needed: when writing it ourselves would be less safe (security) or noticeably slower (performance). Prefer GTK/GLib/GIO and the standard library. Before adding a crate, check its maintenance, licence and known security advisories ([RustSec](https://rustsec.org)); ask the user before adding one with a real cost. When `cargo audit` reports a security problem in a dependency, update it right away, or replace or remove it if no fixed version exists.
- **Secure and performant:** see the sections below.
- **Modular:** small modules with one responsibility each, so new features can be added later without reworking existing code.
- **Easy to install and package:** few runtime libraries beyond what Fedora's GNOME ships, built into an RPM.

## Security

- Keep account data private (0700 folders). Never log message contents, phone numbers or QR codes.
- Treat everything from the web content as untrusted: validate every message the page sends to the app before acting on it.
- Only allow `web.whatsapp.com` inside the web view; open every other link in the user's default browser.
- Grant camera, microphone and notification permission only to WhatsApp's own origin; refuse all other permission requests.
- WebKitGTK 6 always runs web content in its sandbox; don't weaken it (e.g. with `add_path_to_sandbox`) without need. The web inspector (developer tools) is on in debug builds only.
- Sanitize download file names before saving, so a file can't be written outside the Downloads folder.

## Performance

- Nothing slow on the GTK main thread: do disk and network work asynchronously (GIO async APIs or a background thread), never in the code that draws the window.
- Open files through the GNOME file picker via the desktop portal, asynchronously, so the app never blocks while it's open. This is exactly the kind of blocking file operation that makes a GUI freeze.
- Keep the app light when it's hidden or minimised: don't redraw or do work for anything that isn't visible.
- Test the attach-and-send flow with a game running, as part of every phase.

## Architecture

- `app.rs` starts the `adw::Application` (single instance: a second launch presents the existing window; `--background` keeps the first window hidden, as starting at login does). `window.rs` builds the window (a header bar with the main menu above a toast overlay holding an `adw::ViewStack` with one page per account; with several accounts the header shows a view switcher with unread badges) and returns `window::Main`, which the feature modules (`background`, `notifications`, `downloads`) take in their `attach`.
- `accounts.rs` owns the open accounts: one `Session` (web view, stack page, unread count) per account in the settings, kept in sync as accounts are added, renamed or removed (removing one terminates its web process and deletes its data via `paths::remove_account_data`). Features never hold a single web view: they register with `Accounts::on_session`, which runs for every page, including ones added later, and read the total unread count with `connect_unread`.
- `settings/` holds the user's settings: `values.rs` is the `Values` struct and its key-file format (`~/.config/whatsup-desktop/settings.ini`, 0600, invalid values fall back to defaults), `mod.rs` loads it once at startup, saves in the background on every change and calls `connect_changed` listeners, and `autostart.rs` writes or removes `~/.config/autostart/app.whatsup_desktop.WhatsUp.desktop`. Features read `settings.get()` when they act, or listen for changes (zoom and spell checking in `web::follow_settings`). `preferences/` is the Settings dialog (only the app's own settings; WhatsApp's stay in its page; rows write their setting immediately), one file per page: `general.rs` (with `tray_hint.rs`, shown under Show Tray Icon while no StatusNotifierWatcher is on the bus: Debian's MATE turns the panel's support off with `org.mate.panel enable-sni-support=false`, so on MATE it explains the terminal and dconf Editor fixes and offers to set the key and restart the panel, with `web::original_environment` so apps started from the panel don't inherit WhatsUp's environment changes), `chats.rs` (with `downloads.rs` and the installed dictionaries from `languages.rs`) and `keys.rs`, the shortcut editor. `shortcuts.rs` is the one list of keyboard shortcuts with their defaults; the user's changes live in `Values::shortcuts` (`""` turns one off) and must pass `settings::accel::is_safe` (Ctrl, Alt or Super, or an F-key). It registers them as accelerators (shown in menus), handles them in the window's capture phase so WhatsApp Web can't swallow them while typing, and is suspended while the editor records keys. The Keyboard Shortcuts overview (`shortcuts::show`) and the editor's keycap widget (`preferences/keys.rs`) have two forms behind the `adw-shortcuts` feature: `AdwShortcutsDialog`/`AdwShortcutLabel` with it (Fedora/Arch, default), a plain dialog and `GtkShortcutLabel` without it (Debian stable's libadwaita 1.7) — keep both compiling (`cargo clippy --no-default-features`). The window size is saved on close and quit; quitting saves synchronously (`Settings::save_now`), since a background save wouldn't finish.
- `web/` owns everything about the web content and is the trust boundary: `policy.rs` decides which navigations load inside (only `https://web.whatsapp.com`) and which links may be handed to the desktop (`http`, `https`, `mailto`); `permissions.rs` allows only notifications and camera/microphone, and only for WhatsApp; `style.rs` injects the user's stylesheets; `prepare_environment` (first thing in `main`) sets the variables web processes inherit: `nvidia.rs` turns off WebKit's GL video sink on NVIDIA (its GStreamer GL conversion crashes the driver with SIGFPE; don't disable the DMA-BUF renderer instead, that makes everything judder) and `audio.rs` stops PipeWire restoring a saved per-app volume (WebKit copies it into every video, so one silent preview muted all later videos). Anything new that reacts to page content (script messages, downloads, notifications) belongs here and must go through the same checks.
- `appearance.rs` applies the light/dark Style setting (Settings → General → Appearance); the theme probes pick up the change, so WhatsApp Web follows.
- `background.rs` makes the app keep running when the window closes (it only hides) and feeds the unread count, read from WhatsApp's page title by `unread.rs`, to the tray and to `badge.rs` (the Unity launcher count that docks such as Dash to Dock show). `Ctrl+Q` and the tray's Quit really quit.
- `notifications.rs` turns WhatsApp Web's notifications into GNOME notifications (`GNotification`, so Do Not Disturb and per-app settings apply); clicking one presents the window and calls WebKit's `clicked()` so WhatsApp opens the chat. GNOME only shows them for an app whose `.desktop` file is installed, so test notifications with the RPM installed. `web/permissions.rs` pre-grants notification permission to WhatsApp's origin, so WhatsApp never asks.
- `downloads/` saves files from chats to the Downloads folder: `filename.rs` cleans the page's suggested name (no folders, hidden names or control characters) and picks an unused one (`name (1).ext`); `mod.rs` shows a toast with Open when it's saved. `web/policy.rs` turns WhatsApp's `blob:https://web.whatsapp.com/...` links and anything WebKit can't display into downloads.
- `tray/` is the tray icon, implemented directly on GIO's D-Bus (no crate): `item.rs` is `org.kde.StatusNotifierItem` (GNOME shows it through the AppIndicator extension), `menu.rs` is the `com.canonical.dbusmenu` menu (Show/Hide, Settings, Quit), and `mod.rs` registers both with `org.kde.StatusNotifierWatcher`, again whenever the watcher reappears. Callbacks into the app run from an idle handler, never inside a D-Bus call. Debug builds point the icon theme path at `data/icons`, so the icons work without installing.
- `paths.rs` is the only place that decides file locations, and creates them 0700. Each account has its own WebKit `NetworkSession` in `~/.local/share/whatsup-desktop/accounts/<id>` (cookies, login) and `~/.cache/whatsup-desktop/accounts/<id>`. Account ids (`settings/accounts.rs`) are lowercase letters, digits and dashes only (`is_id`), checked on every read, so they can never point outside those folders; the first account is `default`.
- `web/style.rs` injects, for `web.whatsapp.com` only: the app's own fixes (`app.css`, and `media.js` at document start: WebKitGTK 2.54 garbles media read from `blob:` URLs, so it swaps WhatsApp's blob videos for `data:` URLs with the same contents; `emoji.js` at document start: WebKitGTK 2.54 leaves a WebP image blank for good when it paints it before all its data has arrived, which blanked random emoji sprite sheets when the picker opened, so it hides each sheet WhatsApp adds (through `insertRule`) until an unpainted image has loaded it; and `viewer.js`: WebKit runs CSS opacity/transform animations on its compositor a frame out of step with the page, which made the media viewer flash on open, zoom and close, so app.css turns off the viewer's fade-in and transitions and `viewer.js` plays them on the page's thread), the theme colors from `theme/` (`Extras::theme`), every `.css` file in `~/.config/whatsup-desktop/css`, and, only when the user turned on Run Custom Scripts, every `.js` file in `~/.config/whatsup-desktop/js`. Files are read asynchronously and swapped in at once; `win.reload-customizations` reloads them and the pages.
- `data/` holds the desktop file, metainfo and icons; `packaging/fedora/whatsup-desktop.spec` and the `Makefile` package them.

## Theming

The app must follow the active system theme, whatever it is. Themes commonly do this by linking `~/.config/gtk-4.0/gtk.css` to the current scheme's stylesheet; the app must work with any of them, not any one theme in particular:

- The native parts are plain libadwaita, so they pick up any theme automatically; the app must not override it: use libadwaita's named colors (`@accent_bg_color`, `@window_bg_color`, ...) in app CSS, never hardcoded colors, and don't force a `GTK_THEME`. The one exception is light or dark: `appearance.rs` applies the user's Style choice (`Values::style`: Follow System, the default, or Light or Dark, in Settings → General → Appearance) through `adw::StyleManager::set_color_scheme`, and nothing else forces a color scheme. A theme linked through `gtk.css` with only one scheme's colors still wins over the choice.
- WhatsApp Web gets the theme's colors from `theme/`: invisible probe widgets each take one libadwaita color variable (`--window-bg-color`, `--accent-bg-color`, ...) as their text color, `theme/mod.rs` reads them back (again on light/dark, accent and contrast changes), and `theme/css.rs` maps them onto WhatsApp's `--WDS-*` variables. This works for Adwaita and any other libadwaita theme, without a theme-specific stylesheet (setting: Use System Colours). A user who wants a different look can still drop their own stylesheets on top.

## Development container

All building and development happens in a container: never install build tools or `-devel` packages on the host. Use a dedicated toolbox, `whatsup-desktop`, on Fedora 44 like the host, so RPMs built in it match the host's libraries. Inside it, anything development needs is allowed without asking, including installing and removing packages. Leave the user's other toolboxes (`ba`, `fc44`) alone.

```sh
toolbox create whatsup-desktop --image registry.fedoraproject.org/fedora-toolbox:44   # if it doesn't exist
toolbox run -c whatsup-desktop sudo dnf install -y cargo clippy rustfmt libadwaita-devel \
    webkitgtk6.0-devel blueprint-compiler rpm-build google-noto-color-emoji-fonts \
    desktop-file-utils libappstream-glib make hunspell-en-US
toolbox run -c whatsup-desktop sudo sh -c 'mkdir -p /opt/cargo && chown "$SUDO_USER": /opt/cargo &&
    printf "#!/bin/sh\nexport CARGO_HOME=/opt/cargo\nexec /usr/bin/cargo \"\$@\"\n" > /usr/local/bin/cargo &&
    chmod 755 /usr/local/bin/cargo'                                                 # crates stay in the container
toolbox run -c whatsup-desktop cargo install --locked cargo-audit                    # dependency security checks
```

The toolbox has its own fonts: without the emoji font, emoji show as white outlines there but not on the host. Toolbox shares the home directory and the Wayland session, so the repo is at the same path inside and `cargo run` opens the window on the host desktop.

No crates or cargo tools on the host either: since the home directory is shared, cargo's default `~/.cargo` would land on the host. The `/usr/local/bin/cargo` wrapper above (ahead of `/usr/bin` on the toolbox's `PATH`, also for `toolbox run` and `make rpm`) points `CARGO_HOME` at the container's own `/opt/cargo` (registry, `cargo install`ed tools, cargo-audit's advisory database). After recreating the toolbox, set the wrapper up again before running cargo; `~/.cargo` should never exist on the host. The Debian container keeps its own (`/opt/rust/cargo`, and `debian/cargo` during the build).

## Commands

Run everything through the toolbox:

```sh
toolbox run -c whatsup-desktop cargo build                 # build
toolbox run -c whatsup-desktop cargo run                   # run the app
toolbox run -c whatsup-desktop cargo test                  # all tests
toolbox run -c whatsup-desktop cargo test <name>           # tests whose name matches <name>
toolbox run -c whatsup-desktop cargo clippy --all-targets  # lint
toolbox run -c whatsup-desktop cargo fmt                   # format
toolbox run -c whatsup-desktop cargo audit                 # check dependencies for security advisories
```

Officially supported: Fedora, the primary hand-tested target. Debian stable is supported too, as its own `.deb` built in a podman container (it builds `--no-default-features`, using the libadwaita 1.7 shortcuts fallback); Arch builds but is only documented, not tested per release. `make install` (`PREFIX`, `DESTDIR`, `BINARY`) is the one list of installed files; every package uses it, so a new file only needs adding there (and to the spec's `%files`).

- **Fedora (RPM):** `packaging/fedora/whatsup-desktop.spec`, built in the toolbox with `toolbox run -c whatsup-desktop make rpm`. It packs the committed code (`git archive`, so commit first) and the vendored crates (`cargo vendor`, so `rpmbuild` builds offline) into `target/rpm/SOURCES` and builds into `target/rpm/RPMS/x86_64/`. The spec must `Requires: webkitgtk6.0`, `libadwaita` and `google-noto-color-emoji-fonts`. Keep `BuildRequires`/`Requires` in sync with what the toolbox needs.
- **Debian (deb):** `packaging/debian/` (standard `debian/` files, staged to the source root by `make deb`), built in the `whatsup-desktop-deb` podman container, whose `Containerfile` lives there: `podman build -t whatsup-desktop-deb packaging/debian` then `podman run --rm -v "$PWD":/src:Z -w /src whatsup-desktop-deb make deb`, which writes to `target/deb/`. Debian stable (trixie) ships libadwaita 1.7 / GTK 4.18 / WebKitGTK 2.54, so the build uses `--no-default-features` (the `adw-shortcuts` cargo feature, on by default, is what pulls in libadwaita 1.8's `AdwShortcutsDialog`/`AdwShortcutLabel`; off, `shortcuts.rs` and `preferences/keys.rs` fall back to a plain dialog and the deprecated `GtkShortcutLabel`). The container carries its Rust toolchain from rustup, since Debian's `rustc` is older than the gtk-rs crates require; the compiler version doesn't affect the `.deb`'s runtime dependencies.
- **Arch:** `packaging/arch/PKGBUILD` builds from the source tree (`makepkg -si` in that folder).
- **Versions:** a release sets the same version in `Cargo.toml`, `packaging/fedora/whatsup-desktop.spec` (plus a `%changelog` entry), `packaging/arch/PKGBUILD`, `packaging/debian/changelog` and a `<release>` in the metainfo; `make check-versions` (run by `make rpm` and CI) refuses any of them that differ from `Cargo.toml`. The spec's `Release` stays `1`: a packaging-only change (spec, `Requires`, a rebuild) ships as a new patch version with its own tag, never as a `Release` bump. The COPR repo builds from its own spec in `inittux111/pkgs-upstream-latest`, whose daily job follows the latest GitHub release and resets `Release` to 1, so this keeps both at `X.Y.Z-1`.

The user installs the package on the host (`sudo dnf install ./<file>.rpm`).

## Verification

Automated tests cover the logic, but behaviour and feel are checked by hand. When you finish a phase or feature, tell the user what to try and how it should behave. Commit after the user confirms a phase; push only when asked.

Test anything graphical or media-related on the host before handing it over, not only in the toolbox. The toolbox has no NVIDIA libraries and other GStreamer plugins, so it hides driver and rendering bugs. The user's machine has an NVIDIA GPU (proprietary driver, Wayland); some bugs only show there (the GStreamer GL crash), others on any GPU (garbled blob videos, a saved volume of 0, compositor animations a frame out of step). Check, with and without the change:

- **Picture viewer:** open, zoom in, zoom out and close a picture without flashes or judder.
- **Video:** a chat video plays with picture and sound; drawing it onto a canvas doesn't crash the web process (`coredumpctl`).
- **Rendering:** reaction emoji all show, scrolling stays smooth, and `webkit://gpu` still reports the DMA-BUF renderer.

One-frame flashes are hard to see by eye: capture the frames WebKit hands to GTK (a temporary tick callback on the web view rendering `gtk::WidgetPaintable` to textures), drive the page through the remote inspector (`WEBKIT_INSPECTOR_HTTP_SERVER`) and count frames that differ from both neighbours. Keep such debugging code out of commits, and quit test builds with `gapplication action app.whatsup_desktop.WhatsUp quit`: killing them can sign the user out of WhatsApp.
