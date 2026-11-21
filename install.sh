#!/bin/sh
# Symlinks the `inkover` launcher into ~/.local/bin and drops an example config.
set -e
repo=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$HOME/.local/bin"
ln -sf "$repo/bin/inkover" "$HOME/.local/bin/inkover"
echo "linked ~/.local/bin/inkover -> $repo/bin/inkover"

cfg="${XDG_CONFIG_HOME:-$HOME/.config}/inkover/config.toml"
if [ ! -e "$cfg" ]; then
    mkdir -p "$(dirname "$cfg")"
    cp "$repo/config.example.toml" "$cfg"
    echo "wrote example config to $cfg"
fi

if command -v cargo >/dev/null 2>&1; then
    echo "building the control panel (inkover-gui)…"
    (cd "$repo/gui" && cargo build --release --quiet)
    ln -sf "$repo/gui/target/release/inkover-gui" "$HOME/.local/bin/inkover-gui"
    mkdir -p "$HOME/.local/share/applications"
    cp "$repo/contrib/inkover.desktop" "$HOME/.local/share/applications/inkover.desktop"
    echo "linked ~/.local/bin/inkover-gui and installed the app launcher"
else
    echo "cargo not found: skipping the GUI (the overlay works without it)"
fi

missing=""
for bin in grim slurp wl-copy notify-send; do
    command -v "$bin" >/dev/null 2>&1 || missing="$missing $bin"
done
python3 -c 'import gi; gi.require_version("Gtk4LayerShell", "1.0")' 2>/dev/null \
    || missing="$missing gtk4-layer-shell"
[ -n "$missing" ] && echo "optional/required packages not found:$missing"
echo "done. Bind a key to: inkover toggle"
