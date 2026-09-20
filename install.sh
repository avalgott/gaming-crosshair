#!/bin/sh
# Install crosshair to ~/.local/bin from the latest GitHub release.
# CROSSHAIR_INSTALL_DIR overrides the destination; CROSSHAIR_DL_BASE
# overrides the download base (used by the tests and self-hosted mirrors).
set -eu

INSTALL_DIR=${CROSSHAIR_INSTALL_DIR:-"$HOME/.local/bin"}
DL_BASE=${CROSSHAIR_DL_BASE:-"https://github.com/avalgott/gaming-crosshair/releases/latest/download"}

mkdir -p "$INSTALL_DIR"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' 0 1 2 15

curl -fsSL -o "$tmp/crosshair" "$DL_BASE/crosshair" ||
    { echo "crosshair: download failed ($DL_BASE/crosshair)" >&2; exit 1; }
curl -fsSL -o "$tmp/crosshair.sha256" "$DL_BASE/crosshair.sha256" ||
    { echo "crosshair: download failed ($DL_BASE/crosshair.sha256)" >&2; exit 1; }

# The sidecar references the bare file name; verify from the staging dir.
(cd "$tmp" && sha256sum -c crosshair.sha256 >/dev/null)

# install(1) unlinks the destination before writing (unlike cp), so
# reinstalling over a running overlay never touches the inode the daemon
# is executing.
install -m 0755 "$tmp/crosshair" "$INSTALL_DIR/crosshair"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "warning: $INSTALL_DIR is not on your PATH" >&2
        echo "         add it to PATH, or run $INSTALL_DIR/crosshair" >&2
        ;;
esac

# The dynamic linker loads libgtk-4.so at process start, so this smoke test
# doubles as a runtime-dependency check.
if "$INSTALL_DIR/crosshair" --version >/dev/null 2>&1; then
    echo "installed: $("$INSTALL_DIR/crosshair" --version)"
else
    echo "warning: installed, but 'crosshair --version' failed;" >&2
    echo "         is GTK4 + gtk4-layer-shell installed? (see the README)" >&2
fi

# Best-effort hint: a running overlay keeps the previous binary until
# restarted.
if command -v pgrep >/dev/null 2>&1 && pgrep -x crosshair >/dev/null; then
    echo "note: the overlay is still running the previous binary;"
    echo "      restart it with 'crosshair --stop && crosshair --start'"
fi
