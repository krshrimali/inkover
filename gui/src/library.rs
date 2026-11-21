//! Library page: recent screenshots, ink exports and recordings.
use std::path::PathBuf;
use std::rc::Rc;
use std::time::SystemTime;

use adw::prelude::*;

use crate::ctx::{button, CtxRef};
use crate::settings::expand;

pub struct LibraryPage {
    pub widget: gtk::Box,
    list: gtk::ListBox,
    empty: adw::StatusPage,
    ctx: CtxRef,
}

struct Entry {
    path: PathBuf,
    modified: SystemTime,
    size: u64,
}

impl LibraryPage {
    pub fn new(ctx: &CtxRef) -> Rc<Self> {
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let clamp = adw::Clamp::builder().maximum_size(720).build();
        let inner = gtk::Box::new(gtk::Orientation::Vertical, 12);
        inner.set_margin_top(18);
        inner.set_margin_bottom(18);
        inner.set_margin_start(12);
        inner.set_margin_end(12);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::builder().label("Recent captures").halign(gtk::Align::Start).hexpand(true).build();
        title.add_css_class("title-4");
        let refresh = button("", Some("view-refresh-symbolic"), "Refresh");
        refresh.add_css_class("flat");
        head.append(&title);
        head.append(&refresh);
        inner.append(&head);

        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        inner.append(&list);

        let empty = adw::StatusPage::builder()
            .icon_name("camera-photo-symbolic")
            .title("Nothing captured yet")
            .description("Screenshots, ink exports and recordings will show up here")
            .build();
        inner.append(&empty);

        clamp.set_child(Some(&inner));
        let scroll = gtk::ScrolledWindow::builder().child(&clamp).vexpand(true).build();
        outer.append(&scroll);

        let this = Rc::new(LibraryPage { widget: outer, list, empty, ctx: ctx.clone() });
        let t = this.clone();
        refresh.connect_clicked(move |_| t.refresh());
        this.refresh();
        this
    }

    fn entries(&self) -> Vec<Entry> {
        let s = &self.ctx.settings;
        let mut dirs = vec![expand(&s.str("save_dir", "~/Pictures/inkover"))];
        let rec = expand(&s.str("record_dir", "~/Videos/inkover"));
        if !dirs.contains(&rec) {
            dirs.push(rec);
        }
        let mut out = vec![];
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let path = e.path();
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("");
                if !matches!(ext, "png" | "mp4" | "mkv" | "webm") {
                    continue;
                }
                if let Ok(md) = e.metadata() {
                    out.push(Entry { path, modified: md.modified().unwrap_or(SystemTime::UNIX_EPOCH), size: md.len() });
                }
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified));
        out.truncate(40);
        out
    }

    pub fn refresh(&self) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let entries = self.entries();
        self.empty.set_visible(entries.is_empty());
        self.list.set_visible(!entries.is_empty());
        for e in entries {
            let name = e.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let is_video = !name.ends_with(".png");
            let age = e.modified.elapsed().map(|d| d.as_secs()).unwrap_or(0);
            let when = match age {
                0..=59 => "just now".to_string(),
                60..=3599 => format!("{} min ago", age / 60),
                3600..=86399 => format!("{} h ago", age / 3600),
                _ => format!("{} days ago", age / 86400),
            };
            let size = if e.size > 1 << 20 {
                format!("{:.1} MB", e.size as f64 / (1u64 << 20) as f64)
            } else {
                format!("{} KB", e.size / 1024)
            };
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&name).as_str())
                .subtitle(format!("{} · {}", when, size))
                .activatable(true)
                .build();

            // Fixed 96x54 slot; the image is pre-scaled to fit inside it.
            let icon = |name: &str| -> gtk::Widget {
                let i = gtk::Image::from_icon_name(name);
                i.set_pixel_size(28);
                i.upcast()
            };
            let thumb: gtk::Widget = if is_video {
                icon("video-x-generic-symbolic")
            } else {
                match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&e.path, 96, 54, true) {
                    Ok(pb) => {
                        // Letterbox into an exact 96x54 canvas so layout can't stretch it.
                        let canvas = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 96, 54);
                        if let Some(canvas) = canvas {
                            canvas.fill(0);
                            let (x, y) = ((96 - pb.width()) / 2, (54 - pb.height()) / 2);
                            pb.copy_area(0, 0, pb.width(), pb.height(), &canvas, x, y);
                            let p = gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&canvas));
                            p.set_can_shrink(false);
                            p.upcast()
                        } else {
                            icon("image-x-generic-symbolic")
                        }
                    }
                    Err(_) => icon("image-x-generic-symbolic"),
                }
            };
            let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            slot.set_size_request(96, 54);
            slot.set_valign(gtk::Align::Center);
            slot.add_css_class("thumb");
            slot.set_hexpand(false);
            slot.set_overflow(gtk::Overflow::Hidden);
            thumb.set_hexpand(true);
            thumb.set_vexpand(false);
            thumb.set_halign(gtk::Align::Center);
            thumb.set_valign(gtk::Align::Center);
            slot.append(&thumb);
            row.add_prefix(&slot);

            let folder = button("", Some("folder-open-symbolic"), "Show in folder");
            folder.add_css_class("flat");
            row.add_suffix(&folder);
            let c = self.ctx.clone();
            let p = e.path.clone();
            folder.connect_clicked(move |_| c.reveal_path(&p));
            let c = self.ctx.clone();
            let p = e.path.clone();
            row.connect_activated(move |_| c.open_path(&p, false));
            self.list.append(&row);
        }
    }
}
