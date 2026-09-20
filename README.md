# crosshair

A tiny overlay that puts a white dot at the exact center of every screen,
like the built-in crosshairs on gaming monitors (ASUS GamePlus, BenQ, LG).
It floats above everything, fullscreen games included, never steals focus,
and clicks pass straight through to whatever is underneath. Works on any
Linux distribution.

```
crosshair --start     # show the dot on every monitor, runs in the background
crosshair --calibrate # open the panel and move the dot live
crosshair --update    # update to the latest release
crosshair --stop      # remove it
```

`--start` runs in the background: the terminal returns right away and
Ctrl+C has nothing to kill. `--calibrate` detaches the same way and opens
a small panel; Esc (or the close button) ends it. `crosshair --stop` from
any terminal turns everything off. Startup problems still print in the
terminal; anything that happens later goes to
`$XDG_RUNTIME_DIR/crosshair.log` (or `/tmp/crosshair-<uid>.log`).

## Calibrating the crosshair

The dot starts at the true center of every monitor, even on desktops with
a status bar. If it does not line up with the crosshair you care about
(in-game HUDs are rarely at the exact pixel center), open the panel:

```
crosshair --calibrate
```

- Use the keyboard arrow keys: Left/Right move the dot horizontally,
  Up/Down vertically, one pixel per press (hold a key to keep going).
- Drag the sliders under each row to set an offset directly (up to ±100
  px); the arrow keys reach the full ±2000 range.
- Every change saves and applies instantly, no restart needed.
- If the overlay is not running, the panel starts it for you, so there is
  always a dot to watch. Closing the panel leaves the crosshair on; run
  `crosshair --stop` to remove it.
- Reset puts the dot back at the true center.

## Configuring the dot

You can also change the dot by hand, in
`~/.config/crosshair/config.toml`:

```toml
[dot]
size = 4            # 2..64 px (even sizes center symmetrically)
color = "#ffffff"   # #rgb or #rrggbb
offset_x = 0        # positive = right, in logical px
offset_y = 0        # positive = down
```

Missing or invalid files fall back to the defaults above. The overlay
reads the file when it starts, so edit it before `crosshair --start`, or
use the panel for live changes. `--start` and `--stop` never write to
disk; `--calibrate` saves the config, rewriting the file from the parsed
values, so comments and unknown keys in a hand-edited file are not
preserved.

## Install

Paste this into a terminal. No sudo, everything installs as your user:

```bash
curl -fsSL https://raw.githubusercontent.com/avalgott/gaming-crosshair/main/install.sh | sh
```

The script downloads the latest release, checks its SHA-256 checksum, and
installs the binary to `~/.local/bin` (set `CROSSHAIR_INSTALL_DIR` to pick
another directory). If `~/.local/bin` is not on your PATH it says so, with
the one-liner to fix it. It is a small script; download and read it before
piping if you prefer.

Update whenever you like:

```
crosshair --update
```

It downloads the new version, verifies the checksum, and swaps itself in
place. A running overlay is stopped and restarted automatically, and if
the new version fails to start, the previous one is rolled back. The
calibration panel also shows an "Update available" link when a newer
release exists, and nothing at all otherwise (offline, GitHub down,
already current).

To remove crosshair completely:

```bash
curl -fsSL https://raw.githubusercontent.com/avalgott/gaming-crosshair/main/uninstall.sh | sh
```

It stops the overlay and removes the binary and `~/.config/crosshair`
(`XDG_CONFIG_HOME` is honored).

The prebuilt binary needs system GTK4 (≥ 4.10) and gtk4-layer-shell, the
same runtime libraries the source build needs, and glibc ≥ 2.39 on
x86_64 (CI builds on Ubuntu 24.04).

## Compatibility

crosshair picks the right backend automatically:

- Wayland on wlroots (Hyprland, sway), KDE and Mir: the dot sits on the
  overlay layer, above every window including fullscreen games.
- GNOME Wayland: above desktop apps, but not above native fullscreen
  windows (a GNOME Shell limitation; a shell extension could fix this
  later).
- Xorg: a fullscreen window kept above everything. Games running in
  XWayland need the X11 backend forced: `GDK_BACKEND=x11 crosshair
  --start`.

In every mode the overlay ignores input completely, so clicks, keys and
scroll pass straight through, and it never takes focus.

Two small caveats: plug in a new monitor and you need to restart
crosshair to pick it up, and at fractional scaling (125%, 150%...) the
dot edges look slightly soft. Both acceptable.

## Building from source

Prefer the installer above? Skip this. Otherwise you need Rust ≥ 1.92 and
the system GTK4 and gtk4-layer-shell libraries:

```bash
cargo build --release
cargo install --path .        # optional: puts `crosshair` on your PATH
```

## Support

If crosshair makes your aim a little truer and you'd like to support
ongoing development and future releases, consider buying me a coffee. It
genuinely helps keep the project going.

<a href="https://buymeacoffee.com/avalgott">
  <img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png"
       height="50"
       alt="Buy Me A Coffee">
</a>

Bug reports, feature suggestions, and contributions are always appreciated.
