# Rust crates come from a vendored tarball, so the build runs offline.
%global debug_package %{nil}

Name:           whatsup-desktop
Version:        1.3.0
Release:        1%{?dist}
Summary:        WhatsApp Web for GNOME
License:        GPL-3.0-or-later
URL:            https://github.com/inittux111/whatsup-desktop
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz
# Early builds were packaged as whatsup, a name Fedora already uses for a
# cluster tool; the version bound leaves that one alone.
Obsoletes:      whatsup < 1.1

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  pkgconfig(libadwaita-1)
BuildRequires:  pkgconfig(webkitgtk-6.0)
BuildRequires:  desktop-file-utils
BuildRequires:  make

BuildRequires:  libappstream-glib
Requires:       libadwaita
Requires:       webkitgtk6.0
Requires:       google-noto-color-emoji-fonts

%description
WhatsUp runs WhatsApp Web in a native GNOME window that stays responsive,
follows your theme and keeps your account data private.

%prep
%autosetup -n %{name}-%{version}
tar -xzf %{SOURCE1}
mkdir -p .cargo
cat > .cargo/config.toml <<'CONF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
CONF

%build
cargo build --release --offline --locked

%check
cargo test --release --offline --locked
desktop-file-validate data/app.whatsup_desktop.WhatsUp.desktop
appstream-util validate-relax --nonet data/app.whatsup_desktop.WhatsUp.metainfo.xml

%install
make install DESTDIR=%{buildroot} PREFIX=%{_prefix}

%files
%{_bindir}/%{name}
%{_datadir}/applications/app.whatsup_desktop.WhatsUp.desktop
%{_datadir}/metainfo/app.whatsup_desktop.WhatsUp.metainfo.xml
%{_datadir}/icons/hicolor/scalable/apps/app.whatsup_desktop.WhatsUp.svg
%{_datadir}/icons/hicolor/symbolic/apps/app.whatsup_desktop.WhatsUp-symbolic.svg
%{_datadir}/icons/hicolor/symbolic/apps/app.whatsup_desktop.WhatsUp-unread-symbolic.svg
%{_datadir}/icons/hicolor/scalable/apps/app.whatsup_desktop.WhatsUp-tray.svg
%{_datadir}/icons/hicolor/scalable/apps/app.whatsup_desktop.WhatsUp-tray-unread.svg

%changelog
* Fri Oct 30 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.3.0-1
- Copy a picture from the picture viewer (right-click, Copy Image) and
  paste it into another chat or app
- Paste files copied in Files into a chat to attach them

* Sat Oct 24 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.2.1-1
- Fix emoji missing from the emoji picker, messages and reactions
- Show unread chats on the tray icon in trays that ignore the attention
  icon, such as Debian's MATE panel

* Sat Oct 24 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.2.0-1
- Add a light or dark Style setting (Settings, General, Appearance)
- Explain a missing tray icon in Settings; on Debian's MATE, turn the
  panel's tray support on

* Mon Oct 19 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.1.0-1
- Add Debian stable (trixie) support: a .deb built in a podman container,
  with the keyboard shortcuts dialog falling back to libadwaita 1.7

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.6-1
- Drop Debian, Ubuntu and openSUSE packaging; Fedora and Arch only

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.5-1
- WhatsUp recovers more smoothly when WhatsApp Web's page crashes: it
  backs off instead of reloading in a tight loop, and keeps retrying
  until it recovers
- Tightened file permissions for account folders and the settings file

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.4-1
- Starting at login no longer brings up the window when WhatsUp is
  already running
- The tray menu's Hide WhatsUp hides the window even when it isn't in
  front, and the tray menu follows the dbusmenu specification
- The dock badge comes back after unlocking the screen
- Settings are always saved readable only by you, and quickly turning
  Start at Login on and off can't leave it on
- Download names can't become hidden files or disguise their extension,
  and keep their extension when shortened
- Fixes for reloading custom stylesheets and scripts, the picture
  viewer's zoom animation and the Accounts settings page

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.3-1
- The tray icon now looks like the app icon: a green speech bubble with
  a white rising chevron, instead of a phone handset
- The app icon uses the tray's flat green and white

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.2-1
- Videos in chats play, with sound: WebKitGTK garbled videos it read
  from memory, and a saved volume of 0 muted every later video
- Smooth pictures and animations on NVIDIA again, with every reaction
  emoji shown: only WebKit's GL video sink is turned off there now
- Pictures no longer flash when they open, zoom or close

* Sun Oct 18 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.1-1
- Release the NVIDIA graphics fix of 1.0.0-3 as its own version, so the
  RPM, deb, COPR build and GitHub release all match

* Sat Oct 17 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.0-3
- Fix WhatsApp Web freezing on NVIDIA graphics: videos drawn onto a
  canvas crashed the web page; it now also reloads after any crash

* Sat Oct 17 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.0-2
- Replace the early builds packaged as whatsup
- New app ID: app.whatsup_desktop.WhatsUp

* Sat Oct 17 2026 Maarten van der Rijst <claude@feedmebits.nl> - 1.0.0-1
- First release: WhatsApp Web in a native GNOME window, with multiple
  accounts, a tray icon, notifications, downloads, settings, editable
  shortcuts, and colours from the system theme

* Sat Oct 17 2026 Maarten van der Rijst <claude@feedmebits.nl> - 0.1.0-1
- First package
