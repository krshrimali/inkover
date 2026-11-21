"""Screenshots (screen + ink, via grim) and ink-only PNG export."""
import os
import shlex
import time

import cairo
from gi.repository import Gio, GLib

from .strokes import render_stroke


def _out_path(cfg, suffix):
    os.makedirs(cfg.save_dir, exist_ok=True)
    return os.path.join(cfg.save_dir, time.strftime(f"inkover_%Y-%m-%d_%H-%M-%S_{suffix}.png"))


def _run(argv, on_done=None):
    """Run a command asynchronously; on_done(ok, stdout) when it exits."""
    try:
        proc = Gio.Subprocess.new(argv, Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_SILENCE)
    except GLib.Error as e:
        print(f"inkover: cannot run {argv[0]}: {e.message}", flush=True)
        if on_done:
            on_done(False, "")
        return

    def cb(p, res):
        try:
            _, out, _ = p.communicate_utf8_finish(res)
        except GLib.Error:
            out = ""
        if on_done:
            on_done(p.get_successful(), out or "")
    proc.communicate_utf8_async(None, None, cb)


def _deliver(app, path, what):
    cfg = app.cfg
    if cfg.copy_to_clipboard:
        _run(["sh", "-c", f"wl-copy --type image/png < {shlex.quote(path)}"])
    if cfg.notify:
        extra = " (copied to clipboard)" if cfg.copy_to_clipboard else ""
        _run(["notify-send", "-a", "inkover", "-i", path, "-t", "3000", f"{what} saved{extra}", path])
    print(f"inkover: saved {path}", flush=True)


def screenshot(app, canvas, region=False):
    """Capture the live screen with ink on top, toolbar/HUD hidden."""
    cfg = app.cfg

    def shoot(grim_args, suffix):
        app.set_chrome_hidden(True)
        path = _out_path(cfg, suffix)

        def grab():
            def done(ok, _out):
                app.set_chrome_hidden(False)
                if ok:
                    _deliver(app, path, "Screenshot")
                else:
                    print("inkover: grim failed", flush=True)
            _run(["grim", *grim_args, path], done)
            return False
        GLib.timeout_add(150, grab)   # give the compositor a frame without the chrome

    if region:
        def picked(ok, out):
            geom = out.strip()
            if ok and geom:
                shoot(["-g", geom], "region")
        _run(["slurp", "-d"], picked)
    else:
        shoot(["-o", canvas.connector], canvas.connector)


def export_ink(app, canvas):
    """Save just the ink (transparent background) at the monitor's native resolution."""
    canvas.finish()
    geo = canvas.monitor.get_geometry()
    scale = canvas.monitor.get_scale_factor()
    surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, geo.width * scale, geo.height * scale)
    surf.set_device_scale(scale, scale)
    cr = cairo.Context(surf)
    for s in canvas.strokes:
        render_stroke(cr, s)
    path = _out_path(app.cfg, f"ink_{canvas.connector}")
    surf.write_to_png(path)
    _deliver(app, path, "Ink")
