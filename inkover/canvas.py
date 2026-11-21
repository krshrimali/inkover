"""One fullscreen overlay window (ink + toolbar) for a single monitor."""
import math
import time

import cairo
import gi
gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gtk4LayerShell", "1.0")
from gi.repository import Gtk, Gdk, GLib, Gtk4LayerShell as LS

from .strokes import Stroke, render_stroke, SHAPES
from .toolbar import Toolbar

BOARDS = [None, (1, 1, 1), (0.08, 0.08, 0.09)]
FULL_REGION = cairo.Region(cairo.RectangleInt(0, 0, 1 << 15, 1 << 15))

HELP = [
    "p pen  h highlighter  k laser  e eraser  t text",
    "l line  a arrow  r rect  o ellipse   Shift+drag = straight",
    "1-9 colour   [ ] / scroll size   u undo   Ctrl+Shift+Z redo",
    "c clear  Shift+C clear all  b board  F2 toolbar",
    "Ctrl+S screenshot  Ctrl+Shift+S region  Ctrl+E export ink",
    "Ctrl+R record monitor  Ctrl+Shift+R record region (again = stop)",
    "Esc use screen (pass)   q quit   F1 hide help",
]


class Canvas:
    def __init__(self, app, monitor):
        self.app = app
        self.st = app.state
        self.cfg = app.cfg
        self.monitor = monitor
        self.connector = monitor.get_connector() or "?"
        self.strokes = []
        self.undo_stack = []   # ("add", s) | ("erase", [(i, s)]) | ("clear", [s...])
        self.redo_stack = []
        self.cur = None
        self.straight = False
        self.erasing = False
        self.erased_batch = []
        self.editing = None    # text stroke being typed
        self.lasers = []
        self.laser_tick = None
        self.cache = None
        self.cache_dirty = True
        self.hover = None
        self.hud_until = 0.0
        self.last_stylus_t = 0.0
        self.drag_origin = None

        w = self.window = Gtk.Window(application=app)
        LS.init_for_window(w)
        LS.set_monitor(w, monitor)
        LS.set_layer(w, LS.Layer.OVERLAY)
        LS.set_namespace(w, "inkover")
        for edge in (LS.Edge.TOP, LS.Edge.BOTTOM, LS.Edge.LEFT, LS.Edge.RIGHT):
            LS.set_anchor(w, edge, True)
        LS.set_exclusive_zone(w, -1)
        w.add_css_class("inkover")

        overlay = Gtk.Overlay()
        a = self.area = Gtk.DrawingArea()
        a.set_draw_func(self.draw)
        a.set_cursor(Gdk.Cursor.new_from_name("crosshair"))
        overlay.set_child(a)
        self.toolbar = Toolbar(app, self)
        if self.cfg.toolbar != "off":
            overlay.add_overlay(self.toolbar.widget)
        w.set_child(overlay)

        sty = Gtk.GestureStylus()
        sty.set_button(0)
        sty.connect("down", self.on_sty_down)
        sty.connect("motion", self.on_sty_motion)
        sty.connect("up", self.on_sty_up)
        sty.connect("proximity", self.on_sty_hover)
        a.add_controller(sty)

        drag = Gtk.GestureDrag()            # mouse fallback; pen events are filtered out
        drag.set_button(0)
        drag.connect("drag-begin", self.on_drag_begin)
        drag.connect("drag-update", self.on_drag_update)
        drag.connect("drag-end", self.on_drag_end)
        a.add_controller(drag)

        mot = Gtk.EventControllerMotion()
        mot.connect("motion", lambda c, x, y: self._set_hover((x, y)))
        mot.connect("leave", lambda *_: self._set_hover(None))
        a.add_controller(mot)

        scr = Gtk.EventControllerScroll.new(Gtk.EventControllerScrollFlags.VERTICAL)
        scr.connect("scroll", lambda c, dx, dy: (app.change_size(-1 if dy > 0 else 1), True)[1])
        a.add_controller(scr)

        key = Gtk.EventControllerKey()
        key.connect("key-pressed", self.on_key)
        w.add_controller(key)

        w.connect("realize", lambda *_: self.apply_mode())
        w.present()

    # ------------------------------------------------------------------ mode
    def apply_mode(self):
        drawing = self.st.drawing_mode
        if not drawing:
            self.finish()
            self.hover = None
        LS.set_keyboard_mode(self.window,
                             LS.KeyboardMode.ON_DEMAND if drawing else LS.KeyboardMode.NONE)
        self.toolbar.sync()
        # Let the toolbar re-layout before measuring it for the input region.
        GLib.timeout_add(60, self._update_input_region)
        self.flash_hud()

    def _update_input_region(self):
        surf = self.window.get_surface()
        if surf is None:
            return False
        if self.st.drawing_mode:
            surf.set_input_region(FULL_REGION)
        else:
            # PASS: everything clicks through except the collapsed toolbar button.
            region = cairo.Region()
            tb = self.toolbar.widget
            if self.cfg.toolbar != "off" and tb.get_visible() and self.st.toolbar_visible:
                ok, r = tb.compute_bounds(self.window)
                if ok:
                    region = cairo.Region(cairo.RectangleInt(
                        int(r.get_x()), int(r.get_y()),
                        int(math.ceil(r.get_width())), int(math.ceil(r.get_height()))))
            surf.set_input_region(region)
        return False

    # ----------------------------------------------------------------- input
    def _pen_active(self, g):
        dev = g.get_device()
        if dev is not None and dev.get_source() == Gdk.InputSource.PEN:
            return True
        return time.monotonic() - self.last_stylus_t < 0.05

    def _override_tool(self, g):
        """Tool forced by eraser tip / pen side buttons / right-click, else None."""
        tool = g.get_device_tool() if hasattr(g, "get_device_tool") else None
        if tool is not None and tool.get_tool_type() == Gdk.DeviceToolType.ERASER:
            return "eraser"
        btn = g.get_current_button()
        mods = g.get_current_event_state()
        if btn == 3 or mods & Gdk.ModifierType.BUTTON3_MASK:
            choice = self.cfg.pen_button_upper
        elif btn == 2 or mods & Gdk.ModifierType.BUTTON2_MASK:
            choice = self.cfg.pen_button_lower
        else:
            return None
        return None if choice == "none" else choice

    @staticmethod
    def _pressure(g):
        ok, p = g.get_axis(Gdk.AxisUse.PRESSURE)
        return p if ok else 1.0

    @staticmethod
    def _shift(g):
        return bool(g.get_current_event_state() & Gdk.ModifierType.SHIFT_MASK)

    def on_sty_down(self, g, x, y):
        self.last_stylus_t = time.monotonic()
        self.begin(x, y, self._pressure(g), self._override_tool(g), self._shift(g))

    def on_sty_motion(self, g, x, y):
        self.last_stylus_t = time.monotonic()
        self._set_hover((x, y))
        if self.cur is None and not self.erasing:
            return
        try:   # full-rate samples GTK coalesced since the last frame
            res = g.get_backlog()
            backlog = res[1] if isinstance(res, tuple) else res
            for tc in backlog or ():
                ax = tc.axes
                self.extend(ax[Gdk.AxisUse.X], ax[Gdk.AxisUse.Y],
                            ax[Gdk.AxisUse.PRESSURE] or self._pressure(g), redraw=False)
        except Exception:
            pass
        self.extend(x, y, self._pressure(g))

    def on_sty_up(self, g, x, y):
        self.last_stylus_t = time.monotonic()
        if self.cur is not None or self.erasing:
            self.extend(x, y, self._pressure(g))
        self.end_stroke()

    def on_sty_hover(self, g, x, y):
        self.last_stylus_t = time.monotonic()
        self._set_hover((x, y))

    def on_drag_begin(self, g, x, y):
        if self._pen_active(g):
            return
        self.drag_origin = (x, y)
        self.begin(x, y, 1.0, self._override_tool(g), self._shift(g))

    def on_drag_update(self, g, dx, dy):
        if self.drag_origin is None or self._pen_active(g):
            return
        ox, oy = self.drag_origin
        self.extend(ox + dx, oy + dy, 1.0)

    def on_drag_end(self, g, dx, dy):
        if self.drag_origin is None:
            return
        self.drag_origin = None
        self.end_stroke()

    def _set_hover(self, pos):
        self.hover = pos
        self.area.queue_draw()

    # ------------------------------------------------------------- strokes
    def begin(self, x, y, p, override, shift):
        if not self.st.drawing_mode:
            return
        self.finish()
        tool = override or self.st.tool
        cfg, st = self.cfg, self.st
        if tool == "eraser":
            self.erasing = True
            self.erased_batch = []
            self._erase_at(x, y)
        elif tool == "text":
            s = Stroke("text", st.color, st.size * cfg.text_scale)
            s.font = cfg.font
            s.add(x, y - s.size * 0.6, 1.0)
            self.editing = s
        else:
            if tool == "highlighter":
                s = Stroke(tool, st.color, st.size * cfg.highlighter_scale, cfg.highlighter_alpha)
            elif tool == "laser":
                s = Stroke(tool, cfg.laser_color, max(3.0, st.size))
            else:
                s = Stroke(tool, st.color, st.size)
            s.add(x, y, p)
            self.straight = shift and tool not in SHAPES
            self.cur = s
        self.area.queue_draw()

    def extend(self, x, y, p, redraw=True):
        if self.erasing:
            self._erase_at(x, y)
        elif self.cur is not None:
            if self.straight:
                del self.cur.pts[1:]
            self.cur.add(x, y, p)
        if redraw:
            self.area.queue_draw()

    def end_stroke(self):
        if self.erasing:
            self.erasing = False
            if self.erased_batch:
                self._push(("erase", self.erased_batch))
            self.erased_batch = []
        if self.cur is not None:
            s, self.cur = self.cur, None
            if s.tool == "laser":
                s.ended = time.monotonic()
                self.lasers.append(s)
                self._start_laser_tick()
            elif not s.is_empty():
                self._commit(s)
        self.area.queue_draw()

    def finish(self):
        """End any in-progress stroke or text edit."""
        self.end_stroke()
        if self.editing is not None:
            s, self.editing = self.editing, None
            if not s.is_empty():
                self._commit(s)
            self.area.queue_draw()

    def _commit(self, s):
        self.strokes.append(s)
        self._push(("add", s))
        if self.cache is not None and not self.cache_dirty:
            render_stroke(cairo.Context(self.cache), s)

    def _push(self, action):
        self.undo_stack.append(action)
        self.redo_stack.clear()

    def _erase_at(self, x, y):
        r = self.st.size * self.cfg.eraser_scale / 2 + 4
        hit = [i for i, s in enumerate(self.strokes) if s.hit(x, y, r)]
        for i in reversed(hit):
            self.erased_batch.append((i, self.strokes.pop(i)))
        if hit:
            self.cache_dirty = True

    def _start_laser_tick(self):
        if self.laser_tick is None:
            self.laser_tick = self.area.add_tick_callback(self._on_laser_tick)

    def _on_laser_tick(self, *_):
        now = time.monotonic()
        self.lasers = [s for s in self.lasers if now - s.ended < self.cfg.laser_fade]
        self.area.queue_draw()
        if not self.lasers:
            self.laser_tick = None
            return GLib.SOURCE_REMOVE
        return GLib.SOURCE_CONTINUE

    # -------------------------------------------------------------- history
    def undo(self):
        self.finish()
        if not self.undo_stack:
            return
        kind, data = act = self.undo_stack.pop()
        if kind == "add":
            self.strokes.remove(data)
        elif kind == "erase":
            for i, s in reversed(data):
                self.strokes.insert(i, s)
        elif kind == "clear":
            self.strokes = list(data)
        self.redo_stack.append(act)
        self.cache_dirty = True
        self.area.queue_draw()

    def redo(self):
        self.finish()
        if not self.redo_stack:
            return
        kind, data = act = self.redo_stack.pop()
        if kind == "add":
            self.strokes.append(data)
        elif kind == "erase":
            for i, _ in data:
                self.strokes.pop(i)
        elif kind == "clear":
            self.strokes = []
        self.undo_stack.append(act)
        self.cache_dirty = True
        self.area.queue_draw()

    def clear(self):
        self.finish()
        if self.strokes:
            self._push(("clear", list(self.strokes)))
            self.strokes = []
            self.cache_dirty = True
        self.area.queue_draw()

    # ------------------------------------------------------------------ keys
    def on_key(self, ctl, keyval, keycode, state):
        name = (Gdk.keyval_name(keyval) or "").lower()
        ctrl = bool(state & Gdk.ModifierType.CONTROL_MASK)
        shift = bool(state & Gdk.ModifierType.SHIFT_MASK)

        if self.editing is not None and not ctrl:
            return self._text_key(name, keyval)

        cmd = None
        if ctrl:
            cmd = {"z": "redo" if shift else "undo", "y": "redo",
                   "s": "screenshot region" if shift else "screenshot full",
                   "r": "record region" if shift else "record full", "e": "export"}.get(name)
        elif name == "escape":
            cmd = "pass"
        elif name == "c":
            cmd = "clear all" if shift else None
            if not shift:
                self.clear()
                return True
        elif name == "u":
            cmd = "undo"
        elif name == "q":
            cmd = "quit"
        elif name == "b":
            cmd = "board"
        elif name == "f1" or name == "question":
            cmd = "help"
        elif name == "f2":
            cmd = "toolbar"
        elif name in ("bracketright", "plus", "equal", "kp_add"):
            cmd = "size +"
        elif name in ("bracketleft", "minus", "kp_subtract"):
            cmd = "size -"
        elif len(name) == 1 and name in "123456789":
            cmd = f"color {name}"
        else:
            tool = {"p": "pen", "h": "highlighter", "k": "laser", "e": "eraser", "t": "text",
                    "l": "line", "a": "arrow", "r": "rect", "o": "ellipse"}.get(name)
            if tool:
                cmd = f"tool {tool}"
        if cmd is None:
            return False
        self.app.command(cmd.split(), source=self)
        return True

    def _text_key(self, name, keyval):
        s = self.editing
        if name == "escape":
            self.finish()
        elif name in ("return", "kp_enter"):
            s.text += "\n"
        elif name == "backspace":
            s.text = s.text[:-1]
        else:
            ch = Gdk.keyval_to_unicode(keyval)
            if not ch or ch < 32:
                return False
            s.text += chr(ch)
        s._bbox = None
        self.area.queue_draw()
        return True

    # -------------------------------------------------------------- drawing
    def flash_hud(self):
        self.hud_until = time.monotonic() + 1.6
        self.area.queue_draw()
        GLib.timeout_add(1700, lambda: (self.area.queue_draw(), False)[1])

    def _ensure_cache(self, w, h):
        scale = self.area.get_scale_factor()
        if (self.cache is None or self.cache.get_width() != w * scale
                or self.cache.get_height() != h * scale):
            self.cache = cairo.ImageSurface(cairo.FORMAT_ARGB32, w * scale, h * scale)
            self.cache.set_device_scale(scale, scale)
            self.cache_dirty = True
        if self.cache_dirty:
            cr = cairo.Context(self.cache)
            cr.set_operator(cairo.OPERATOR_CLEAR)
            cr.paint()
            cr.set_operator(cairo.OPERATOR_OVER)
            for s in self.strokes:
                render_stroke(cr, s)
            self.cache_dirty = False

    def draw(self, area, cr, w, h):
        st = self.st
        bg = BOARDS[st.board]
        if bg is not None:
            cr.set_source_rgb(*bg)
            cr.paint()
        self._ensure_cache(w, h)
        cr.set_source_surface(self.cache, 0, 0)
        cr.paint()
        if self.cur is not None:
            render_stroke(cr, self.cur)
        now = time.monotonic()
        for s in self.lasers:
            render_stroke(cr, s, alpha_mult=max(0.0, 1 - (now - s.ended) / self.cfg.laser_fade))
        if self.editing is not None:
            self._draw_text_edit(cr, self.editing)

        if self.app.chrome_hidden:
            return
        recording = self.app.recorder.active
        if st.drawing_mode and not recording:
            # Thin coloured frame = the overlay is capturing input.
            cr.set_source_rgba(*st.color, 0.55)
            cr.set_line_width(3)
            cr.rectangle(1.5, 1.5, w - 3, h - 3)
            cr.stroke()
        if st.drawing_mode and self.hover is not None:
            self._draw_brush_preview(cr, *self.hover)
        if recording:
            return
        if now < self.hud_until or (st.drawing_mode and st.show_help):
            self._draw_hud(cr, w, h)

    def _draw_text_edit(self, cr, s):
        render_stroke(cr, s)
        x0, y0, x1, y1 = s.bbox()
        if self.app.chrome_hidden:
            return
        cr.save()
        cr.set_source_rgba(*s.color, 0.8)
        cr.set_line_width(1)
        cr.set_dash([4, 3])
        cr.rectangle(x0 - 4, y0 - 2, max(x1 - x0, s.size * 0.6) + 8, y1 - y0 + 4)
        cr.stroke()
        cr.restore()

    def _draw_brush_preview(self, cr, x, y):
        st, cfg = self.st, self.cfg
        tool = "eraser" if self.erasing else st.tool
        if tool == "eraser":
            r = st.size * cfg.eraser_scale / 2 + 4
        elif tool == "highlighter":
            r = st.size * cfg.highlighter_scale / 2
        elif tool == "text":
            r = 3
        else:
            r = max(st.size / 2, 2)
        cr.save()
        cr.arc(x, y, r, 0, 2 * math.pi)
        cr.set_source_rgba(0, 0, 0, 0.6)
        cr.set_line_width(2.5)
        cr.stroke_preserve()
        cr.set_source_rgba(1, 1, 1, 0.9)
        cr.set_line_width(1)
        cr.stroke()
        cr.restore()

    def _draw_hud(self, cr, w, h):
        st = self.st
        mode = "DRAW" if st.drawing_mode else "PASS (screen usable)"
        lines = [f"{mode}   {st.tool}   size {st.size:.0f}   [{self.connector}]"]
        if st.drawing_mode and st.show_help:
            lines += HELP
        elif st.drawing_mode:
            lines.append("F1 help   Esc to use screen")
        cr.save()
        cr.select_font_face("monospace", cairo.FONT_SLANT_NORMAL, cairo.FONT_WEIGHT_BOLD)
        cr.set_font_size(15)
        lh = 22
        tw = max(cr.text_extents(line).x_advance for line in lines)
        bw, bh = tw + 56, lh * len(lines) + 18
        x0, y0 = (w - bw) / 2, 24
        cr.set_source_rgba(0.07, 0.07, 0.09, 0.85)
        _rounded(cr, x0, y0, bw, bh, 10)
        cr.fill()
        cr.set_source_rgb(*st.color)
        cr.arc(x0 + 20, y0 + 9 + lh * 0.55, 7, 0, 2 * math.pi)
        cr.fill()
        cr.set_source_rgb(0.95, 0.95, 0.95)
        for i, line in enumerate(lines):
            cr.move_to(x0 + 38, y0 + 9 + lh * (i + 1) - 6)
            cr.show_text(line)
        cr.restore()


def _rounded(cr, x, y, w, h, r):
    cr.new_sub_path()
    cr.arc(x + w - r, y + r, r, -math.pi / 2, 0)
    cr.arc(x + w - r, y + h - r, r, 0, math.pi / 2)
    cr.arc(x + r, y + h - r, r, math.pi / 2, math.pi)
    cr.arc(x + r, y + r, r, math.pi, 1.5 * math.pi)
    cr.close_path()
