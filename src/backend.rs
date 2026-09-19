#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Wayland with wlr-layer-shell (wlroots compositors, KDE, Mir).
    LayerShell,
    /// Wayland without layer-shell (GNOME Mutter) — plain fullscreen window.
    XdgFallback,
    /// X11 (Xorg or XWayland games).
    X11,
}

pub fn detect() -> Backend {
    // Explicit override for XWayland/Xorg games.
    if std::env::var_os("GDK_BACKEND").is_some_and(|v| v == "x11") {
        return Backend::X11;
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return if gtk4_layer_shell::is_supported() {
            Backend::LayerShell
        } else {
            Backend::XdgFallback
        };
    }
    if std::env::var_os("DISPLAY").is_some() {
        return Backend::X11;
    }
    eprintln!("crosshair: no Wayland or X11 display found");
    std::process::exit(1);
}
