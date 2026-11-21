"""Pen-friendly toolbar floating on each monitor's overlay.

In PASS mode it collapses to a single button, which is the only part of the
overlay that accepts input, so one tap brings you back to drawing.
"""
import gi
gi.require_version("Gtk", "4.0")
from gi.repository import Gtk

TOOL_BUTTONS = [
    ("pen", "✎", "Pen (p)"),
    ("highlighter", "▬", "Highlighter (h)"),
    ("laser", "✦", "Laser pointer, fades away (k)"),
    ("eraser", "⌫", "Eraser (e), or hold a pen button"),
    ("text", "T", "Text (t)"),
    ("line", "╱", "Line (l)"),
    ("arrow", "➜", "Arrow (a)"),
    ("rect", "▭", "Rectangle (r)"),
    ("ellipse", "◯", "Ellipse (o)"),
]


def css_for_palette(palette):
    rules = []
    for i, (r, g, b) in enumerate(palette):
        rules.append(f".inkbar button.swatch.swatch-{i} {{ background: rgb({int(r*255)},{int(g*255)},{int(b*255)}); }}")
    return "\n".join(rules)


BASE_CSS = """
window.inkover { background: transparent; }
.inkbar { background: rgba(22, 22, 27, 0.90); border-radius: 14px; padding: 6px;
          box-shadow: 0 4px 16px rgba(0,0,0,0.45); }
.inkbar button { min-width: 36px; min-height: 36px; padding: 0; margin: 1px;
                 border-radius: 9px; border: none; box-shadow: none; outline: none;
                 background: transparent; background-image: none;
                 color: #e8e8ea; font-size: 18px; }
.inkbar button:hover { background: rgba(255,255,255,0.12); }
.inkbar button.active { background: rgba(255,255,255,0.26); }
.inkbar button.swatch { min-width: 20px; min-height: 20px; margin: 4px; border-radius: 99px;
                        border: 2px solid rgba(255,255,255,0.15); }
.inkbar button.swatch.active { border: 3px solid #ffffff; }
.inkbar button.mode { background: rgba(255,255,255,0.10); }
.inkbar button.recording { color: #ff3b3b; background: rgba(255,59,59,0.18); }
.inkbar label.rectime { color: #ff6b6b; font-size: 11px; font-weight: bold; }
.inkbar label.size { color: #e8e8ea; font-size: 12px; font-weight: bold; }
.inkbar separator { background: rgba(255,255,255,0.15); margin: 4px; min-width: 1px; min-height: 1px; }
"""


class Toolbar:
    def __init__(self, app, canvas):
        self.app = app
        self.canvas = canvas
        cfg = app.cfg
        side = cfg.toolbar if cfg.toolbar in ("left", "right", "top", "bottom") else "left"
        vertical = side in ("left", "right")
        orient = Gtk.Orientation.VERTICAL if vertical else Gtk.Orientation.HORIZONTAL

        box = self.widget = Gtk.Box(orientation=orient, spacing=2)
        box.add_css_class("inkbar")
        box.set_halign({"left": Gtk.Align.START, "right": Gtk.Align.END}.get(side, Gtk.Align.CENTER))
        box.set_valign({"top": Gtk.Align.START, "bottom": Gtk.Align.END}.get(side, Gtk.Align.CENTER))
        for m in ("top", "bottom", "start", "end"):
            getattr(box, f"set_margin_{m}")(14)

        self.mode_btn = self._button("✎", "Draw / use screen (Esc)", lambda: app.command(["toggle"]))
        self.mode_btn.add_css_class("mode")
        box.append(self.mode_btn)

        # Everything below the mode button hides in PASS mode.
        self.body = Gtk.Box(orientation=orient, spacing=2)
        box.append(self.body)
        sep = lambda: Gtk.Separator(orientation=Gtk.Orientation.HORIZONTAL if vertical
                                    else Gtk.Orientation.VERTICAL)

        self.tool_btns = {}
        grid = Gtk.Grid()
        cols = 2 if vertical else 9
        for i, (tool, glyph, tip) in enumerate(TOOL_BUTTONS):
            b = self._button(glyph, tip, lambda t=tool: app.command(["tool", t]))
            self.tool_btns[tool] = b
            grid.attach(b, i % cols, i // cols, 1, 1)
        self.body.append(sep())
        self.body.append(grid)
        self.body.append(sep())

        self.swatches = []
        sw_grid = Gtk.Grid()
        sw_grid.set_halign(Gtk.Align.CENTER)
        for i in range(len(cfg.palette)):
            b = self._button("", f"Colour {i + 1}", lambda n=i + 1: app.command(["color", str(n)]))
            b.add_css_class("swatch")
            b.add_css_class(f"swatch-{i}")
            self.swatches.append(b)
            sw_grid.attach(b, i % 3, i // 3, 1, 1)
        self.body.append(sw_grid)
        self.body.append(sep())

        size_box = Gtk.Box(orientation=orient, spacing=0)
        size_box.set_halign(Gtk.Align.CENTER)
        size_box.append(self._button("−", "Smaller ([)", lambda: app.command(["size", "-"])))
        self.size_lbl = Gtk.Label()
        self.size_lbl.add_css_class("size")
        self.size_lbl.set_width_chars(3)
        size_box.append(self.size_lbl)
        size_box.append(self._button("+", "Bigger (])", lambda: app.command(["size", "+"])))
        self.body.append(size_box)
        self.body.append(sep())

        actions = Gtk.Grid()
        acts = [
            ("↶", "Undo (u / Ctrl+Z)", ["undo"]),
            ("↷", "Redo (Ctrl+Shift+Z)", ["redo"]),
            ("🗑", "Clear this screen (c)", ["clear"]),
            ("▢", "Board: none / white / black (b)", ["board"]),
            ("⛶", "Screenshot this monitor (Ctrl+S)", ["screenshot", "full"]),
            ("⬚", "Screenshot a region (Ctrl+Shift+S)", ["screenshot", "region"]),
            ("⇩", "Export ink only, transparent PNG (Ctrl+E)", ["export"]),
            ("⏺", "Record this monitor (Ctrl+R; Ctrl+Shift+R for a region)", ["record", "full"]),
            ("✕", "Quit (q)", ["quit"]),
        ]
        for i, (glyph, tip, cmd) in enumerate(acts):
            b = self._button(glyph, tip, lambda c=cmd: app.command(c, source=canvas))
            actions.attach(b, i % cols, i // cols, 1, 1)
            if cmd[0] == "record":
                self.rec_btn = b
        # While recording, a red stop button + timer also shows in PASS mode.
        self.rec_stop = self._button("⏹", "Stop recording", lambda: app.command(["record"]))
        self.rec_stop.add_css_class("recording")
        self.rec_stop.set_visible(False)
        box.append(self.rec_stop)
        self.rec_time = Gtk.Label()
        self.rec_time.add_css_class("rectime")
        self.rec_time.set_visible(False)
        box.append(self.rec_time)
        self.body.append(actions)

    def _button(self, label, tip, fn):
        b = Gtk.Button(label=label)
        b.set_tooltip_text(tip)
        b.set_focus_on_click(False)
        b.set_can_focus(False)
        b.connect("clicked", lambda *_: fn())
        return b

    def sync(self):
        st = self.app.state
        rec = self.app.recorder
        hide_for_rec = rec.active and self.app.cfg.record_hide_toolbar
        self.widget.set_visible(st.toolbar_visible and not self.app.chrome_hidden and not hide_for_rec)
        self.rec_btn.set_visible(not rec.active)
        self.rec_stop.set_visible(rec.active)
        self.rec_time.set_visible(rec.active)
        if rec.active:
            m, s = divmod(rec.elapsed(), 60)
            self.rec_time.set_text(f"{m}:{s:02d}")
        self.body.set_visible(st.drawing_mode)
        self.mode_btn.set_label("👆" if st.drawing_mode else "✎")
        self.mode_btn.set_tooltip_text("Use the screen (Esc)" if st.drawing_mode else "Draw")
        for tool, b in self.tool_btns.items():
            (b.add_css_class if tool == st.tool else b.remove_css_class)("active")
        for i, b in enumerate(self.swatches):
            (b.add_css_class if i == st.color_idx else b.remove_css_class)("active")
        self.size_lbl.set_text(f"{st.size:.0f}")
