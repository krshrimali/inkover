-- inkover binds for a Lua Hyprland config
hl.bind("SUPER + ALT + D",         hl.dsp.exec_cmd("~/.local/bin/inkover toggle"))
hl.bind("SUPER + ALT + C",         hl.dsp.exec_cmd("~/.local/bin/inkover clear all"))
hl.bind("SUPER + ALT + S",         hl.dsp.exec_cmd("~/.local/bin/inkover screenshot region"))
hl.bind("SUPER + ALT + V",         hl.dsp.exec_cmd("~/.local/bin/inkover record region"))
hl.bind("SUPER + ALT + G",         hl.dsp.exec_cmd("~/.local/bin/inkover-gui"))
hl.bind("SUPER + ALT + SHIFT + D", hl.dsp.exec_cmd("~/.local/bin/inkover quit"))
