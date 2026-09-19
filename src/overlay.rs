use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::backend::Backend;
use crate::config::Config;

/// Dot parameters shared by every draw closure, one per app instance.
/// SIGUSR1 swaps in freshly loaded values so offsets/size/color can be
/// tuned live without restarting.
#[derive(Clone, Copy)]
struct DotParams {
    r: f64,
    g: f64,
    b: f64,
    size: f64,
    offset_x: f64,
    offset_y: f64,
}

impl DotParams {
    fn from_config(cfg: &Config) -> Self {
        let (r, g, b) = crate::config::parse_hex_color(&cfg.dot.color).unwrap_or((1.0, 1.0, 1.0));
        Self {
            r,
            g,
            b,
            size: cfg.dot.size.clamp(2, 64) as f64,
            offset_x: cfg.dot.offset_x.clamp(-2000, 2000) as f64,
            offset_y: cfg.dot.offset_y.clamp(-2000, 2000) as f64,
        }
    }
}

/// Set by the SIGUSR1 handler (async-signal-safe: a flag store, nothing more).
static RELOAD_REQUESTED: AtomicBool = AtomicBool::new(false);

extern "C" fn handle_usr1(_: libc::c_int) {
    RELOAD_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn show_all(app: &gtk4::Application, backend: Backend, cfg: &Config) {
    install_transparent_css();

    let params = Rc::new(RefCell::new(DotParams::from_config(cfg)));
    let mut areas: Vec<gtk4::DrawingArea> = Vec::new();

    // One window per output on every backend, so the dot sits on every
    // monitor (the headline behavior, now true for X11 and the XDG fallback
    // too, not just the layer shell).
    let monitors = list_monitors();
    if monitors.is_empty() {
        // No monitor list (headless or broken display): one generic window
        // per backend, as before.
        match backend {
            Backend::LayerShell => make_layer_window(app, None, &params, &mut areas),
            Backend::XdgFallback => make_fallback_window(app, None, &params, &mut areas),
            Backend::X11 => make_x11_window(app, None, 0, &params, &mut areas),
        }
    } else {
        match backend {
            Backend::LayerShell => {
                for monitor in &monitors {
                    make_layer_window(app, Some(monitor), &params, &mut areas);
                }
            }
            Backend::XdgFallback => {
                for monitor in &monitors {
                    make_fallback_window(app, Some(monitor), &params, &mut areas);
                }
            }
            Backend::X11 => {
                for (index, monitor) in monitors.iter().enumerate() {
                    make_x11_window(app, Some(monitor), index, &params, &mut areas);
                }
            }
        }
    }

    watch_config_reload(params, areas);
}

fn list_monitors() -> Vec<gtk4::gdk::Monitor> {
    gtk4::gdk::Display::default()
        .map(|display| {
            let list = display.monitors();
            (0..list.n_items())
                .filter_map(|i| {
                    list.item(i).and_then(|o| o.downcast::<gtk4::gdk::Monitor>().ok())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn install_transparent_css() {
    // GTK4's default theme paints the window background; without this CSS the
    // "transparent" fullscreen window would be an opaque wall.
    let provider = gtk4::CssProvider::new();
    provider.load_from_data("window { background-color: transparent; }");
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

/// `kill -USR1 <pid>` re-reads the config and redraws every dot. The signal
/// handler only sets a flag; this main-loop timeout does the actual work.
fn watch_config_reload(params: Rc<RefCell<DotParams>>, areas: Vec<gtk4::DrawingArea>) {
    unsafe {
        libc::signal(libc::SIGUSR1, handle_usr1 as *const () as libc::sighandler_t);
    }
    let _reload_source = gtk4::glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
        if RELOAD_REQUESTED.swap(false, Ordering::SeqCst) {
            *params.borrow_mut() = DotParams::from_config(&crate::config::load());
            for area in &areas {
                area.queue_draw();
            }
        }
        gtk4::glib::ControlFlow::Continue
    });
    // SourceId has no Drop impl in glib 0.22 — the source stays attached to
    // the main context until the process exits, which is exactly what we want.
}

fn make_layer_window(
    app: &gtk4::Application,
    monitor: Option<&gtk4::gdk::Monitor>,
    params: &Rc<RefCell<DotParams>>,
    areas: &mut Vec<gtk4::DrawingArea>,
) {
    let window = gtk4::ApplicationWindow::new(app);
    window.set_decorated(false);
    window.set_focusable(false);
    window.set_title(Some("crosshair"));

    // No set_resizable(false) here: with a fixed-size window GTK pins its
    // min/max size to the default size, and the layer-shell configure
    // (monitor-sized) then gets clamped right back — the window would stay
    // 64x64 in the corner of the surface. Layer windows have no decorations
    // and KeyboardMode::None, so user resizing isn't possible anyway.

    // Must happen before the window is realized/presented.
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("crosshair"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(0);
    for (edge, anchored) in [
        (Edge::Left, true),
        (Edge::Right, true),
        (Edge::Top, true),
        (Edge::Bottom, true),
    ] {
        window.set_anchor(edge, anchored);
    }
    if let Some(m) = monitor {
        window.set_monitor(Some(m));
    }

    // Logical monitor size. The compositor may shrink an anchored layer
    // surface around bars, so the surface center is not the screen center —
    // the draw closure uses this to correct back to the true center.
    let monitor_size = monitor.map(|m| {
        let g = m.geometry();
        (g.width() as f64, g.height() as f64)
    });

    let area = dot_drawing_area(params, monitor_size);
    window.set_child(Some(&area));
    areas.push(area);
    window.connect_map(|w| apply_empty_input_region(w));
    window.present();
}

fn make_fallback_window(
    app: &gtk4::Application,
    monitor: Option<&gtk4::gdk::Monitor>,
    params: &Rc<RefCell<DotParams>>,
    areas: &mut Vec<gtk4::DrawingArea>,
) {
    // GNOME Wayland: no layer shell, no keep-above in GTK4 — fullscreen state
    // stacks above normal windows (not above native fullscreen games;
    // documented limitation). A fullscreen window spans the whole monitor,
    // so the surface center is the true screen center.
    let window = gtk4::ApplicationWindow::new(app);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_focusable(false);
    window.set_title(Some("crosshair"));
    match monitor {
        Some(m) => window.fullscreen_on_monitor(m),
        None => window.fullscreen(),
    }
    let area = dot_drawing_area(params, None);
    window.set_child(Some(&area));
    areas.push(area);
    window.connect_map(|w| apply_empty_input_region(w));
    window.present();
}

fn make_x11_window(
    app: &gtk4::Application,
    monitor: Option<&gtk4::gdk::Monitor>,
    index: usize,
    params: &Rc<RefCell<DotParams>>,
    areas: &mut Vec<gtk4::DrawingArea>,
) {
    let window = gtk4::ApplicationWindow::new(app);
    window.set_decorated(false);
    window.set_resizable(false);
    window.set_focusable(false);
    // Unique per window, not just per process: x11.rs finds each XID by
    // title.
    let title = format!("crosshair-{}-{index}", std::process::id());
    window.set_title(Some(&title));

    match monitor {
        Some(m) => window.fullscreen_on_monitor(m),
        None => window.fullscreen(),
    }

    let area = dot_drawing_area(params, None);
    window.set_child(Some(&area));
    areas.push(area);
    let t = title.clone();
    window.connect_map(move |w| {
        apply_empty_input_region(w);
        crate::x11::apply(&t);
    });
    window.present();
}

fn apply_empty_input_region(window: &gtk4::ApplicationWindow) {
    if let Some(surface) = window.surface() {
        // Some(&empty) = click-through on both Wayland and X11. None would
        // mean "reset to full input" — the opposite of what we want.
        let empty = gtk4::cairo::Region::create();
        surface.set_input_region(Some(&empty));
    }
}

fn dot_drawing_area(
    params: &Rc<RefCell<DotParams>>,
    // Some((monitor_w, monitor_h)) when the surface may not span the full
    // output (layer-shell under a bar); None → surface center is correct.
    monitor: Option<(f64, f64)>,
) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    let params = params.clone();
    area.set_draw_func(move |_, ctx, width, height| {
        let p = params.borrow();

        // Center of the surface, corrected to the true screen center when we
        // know the monitor size: the surface spans the output minus bars (all
        // four edges anchored, no margins), so with a top bar of height H−h
        // the true center H/2 sits at h − H/2 in surface coordinates. No bar
        // ⇒ h == H ⇒ plain surface center — the formula degrades gracefully.
        // (Exact for top bars, the common case; a bottom or side bar leaves a
        // small residual that the user offsets absorb.)
        let (mut cx, mut cy) = match monitor {
            Some((mw, mh)) => (width as f64 - mw / 2.0, height as f64 - mh / 2.0),
            None => (width as f64 / 2.0, height as f64 / 2.0),
        };
        // User offsets, relative to that center (positive = right/down).
        cx += p.offset_x;
        cy += p.offset_y;

        // Even-sized dot centered on the logical center: with even screen dims
        // (1920×1080) there is no single center pixel; 4×4 at the center keeps
        // symmetry. Integer math at scale 1 → pixel-perfect.
        let x = cx - p.size / 2.0;
        let y = cy - p.size / 2.0;
        ctx.set_source_rgb(p.r, p.g, p.b);
        ctx.rectangle(x, y, p.size, p.size);
        let _ = ctx.fill();
    });
    area
}
