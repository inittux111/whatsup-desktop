# whatsup-desktop

An unofficial WhatsApp client for Linux, built as a personal-use project:
WhatsApp Web in a native GNOME window. whatsup-desktop runs WhatsApp's
official web client in WebKitGTK, inside a GTK4 + libadwaita app written in
Rust for speed and a light footprint. It was inspired by
[ZapZap](https://github.com/rafatosta/zapzap), another unofficial
WhatsApp client, written in Python.

## Use at your own risk

whatsup-desktop is an unofficial WhatsApp client for Linux, built as a
personal-use project. It isn't affiliated with, endorsed by or supported by
WhatsApp or Meta; WhatsApp is a trademark of WhatsApp LLC.

- It shows WhatsApp's own web client and doesn't use any unofficial protocol,
  but WhatsApp's terms and how they enforce them are up to WhatsApp. Nothing
  guarantees your account won't be restricted.
- WhatsApp can change WhatsApp Web at any time, and some features may stop
  working until whatsup-desktop is updated.
- It comes with no warranty, as the GPL says: you use it at your own risk.

## Features

- **Multiple accounts**, each with its own login; switch between them from the
  title bar, which shows each account's unread count
- **Tray icon** with the unread count, in green or matching your top bar, and a
  menu with Show/Hide, Settings and Quit
- **Runs in the background** when you close the window; optionally starts at
  login, hidden in the tray
- **GNOME notifications** that follow Do Not Disturb; clicking one opens the
  chat; message previews can be turned off
- **Downloads** to a folder of your choice, never overwriting a file, or asks
  where to save each one
- **Copy and paste pictures and files**: copy a picture from the full-size
  viewer (right-click → Copy Image) and paste it into another chat or app;
  paste screenshots, or files copied in Files, straight into a chat
- **Zoom**, **spell checking** in the languages you pick, and **keyboard
  shortcuts** you can change
- **Follows your GNOME theme**: WhatsApp itself takes its colours from your
  theme and accent colour, light or dark, and changes along with them; your
  own stylesheets and (if you turn them on) scripts go on top
- **Light or dark style**: follows the system, or always light or always
  dark, whatever GNOME uses
- Remembers its window size, and shows the unread count on the app icon in
  docks that support it

## Installing

whatsup-desktop officially supports Fedora 43 and newer (which ship libadwaita
1.8). Build the RPM ([see below](#building-from-source)), then:

```sh
sudo dnf install ./target/rpm/RPMS/x86_64/whatsup-desktop-*.rpm
```

The RPM provided on the [releases](https://github.com/inittux111/whatsup-desktop/releases/latest) page is only supported for Fedora 44. My
personal [COPR repo](https://copr.fedorainfracloud.org/coprs/inittux0101/pkgs-upstream-latest)
provides builds for other recent Fedora versions.

**Debian stable** (trixie) is also supported: it ships older GNOME libraries
(libadwaita 1.7, GTK 4.18), so that build falls back to a libadwaita 1.7
keyboard-shortcuts dialog, but is otherwise the same app. Build the `.deb`
([see below](#debian)), then:

```sh
sudo apt install ./target/deb/whatsup-desktop_*.deb
```

The `.deb` provided on the [releases](https://github.com/inittux111/whatsup-desktop/releases/latest) page is built for Debian 13 (trixie).

**MATE on Debian:** Debian turns off the MATE panel's support for tray icons
like whatsup-desktop's, so the icon doesn't show until it's turned back on.
In that case Settings → General → Tray and App Icon shows "Tray Icon Not
Showing", with a Turn On button that turns it back on and restarts the panel.

Arch builds too, but isn't officially supported;
see [Other systems](#other-systems-not-officially-supported).

## Settings

Open Settings from the ☰ menu, the tray icon's menu, or with `Ctrl+,`. These
are whatsup-desktop's own settings; WhatsApp's (privacy, chats, account) stay in
WhatsApp's menu. Changes apply straight away.

### General

| Setting | What it does | Default |
|---|---|---|
| Style | Light or dark: Follow System, Light or Dark, for the window and WhatsApp | Follow System |
| Run in Background | Closing the window keeps whatsup-desktop running in the tray | On |
| Start at Login | Opens whatsup-desktop when you log in | Off |
| Start in Background | When starting at login, stays hidden in the tray | On |
| Show Tray Icon | The icon in the top bar with the unread count | On |
| Coloured Tray Icon | Green tray icon; off matches the top bar's other icons | On |
| Show Unread Count on App Icon | In docks that show one, such as Dash to Dock | On |
| Show Notifications | Notifies about new messages | On |
| Show Message Previews | Shows who wrote and what in notifications | On |

### Chats

| Setting | What it does | Default |
|---|---|---|
| Zoom | Size of text and pictures, from 50% to 300% | 100% |
| Show Zoom in Menu | Zoom Out, Reset Zoom and Zoom In in the ☰ menu | On |
| Check Spelling | Underlines misspelled words while typing | On |
| Spelling Languages | The installed dictionaries to check against | System languages |
| Download Folder | Where downloaded files are saved | Downloads |
| Ask Where to Save | Asks for a folder for each download instead | Off |
| Use System Colours | WhatsApp takes its colours from your GNOME theme | On |
| Stylesheets / Scripts | Open the folders for your own CSS and JavaScript | |
| Run Custom Scripts | Runs your scripts in WhatsApp (they can read your messages) | Off |
| Reload Styles and Scripts | Applies changes to them without a restart | |

### Accounts

Add an account (it opens with a QR code to link a phone), rename one, or
remove one. Removing an account deletes its login data from this computer;
the chats stay on the phone.

### Shortcuts

Every shortcut can be changed: click it and press the new keys, Backspace
turns it off, and ↶ resets it. Shortcuts need Ctrl, Alt or Super, or an F-key,
and can't be used twice. The defaults:

| Action | Keys |
|---|---|
| Settings | `Ctrl+,` |
| Keyboard Shortcuts | `Ctrl+?` |
| Close Window | `Ctrl+W` |
| Quit | `Ctrl+Q` |
| Next / Previous Account | `Ctrl+Page Down` / `Ctrl+Page Up` |
| Zoom In / Out / Reset | `Ctrl++` / `Ctrl+-` / `Ctrl+0` |
| Reload | `Ctrl+R` or `F5` |

## Customizing

| What | Where |
|---|---|
| Stylesheets for WhatsApp Web | `~/.config/whatsup-desktop/css/*.css` |
| Scripts for WhatsApp Web (off by default) | `~/.config/whatsup-desktop/js/*.js` |
| Settings | `~/.config/whatsup-desktop/settings.ini` (or Settings, `Ctrl+,`) |

WhatsApp's colours come from your GNOME theme (Settings → Chats → Use System
Colours). Stylesheets go on top of that, in name order. Settings → Chats →
Styles and Scripts opens these folders and reloads them without a restart.

## Building from source

Builds run in a container, so no build tools end up on your system.

### Fedora

In a Fedora toolbox, which also runs the app on your desktop while developing:

```sh
toolbox create whatsup-desktop --image registry.fedoraproject.org/fedora-toolbox:44
toolbox run -c whatsup-desktop sudo dnf install -y cargo clippy rustfmt libadwaita-devel \
    webkitgtk6.0-devel rpm-build google-noto-color-emoji-fonts \
    desktop-file-utils libappstream-glib make hunspell-en-US
# Toolbox shares your home folder: keep cargo's crates in the container, not in ~/.cargo
toolbox run -c whatsup-desktop sudo sh -c 'mkdir -p /opt/cargo && chown "$SUDO_USER": /opt/cargo &&
    printf "#!/bin/sh\nexport CARGO_HOME=/opt/cargo\nexec /usr/bin/cargo \"\$@\"\n" > /usr/local/bin/cargo &&
    chmod 755 /usr/local/bin/cargo'

toolbox run -c whatsup-desktop cargo run       # run the app
toolbox run -c whatsup-desktop cargo test      # run the tests
toolbox run -c whatsup-desktop make rpm        # build the RPM into target/rpm/RPMS/
```

`make rpm` packs the committed code, so commit your changes first.

### Debian

Debian stable (trixie) builds in a podman container, which also carries the
Rust toolchain (Debian's own `rustc` is older than the gtk-rs crates need):

```sh
podman build -t whatsup-desktop-deb packaging/debian
podman run --rm -v "$PWD":/src:Z -w /src whatsup-desktop-deb make deb
```

`make deb` builds `--no-default-features`, which selects the libadwaita 1.7
shortcuts dialog, and writes the package to `target/deb/`. The crates are
fetched from crates.io during the build, so the container needs network
access. To run the tests or a one-off command in the same environment:

```sh
podman run --rm -v "$PWD":/src:Z -w /src whatsup-desktop-deb \
    cargo test --no-default-features
```

### Other systems (not officially supported)

This builds fine, but isn't tested with every release.

On **Arch**, the package recipe is in `packaging/arch`:

```sh
sudo pacman -S --needed base-devel rust libadwaita gtk4 webkitgtk-6.0 noto-fonts-emoji
cd packaging/arch && makepkg -si
```

Notifications and starting at login need the installed package, as GNOME only
shows notifications for apps with an installed `.desktop` file.

## Privacy

- Only `web.whatsapp.com` loads inside the app; other links open in your
  browser.
- WhatsApp Web may only use notifications, the camera and the microphone.
- Each account's data stays in private folders (`~/.local/share/whatsup-desktop` and
  `~/.cache/whatsup-desktop`, readable only by you).

## Licence

GPL-3.0-or-later
