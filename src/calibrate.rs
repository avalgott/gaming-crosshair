use gtk4::gio::prelude::*;
use gtk4::prelude::*;

use std::cell::Cell;
use std::rc::Rc;

/// Calibration panel: a window that nudges the dot's offset_x/offset_y. Arrow keys move one logical pixel per press, the
/// slider under each row sets the offset by dragging. Every change saves
/// the config and SIGUSR1s the running daemon, so the dot moves live on
/// every monitor. If the daemon is not running it is started first, so
/// there is always a dot to watch while calibrating.
/// `report` is the detach pipe from main: it goes ready once the panel is
/// up, so the waiting terminal returns. The window owns its process — Esc
/// (or the close button) closes the window, the application quits, and the
/// process ends itself.
pub fn run(report: crate::DaemonReport) -> gtk4::glib::ExitCode {
    let app = gtk4::Application::new(
        Some("com.avalgott.crosshair.calibrate"),
        gtk4::gio::ApplicationFlags::default(),
    );
    app.connect_activate({
        let report = report.clone();
        move |app| {
            // Unique application id: a second `crosshair --calibrate`
            // activates this instance instead of building a second window.
            if !app.windows().is_empty() {
                app.windows()[0].present();
                report.ready();
                return;
            }
            ensure_daemon();
            build_window(app);
            report.ready();
        }
    });
    // GLib must not see "--calibrate" (its option parser would reject it);
    // clap already handled the arguments.
    let code = app.run_with_args::<&str>(&[]);
    // If a panel was already running, this instance's activate never fired
    // (the activation went to the primary); the panel is up by the time
    // run() returns, so report ready here. In the primary the pipe is
    // already closed and this is a harmless failed write.
    report.ready();
    code
}

/// Start the overlay if it is not running. Re-uses the normal `--start`
/// path (double fork, readiness report, pidfile); this process waits for
/// the spawned parent to report back, which it only does once the daemon
/// holds the PID file, so the first nudge always signals a live daemon.
fn ensure_daemon() {
    if crate::pidfile::running_pid().is_some() {
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            eprintln!("crosshair: cannot locate the binary to start the overlay: {e}");
            return;
        }
    };
    // Stdio is inherited, so --start failures print here normally.
    match std::process::Command::new(exe).arg("--start").status() {
        Ok(status) if status.success() => {}
        Ok(status) => eprintln!(
            "crosshair: could not start the overlay (--start exited with {status}); \
             offsets are saved and apply on the next --start"
        ),
        Err(e) => eprintln!(
            "crosshair: could not start the overlay: {e}; \
             offsets are saved and apply on the next --start"
        ),
    }
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
}

/// Everything a commit touches: the value labels, the sliders, and the
/// in-memory last-committed offsets. `syncing` guards against the
/// value-changed echo when commit() moves a slider programmatically.
struct Controls {
    label_x: gtk4::Label,
    label_y: gtk4::Label,
    scale_x: gtk4::Scale,
    scale_y: gtk4::Scale,
    current: Cell<(i32, i32)>,
    syncing: Cell<bool>,
}

fn build_window(app: &gtk4::Application) {
    install_css();

    let cfg = crate::config::load();

    let label_x = value_label(cfg.dot.offset_x);
    let label_y = value_label(cfg.dot.offset_y);
    let scale_x = slider();
    let scale_y = slider();

    let controls = Rc::new(Controls {
        label_x: label_x.clone(),
        label_y: label_y.clone(),
        scale_x: scale_x.clone(),
        scale_y: scale_y.clone(),
        current: Cell::new((cfg.dot.offset_x, cfg.dot.offset_y)),
        syncing: Cell::new(false),
    });

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.set_margin_top(20);
    root.set_margin_bottom(20);
    root.set_margin_start(20);
    root.set_margin_end(20);

    // Header: crosshair chip, then title + subtitle.
    let chip = gtk4::Label::new(Some("⊕"));
    chip.add_css_class("calibrate-chip");
    chip.set_halign(gtk4::Align::Start);
    chip.set_valign(gtk4::Align::Center);

    let title = gtk4::Label::new(Some("Crosshair Calibration"));
    title.add_css_class("calibrate-title");
    title.set_halign(gtk4::Align::Start);
    let subtitle = gtk4::Label::new(Some("Adjust the crosshair position using the arrow keys"));
    subtitle.add_css_class("calibrate-subtitle");
    subtitle.set_halign(gtk4::Align::Start);

    let header_text = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    header_text.set_valign(gtk4::Align::Center);
    header_text.append(&title);
    header_text.append(&subtitle);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    header.append(&chip);
    header.append(&header_text);
    root.append(&header);

    // Horizontal row, its slider, then the vertical pair. Each offset row
    // hugs its slider (they belong together), the two groups sit closer to
    // each other than to the header and footer.
    let row_x = offset_row("↔", "Horizontal offset", &label_x);
    row_x.set_margin_top(18);
    root.append(&row_x);

    scale_x.add_css_class("calibrate-slider");
    scale_x.set_margin_top(8);
    root.append(&scale_x);

    let row_y = offset_row("↕", "Vertical offset", &label_y);
    row_y.set_margin_top(8);
    root.append(&row_y);

    scale_y.add_css_class("calibrate-slider");
    scale_y.set_margin_top(8);
    root.append(&scale_y);

    // Bottom: Reset (left) and the Esc hint (right) share a row.
    let reset = gtk4::Button::new();
    reset.add_css_class("calibrate-reset");
    reset.set_halign(gtk4::Align::Start);
    let reset_icon = gtk4::Label::new(Some("↺"));
    reset_icon.add_css_class("calibrate-reset-icon");
    let reset_text = gtk4::Label::new(Some("Reset"));
    let reset_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    reset_box.append(&reset_icon);
    reset_box.append(&reset_text);
    reset.set_child(Some(&reset_box));

    let hint_esc = gtk4::Label::new(Some("Esc to close"));
    hint_esc.add_css_class("calibrate-esc");
    hint_esc.set_halign(gtk4::Align::End);
    hint_esc.set_valign(gtk4::Align::Center);
    hint_esc.set_hexpand(true);

    let bottom = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    bottom.set_margin_top(24);
    bottom.append(&reset);
    bottom.append(&hint_esc);
    root.append(&bottom);

    let window = gtk4::ApplicationWindow::new(app);
    window.set_title(Some("Crosshair Calibration"));
    window.set_resizable(false);
    window.set_default_size(460, -1);
    window.set_child(Some(&root));

    // Key handling on the window (bubble phase): arrows nudge even while
    // the Reset button has focus, and Stop keeps default keynav from
    // stealing them. Holding a key auto-repeats, one pixel per repeat.
    let key = gtk4::EventControllerKey::new();
    {
        let window = window.clone();
        let controls = controls.clone();
        key.connect_key_pressed(move |_, keyval, _code, _mods| {
            use gtk4::gdk::Key;
            match keyval {
                Key::Escape => {
                    window.close();
                    return gtk4::glib::Propagation::Stop;
                }
                Key::Left => step(&controls, Axis::X, -1),
                Key::Right => step(&controls, Axis::X, 1),
                Key::Up => step(&controls, Axis::Y, -1),
                Key::Down => step(&controls, Axis::Y, 1),
                _ => return gtk4::glib::Propagation::Proceed,
            }
            gtk4::glib::Propagation::Stop
        });
    }
    window.add_controller(key);

    {
        let controls = controls.clone();
        reset.connect_clicked(move |_| commit(&controls, 0, 0));
    }

    {
        let controls = controls.clone();
        scale_x.connect_value_changed(move |s| slider_changed(s, &controls, Axis::X));
    }
    {
        let controls = controls.clone();
        scale_y.connect_value_changed(move |s| slider_changed(s, &controls, Axis::Y));
    }

    // Initial slider positions. The handlers are attached by now and the
    // syncing guard makes the resulting events no-ops; offsets beyond the
    // ±100 slider range leave the knob pegged at the end.
    {
        let controls = controls.clone();
        controls.syncing.set(true);
        controls
            .scale_x
            .set_value(cfg.dot.offset_x.clamp(-100, 100) as f64);
        controls
            .scale_y
            .set_value(cfg.dot.offset_y.clamp(-100, 100) as f64);
        controls.syncing.set(false);
    }

    window.present();
}

/// One offset row: axis glyph, name, and the value. The name column takes
/// all the slack, so the value sits flush at the right end of the line.
fn offset_row(glyph: &str, name: &str, value: &gtk4::Label) -> gtk4::Grid {
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(14);

    let icon = gtk4::Label::new(Some(glyph));
    icon.add_css_class("calibrate-glyph");
    icon.set_width_chars(2);
    icon.set_halign(gtk4::Align::Start);
    grid.attach(&icon, 0, 0, 1, 1);

    let name = gtk4::Label::new(Some(name));
    name.add_css_class("calibrate-name");
    name.set_halign(gtk4::Align::Start);
    name.set_hexpand(true);
    grid.attach(&name, 1, 0, 1, 1);

    grid.attach(value, 2, 0, 1, 1);

    grid
}

/// Fixed-width value label ("+2000" is the widest possible) so the window
/// does not jitter as the digits change. Centered vertically: without it the
/// grid stretches the label to the full row height and the box would render
/// square instead of hugging the text.
fn value_label(offset: i32) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&format!("{:+}", offset)));
    label.add_css_class("calibrate-value");
    label.set_width_chars(5);
    label.set_halign(gtk4::Align::End);
    label.set_valign(gtk4::Align::Center);
    label
}

/// The sliders cover ±100 px (fine calibration); arrows reach the full
/// ±2000 range. Not focusable, so the arrow keys always nudge the offsets
/// instead of being captured by the scale's own key handling.
fn slider() -> gtk4::Scale {
    let scale = gtk4::Scale::with_range(gtk4::Orientation::Horizontal, -100.0, 100.0, 1.0);
    scale.set_draw_value(false);
    scale.set_round_digits(0);
    scale.set_hexpand(true);
    scale.set_focusable(false);
    scale
}

/// Drag events from a slider: commit the rounded value on the slider's axis.
/// The syncing flag keeps commits triggered by programmatic set_value from
/// re-entering.
fn slider_changed(scale: &gtk4::Scale, controls: &Controls, axis: Axis) {
    if controls.syncing.get() {
        return;
    }
    let value = scale.value().round() as i32;
    let (x, y) = controls.current.get();
    match axis {
        Axis::X => {
            if value != x {
                commit(controls, value, y);
            }
        }
        Axis::Y => {
            if value != y {
                commit(controls, x, value);
            }
        }
    }
}

/// Move one axis by `delta` logical pixels.
fn step(controls: &Controls, axis: Axis, delta: i32) {
    let (x, y) = controls.current.get();
    match axis {
        Axis::X => commit(controls, x + delta, y),
        Axis::Y => commit(controls, x, y + delta),
    }
}

/// The apply spine shared by every control, present and future: save the
/// freshly loaded config, update the labels and sliders, and poke the
/// daemon so the dots redraw from the new config immediately.
fn commit(controls: &Controls, offset_x: i32, offset_y: i32) {
    let mut cfg = crate::config::load();
    cfg.dot.offset_x = offset_x.clamp(-2000, 2000);
    cfg.dot.offset_y = offset_y.clamp(-2000, 2000);
    if let Err(e) = crate::config::save(&cfg) {
        eprintln!("crosshair: cannot save config: {e}");
        return;
    }
    controls
        .current
        .set((cfg.dot.offset_x, cfg.dot.offset_y));
    controls
        .label_x
        .set_text(&format!("{:+}", cfg.dot.offset_x));
    controls
        .label_y
        .set_text(&format!("{:+}", cfg.dot.offset_y));
    controls.syncing.set(true);
    controls.scale_x.set_value(cfg.dot.offset_x as f64);
    controls.scale_y.set_value(cfg.dot.offset_y as f64);
    controls.syncing.set(false);
    notify_daemon();
}

/// SIGUSR1 makes the running daemon re-read the config and redraw. If the
/// daemon died between the pidfile check and the signal the kill fails
/// harmlessly; the next --start picks the saved offsets up.
fn notify_daemon() {
    if let Some(pid) = crate::pidfile::running_pid() {
        let _ = unsafe { libc::kill(pid, libc::SIGUSR1) };
    }
}

/// Named CSS classes keep the panel styled and give future controls (the
/// color picker, for one) a home; the provider pattern matches overlay.rs.
fn install_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(include_str!("calibrate.css"));
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}
