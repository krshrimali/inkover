"""Tiny line-based control socket so `inkover <command>` can drive the daemon."""
import os
import socket

from gi.repository import GLib

SOCK_PATH = os.path.join(os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "inkover.sock")


class Server:
    def __init__(self, handler):
        self.handler = handler
        if os.path.exists(SOCK_PATH):
            os.unlink(SOCK_PATH)
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.bind(SOCK_PATH)
        os.chmod(SOCK_PATH, 0o600)
        self.sock.listen(8)
        self.sock.setblocking(False)
        GLib.io_add_watch(self.sock.fileno(), GLib.PRIORITY_DEFAULT, GLib.IO_IN, self._accept)

    def _accept(self, *_):
        try:
            conn, _ = self.sock.accept()
        except BlockingIOError:
            return True
        with conn:
            conn.settimeout(1.0)
            data = b""
            try:
                while not data.endswith(b"\n"):
                    chunk = conn.recv(4096)
                    if not chunk:
                        break
                    data += chunk
                reply = self.handler(data.decode().split())
                conn.sendall((reply or "ok").encode() + b"\n")
            except Exception as e:
                try:
                    conn.sendall(f"error: {e}\n".encode())
                except OSError:
                    pass
        return True

    def close(self):
        try:
            self.sock.close()
            os.unlink(SOCK_PATH)
        except OSError:
            pass


def send(args, timeout=2.0):
    """Send a command to the running daemon. Returns reply, or None if not running."""
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    try:
        s.connect(SOCK_PATH)
    except (FileNotFoundError, ConnectionRefusedError):
        return None
    with s:
        s.sendall((" ".join(args) + "\n").encode())
        out = b""
        while True:
            chunk = s.recv(4096)
            if not chunk:
                break
            out += chunk
        return out.decode().strip()
