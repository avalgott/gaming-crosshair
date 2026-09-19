#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Backend {
    /// Wayland with wlr-layer-shell (wlroots compositors, KDE, Mir).
    LayerShell,
    /// Wayland without layer-shell (GNOME Mutter) — plain fullscreen window.
    XdgFallback,
    /// X11 (Xorg or XWayland games).
    X11,
}

/// Requires GTK to be initialized first (gtk_layer_is_supported needs
/// gtk_init).
pub fn detect() -> Result<Backend, String> {
    // Explicit override for XWayland/Xorg games.
    if std::env::var_os("GDK_BACKEND").is_some_and(|v| v == "x11") {
        return Ok(Backend::X11);
    }
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Ok(if gtk4_layer_shell::is_supported() {
            Backend::LayerShell
        } else {
            Backend::XdgFallback
        });
    }
    if std::env::var_os("DISPLAY").is_some() {
        return Ok(Backend::X11);
    }
    Err("no Wayland or X11 display found".into())
}
