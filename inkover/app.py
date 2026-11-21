"""The daemon: one Canvas per monitor plus the shared tool state and command dispatcher."""
import json
import os
import signal
import subprocess

import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gtk, Gdk, Gio, GLib

from . import capture, ipc
from .canvas import Canvas, BOARDS
from .config import Config, TOOLS
from .toolbar import BASE_CSS, css_for_palette

try:
    gi.require_version("GLibUnix", "2.0")
    from gi.repository import GLibUnix
    _signal_add = GLibUnix.signal_add
except (ValueError, ImportError):
    _signal_add = GLib.unix_signal_add

COMMANDS_HELP = """commands:
  toggle | draw | pass        switch DRAW (pen draws) / PASS (screen usable, ink stays)
  tool NAME                   pen highlighter laser eraser text line arrow rect ellipse
  color N                     1-9 from the palette
  size + | - | N              stroke size
  undo | redo | clear [all] | board | toolbar | help
  screenshot [full|region]    capture screen + ink -> file + clipboard
  export                      save ink only as a transparent PNG
  status | quit"""


class State:
    def __init__(self, cfg):
        self.cfg = cfg
        self.tool = "pen"
        self.color_idx = 0
        self.size = cfg.size
        self.board = 0
        self.drawing_mode = not cfg.start_in_pass
        self.show_help = False
        self.toolbar_visible = cfg.toolbar != "off"

    @property
    def color(self):
        return self.cfg.palette[self.color_idx]


class InkoverApp(Gtk.Application):
    def __init__(self, cfg=None):
        super().__init__(application_id="io.github.krshrimali.inkover",
                         flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.cfg = cfg or Config.load()
        self.state = State(self.cfg)
        self.canvases = []
        self.chrome_hidden = False
        self.server = None
        self.connect("activate", self.on_activate)

    def on_activate(self, *_):
        css = Gtk.CssProvider()
        css.load_from_string(BASE_CSS + css_for_palette(self.cfg.palette))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_USER + 1)

        mons = Gdk.Display.get_default().get_monitors()
        for i in range(mons.get_n_items()):
            m = mons.get_item(i)
            if self.cfg.monitors and m.get_connector() not in self.cfg.monitors:
                continue
            self.canvases.append(Canvas(self, m))
        if not self.canvases:
            print("inkover: no matching monitors", flush=True)
            self.quit()
            return

        self.server = ipc.Server(self.handle_ipc)
        for sig, cmd in ((signal.SIGUSR1, ["toggle"]), (signal.SIGUSR2, ["clear", "all"]),
                         (signal.SIGTERM, ["quit"]), (signal.SIGINT, ["quit"])):
            _signal_add(GLib.PRIORITY_DEFAULT, sig, lambda c=cmd: (self.command(c), True)[1])
        self.hold()

    # ----------------------------------------------------------- commands
    def handle_ipc(self, args):
        return self.command(args)

    def command(self, args, source=None):
        """Single entry point for keys, toolbar buttons, IPC and signals."""
        if not args:
            return "error: empty command"
        cmd, rest = args[0], args[1:]
        st = self.state
        target = source or self._focused_canvas()

        if cmd in ("toggle", "draw", "pass"):
            self.set_mode({"toggle": not st.drawing_mode, "draw": True, "pass": False}[cmd])
        elif cmd == "tool":
            if not rest or rest[0] not in TOOLS:
                return f"error: tool must be one of {' '.join(TOOLS)}"
            st.tool = rest[0]
            self._changed()
        elif cmd == "color":
            try:
                n = int(rest[0])
                assert 1 <= n <= len(self.cfg.palette)
            except (IndexError, ValueError, AssertionError):
                return f"error: color must be 1-{len(self.cfg.palette)}"
            st.color_idx = n - 1
            if st.tool == "eraser":
                st.tool = "pen"
            self._changed()
        elif cmd == "size":
            arg = rest[0] if rest else "+"
            if arg in ("+", "-"):
                st.size *= 1.2 if arg == "+" else 1 / 1.2
            else:
                try:
                    st.size = float(arg)
                except ValueError:
                    return "error: size + | - | NUMBER"
            st.size = max(self.cfg.size_min, min(self.cfg.size_max, st.size))
            self._changed()
        elif cmd == "undo":
            target.undo()
        elif cmd == "redo":
            target.redo()
        elif cmd == "clear":
            for c in (self.canvases if rest[:1] == ["all"] else [target]):
                c.clear()
        elif cmd == "board":
            st.board = (st.board + 1) % len(BOARDS)
            self.redraw()
        elif cmd == "toolbar":
            st.toolbar_visible = not st.toolbar_visible
            self._changed(flash=False)
        elif cmd == "help":
            st.show_help = not st.show_help
            self.redraw()
        elif cmd == "screenshot":
            capture.screenshot(self, target, region=rest[:1] == ["region"])
        elif cmd == "export":
            capture.export_ink(self, target)
        elif cmd == "status":
            return json.dumps({"mode": "draw" if st.drawing_mode else "pass", "tool": st.tool,
                               "color": st.color_idx + 1, "size": round(st.size, 1),
                               "monitors": {c.connector: len(c.strokes) for c in self.canvases}})
        elif cmd == "quit":
            GLib.idle_add(self._shutdown)
        else:
            return "error: unknown command\n" + COMMANDS_HELP
        return "ok"

    def _shutdown(self):
        if self.server:
            self.server.close()
        self.quit()
        return False

    def _focused_canvas(self):
        """Canvas on Hyprland's focused monitor (falls back to the first one)."""
        if len(self.canvases) > 1 and os.environ.get("HYPRLAND_INSTANCE_SIGNATURE"):
            try:
                out = subprocess.run(["hyprctl", "-j", "monitors"], capture_output=True,
                                     text=True, timeout=1).stdout
                name = next(m["name"] for m in json.loads(out) if m.get("focused"))
                for c in self.canvases:
                    if c.connector == name:
                        return c
            except Exception:
                pass
        return self.canvases[0]

    # -------------------------------------------------------------- state
    def set_mode(self, drawing):
        self.state.drawing_mode = drawing
        for c in self.canvases:
            c.apply_mode()

    def set_chrome_hidden(self, hidden):
        self.chrome_hidden = hidden
        for c in self.canvases:
            c.toolbar.sync()
            c.area.queue_draw()

    def _changed(self, flash=True):
        for c in self.canvases:
            c.toolbar.sync()
            if flash:
                c.flash_hud()
            if not self.state.drawing_mode:
                c._update_input_region()

    def redraw(self):
        for c in self.canvases:
            c.area.queue_draw()
