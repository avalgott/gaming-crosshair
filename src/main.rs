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

    // Detach from the terminal BEFORE touching GTK (forking after GLib init
    // is unsafe). The original process waits for the daemon's readiness
    // report, so `crosshair --start` errors still reach the terminal.
    let report = daemonize();

    // ---- from here on we are the daemon: no controlling terminal, stdio
    // ---- goes to the log file, and Ctrl+C can no longer reach us.

    // Startup that the waiting parent must hear about. The PID file is
    // claimed atomically (O_EXCL) so two racing instances can't both win.
    let startup: Result<(backend::Backend, config::Config), String> = (|| {
        gtk4::init().map_err(|e| format!("cannot initialize GTK: {e}"))?;
        // GTK must be initialized before backend::detect() probes the layer
        // shell (gtk_layer_is_supported needs gtk_init).
        let backend = backend::detect()?;
        pidfile::claim().map_err(|e| e.to_string())?;
        Ok((backend, config::load()))
    })();
    let (backend, cfg) = match startup {
        Ok(ok) => {
            report.ready();
            ok
        }
        Err(e) => report.fail(&e),
    };

    // Installed only after a successful claim, so a signal can never remove
    // someone else's PID file.
    ctrlc::set_handler(|| {
        let _ = std::fs::remove_file(pidfile::path());
        std::process::exit(0);
    })
    .expect("crosshair: failed to install signal handler");

    // Removes the PID file on any unwind path (e.g. a panic) as well as on
    // normal exit. The ctrlc handler above removes it before exit() itself.
    let _guard = PidFileGuard;

    let app = gtk4::Application::new(
        Some("com.avalgott.crosshair"),
        gtk4::gio::ApplicationFlags::default(),
    );
    app.connect_activate(move |app| {
        overlay::show_all(app, backend, &cfg);
    });
    // GApplication::run() would forward our argv to GLib, whose own option
    // parser rejects "--start" ("Unknown option --start"). clap already parsed
    // everything — give GLib an empty argv.
    app.run_with_args::<&str>(&[])
}

/// Double-fork into the background: the first child starts a new session
/// (setsid), the second fork guarantees the daemon can never re-acquire a
/// controlling terminal. Stdio goes to the log file, stdin from /dev/null.
/// Returns in the daemon with a handle that reports readiness (or a startup
/// error) to the waiting original process, which exits accordingly.
fn daemonize() -> DaemonReport {
    let mut fds = [0i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        eprintln!("crosshair: pipe failed");
        std::process::exit(1);
    }

    match unsafe { libc::fork() } {
        -1 => {
            eprintln!("crosshair: fork failed");
            std::process::exit(1);
        }
        0 => {} // first child continues below
        _ => {
            // Original process: wait up to 5 s for the daemon's report.
            unsafe { libc::close(fds[1]) };
            let mut pfd = libc::pollfd {
                fd: fds[0],
                events: libc::POLLIN,
                revents: 0,
            };
            let readable = unsafe { libc::poll(&mut pfd, 1, 5000) } > 0
                && pfd.revents & libc::POLLIN != 0;
            let mut msg = String::new();
            if readable {
                let mut buf = [0u8; 256];
                let n = unsafe { libc::read(fds[0], buf.as_mut_ptr() as *mut _, buf.len()) };
                if n > 0 {
                    msg = String::from_utf8_lossy(&buf[..n as usize]).into_owned();
                }
            }
            if msg.starts_with('R') {
                std::process::exit(0);
            }
            if let Some(err) = msg.strip_prefix("E:") {
                eprintln!("crosshair: {err}");
            } else {
                eprintln!("crosshair: did not start (daemon exited during startup; see the log file)");
            }
            std::process::exit(1);
        }
    }

    // First child: leave the session, then fork once more.
    unsafe { libc::setsid() };
    match unsafe { libc::fork() } {
        -1 => std::process::exit(1),
        0 => {} // daemon continues below
        _ => std::process::exit(0),
    }

    // Daemon: stdio → log file (stdin from /dev/null).
    unsafe { libc::close(fds[0]) };
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(pidfile::log_path())
        .ok();
    let null = std::fs::File::open("/dev/null").ok();
    for (target, source) in [
        (libc::STDIN_FILENO, &null),
        (libc::STDOUT_FILENO, &log),
        (libc::STDERR_FILENO, &log),
    ] {
        if let Some(f) = source {
            let _ = unsafe { libc::dup2(std::os::fd::AsRawFd::as_raw_fd(f), target) };
        }
    }

    DaemonReport { fd: fds[1] }
}

/// Write end of the startup pipe: reports "R" (ready) or "E:<error>" to the
/// waiting original process.
struct DaemonReport {
    fd: i32,
}

impl DaemonReport {
    fn ready(&self) {
        let _ = unsafe { libc::write(self.fd, b"R".as_ptr() as *const _, 1) };
        unsafe { libc::close(self.fd) };
    }

    fn fail(&self, msg: &str) -> ! {
        let text = format!("E:{msg}");
        let _ = unsafe {
            libc::write(self.fd, text.as_ptr() as *const _, text.len())
        };
        unsafe { libc::close(self.fd) };
        std::process::exit(1);
    }
}
