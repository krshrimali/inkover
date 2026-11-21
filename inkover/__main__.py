"""CLI: `inkover [command]`. Starts the overlay if needed, otherwise sends the command."""
import os
import subprocess
import sys
import time

from . import __version__
from . import ipc

LAYER_SHELL_LIB = "/usr/lib/libgtk4-layer-shell.so"
STARTERS = {"toggle", "draw", "pass", "start"}


def run_daemon():
    # gtk4-layer-shell has to be loaded before libwayland-client, so re-exec
    # ourselves with it preloaded if that hasn't happened yet.
    preload = os.environ.get("LD_PRELOAD", "")
    if "gtk4-layer-shell" not in preload and os.path.exists(LAYER_SHELL_LIB):
        env = dict(os.environ, LD_PRELOAD=f"{LAYER_SHELL_LIB} {preload}".strip())
        os.execvpe(sys.executable, [sys.executable, "-m", "inkover", "daemon"], env)
    from .app import InkoverApp
    InkoverApp().run([])


def spawn_daemon(start_in_pass=False):
    log = open(os.path.join(os.environ.get("XDG_RUNTIME_DIR", "/tmp"), "inkover.log"), "ab")
    env = dict(os.environ)
    if start_in_pass:
        env["INKOVER_START_PASS"] = "1"
    pkg_parent = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    env["PYTHONPATH"] = pkg_parent + os.pathsep + env.get("PYTHONPATH", "")
    subprocess.Popen([sys.executable, "-m", "inkover", "daemon"], env=env, stdout=log,
                     stderr=log, stdin=subprocess.DEVNULL, start_new_session=True)
    for _ in range(50):     # wait up to ~5s for the socket
        time.sleep(0.1)
        if ipc.send(["status"]) is not None:
            return True
    return False


def main(argv=None):
    args = list(sys.argv[1:] if argv is None else argv) or ["toggle"]
    if args[0] in ("-h", "--help", "help"):
        from .app import COMMANDS_HELP
        print(f"inkover {__version__}: draw on your live screen\n\n"
              f"usage: inkover [command]   (default: toggle; starts the overlay if not running)\n"
              f"       inkover daemon      run in the foreground\n\n{COMMANDS_HELP}")
        return 0
    if args[0] in ("-V", "--version"):
        print(__version__)
        return 0
    if args[0] == "daemon":
        run_daemon()
        return 0

    reply = ipc.send(args)
    if reply is None:
        if args[0] not in STARTERS:
            print("inkover is not running", file=sys.stderr)
            return 1
        if not spawn_daemon(start_in_pass=args[0] == "pass"):
            print("inkover: daemon failed to start; see $XDG_RUNTIME_DIR/inkover.log", file=sys.stderr)
            return 1
        return 0
    print(reply)
    return 0 if not reply.startswith("error") else 1


if __name__ == "__main__":
    sys.exit(main())
