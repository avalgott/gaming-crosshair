# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.2.0] - 2026-09-20

### Summary

Second release: a calibration panel (`crosshair --calibrate`) that moves
the dot while it stays live on every monitor. Arrow keys nudge one
logical pixel per press, sliders cover the ±100 px fine range, and Reset
recenters. It runs detached like `--start`, starts the overlay if none is
running, and Esc closes the window, ending the panel's own process.

### Added

- `crosshair --calibrate`: a small dark panel ("Crosshair Calibration")
  that moves the dot with the arrow keys (Left/Right horizontal, Up/Down
  vertical, 1 px per press) and draggable sliders under each offset row (up
  to ±100 px). Every change saves the config and applies live on every
  monitor. It runs detached like `--start`, so the terminal returns as
  soon as the panel is up, and Esc closes the window, ending the process.
  It starts the overlay automatically if none is running, so there is
  always a dot to watch; closing the window leaves the crosshair on until
  `crosshair --stop`. Reset recenters, and saving rewrites config.toml
  from the parsed values (comments and unknown keys are not preserved).

## [v0.1.0] - 2026-09-19

### Summary

The first release: a tiny app that draws a white dot at the exact center of
every screen — like the built-in crosshairs on gaming monitors. It floats
above all windows (fullscreen games included), never steals focus, and
ignores clicks. Turn it on with `crosshair --start` and off with
`crosshair --stop`; it runs in the background, so no terminal stays open.
The dot can be moved, resized, and recolored from a small config file, and
changes apply instantly without restarting.

### Added

- `crosshair --start` / `crosshair --stop` commands. `--start` runs
  detached in the background — the terminal returns immediately and Ctrl+C
  has nothing to kill; `--stop` from any terminal turns it off.
- A white 4×4 px dot at the true center of every monitor. On desktops with
  a status bar the app corrects the bar offset automatically, so the dot
  sits at the real screen center, not the center of the leftover area.
- Click-through and focus-free behavior on every backend: the surface has
  an empty input region, so mouse and keyboard events pass straight
  through.
- Wayland layer-shell support (wlroots compositors, KDE, Mir): the dot
  sits on the overlay layer, above every window including fullscreen ones.
- X11 support (Xorg, and XWayland games via `GDK_BACKEND=x11`): a
  fullscreen window kept above other X11 windows.
- GNOME Wayland fallback (Mutter has no layer shell): a plain fullscreen
  window — above desktop apps, not above native fullscreen windows
  (GNOME limitation).
- Per-user config at `~/.config/crosshair/config.toml`: dot size (2–64 px),
  color, and `offset_x`/`offset_y` to nudge the position. Missing or
  invalid files fall back to safe defaults; nothing is ever written to
  disk.
- Live reload: `kill -USR1 $(cat $XDG_RUNTIME_DIR/crosshair.pid)` re-reads
  the config and redraws every dot without a restart.
- Clear failure reporting: `--start` errors (already running, no display…)
  still print in the terminal; later diagnostics go to
  `$XDG_RUNTIME_DIR/crosshair.log`.

### Fixed

- PID file hardening: the daemon holds an exclusive lock on the file for its
  lifetime, so `--stop` can never signal an unrelated process that reused a
  stale PID, and malformed contents (zero, negative, or garbage) are removed
  instead of being interpreted as kill targets
- Stale PID file cleanup now unlinks while still holding the flock, closing
  a race where a concurrent `--start` could have its fresh PID file deleted,
  and the `/tmp` fallback (no `XDG_RUNTIME_DIR`) uses a mode-0700 per-user
  directory so other users cannot pre-create or symlink the PID and log
  files
- Non-ASCII color values now fall back to white instead of crashing startup
  or live reload
- The X11 and GNOME fallback backends now draw the dot on every monitor,
  matching the layer-shell backend and the README claim

[v0.2.0]: https://github.com/avalgott/gaming-crosshair/releases/tag/v0.2.0
[v0.1.0]: https://github.com/avalgott/gaming-crosshair/releases/tag/v0.1.0
