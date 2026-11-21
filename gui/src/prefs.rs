//! Settings page: every config.toml key, saved on change and hot-reloaded.
use adw::prelude::*;

use crate::ctx::{button, CtxRef};
use crate::settings::{expand, DEFAULT_PALETTE};

const TOOL_CHOICES: [(&str, &str); 6] = [
    ("eraser", "Eraser"),
    ("highlighter", "Highlighter"),
    ("laser", "Laser pointer"),
    ("line", "Straight line"),
    ("pen", "Pen"),
    ("none", "Nothing"),
];

fn spin(ctx: &CtxRef, g: &adw::PreferencesGroup, key: &'static str, title: &str, sub: &str,
        range: (f64, f64, f64), digits: u32, default: f64) {
    let row = adw::SpinRow::with_range(range.0, range.1, range.2);
    row.set_title(title);
    if !sub.is_empty() {
        row.set_subtitle(sub);
    }
    row.set_digits(digits);
    row.set_value(ctx.settings.f64(key, default));
    let c = ctx.clone();
    row.connect_value_notify(move |r| {
        if digits == 0 {
            c.settings.set_int(key, r.value().round() as i64);
        } else {
            c.settings.set_f64(key, r.value());
        }
        c.save_settings();
    });
    g.add(&row);
}

fn switch(ctx: &CtxRef, g: &adw::PreferencesGroup, key: &'static str, title: &str, sub: &str, default: bool) {
    let row = adw::SwitchRow::builder().title(title).active(ctx.settings.bool(key, default)).build();
    if !sub.is_empty() {
        row.set_subtitle(sub);
    }
    let c = ctx.clone();
    row.connect_active_notify(move |r| {
        c.settings.set_bool(key, r.is_active());
        c.save_settings();
    });
    g.add(&row);
}

fn combo(ctx: &CtxRef, g: &adw::PreferencesGroup, key: &'static str, title: &str, sub: &str,
         choices: &'static [(&'static str, &'static str)], default: &str) {
    let labels: Vec<&str> = choices.iter().map(|c| c.1).collect();
    let now = ctx.settings.str(key, default);
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(&labels))
        .selected(choices.iter().position(|c| c.0 == now).unwrap_or(0) as u32)
        .build();
    if !sub.is_empty() {
        row.set_subtitle(sub);
    }
    let c = ctx.clone();
    row.connect_selected_notify(move |r| {
        c.settings.set_str(key, choices[r.selected() as usize].0);
        c.save_settings();
    });
    g.add(&row);
}

fn rgba_hex(c: &gtk::gdk::RGBA) -> String {
    let f = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", f(c.red()), f(c.green()), f(c.blue()))
}

fn color_button(hex: &str) -> gtk::ColorDialogButton {
    let dlg = gtk::ColorDialog::new();
    dlg.set_with_alpha(false);
    let b = gtk::ColorDialogButton::new(Some(dlg));
    if let Ok(c) = gtk::gdk::RGBA::parse(hex) {
        b.set_rgba(&c);
    }
    b.set_valign(gtk::Align::Center);
    b
}

fn folder(ctx: &CtxRef, g: &adw::PreferencesGroup, key: &'static str, title: &str, default: &'static str) {
    let row = adw::ActionRow::builder().title(title).subtitle(ctx.settings.str(key, default)).build();
    let open = button("", Some("folder-open-symbolic"), "Open folder");
    open.add_css_class("flat");
    let pick = button("Change…", None, "Choose a different folder");
    row.add_suffix(&open);
    row.add_suffix(&pick);
    let c = ctx.clone();
    open.connect_clicked(move |_| c.open_path(&expand(&c.settings.str(key, default)), true));
    let c = ctx.clone();
    let r = row.clone();
    pick.connect_clicked(move |_| {
        let dlg = gtk::FileDialog::builder().title("Choose folder").modal(true).build();
        let start = expand(&c.settings.str(key, default));
        let _ = std::fs::create_dir_all(&start);
        dlg.set_initial_folder(Some(&gtk::gio::File::for_path(&start)));
        let c2 = c.clone();
        let r2 = r.clone();
        dlg.select_folder(Some(&c.win), gtk::gio::Cancellable::NONE, move |res| {
            if let Ok(path) = res.map(|f| f.path()) {
                if let Some(p) = path {
                    let mut s = p.to_string_lossy().to_string();
                    if let Ok(home) = std::env::var("HOME") {
                        if let Some(rest) = s.strip_prefix(&format!("{home}/")) {
                            s = format!("~/{rest}");
                        }
                    }
                    c2.settings.set_str(key, &s);
                    r2.set_subtitle(&s);
                    c2.save_settings();
                }
            }
        });
    });
    g.add(&row);
}

pub fn page(ctx: &CtxRef) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // ------------------------------------------------------------- palette
    let g = adw::PreferencesGroup::builder()
        .title("Palette")
        .description("Colours 1–9, chosen with the number keys or the toolbar")
        .build();
    let row = adw::ActionRow::new();
    let sw_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    sw_box.set_halign(gtk::Align::Center);
    sw_box.set_hexpand(true);
    sw_box.set_margin_top(6);
    sw_box.set_margin_bottom(6);
    let palette = ctx.settings.palette();
    let mut btns = vec![];
    for i in 0..9 {
        let b = color_button(palette.get(i).map(|s| s.as_str()).unwrap_or(DEFAULT_PALETTE[i]));
        b.set_tooltip_text(Some(&format!("Colour {}", i + 1)));
        sw_box.append(&b);
        btns.push(b);
    }
    row.set_child(Some(&sw_box));
    g.add(&row);
    let save_palette = {
        let ctx = ctx.clone();
        let btns = btns.clone();
        move || {
            let p: Vec<String> = btns.iter().map(|b| rgba_hex(&b.rgba())).collect();
            ctx.settings.set_strings("palette", &p);
            ctx.save_settings();
        }
    };
    for b in &btns {
        let s = save_palette.clone();
        b.connect_rgba_notify(move |_| s());
    }
    let reset = adw::ButtonRow::builder().title("Reset palette").build();
    let bs = btns.clone();
    reset.connect_activated(move |_| {
        for (b, hex) in bs.iter().zip(DEFAULT_PALETTE) {
            if let Ok(c) = gtk::gdk::RGBA::parse(hex) {
                b.set_rgba(&c);
            }
        }
    });
    g.add(&reset);
    page.add(&g);

    // ---------------------------------------------------------------- ink
    let g = adw::PreferencesGroup::builder().title("Ink").build();
    spin(ctx, &g, "size", "Starting size", "", (1.0, 80.0, 1.0), 1, 5.0);
    spin(ctx, &g, "size_min", "Minimum size", "", (0.5, 20.0, 0.5), 1, 1.0);
    spin(ctx, &g, "size_max", "Maximum size", "", (10.0, 300.0, 5.0), 0, 80.0);
    spin(ctx, &g, "highlighter_alpha", "Highlighter opacity", "", (0.05, 1.0, 0.05), 2, 0.35);
    spin(ctx, &g, "highlighter_scale", "Highlighter width", "× pen size", (1.0, 12.0, 0.5), 1, 4.0);
    spin(ctx, &g, "eraser_scale", "Eraser size", "× pen size", (1.0, 12.0, 0.5), 1, 4.0);
    page.add(&g);

    // -------------------------------------------------------- laser & text
    let g = adw::PreferencesGroup::builder().title("Laser &amp; text").build();
    spin(ctx, &g, "laser_fade", "Laser fade time", "Seconds before laser ink disappears", (0.2, 10.0, 0.1), 1, 1.2);
    let laser_row = adw::ActionRow::builder().title("Laser colour").build();
    let laser_btn = color_button(&ctx.settings.str("laser_color", "#ff2626"));
    laser_row.add_suffix(&laser_btn);
    let c = ctx.clone();
    laser_btn.connect_rgba_notify(move |b| {
        c.settings.set_str("laser_color", &rgba_hex(&b.rgba()));
        c.save_settings();
    });
    g.add(&laser_row);
    let font_row = adw::ActionRow::builder().title("Text font").build();
    let font_btn = gtk::FontDialogButton::new(Some(gtk::FontDialog::new()));
    font_btn.set_level(gtk::FontLevel::Face);
    font_btn.set_valign(gtk::Align::Center);
    font_btn.set_font_desc(&gtk::pango::FontDescription::from_string(&ctx.settings.str("font", "Sans Bold")));
    font_row.add_suffix(&font_btn);
    let c = ctx.clone();
    font_btn.connect_font_desc_notify(move |b| {
        if let Some(d) = b.font_desc() {
            let mut d = d.clone();
            d.unset_fields(gtk::pango::FontMask::SIZE);
            c.settings.set_str("font", &d.to_string());
            c.save_settings();
        }
    });
    g.add(&font_row);
    spin(ctx, &g, "text_scale", "Text size", "Text height = pen size × this", (1.0, 20.0, 0.5), 1, 5.0);
    page.add(&g);

    // -------------------------------------------------------------- pen
    let g = adw::PreferencesGroup::builder()
        .title("Pen buttons")
        .description("What the stylus side buttons do while held down")
        .build();
    combo(ctx, &g, "pen_button_lower", "Lower button", "", &TOOL_CHOICES, "eraser");
    combo(ctx, &g, "pen_button_upper", "Upper button", "Also right-click with a mouse", &TOOL_CHOICES, "eraser");
    page.add(&g);

    // ------------------------------------------------ toolbar & monitors
    let g = adw::PreferencesGroup::builder()
        .title("Overlay")
        .description("Toolbar position and monitors apply after restarting the overlay")
        .build();
    combo(ctx, &g, "toolbar", "Toolbar position", "",
          &[("left", "Left"), ("right", "Right"), ("top", "Top"), ("bottom", "Bottom"), ("off", "Hidden")], "left");
    switch(ctx, &g, "start_in_pass", "Start in pass mode", "Ink shows, but the screen stays usable until you switch to draw", false);

    let chosen = ctx.settings.strings("monitors").unwrap_or_default();
    let exp = adw::ExpanderRow::builder()
        .title("Monitors")
        .subtitle(if chosen.is_empty() { "All monitors".to_string() } else { chosen.join(", ") })
        .build();
    let mons = ctx.monitors();
    let mut rows = vec![];
    for m in &mons {
        let r = adw::SwitchRow::builder().title(m).active(chosen.contains(m)).build();
        exp.add_row(&r);
        rows.push((m.clone(), r));
    }
    for (_, r) in &rows {
        let c = ctx.clone();
        let rows2 = rows.clone();
        let exp2 = exp.clone();
        r.connect_active_notify(move |_| {
            let sel: Vec<String> = rows2.iter().filter(|(_, r)| r.is_active()).map(|(m, _)| m.clone()).collect();
            exp2.set_subtitle(&if sel.is_empty() { "All monitors".to_string() } else { sel.join(", ") });
            c.settings.set_strings("monitors", &sel);
            c.save_settings();
        });
    }
    g.add(&exp);
    page.add(&g);

    // ------------------------------------------------------------ saving
    let g = adw::PreferencesGroup::builder().title("Screenshots").build();
    folder(ctx, &g, "save_dir", "Save to", "~/Pictures/inkover");
    switch(ctx, &g, "copy_to_clipboard", "Copy to clipboard", "", true);
    switch(ctx, &g, "notify", "Show notifications", "", true);
    page.add(&g);

    // ---------------------------------------------------------- recording
    let g = adw::PreferencesGroup::builder().title("Recording").build();
    folder(ctx, &g, "record_dir", "Save to", "~/Videos/inkover");
    combo(ctx, &g, "record_audio", "Audio", "",
          &[("none", "None"), ("mic", "Microphone"), ("desktop", "Desktop sound"), ("both", "Mic + desktop")], "none");
    spin(ctx, &g, "record_fps", "Frame rate", "", (10.0, 144.0, 1.0), 0, 60.0);
    combo(ctx, &g, "recorder", "Recorder", "gpu-screen-recorder encodes on the GPU",
          &[("auto", "Automatic"), ("gpu-screen-recorder", "gpu-screen-recorder"), ("wf-recorder", "wf-recorder")], "auto");
    switch(ctx, &g, "record_hide_toolbar", "Hide toolbar while recording", "Keeps the toolbar out of the video", false);
    page.add(&g);

    // -------------------------------------------------------------- misc
    let g = adw::PreferencesGroup::builder().title("Config file").description(ctx.settings.path().to_string_lossy().as_ref()).build();
    let open_cfg = adw::ButtonRow::builder().title("Open config.toml").start_icon_name("text-x-generic-symbolic").build();
    let c = ctx.clone();
    open_cfg.connect_activated(move |_| {
        let p = c.settings.path().clone();
        let _ = c.settings.save();
        c.open_path(&p, false);
    });
    g.add(&open_cfg);
    let restart = adw::ButtonRow::builder()
        .title("Restart overlay")
        .start_icon_name("view-refresh-symbolic")
        .build();
    restart.set_tooltip_text(Some("Applies toolbar/monitor changes. Ink is cleared."));
    let c = ctx.clone();
    restart.connect_activated(move |_| {
        if crate::ipc::send(&["quit"]).is_some() {
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(700), || {
                crate::ipc::run_cli(&["pass"]);
            });
            c.toast("Restarting overlay…");
        } else {
            c.toast("Overlay isn't running");
        }
    });
    g.add(&restart);
    page.add(&g);

    page
}
