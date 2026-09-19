# crosshair

A tiny standalone overlay that draws a white dot at the exact center of every
screen — like the built-in crosshairs on gaming monitors (ASUS GamePlus,
BenQ, LG). It floats above all windows, is fully click-through, never takes
focus, and works on any Linux distribution.

```
crosshair --start   # show the dot on every monitor — runs in the background
crosshair --stop    # remove it
```

`--start` double-forks into the background: the terminal returns
immediately and Ctrl+C has nothing to kill. `crosshair --stop` (from any
terminal) is the off switch. Startup errors are still shown in the
terminal; later diagnostics go to `$XDG_RUNTIME_DIR/crosshair.log` (fallback
`/tmp/crosshair-<uid>.log`).

The dot is 4×4 px, pure white, configurable via
`~/.config/crosshair/config.toml`:

```toml
[dot]
size = 4            # 2..64 px (even sizes center symmetrically)
color = "#ffffff"   # #rgb or #rrggbb
offset_x = 0        # nudge off center: positive = right, in logical px
offset_y = 0        # positive = down
```

Missing or invalid config files fall back to the defaults above. Nothing is
ever written to disk.

## Tuning the dot position

The dot is drawn at the **true screen center**. On compositors with a status
bar (Hyprland, sway, KDE…) the layer surface does not span the bar, so the
app corrects the bar offset automatically: it knows the monitor size and the
surface size, and the difference is exactly the bar height. No bar ⇒ no
correction.

If the dot still doesn't line up with the crosshair you care about (in-game
HUDs are rarely at the exact pixel center), nudge it with `offset_x` /
`offset_y` — no restart needed, changes apply live:

```bash
$EDITOR ~/.config/crosshair/config.toml   # tweak offsets
kill -USR1 $(cat $XDG_RUNTIME_DIR/crosshair.pid)   # or: pkill -USR1 crosshair
```

Offsets are signed, in logical pixels, relative to the (already corrected)
screen center, and apply to every monitor.

## Requirements

- GTK4 (≥ 4.22 tested) and gtk4-layer-shell (≥ 1.3 tested), both provided by
  the system package manager
- Rust toolchain ≥ 1.92

```bash
cargo build --release
cargo install --path .        # optional: puts `crosshair` on your PATH
```

## How it works

| Backend | When | How |
|---|---|---|
| Layer shell | Wayland on wlroots (Hyprland/sway), KDE, Mir | `zwlr_layer_shell_v1` overlay layer, one fullscreen transparent surface per monitor. Above every window, including fullscreen ones. |
| XDG fallback | GNOME Wayland (Mutter has no layer shell) | Plain fullscreen transparent window — above desktop apps, but not above native fullscreen windows (GNOME limitation) |
| X11 | Xorg, or XWayland games | Fullscreen window + `_NET_WM_STATE_ABOVE` + direct restack every 2 s + empty XShape input region + `WM_HINTS input=false` (never takes focus) |

Backend detection is automatic. To force the X11 backend (e.g. for games
running in XWayland): `GDK_BACKEND=x11 crosshair --start`.

In every mode the surface has an empty input region — clicks, keys and scroll
pass straight through to whatever is underneath, and the window never takes
focus.

## Known limitations

- **GNOME Wayland**: no layer shell, so the dot does not appear over native
  fullscreen windows (games). A GNOME Shell extension could fix this later.
- **X11 mode on a Wayland compositor**: the X11 window stays above other X11
  windows (games in XWayland) but Wayland-native apps can cover it. Use the
  default layer-shell mode for Wayland apps.
- **Hyprland's XWM** ignores the `_NET_WM_STATE_ABOVE` request on fullscreen
  XWayland windows; the app compensates with a direct restack every 2 s.
- No monitor hotplug handling (restart to pick up a new monitor), and at
  fractional scales the dot edges are antialiased — both acceptable for now.
- The automatic bar correction is exact for a top bar (the common case) and
  does nothing when there is no bar; a bottom or side bar leaves a small
  residual that the user offsets absorb.

## Verification (Hyprland 0.56, NVIDIA RTX 5080)

- Layer surfaces land on overlay level 3 on all monitors, anchored below the
  bar; the dot's pixels sit at each monitor's **true screen center** — the
  bar offset is corrected automatically (verified by screenshot pixel
  analysis at scales 1.25 and 1.6; eDP-2 lands within ~1 native px, the
  fractional-scale rasterization bias)
- User offsets + live reload: SIGUSR1 re-reads the config and redraws
  without a restart — verified on Hyprland (dot jumped by exactly the
  configured offset on all monitors, then back)
- Click-through verified on X11 by reading the window's input shape back
  (0 rectangles); on Wayland the same GDK call maps to
  `wl_surface.set_input_region(empty)`
- `--start` returns in ~30 ms (daemonized) and shows the dot; `--start`
  twice → "already running", exit 1; `--stop` from any terminal → exit 0,
  PID file removed; `--stop` with nothing running → exit 1; SIGTERM → clean
  exit, PID file removed
