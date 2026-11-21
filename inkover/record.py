"""Screen recording (live screen + ink) via gpu-screen-recorder or wf-recorder."""
import os
import shutil
import signal
import subprocess
import time

from gi.repository import Gio, GLib

from .capture import _run


def _slurp_to_gsr(geom):
    """slurp's 'X,Y WxH' -> gpu-screen-recorder's 'WxH+X+Y'."""
    pos, size = geom.split()
    x, y = pos.split(",")
    return f"{size}+{x}+{y}"


def _default_sink_monitor():
    try:
        sink = subprocess.run(["pactl", "get-default-sink"], capture_output=True,
                              text=True, timeout=1).stdout.strip()
        return f"{sink}.monitor" if sink else None
    except (OSError, subprocess.SubprocessError):
        return None


class Recorder:
    def __init__(self, app):
        self.app = app
        self.proc = None
        self.path = None
        self.started = None
        self.stopping = False
        self.log_path = os.path.join(os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "inkover-record.log")

    @property
    def active(self):
        return self.proc is not None

    def elapsed(self):
        return 0 if self.started is None else int(time.monotonic() - self.started)

    def backend(self):
        want = self.app.cfg.recorder
        for name in (["gpu-screen-recorder", "wf-recorder"] if want == "auto" else [want]):
            if shutil.which(name):
                return name
        return None

    # ------------------------------------------------------------ control
    def toggle(self, canvas, region=False):
        if self.active:
            self.stop()
            return "stopping"
        backend = self.backend()
        if backend is None:
            msg = "no recorder found: install gpu-screen-recorder or wf-recorder"
            self.app.notify("Recording failed", msg)
            return f"error: {msg}"
        if region:
            def picked(ok, out):
                if ok and out.strip():
                    self._launch(backend, canvas, out.strip())
            _run(["slurp", "-d"], picked)
        else:
            self._launch(backend, canvas, None)
        return "starting"

    def _argv(self, backend, canvas, geom, path):
        cfg = self.app.cfg
        audio = cfg.record_audio
        if backend == "gpu-screen-recorder":
            argv = ["gpu-screen-recorder", "-f", str(cfg.record_fps), "-c", "mp4", "-o", path]
            if geom:
                argv += ["-w", "region", "-region", _slurp_to_gsr(geom)]
            else:
                argv += ["-w", canvas.connector]
            src = {"mic": "default_input", "desktop": "default_output",
                   "both": "default_output|default_input"}.get(audio)
            if src:
                argv += ["-a", src]
            return argv
        argv = ["wf-recorder", "-y", "-r", str(cfg.record_fps), "-f", path]
        argv += ["-g", geom] if geom else ["-o", canvas.connector]
        if audio == "mic":
            argv.append("--audio")
        elif audio in ("desktop", "both"):
            mon = _default_sink_monitor()
            argv.append(f"--audio={mon}" if mon else "--audio")
        return argv

    def _launch(self, backend, canvas, geom):
        cfg = self.app.cfg
        os.makedirs(cfg.record_dir, exist_ok=True)
        suffix = "region" if geom else canvas.connector
        self.path = os.path.join(cfg.record_dir, time.strftime(f"inkover_%Y-%m-%d_%H-%M-%S_{suffix}.mp4"))
        argv = self._argv(backend, canvas, geom, self.path)
        launcher = Gio.SubprocessLauncher.new(Gio.SubprocessFlags.STDOUT_SILENCE)
        launcher.set_stderr_file_path(self.log_path)
        try:
            self.proc = launcher.spawnv(argv)
        except GLib.Error as e:
            self.proc = None
            self.app.notify("Recording failed", e.message)
            return
        print(f"inkover: recording: {' '.join(argv)}", flush=True)
        self.started = time.monotonic()
        self.stopping = False
        self.proc.wait_async(None, self._on_exit)
        GLib.timeout_add(1000, self._tick)
        self.app.recording_changed()

    def stop(self):
        if self.proc is not None and not self.stopping:
            self.stopping = True
            self.proc.send_signal(signal.SIGINT)    # both recorders finalise the file on SIGINT
            # Don't hang forever if the recorder ignores us.
            proc = self.proc
            GLib.timeout_add_seconds(10, lambda: (proc.force_exit() if self.proc is proc else None, False)[1])

    def _tick(self):
        if not self.active:
            return False
        self.app.recording_changed(tick=True)
        return True

    def _on_exit(self, proc, res):
        try:
            proc.wait_finish(res)
        except GLib.Error:
            pass
        secs = self.elapsed()
        path, was_stopping = self.path, self.stopping
        self.proc = None
        self.started = None
        self.stopping = False
        self.app.recording_changed()
        if os.path.exists(path) and os.path.getsize(path) > 0:
            mins, s = divmod(secs, 60)
            self.app.notify(f"Recording saved ({mins}:{s:02d})", path)
            print(f"inkover: saved {path}", flush=True)
        else:
            tail = ""
            try:
                with open(self.log_path, errors="replace") as f:
                    tail = f.read()[-300:].strip()
            except OSError:
                pass
            why = "stopped" if was_stopping else "recorder exited"
            self.app.notify("Recording failed", f"{why}: {tail or 'see ' + self.log_path}")
