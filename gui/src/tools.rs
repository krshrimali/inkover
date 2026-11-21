//! Tools page: live tool / colour / size picker that mirrors the overlay.
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use adw::prelude::*;

use crate::ctx::CtxRef;
use crate::ipc::Status;

const TOOLS: [(&str, &str, &str); 9] = [
    ("pen", "✎", "Pen"),
    ("highlighter", "▬", "Highlighter"),
    ("laser", "✦", "Laser"),
    ("eraser", "⌫", "Eraser"),
    ("text", "T", "Text"),
    ("line", "╱", "Line"),
    ("arrow", "➜", "Arrow"),
    ("rect", "▭", "Rectangle"),
    ("ellipse", "◯", "Ellipse"),
];

pub struct ToolsPage {
    pub widget: adw::PreferencesPage,
    tool_btns: Vec<(String, gtk::ToggleButton)>,
    swatches: Vec<gtk::Button>,
    size: gtk::Scale,
    size_edited: Rc<Cell<Instant>>,
    css: gtk::CssProvider,
    palette: RefCell<Vec<String>>,
}

impl ToolsPage {
    pub fn new(ctx: &CtxRef) -> Rc<Self> {
        let page = adw::PreferencesPage::new();

        let g = adw::PreferencesGroup::builder().title("Tool").build();
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(5)
            .min_children_per_line(3)
            .row_spacing(8)
            .column_spacing(8)
            .homogeneous(true)
            .build();
        let mut tool_btns = vec![];
        let mut first: Option<gtk::ToggleButton> = None;
        for (name, glyph, label) in TOOLS {
            let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
            let g_lbl = gtk::Label::new(Some(glyph));
            g_lbl.add_css_class("tool-glyph");
            content.append(&g_lbl);
            content.append(&gtk::Label::new(Some(label)));
            let b = gtk::ToggleButton::builder().child(&content).build();
            b.add_css_class("tool-btn");
            if let Some(f) = &first {
                b.set_group(Some(f));
            } else {
                first = Some(b.clone());
            }
            let c = ctx.clone();
            let n = name.to_string();
            b.connect_toggled(move |b| {
                if b.is_active() {
                    c.live(&["tool", &n]);
                }
            });
            flow.insert(&b, -1);
            tool_btns.push((name.to_string(), b));
        }
        g.add(&flow);
        page.add(&g);

        let g = adw::PreferencesGroup::builder()
            .title("Colour")
            .description("Edit the palette in Settings")
            .build();
        let sw_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        sw_box.set_halign(gtk::Align::Center);
        let mut swatches = vec![];
        for i in 0..9 {
            let b = gtk::Button::new();
            b.add_css_class("swatch");
            b.add_css_class(&format!("swatch-{i}"));
            b.set_tooltip_text(Some(&format!("Colour {} (key {})", i + 1, i + 1)));
            let c = ctx.clone();
            b.connect_clicked(move |_| c.live(&["color", &(i + 1).to_string()]));
            sw_box.append(&b);
            swatches.push(b);
        }
        g.add(&sw_box);
        page.add(&g);

        let g = adw::PreferencesGroup::builder().title("Size").build();
        let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 1.0, 80.0, 1.0);
        size.set_draw_value(true);
        size.set_digits(0);
        size.set_hexpand(true);
        for m in [2.0, 5.0, 10.0, 20.0, 40.0, 80.0] {
            size.add_mark(m, gtk::PositionType::Bottom, Some(&format!("{m}")));
        }
        let size_edited = Rc::new(Cell::new(Instant::now() - Duration::from_secs(10)));
        let c = ctx.clone();
        let se = size_edited.clone();
        size.connect_value_changed(move |s| {
            if !c.updating.get() {
                se.set(Instant::now());
                c.live(&["size", &format!("{}", s.value().round())]);
            }
        });
        g.add(&size);
        page.add(&g);

        let css = gtk::CssProvider::new();
        if let Some(d) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&d, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
        let this = Rc::new(ToolsPage {
            widget: page,
            tool_btns,
            swatches,
            size,
            size_edited,
            css,
            palette: RefCell::new(vec![]),
        });
        this.set_palette(&ctx.settings.palette());
        this
    }

    fn set_palette(&self, palette: &[String]) {
        if *self.palette.borrow() == palette {
            return;
        }
        let mut css = String::from(
            "button.swatch { min-width: 34px; min-height: 34px; padding: 0; border-radius: 999px; \
             border: 2px solid alpha(currentColor, 0.2); box-shadow: none; }\n\
             button.swatch.active { border: 3px solid @window_fg_color; box-shadow: 0 0 0 2px alpha(@window_fg_color, 0.25); }\n\
             .thumb { background: alpha(currentColor, 0.08); border-radius: 6px; }\n\
             button.tool-btn { padding: 8px 4px; }\n\
             .tool-glyph { font-size: 22px; }\n",
        );
        for (i, c) in palette.iter().enumerate() {
            css.push_str(&format!("button.swatch.swatch-{i} {{ background: {c}; }}\n"));
        }
        self.css.load_from_string(&css);
        for (i, b) in self.swatches.iter().enumerate() {
            b.set_visible(i < palette.len());
        }
        *self.palette.borrow_mut() = palette.to_vec();
    }

    pub fn update(&self, st: Option<&Status>) {
        self.widget.set_sensitive(st.is_some());
        let Some(s) = st else { return };
        if !s.palette.is_empty() {
            self.set_palette(&s.palette);
        }
        for (name, b) in &self.tool_btns {
            if *name == s.tool && !b.is_active() {
                b.set_active(true);
            }
        }
        for (i, b) in self.swatches.iter().enumerate() {
            if i + 1 == s.color { b.add_css_class("active") } else { b.remove_css_class("active") }
        }
        // Don't fight the slider while the user is dragging it.
        if self.size_edited.get().elapsed() > Duration::from_millis(800)
            && (self.size.value() - s.size).abs() > 0.5
        {
            self.size.set_value(s.size);
        }
    }
}
