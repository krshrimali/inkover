//! inkover-gui: control panel for the inkover screen-annotation overlay.
mod control;
mod ctx;
mod ipc;
mod library;
mod prefs;
mod settings;
mod tools;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use ctx::Ctx;

const APP_ID: &str = "io.github.krshrimali.inkover.Gui";

fn build(app: &adw::Application) {
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("inkover")
        .default_width(640)
        .default_height(780)
        .build();
    let toasts = adw::ToastOverlay::new();
    let ctx = Rc::new(Ctx {
        win: win.clone(),
        toasts: toasts.clone(),
        settings: settings::Settings::load(),
        status: RefCell::new(None),
        updating: Cell::new(false),
    });

    let stack = adw::ViewStack::new();
    let control = control::ControlPage::new(&ctx);
    let tools = tools::ToolsPage::new(&ctx);
    let library = library::LibraryPage::new(&ctx);
    stack.add_titled_with_icon(&control.widget, Some("control"), "Control", "media-playback-start-symbolic");
    stack.add_titled_with_icon(&tools.widget, Some("tools"), "Tools", "document-edit-symbolic");
    stack.add_titled_with_icon(&library.widget, Some("library"), "Library", "folder-pictures-symbolic");
    stack.add_titled_with_icon(&prefs::page(&ctx), Some("settings"), "Settings", "emblem-system-symbolic");

    let header = adw::HeaderBar::new();
    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    header.set_title_widget(Some(&switcher));

    let banner = adw::Banner::builder()
        .title("The overlay isn't running")
        .button_label("Start drawing")
        .revealed(true)
        .build();
    let c = ctx.clone();
    banner.connect_button_clicked(move |_| c.cmd(&["draw"]));

    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.add_top_bar(&banner);
    view.set_content(Some(&stack));
    toasts.set_child(Some(&view));
    win.set_content(Some(&toasts));

    let lib = library.clone();
    stack.connect_visible_child_name_notify(move |s| {
        if s.visible_child_name().as_deref() == Some("library") {
            lib.refresh();
        }
    });
    if let Some(page) = std::env::args().nth(1) {
        stack.set_visible_child_name(&page);
    }

    // Poll the daemon and sync every page.
    let tick = {
        let ctx = ctx.clone();
        let banner = banner.clone();
        let (control, tools, library) = (control.clone(), tools.clone(), library.clone());
        let was_recording = Cell::new(false);
        move || {
            let st = ipc::status();
            let recording = st.as_ref().is_some_and(|s| s.recording.is_some());
            if was_recording.replace(recording) && !recording {
                // A recording just finished: let it land on disk, then refresh the list.
                let lib = library.clone();
                gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || lib.refresh());
            }
            if *ctx.status.borrow() == st && st.is_some() {
                return;
            }
            *ctx.status.borrow_mut() = st.clone();
            ctx.updating.set(true);
            banner.set_revealed(st.is_none());
            control.update(st.as_ref());
            tools.update(st.as_ref());
            ctx.updating.set(false);
        }
    };
    tick();
    gtk::glib::timeout_add_local(Duration::from_millis(600), move || {
        tick();
        gtk::glib::ControlFlow::Continue
    });

    win.present();
}

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    app.connect_command_line(|app, _| {
        app.activate();
        0.into()
    });
    app.connect_activate(|app| {
        if let Some(w) = app.active_window() {
            w.present();
        } else {
            build(app);
        }
    });
    app.run()
}
