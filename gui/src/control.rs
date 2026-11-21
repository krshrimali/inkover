//! Control page: overlay on/off, draw/pass, capture & recording, ink actions.
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;

use crate::ctx::{button, CtxRef};
use crate::ipc::Status;
use crate::settings::expand;

pub struct ControlPage {
    pub widget: adw::PreferencesPage,
    overlay_row: adw::ActionRow,
    start_btn: gtk::Button,
    stop_btn: gtk::Button,
    draw_row: adw::SwitchRow,
    toolbar_row: adw::SwitchRow,
    board_row: adw::ComboRow,
    rec_row: adw::ActionRow,
    rec_full: gtk::Button,
    rec_region: gtk::Button,
    rec_stop: gtk::Button,
    ink_group: adw::PreferencesGroup,
}

const AUDIO: [&str; 4] = ["none", "mic", "desktop", "both"];

impl ControlPage {
    pub fn new(ctx: &CtxRef) -> Rc<Self> {
        let page = adw::PreferencesPage::new();

        // ---------------------------------------------------------- overlay
        let g = adw::PreferencesGroup::builder().title("Overlay").build();
        let overlay_row = adw::ActionRow::builder().title("Overlay").subtitle("Not running").build();
        let start_btn = button("Start", Some("media-playback-start-symbolic"), "Start drawing (Super+Alt+D)");
        start_btn.add_css_class("suggested-action");
        let stop_btn = button("Quit", Some("window-close-symbolic"), "Close the overlay (ink is discarded)");
        stop_btn.add_css_class("destructive-action");
        overlay_row.add_suffix(&start_btn);
        overlay_row.add_suffix(&stop_btn);
        g.add(&overlay_row);

        let draw_row = adw::SwitchRow::builder()
            .title("Draw mode")
            .subtitle("Off = pass mode: ink stays, the screen is usable")
            .build();
        let toolbar_row = adw::SwitchRow::builder().title("Show on-screen toolbar").build();
        let board_row = adw::ComboRow::builder()
            .title("Background")
            .model(&gtk::StringList::new(&["Transparent (live screen)", "Whiteboard", "Blackboard"]))
            .build();
        g.add(&draw_row);
        g.add(&toolbar_row);
        g.add(&board_row);
        page.add(&g);

        // ---------------------------------------------------------- capture
        let g = adw::PreferencesGroup::builder()
            .title("Capture")
            .description("Screenshots and recordings include the live screen and your ink")
            .build();
        let mut mons = vec!["Focused monitor".to_string()];
        mons.extend(ctx.monitors());
        let mon_strs: Vec<&str> = mons.iter().map(|s| s.as_str()).collect();
        let target_row = adw::ComboRow::builder()
            .title("Monitor")
            .model(&gtk::StringList::new(&mon_strs))
            .build();
        g.add(&target_row);

        let shot_row = adw::ActionRow::builder().title("Screenshot").subtitle("Saved and copied to the clipboard").build();
        let shot_mon = button("Monitor", Some("video-display-symbolic"), "Screenshot the selected monitor");
        let shot_reg = button("Region", Some("selection-mode-symbolic"), "Drag to select an area");
        let export = button("Ink only", Some("document-save-symbolic"), "Export the ink as a transparent PNG");
        for b in [&shot_mon, &shot_reg, &export] {
            shot_row.add_suffix(b);
        }
        g.add(&shot_row);

        let rec_row = adw::ActionRow::builder().title("Recording").subtitle("Not recording").build();
        let rec_full = button("Monitor", Some("media-record-symbolic"), "Record the selected monitor");
        let rec_region = button("Region", Some("selection-mode-symbolic"), "Record a selected area");
        let rec_stop = button("Stop", Some("media-playback-stop-symbolic"), "Stop and save the recording");
        rec_stop.add_css_class("destructive-action");
        rec_stop.set_visible(false);
        for b in [&rec_full, &rec_region, &rec_stop] {
            rec_row.add_suffix(b);
        }
        g.add(&rec_row);

        let audio_now = ctx.settings.str("record_audio", "none");
        let audio_row = adw::ComboRow::builder()
            .title("Recording audio")
            .model(&gtk::StringList::new(&["None", "Microphone", "Desktop sound", "Mic + desktop"]))
            .selected(AUDIO.iter().position(|a| *a == audio_now).unwrap_or(0) as u32)
            .build();
        g.add(&audio_row);

        let folders = adw::ActionRow::builder().title("Folders").build();
        let open_shots = button("Screenshots", Some("folder-pictures-symbolic"), "Open the screenshots folder");
        let open_vids = button("Videos", Some("folder-videos-symbolic"), "Open the recordings folder");
        folders.add_suffix(&open_shots);
        folders.add_suffix(&open_vids);
        g.add(&folders);
        page.add(&g);

        // -------------------------------------------------------------- ink
        let ink_group = adw::PreferencesGroup::builder().title("Ink").build();
        let ink_row = adw::ActionRow::builder().title("Edit").build();
        let undo = button("", Some("edit-undo-symbolic"), "Undo");
        let redo = button("", Some("edit-redo-symbolic"), "Redo");
        let clear_mon = button("Clear monitor", None, "Clear ink on the selected monitor");
        let clear_all = button("Clear all", Some("edit-clear-all-symbolic"), "Clear ink on every monitor");
        clear_all.add_css_class("destructive-action");
        for b in [&undo, &redo, &clear_mon, &clear_all] {
            ink_row.add_suffix(b);
        }
        ink_group.add(&ink_row);
        page.add(&ink_group);

        // ------------------------------------------------------- handlers
        let target = {
            let row = target_row.clone();
            let mons = mons.clone();
            move || -> Option<String> {
                let i = row.selected() as usize;
                (i > 0).then(|| mons.get(i).cloned()).flatten()
            }
        };
        let with_target = {
            let ctx = ctx.clone();
            let target = target.clone();
            move |base: &[&str]| {
                let mut args: Vec<String> = base.iter().map(|s| s.to_string()).collect();
                if let Some(t) = target() {
                    args.push(t);
                }
                let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
                ctx.cmd(&refs);
            }
        };

        let c = ctx.clone();
        start_btn.connect_clicked(move |_| c.cmd(&["draw"]));
        let c = ctx.clone();
        stop_btn.connect_clicked(move |_| c.live(&["quit"]));
        let c = ctx.clone();
        draw_row.connect_active_notify(move |r| c.live(&[if r.is_active() { "draw" } else { "pass" }]));
        let c = ctx.clone();
        toolbar_row.connect_active_notify(move |r| {
            let current = c.status.borrow().as_ref().map(|s| s.toolbar_visible);
            if current.is_some_and(|v| v != r.is_active()) {
                c.live(&["toolbar"]);
            }
        });
        let c = ctx.clone();
        board_row.connect_selected_notify(move |r| {
            let now = c.status.borrow().as_ref().map(|s| s.board as u32);
            if let Some(now) = now {
                for _ in 0..(r.selected() + 3 - now) % 3 {
                    c.live(&["board"]);
                }
            }
        });

        // Screenshots and recordings: if this window would be captured, hide it
        // first. The status poll (Ctx::sync_visibility) brings it back afterwards.
        let capture = {
            let ctx = ctx.clone();
            let with_target = with_target.clone();
            let target = target.clone();
            move |args: &'static [&'static str]| {
                let region = args.contains(&"region");
                // A region could cover us; a monitor only matters if it's ours.
                // "Focused monitor" is ours, since we were just clicked.
                let ours = region || target().is_none_or(|t| Some(t) == ctx.own_monitor());
                if !ours {
                    with_target(args);
                    return;
                }
                ctx.hide_for_capture();
                let with_target = with_target.clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
                    with_target(args);
                });
            }
        };
        let s = capture.clone();
        shot_mon.connect_clicked(move |_| s(&["screenshot", "full"]));
        let s = capture.clone();
        shot_reg.connect_clicked(move |_| s(&["screenshot", "region"]));
        let w = with_target.clone();
        export.connect_clicked(move |_| w(&["export"]));
        let s = capture.clone();
        rec_full.connect_clicked(move |_| s(&["record", "full"]));
        let s = capture.clone();
        rec_region.connect_clicked(move |_| s(&["record", "region"]));
        let c = ctx.clone();
        rec_stop.connect_clicked(move |_| c.live(&["record"]));

        let c = ctx.clone();
        audio_row.connect_selected_notify(move |r| {
            c.settings.set_str("record_audio", AUDIO[r.selected() as usize]);
            c.save_settings();
        });
        let c = ctx.clone();
        open_shots.connect_clicked(move |_| {
            let p: PathBuf = expand(&c.settings.str("save_dir", "~/Pictures/inkover"));
            c.open_path(&p, true);
        });
        let c = ctx.clone();
        open_vids.connect_clicked(move |_| {
            let p: PathBuf = expand(&c.settings.str("record_dir", "~/Videos/inkover"));
            c.open_path(&p, true);
        });

        let c = ctx.clone();
        undo.connect_clicked(move |_| c.live(&["undo"]));
        let c = ctx.clone();
        redo.connect_clicked(move |_| c.live(&["redo"]));
        let w = with_target.clone();
        clear_mon.connect_clicked(move |_| w(&["clear"]));
        let c = ctx.clone();
        clear_all.connect_clicked(move |_| c.live(&["clear", "all"]));

        Rc::new(ControlPage {
            widget: page,
            overlay_row,
            start_btn,
            stop_btn,
            draw_row,
            toolbar_row,
            board_row,
            rec_row,
            rec_full,
            rec_region,
            rec_stop,
            ink_group,
        })
    }

    pub fn update(&self, st: Option<&Status>) {
        let running = st.is_some();
        self.start_btn.set_visible(!running);
        self.stop_btn.set_visible(running);
        for w in [self.draw_row.upcast_ref::<gtk::Widget>(), self.toolbar_row.upcast_ref(),
                  self.board_row.upcast_ref(), self.ink_group.upcast_ref()] {
            w.set_sensitive(running);
        }
        match st {
            None => {
                self.overlay_row.set_subtitle("Not running");
                self.rec_row.set_subtitle("Not recording");
                self.rec_stop.set_visible(false);
                self.rec_full.set_visible(true);
                self.rec_region.set_visible(true);
            }
            Some(s) => {
                let strokes: u64 = s.monitors.iter().map(|m| m.1).sum();
                self.overlay_row.set_subtitle(&format!(
                    "Running · {} · {} · {} stroke{}",
                    if s.draw_mode { "draw mode" } else { "pass mode" },
                    s.tool,
                    strokes,
                    if strokes == 1 { "" } else { "s" }
                ));
                self.draw_row.set_active(s.draw_mode);
                self.toolbar_row.set_active(s.toolbar_visible);
                self.board_row.set_selected(s.board as u32);
                let rec = s.recording.is_some();
                self.rec_stop.set_visible(rec);
                self.rec_full.set_visible(!rec);
                self.rec_region.set_visible(!rec);
                match s.recording {
                    Some(secs) => {
                        let name = s
                            .recording_path
                            .as_deref()
                            .and_then(|p| std::path::Path::new(p).file_name())
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        self.rec_row.set_subtitle(&format!("● REC {}:{:02} · {}", secs / 60, secs % 60, name));
                    }
                    None => self.rec_row.set_subtitle("Not recording"),
                }
            }
        }
    }
}
