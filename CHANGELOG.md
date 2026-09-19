# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/avalgott/gaming-crosshair/compare/v0.1.0...HEAD
[v0.1.0]: https://github.com/avalgott/gaming-crosshair/releases/tag/v0.1.0
