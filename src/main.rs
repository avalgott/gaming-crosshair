mod backend;
mod cli;
mod config;
mod overlay;
mod pidfile;
mod x11;

use clap::Parser;
use gtk4::gio::prelude::*;

struct PidFileGuard;

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(pidfile::path());
    }
}

fn main() -> gtk4::glib::ExitCode {
    let cli = cli::Cli::parse();

    if cli.stop {
        std::process::exit(pidfile::stop());
    }

    // clap guarantees at least --start here (arg_required_else_help).
    if let Some(pid) = pidfile::running_pid() {
        eprintln!("crosshair is already running (pid {pid})");
        std::process::exit(1);
    }
    if let Err(e) = pidfile::claim() {
        eprintln!(
            "crosshair: cannot write PID file {}: {e}",
            pidfile::path().display()
        );
        std::process::exit(1);
    }
    ctrlc::set_handler(|| {
        let _ = std::fs::remove_file(pidfile::path());
        std::process::exit(0);
    })
    .expect("crosshair: failed to install signal handler");

    // Removes the PID file on any unwind path (e.g. a panic) as well as on
    // normal exit. The ctrlc handler above removes it before exit() itself.
    let _guard = PidFileGuard;

    let cfg = config::load();

    // GTK must be initialized before backend::detect() probes the layer shell
    // (gtk_layer_is_supported needs gtk_init) — so detect inside activate,
    // which Application::run() invokes after initializing GTK.
    let app = gtk4::Application::new(
        Some("com.avalgott.crosshair"),
        gtk4::gio::ApplicationFlags::default(),
    );
    app.connect_activate(move |app| {
        let backend = backend::detect();
        overlay::show_all(app, backend, &cfg);
    });
    // GApplication::run() would forward our argv to GLib, whose own option
    // parser rejects "--start" ("Unknown option --start"). clap already parsed
    // everything — give GLib an empty argv.
    app.run_with_args::<&str>(&[])
}
