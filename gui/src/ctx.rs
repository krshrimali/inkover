//! State shared by every page.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

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
    /// When we hid the window to keep it out of a capture.
    pub hidden_since: Cell<Option<Instant>>,
    /// Where the window was just before hiding: (monitor, x, y, w, h).
    pub last_place: RefCell<(Option<String>, Option<Rect>)>,
}

pub type Rect = (i32, i32, i32, i32);

fn parse_geom(g: &str) -> Option<Rect> {
    // slurp: "X,Y WxH"
    let (pos, size) = g.trim().split_once(' ')?;
    let (x, y) = pos.split_once(',')?;
    let (w, h) = size.split_once('x')?;
    Some((x.parse().ok()?, y.parse().ok()?, w.parse().ok()?, h.parse().ok()?))
}

fn intersects(a: Rect, b: Rect) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
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

    /// Connector of the monitor this window is on.
    pub fn own_monitor(&self) -> Option<String> {
        let surface = self.win.surface()?;
        let mon = surface.display().monitor_at_surface(&surface)?;
        mon.connector().map(|c| c.to_string())
    }

    /// Global window rectangle. Wayland hides this from clients, so ask Hyprland.
    pub fn own_rect(&self) -> Option<Rect> {
        let out = std::process::Command::new("hyprctl").args(["-j", "clients"]).output().ok()?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
        let me = std::process::id() as u64;
        let c = v.as_array()?.iter().find(|c| c["pid"].as_u64() == Some(me))?;
        Some((
            c["at"][0].as_i64()? as i32,
            c["at"][1].as_i64()? as i32,
            c["size"][0].as_i64()? as i32,
            c["size"][1].as_i64()? as i32,
        ))
    }

    /// Hide the window so it isn't captured; remembers where it was.
    pub fn hide_for_capture(&self) {
        if self.hidden_since.get().is_some() || !self.win.is_visible() {
            return;
        }
        *self.last_place.borrow_mut() = (self.own_monitor(), self.own_rect());
        self.win.set_visible(false);
        self.hidden_since.set(Some(Instant::now()));
    }

    pub fn restore(&self) {
        if self.hidden_since.take().is_some() {
            self.win.set_visible(true);
            self.win.present();
        }
    }

    /// Would this window end up in the running recording? Uses the last known
    /// placement; unknown placement counts as "yes" to stay on the safe side.
    pub fn in_recording(&self, st: &Status) -> bool {
        let (mon, rect) = self.last_place.borrow().clone();
        match st.recording_target.as_deref() {
            Some("region") => match (rect, st.recording_geom.as_deref().and_then(parse_geom)) {
                (Some(r), Some(g)) => intersects(r, g),
                _ => true,
            },
            Some(target) => mon.as_deref().is_none_or(|m| m == target),
            None => true,
        }
    }

    /// Called on every status poll: hide when a recording would capture us,
    /// come back when it (or a screenshot / cancelled selection) is over.
    pub fn sync_visibility(&self, st: Option<&Status>) {
        match (st, self.hidden_since.get()) {
            (Some(s), None) if s.recording.is_some() && self.win.is_visible() => {
                // Recording started elsewhere (toolbar, hotkey, CLI).
                *self.last_place.borrow_mut() = (self.own_monitor(), self.own_rect());
                if self.in_recording(s) {
                    self.hide_for_capture_now();
                }
            }
            (Some(s), Some(_)) if s.recording.is_some() => {
                if !self.in_recording(s) {
                    self.restore(); // e.g. a region on another part of the screen
                }
            }
            (Some(s), Some(t)) => {
                if !s.capturing && t.elapsed() > Duration::from_millis(700) {
                    self.restore();
                }
            }
            (None, Some(t)) => {
                // Overlay still starting up (or gone): give it a moment.
                if t.elapsed() > Duration::from_secs(6) {
                    self.restore();
                }
            }
            _ => {}
        }
    }

    fn hide_for_capture_now(&self) {
        self.win.set_visible(false);
        self.hidden_since.set(Some(Instant::now()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_geometry_and_overlap() {
        assert_eq!(parse_geom("4000,320 1080x800"), Some((4000, 320, 1080, 800)));
        assert_eq!(parse_geom("garbage"), None);
        let win = (4220, 908, 640, 780);
        assert!(intersects(win, parse_geom("4000,320 1080x800").unwrap()));   // overlaps top
        assert!(!intersects(win, parse_geom("0,0 1440x2560").unwrap()));      // other monitor
        assert!(!intersects(win, parse_geom("4860,908 100x100").unwrap()));   // touching edge only
    }
}
