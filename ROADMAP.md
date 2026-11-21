# Roadmap

Ideas roughly in priority order. ✅ = shipped.

## Shipped (0.2)
- ✅ Pressure-sensitive pen, highlighter, stroke eraser, shapes, arrows
- ✅ Draw / pass modes over the live screen
- ✅ Pen-tappable toolbar that collapses to one button in pass mode
- ✅ Laser pointer (fading ink), text tool, Shift for straight lines
- ✅ Screenshots (monitor / region) to file + clipboard; transparent ink export
- ✅ Control socket + CLI (`inkover <command>`), TOML config
- ✅ Rust (GTK4/libadwaita) control panel: control, live tools, library, settings (0.4)
- ✅ Screen recording (monitor / region, optional audio) via gpu-screen-recorder / wf-recorder (0.3)

## Next
- **Tablet ExpressKeys (pad buttons)**: map Intuos pad buttons to inkover commands
  via `GtkPadController` while the overlay has focus.
- **Draggable / dockable toolbar**: drag to any edge and remember the position.
- **Select & move**: lasso strokes, then move, recolour or delete them.
- **Save & restore sessions**: keep ink as JSON between runs and reopen it later.
- **Pages**: several boards per monitor (next/prev page), handy for teaching.
- **Recording extras**: pause/resume, webcam bubble, GIF export, click/keystroke overlay.
- **Smoothing / stabiliser**: optional lag-based stabiliser for neater handwriting.
- **Filled shapes & dashed lines**, opacity slider, custom colour picker.
- **Spotlight / zoom**: dim the screen except around the pen, or magnify a region.
- **Auto-fade mode**: ordinary ink that disappears after N seconds.
- **Packaging**: AUR `inkover-git`, `pyproject.toml`, a systemd user service.
- **Performance**: incremental damage regions instead of full-surface redraws on 4K.

## Known limitations
- Pressure needs a compositor that forwards tablet events (Hyprland, Sway, KDE: yes).
- Screenshots use `grim`, so wlroots-based compositors only (KDE/GNOME need a portal).
- GNOME doesn't support layer-shell, so inkover won't run there.
