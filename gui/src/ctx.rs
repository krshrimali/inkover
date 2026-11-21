//! State shared by every page.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::ipc::{self, Status};
use crate::settings::Settings;

pub struct Ctx {
    pub win: adw::ApplicationWindow,
    pub toasts: adw::ToastOverlay,
    pub settings: Settings,
    pub status: RefCell<Option<Status>>,
    /// True while pages are syncing widgets from status, so handlers don't echo commands back.
    pub updating: Cell<bool>,
}

pub type CtxRef = Rc<Ctx>;

impl Ctx {
    pub fn toast(&self, msg: &str) {
        let t = adw::Toast::new(msg);
        t.set_timeout(3);
        self.toasts.add_toast(t);
    }

    /// Send a command. Starts the daemon via the CLI if it's not running.
    pub fn cmd(&self, args: &[&str]) {
        if self.updating.get() {
            return;
        }
        match ipc::command(args) {
            Some(r) if r.starts_with("error") => self.toast(r.trim_start_matches("error: ")),
            _ => {}
        }
    }

    /// Send a command only if the overlay is running (no auto-start).
    pub fn live(&self, args: &[&str]) {
        if self.updating.get() {
            return;
        }
        if let Some(r) = ipc::send(args) {
            if r.starts_with("error") {
                self.toast(r.trim_start_matches("error: "));
            }
        }
    }

    /// Save config.toml and ask a running overlay to reload it.
    pub fn save_settings(&self) {
        if self.updating.get() {
            return;
        }
        if let Err(e) = self.settings.save() {
            self.toast(&format!("Couldn't save config: {e}"));
            return;
        }
        if let Some(r) = ipc::send(&["reload"]) {
            if let Some(rest) = r.split_once("restart inkover to apply: ") {
                self.toast(&format!("Saved. Restart the overlay to apply: {}", rest.1));
            } else if r.starts_with("error") {
                self.toast(&r);
            }
        }
    }

    pub fn monitors(&self) -> Vec<String> {
        let mut out = vec![];
        if let Some(display) = gtk::gdk::Display::default() {
            let mons = display.monitors();
            for i in 0..mons.n_items() {
                if let Some(m) = mons.item(i).and_downcast::<gtk::gdk::Monitor>() {
                    if let Some(c) = m.connector() {
                        out.push(c.to_string());
                    }
                }
            }
        }
        out
    }

    pub fn open_path(&self, path: &std::path::Path, folder: bool) {
        let _ = std::fs::create_dir_all(if folder { path } else { path.parent().unwrap_or(path) });
        let file = gtk::gio::File::for_path(path);
        gtk::FileLauncher::new(Some(&file)).launch(Some(&self.win), gtk::gio::Cancellable::NONE, |_| {});
    }

    pub fn reveal_path(&self, path: &std::path::Path) {
        let file = gtk::gio::File::for_path(path);
        let launcher = gtk::FileLauncher::new(Some(&file));
        launcher.open_containing_folder(Some(&self.win), gtk::gio::Cancellable::NONE, |_| {});
    }
}

pub fn button(label: &str, icon: Option<&str>, tooltip: &str) -> gtk::Button {
    let b = match icon {
        Some(i) => {
            let content = adw::ButtonContent::builder().label(label).icon_name(i).build();
            gtk::Button::builder().child(&content).build()
        }
        None => gtk::Button::with_label(label),
    };
    b.set_tooltip_text(Some(tooltip));
    b.set_valign(gtk::Align::Center);
    b
}
