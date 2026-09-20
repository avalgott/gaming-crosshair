#!/bin/sh
# Remove crosshair: stop the overlay, delete the binary and the config.
# CROSSHAIR_INSTALL_DIR overrides the binary location. Idempotent: every
# path exits 0, missing pieces are reported, not treated as failures.
set -eu

INSTALL_DIR=${CROSSHAIR_INSTALL_DIR:-"$HOME/.local/bin"}
BIN="$INSTALL_DIR/crosshair"
# Matches where the binary writes its config (config.rs).
CFG_DIR=${XDG_CONFIG_HOME:-"$HOME/.config"}/crosshair

if [ -x "$BIN" ]; then
    "$BIN" --stop || true
elif command -v crosshair >/dev/null 2>&1; then
    # A crosshair installed somewhere else on PATH.
    crosshair --stop || true
fi

if [ -e "$BIN" ]; then
    rm -f "$BIN"
    echo "removed $BIN"
else
    echo "crosshair not installed at $BIN (nothing to remove)"
fi

if [ -d "$CFG_DIR" ]; then
    rm -rf "$CFG_DIR"
    echo "removed config $CFG_DIR"
else
    echo "no config at $CFG_DIR (nothing to remove)"
fi

echo "uninstall complete"
