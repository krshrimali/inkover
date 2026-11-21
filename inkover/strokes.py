"""Ink model (strokes, shapes, text) and cairo rendering. No GTK widgets here."""
import math

import cairo
import gi
gi.require_version("Pango", "1.0")
gi.require_version("PangoCairo", "1.0")
from gi.repository import Pango, PangoCairo

SHAPES = ("line", "arrow", "rect", "ellipse")


def pressure_width(size, p):
    # A light touch still leaves a visible line; full pressure is ~1.6x the base size.
    p = max(0.0, min(1.0, p))
    return size * (0.25 + 1.35 * p ** 0.8)


class Stroke:
    __slots__ = ("tool", "color", "size", "pts", "text", "font", "alpha", "_bbox", "ended")

    def __init__(self, tool, color, size, alpha=1.0):
        self.tool = tool
        self.color = color
        self.size = size
        self.alpha = alpha
        self.pts = []          # [(x, y, pressure)]
        self.text = ""
        self.font = "Sans Bold"
        self._bbox = None
        self.ended = None      # laser: monotonic time the pen lifted

    def add(self, x, y, p):
        if self.tool in SHAPES:
            if len(self.pts) < 2:
                self.pts.append((x, y, p))
            else:
                self.pts[1] = (x, y, p)      # shapes keep start + current end
        else:
            if self.pts:
                lx, ly, _ = self.pts[-1]
                if (x - lx) ** 2 + (y - ly) ** 2 < 0.25:
                    return
            self.pts.append((x, y, p))
        self._bbox = None

    def bbox(self):
        if self._bbox is None:
            if self.tool == "text":
                w, h = text_extent(self)
                x, y, _ = self.pts[0]
                self._bbox = (x, y, x + w, y + h)
            else:
                xs = [p[0] for p in self.pts]
                ys = [p[1] for p in self.pts]
                pad = self.size * 2 + 20
                self._bbox = (min(xs) - pad, min(ys) - pad, max(xs) + pad, max(ys) + pad)
        return self._bbox

    def hit(self, x, y, r):
        if not self.pts:
            return False
        x0, y0, x1, y1 = self.bbox()
        if not (x0 - r <= x <= x1 + r and y0 - r <= y <= y1 + r):
            return False
        if self.tool == "text":
            return True
        r2 = (r + self.size) ** 2
        pts = self.pts
        if self.tool in ("rect", "ellipse") and len(pts) >= 2:
            (ax, ay, _), (bx, by, _) = pts[0], pts[1]
            if self.tool == "rect":
                c = [(ax, ay), (bx, ay), (bx, by), (ax, by), (ax, ay)]
                return any(seg_d2(x, y, *c[i], *c[i + 1]) <= r2 for i in range(4))
            cx, cy = (ax + bx) / 2, (ay + by) / 2
            rx, ry = abs(bx - ax) / 2 or 1, abs(by - ay) / 2 or 1
            d = math.hypot((x - cx) / rx, (y - cy) / ry)
            return abs(d - 1) * min(rx, ry) <= r + self.size
        if len(pts) == 1:
            return (pts[0][0] - x) ** 2 + (pts[0][1] - y) ** 2 <= r2
        return any(seg_d2(x, y, pts[i][0], pts[i][1], pts[i + 1][0], pts[i + 1][1]) <= r2
                   for i in range(len(pts) - 1))

    def is_empty(self):
        if self.tool == "text":
            return not self.text.strip()
        if self.tool in SHAPES:
            if len(self.pts) < 2:
                return True
            (ax, ay, _), (bx, by, _) = self.pts[:2]
            return abs(ax - bx) < 2 and abs(ay - by) < 2
        return not self.pts


def seg_d2(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    L = dx * dx + dy * dy
    t = 0.0 if L == 0 else max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / L))
    qx, qy = ax + t * dx - px, ay + t * dy - py
    return qx * qx + qy * qy


_measure_surface = cairo.ImageSurface(cairo.FORMAT_ARGB32, 1, 1)


def _layout(cr, s):
    layout = PangoCairo.create_layout(cr)
    desc = Pango.FontDescription.from_string(s.font)
    desc.set_absolute_size(s.size * Pango.SCALE)
    layout.set_font_description(desc)
    layout.set_text(s.text or " ", -1)
    return layout


def text_extent(s):
    layout = _layout(cairo.Context(_measure_surface), s)
    w, h = layout.get_pixel_size()
    return w, h


def render_stroke(cr, s, alpha_mult=1.0):
    pts = s.pts
    if not pts:
        return
    r, g, b = s.color
    a = s.alpha * alpha_mult
    cr.save()
    cr.set_line_cap(cairo.LINE_CAP_ROUND)
    cr.set_line_join(cairo.LINE_JOIN_ROUND)

    if s.tool == "pen":
        cr.set_source_rgba(r, g, b, a)
        if len(pts) == 1:
            x, y, p = pts[0]
            cr.arc(x, y, pressure_width(s.size, p) / 2, 0, 2 * math.pi)
            cr.fill()
        else:
            # Variable width: one segment per sample pair; round caps hide the joins.
            px, py, pp = pts[0]
            for x, y, p in pts[1:]:
                cr.set_line_width(pressure_width(s.size, (p + pp) / 2))
                cr.move_to(px, py)
                cr.line_to(x, y)
                cr.stroke()
                px, py, pp = x, y, p
    elif s.tool == "highlighter":
        # One constant-width path so overlapping parts don't get darker.
        cr.set_source_rgba(r, g, b, a)
        cr.set_line_width(s.size)
        cr.set_line_cap(cairo.LINE_CAP_SQUARE)
        smooth_path(cr, pts)
        cr.stroke()
    elif s.tool == "laser":
        smooth_path(cr, pts)
        path = cr.copy_path()
        cr.set_source_rgba(r, g, b, 0.25 * a)       # soft glow
        cr.set_line_width(s.size * 3.5)
        cr.stroke()
        cr.append_path(path)
        cr.set_source_rgba(r, g, b, 0.9 * a)
        cr.set_line_width(s.size)
        cr.stroke_preserve()
        cr.set_source_rgba(1, 1, 1, 0.8 * a)         # hot core
        cr.set_line_width(max(1.0, s.size * 0.35))
        cr.stroke()
    elif s.tool == "text":
        x, y, _ = pts[0]
        cr.move_to(x, y)
        layout = _layout(cr, s)
        PangoCairo.layout_path(cr, layout)
        # Dark outline keeps text readable on any background.
        lum = 0.3 * r + 0.59 * g + 0.11 * b
        o = 0.0 if lum > 0.5 else 1.0
        cr.set_source_rgba(o, o, o, 0.55 * a)
        cr.set_line_width(max(2.0, s.size / 10))
        cr.stroke_preserve()
        cr.set_source_rgba(r, g, b, a)
        cr.fill()
    elif len(pts) >= 2:
        cr.set_source_rgba(r, g, b, a)
        cr.set_line_width(s.size)
        (ax, ay, _), (bx, by, _) = pts[0], pts[1]
        if s.tool in ("line", "arrow"):
            cr.move_to(ax, ay)
            cr.line_to(bx, by)
            cr.stroke()
            if s.tool == "arrow":
                ang = math.atan2(by - ay, bx - ax)
                head = max(14.0, s.size * 4)
                for side in (-1, 1):
                    t = ang + math.pi + side * math.radians(28)
                    cr.move_to(bx, by)
                    cr.line_to(bx + head * math.cos(t), by + head * math.sin(t))
                cr.stroke()
        elif s.tool == "rect":
            cr.rectangle(min(ax, bx), min(ay, by), abs(bx - ax), abs(by - ay))
            cr.stroke()
        elif s.tool == "ellipse":
            cx, cy = (ax + bx) / 2, (ay + by) / 2
            rx, ry = max(abs(bx - ax) / 2, 0.5), max(abs(by - ay) / 2, 0.5)
            cr.save()
            cr.translate(cx, cy)
            cr.scale(rx, ry)
            cr.arc(0, 0, 1, 0, 2 * math.pi)
            cr.restore()
            cr.stroke()
    cr.restore()


def smooth_path(cr, pts):
    """Quadratic midpoint smoothing through the sample points."""
    cr.move_to(pts[0][0], pts[0][1])
    if len(pts) < 3:
        for x, y, _ in pts[1:]:
            cr.line_to(x, y)
        if len(pts) == 1:
            cr.line_to(pts[0][0] + 0.01, pts[0][1])
        return
    for i in range(1, len(pts) - 1):
        mx = (pts[i][0] + pts[i + 1][0]) / 2
        my = (pts[i][1] + pts[i + 1][1]) / 2
        x0, y0 = cr.get_current_point()
        cx, cy = pts[i][0], pts[i][1]
        cr.curve_to(x0 + 2 / 3 * (cx - x0), y0 + 2 / 3 * (cy - y0),
                    mx + 2 / 3 * (cx - mx), my + 2 / 3 * (cy - my), mx, my)
    cr.line_to(pts[-1][0], pts[-1][1])
