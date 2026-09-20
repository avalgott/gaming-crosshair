#!/bin/sh
# Remove crosshair: stop the overlay, delete the binary and the config.
# CROSSHAIR_INSTALL_DIR overrides the binary location. Idempotent: missing
# pieces are reported, not treated as failures. The one abort path: the
# overlay is running and ignores SIGTERM — uninstalling would leave a live
# daemon executing a deleted binary with no way to stop it.
set -eu

INSTALL_DIR=${CROSSHAIR_INSTALL_DIR:-"$HOME/.local/bin"}
BIN="$INSTALL_DIR/crosshair"
# Matches where the binary writes its config (config.rs).
CFG_DIR=${XDG_CONFIG_HOME:-"$HOME/.config"}/crosshair

# --stop exits 0 when it stopped the overlay, and 1 for both "not running"
# (fine) and "daemon ignored SIGTERM for 2 s" (not fine). Distinguish them
# by the message.
stop_overlay() {
    if out=$("$1" --stop 2>&1); then
        return 0
    fi
    case "$out" in
        *"not running"*) return 0 ;;
        *)
            echo "$out" >&2
            echo "crosshair: the overlay is still running; not removing anything" >&2
            exit 1
            ;;
    esac
}

if [ -x "$BIN" ]; then
    stop_overlay "$BIN"
elif command -v crosshair >/dev/null 2>&1; then
    # A crosshair installed somewhere else on PATH.
    stop_overlay crosshair
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
