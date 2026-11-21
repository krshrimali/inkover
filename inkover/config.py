"""User configuration, loaded from ~/.config/inkover/config.toml (all keys optional)."""
import os
import tomllib
from dataclasses import dataclass, field

CONFIG_PATH = os.path.join(
    os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")), "inkover", "config.toml")

DEFAULT_PALETTE = [
    "#ee3333",  # 1 red
    "#26bf4d",  # 2 green
    "#3380f2",  # 3 blue
    "#ffd91a",  # 4 yellow
    "#ff8c1a",  # 5 orange
    "#bf4de6",  # 6 purple
    "#ffffff",  # 7 white
    "#0d0d0d",  # 8 black
    "#1ad9e6",  # 9 cyan
]

TOOLS = ("pen", "highlighter", "eraser", "laser", "line", "arrow", "rect", "ellipse", "text")


def parse_color(s):
    s = s.strip().lstrip("#")
    if len(s) == 3:
        s = "".join(c * 2 for c in s)
    if len(s) != 6:
        raise ValueError(f"bad colour {s!r}")
    return tuple(int(s[i:i + 2], 16) / 255 for i in (0, 2, 4))


@dataclass
class Config:
    palette: list = field(default_factory=lambda: [parse_color(c) for c in DEFAULT_PALETTE])
    size: float = 5.0
    size_min: float = 1.0
    size_max: float = 80.0
    highlighter_alpha: float = 0.35
    highlighter_scale: float = 4.0
    eraser_scale: float = 4.0
    laser_fade: float = 1.2           # seconds a laser stroke lingers after lifting
    laser_color: tuple = (1.0, 0.15, 0.15)
    font: str = "Sans Bold"
    text_scale: float = 5.0           # font px = size * text_scale
    toolbar: str = "left"             # left | right | top | bottom | off
    monitors: list = field(default_factory=list)   # empty = all
    save_dir: str = os.path.expanduser("~/Pictures/inkover")
    copy_to_clipboard: bool = True
    notify: bool = True
    start_in_pass: bool = False
    # What each pen side button does while pressed: any tool name, or "none".
    pen_button_lower: str = "eraser"  # GTK button 2
    pen_button_upper: str = "eraser"  # GTK button 3

    @classmethod
    def load(cls, path=CONFIG_PATH):
        cfg = cls()
        data = {}
        if os.path.exists(path):
            with open(path, "rb") as f:
                data = tomllib.load(f)
        for key, val in data.items():
            if not hasattr(cfg, key):
                print(f"inkover: unknown config key {key!r} ignored", flush=True)
                continue
            if key == "palette":
                val = [parse_color(c) for c in val][:9]
            elif key == "laser_color":
                val = parse_color(val)
            elif key == "save_dir":
                val = os.path.expanduser(val)
            setattr(cfg, key, val)
        if os.environ.get("INKOVER_START_PASS"):
            cfg.start_in_pass = True
        env_mons = os.environ.get("INKOVER_MONITORS")
        if env_mons is not None:
            cfg.monitors = [m for m in env_mons.split(",") if m]
        return cfg
