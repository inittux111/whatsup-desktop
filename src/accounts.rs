//! The open accounts: one WhatsApp Web page per account in the window's
//! stack, kept in step with the settings as the user adds, renames and
//! removes accounts. Features hook into every page with `on_session`,
//! including pages added later.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::glib;
use webkit::prelude::*;

use crate::settings::accounts::Account;
use crate::settings::{Settings, Values};
use crate::{paths, unread, web};

/// One account's page.
pub struct Session {
    pub id: String,
    pub view: webkit::WebView,
    page: adw::ViewStackPage,
    unread: Cell<u32>,
}

impl Session {
    pub fn name(&self) -> String {
        self.page.title().map(|t| t.to_string()).unwrap_or_default()
    }
}

type Hook = Rc<dyn Fn(&Rc<Session>)>;
type UnreadListener = Rc<dyn Fn(u32)>;

#[derive(Clone)]
pub struct Accounts(Rc<Inner>);

struct Inner {
    stack: adw::ViewStack,
    settings: Settings,
    sessions: RefCell<Vec<Rc<Session>>>,
    hooks: RefCell<Vec<Hook>>,
    unread_listeners: RefCell<Vec<UnreadListener>>,
    scripts: Cell<bool>,
    /// Whether WhatsApp Web takes the system theme's colours.
    system_colors: Cell<bool>,
    /// The system theme's colours as a stylesheet, once read.
    theme: RefCell<Option<String>>,
    /// Set while `sync` runs: showing a page writes the settings, which
    /// would call `sync` again halfway through.
    syncing: Cell<bool>,
}

impl Accounts {
    /// Opens a page for every account in the settings, showing the active one.
    pub fn new(stack: &adw::ViewStack, settings: &Settings) -> Self {
        let values = settings.get();
        let accounts = Self(Rc::new(Inner {
            stack: stack.clone(),
            settings: settings.clone(),
            sessions: RefCell::default(),
            hooks: RefCell::default(),
            unread_listeners: RefCell::default(),
            scripts: Cell::new(values.custom_scripts),
            system_colors: Cell::new(values.system_colors),
            theme: RefCell::default(),
            syncing: Cell::new(false),
        }));
        accounts.sync(&values);
        accounts.show(&values.active_account);

        let weak = Rc::downgrade(&accounts.0);
        settings.connect_changed(move |values| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).sync(values);
            }
        });
        let weak = Rc::downgrade(&accounts.0);
        stack.connect_visible_child_name_notify(move |stack| {
            let (Some(inner), Some(id)) = (weak.upgrade(), stack.visible_child_name()) else {
                return;
            };
            inner.settings.update(|v| v.active_account = id.to_string());
        });
        accounts
    }

    /// Calls `f` for every account's page, now and as accounts are added.
    pub fn on_session(&self, f: impl Fn(&Rc<Session>) + 'static) {
        let sessions = self.sessions();
        for session in &sessions {
            f(session);
        }
        self.0.hooks.borrow_mut().push(Rc::new(f));
    }

    /// Calls `f` with the unread count of all accounts together, when it changes.
    pub fn connect_unread(&self, f: impl Fn(u32) + 'static) {
        self.0.unread_listeners.borrow_mut().push(Rc::new(f));
    }

    pub fn sessions(&self) -> Vec<Rc<Session>> {
        self.0.sessions.borrow().clone()
    }

    pub fn current(&self) -> Option<Rc<Session>> {
        let id = self.0.stack.visible_child_name()?;
        self.find(&id)
    }

    pub fn find(&self, id: &str) -> Option<Rc<Session>> {
        self.sessions().into_iter().find(|s| s.id == id)
    }

    /// Whether there's more than one account, so names should be shown.
    pub fn several(&self) -> bool {
        self.0.sessions.borrow().len() > 1
    }

    pub fn show(&self, id: &str) {
        if self.find(id).is_some() {
            self.0.stack.set_visible_child_name(id);
        }
    }

    /// Shows the next (`step` 1) or previous (`step` -1) account, wrapping around.
    pub fn cycle(&self, step: isize) {
        let sessions = self.sessions();
        let Some(current) = self.current() else {
            return;
        };
        let Some(index) = sessions.iter().position(|s| s.id == current.id) else {
            return;
        };
        self.show(&sessions[cycled(index, step, sessions.len())].id);
    }

    pub fn total_unread(&self) -> u32 {
        self.sessions()
            .iter()
            .fold(0u32, |sum, s| sum.saturating_add(s.unread.get()))
    }

    /// Opens pages for new accounts, closes removed ones (deleting their
    /// data, as the user confirmed), renames the rest, and reloads every
    /// page when custom scripts were turned on or off.
    fn sync(&self, values: &Values) {
        if self.0.syncing.replace(true) {
            return;
        }
        self.sync_now(values);
        self.0.syncing.set(false);
    }

    fn sync_now(&self, values: &Values) {
        let open: Vec<Account> = self
            .sessions()
            .iter()
            .map(|s| Account {
                id: s.id.clone(),
                name: s.name(),
            })
            .collect();
        let changes = plan(&open, &values.accounts);
        for id in changes.close {
            if let Some(session) = self.find(id) {
                self.close(&session);
            }
        }
        for account in changes.rename {
            if let Some(session) = self.find(&account.id) {
                session.page.set_title(Some(&account.name));
            }
        }
        for account in changes.open {
            self.open(account);
        }
        if self.0.scripts.replace(values.custom_scripts) != values.custom_scripts {
            self.reload_customizations();
        }
        if self.0.system_colors.replace(values.system_colors) != values.system_colors {
            self.restyle();
        }
        // A new account becomes the active one, so its QR code shows.
        if self.current().is_none_or(|s| s.id != values.active_account) {
            self.show(&values.active_account);
        }
    }

    /// Reloads every page with the current stylesheets and scripts.
    pub fn reload_customizations(&self) {
        for session in self.sessions() {
            web::reload_customizations(&session.view, self.extras());
        }
    }

    /// Gives every page the theme's new colours, as `theme::follow` reads them.
    pub fn set_theme(&self, css: String) {
        if self.0.theme.borrow().as_deref() == Some(css.as_str()) {
            return;
        }
        self.0.theme.replace(Some(css));
        if self.0.system_colors.get() {
            self.restyle();
        }
    }

    fn restyle(&self) {
        for session in self.sessions() {
            web::restyle(&session.view, self.extras());
        }
    }

    fn extras(&self) -> web::Extras {
        web::Extras {
            scripts: self.0.scripts.get(),
            theme: self
                .0
                .system_colors
                .get()
                .then(|| self.0.theme.borrow().clone())
                .flatten(),
        }
    }

    fn open(&self, account: &Account) {
        let dirs = match paths::account_dirs(&account.id) {
            Ok(dirs) => dirs,
            Err(err) => {
                glib::g_warning!(
                    "whatsup-desktop",
                    "cannot prepare an account's folder: {err}"
                );
                return;
            }
        };
        let view = web::build(&dirs, self.extras());
        web::follow_settings(&view, &self.0.settings);
        let page = self
            .0
            .stack
            .add_titled(&view, Some(&account.id), &account.name);
        let session = Rc::new(Session {
            id: account.id.clone(),
            view: view.clone(),
            page,
            unread: Cell::new(0),
        });
        let weak = Rc::downgrade(&self.0);
        let for_title = Rc::downgrade(&session);
        view.connect_title_notify(move |view| {
            let (Some(inner), Some(session)) = (weak.upgrade(), for_title.upgrade()) else {
                return;
            };
            let count = view
                .title()
                .map_or(0, |title| unread::count_from_title(&title));
            if session.unread.replace(count) != count {
                session.page.set_badge_number(count);
                session.page.set_needs_attention(count > 0);
                Self(inner).announce_unread();
            }
        });
        self.0.sessions.borrow_mut().push(session.clone());
        let hooks = self.0.hooks.borrow().clone();
        for hook in hooks {
            hook(&session);
        }
    }

    fn close(&self, session: &Rc<Session>) {
        self.0.sessions.borrow_mut().retain(|s| s.id != session.id);
        self.0.stack.remove(&session.view);
        session.view.terminate_web_process();
        paths::remove_account_data(&session.id);
        self.announce_unread();
    }

    fn announce_unread(&self) {
        let total = self.total_unread();
        let listeners = self.0.unread_listeners.borrow().clone();
        for listener in listeners {
            listener(total);
        }
    }
}

/// The index `step` places after `index` in a list of `len`, wrapping around.
fn cycled(index: usize, step: isize, len: usize) -> usize {
    (index as isize + step).rem_euclid(len as isize) as usize
}

/// What `sync` changes to go from the `open` pages to the `wanted` accounts.
#[derive(Debug, Default, PartialEq)]
struct Plan<'a> {
    /// Pages whose account was removed; closing one deletes its data.
    close: Vec<&'a str>,
    rename: Vec<&'a Account>,
    open: Vec<&'a Account>,
}

fn plan<'a>(open: &'a [Account], wanted: &'a [Account]) -> Plan<'a> {
    let mut changes = Plan::default();
    for page in open {
        if wanted.iter().all(|a| a.id != page.id) {
            changes.close.push(&page.id);
        }
    }
    for account in wanted {
        match open.iter().find(|page| page.id == account.id) {
            Some(page) if page.name != account.name => changes.rename.push(account),
            Some(_) => {}
            None => changes.open.push(account),
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(id: &str, name: &str) -> Account {
        Account {
            id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn cycling_wraps_both_ways() {
        assert_eq!(cycled(0, 1, 3), 1);
        assert_eq!(cycled(2, 1, 3), 0);
        assert_eq!(cycled(0, -1, 3), 2);
        assert_eq!(cycled(0, 1, 1), 0);
    }

    #[test]
    fn unchanged_accounts_change_nothing() {
        let accounts = [account("default", "WhatsApp"), account("work", "Work")];
        assert_eq!(plan(&accounts, &accounts), Plan::default());
    }

    #[test]
    fn opens_renames_and_closes() {
        let open = [account("default", "WhatsApp"), account("work", "Work")];
        let wanted = [account("default", "Home"), account("club", "Club")];
        let changes = plan(&open, &wanted);
        assert_eq!(changes.close, ["work"]);
        assert_eq!(changes.rename, [&wanted[0]]);
        assert_eq!(changes.open, [&wanted[1]]);
    }

    #[test]
    fn closes_only_removed_accounts() {
        // Closing deletes an account's data, so a rename or a reorder must
        // never close anything.
        let open = [account("default", "WhatsApp"), account("work", "Work")];
        let wanted = [account("work", "Job"), account("default", "WhatsApp")];
        assert!(plan(&open, &wanted).close.is_empty());
        assert_eq!(plan(&open, &[]).close, ["default", "work"]);
        assert!(plan(&[], &open).close.is_empty());
    }
}
