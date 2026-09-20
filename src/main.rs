mod backend;
mod calibrate;
mod cli;
mod config;
mod overlay;
mod pidfile;
mod x11;

use clap::Parser;
use gtk4::gio::prelude::*;

fn main() -> gtk4::glib::ExitCode {
    let cli = cli::Cli::parse();

    if cli.calibrate {
        // Start the overlay before detaching: --start can take a few
        // seconds on a slow machine, and letting it run inside the panel's
        // own readiness window could exhaust the deadline. A failure here
        // ends the command with the error visible in the terminal.
        if let Err(e) = calibrate::ensure_daemon() {
            eprintln!("crosshair: could not start the overlay: {e}");
            std::process::exit(1);
        }
        // Detach like --start, so the terminal returns as soon as the panel
        // is up. The panel is then its own process: Esc (or the close
        // button) closes the window, the application quits, and the process
        // ends itself. Startup errors reach the terminal the same way
        // --start's do, through the readiness report.
        let report = daemonize();
        return calibrate::run(report);
    }

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
    // claimed with an exclusive flock so two racing instances can't both win.
    let startup: Result<(backend::Backend, config::Config, pidfile::PidLock), String> = (|| {
        gtk4::init().map_err(|e| format!("cannot initialize GTK: {e}"))?;
        // GTK must be initialized before backend::detect() probes the layer
        // shell (gtk_layer_is_supported needs gtk_init).
        let backend = backend::detect()?;
        let pid_lock = pidfile::claim().map_err(|e| e.to_string())?;
        Ok((backend, config::load(), pid_lock))
    })();
    let (backend, cfg, pid_lock) = match startup {
        Ok(ok) => {
            report.ready();
            ok
        }
        Err(e) => report.fail(&e),
    };

    // Installed only after a successful claim, so a signal can never remove
    // someone else's PID file. The handler only stores a flag: nothing
    // unsafe runs on the signal thread, and the process never exit()s while
    // the X11 housekeeping threads are mid-flight.
    static QUIT_REQUESTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    ctrlc::set_handler(|| {
        QUIT_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    })
    .expect("crosshair: failed to install signal handler");

    // The lock guard removes the PID file (and releases the flock) on any
    // unwind path, e.g. a panic, as well as on normal exit.
    let _guard = pid_lock;

    let app = gtk4::Application::new(
        Some("com.avalgott.crosshair"),
        gtk4::gio::ApplicationFlags::default(),
    );
    // Poll the quit flag in the main loop: SIGTERM/SIGINT land as a clean
    // GTK quit, which tears the windows down and returns from main, so the
    // guard above removes the PID file on the way out.
    let quit_app = app.clone();
    let _quit_source = gtk4::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if QUIT_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            quit_app.quit();
        }
        gtk4::glib::ControlFlow::Continue
    });
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
    let log = pidfile::log_path().ok().and_then(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .ok()
    });
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

    DaemonReport {
        fd: std::sync::Arc::new(std::sync::atomic::AtomicI32::new(fds[1])),
    }
}

/// Write end of the startup pipe: reports "R" (ready) or "E:<error>" to the
/// waiting original process. The fd is taken (swap) and closed exactly once,
/// no matter how many clones exist or how many times ready()/fail() is
/// called, so a repeated call can never write to or close an unrelated fd
/// that was reused in the meantime.
#[derive(Clone)]
struct DaemonReport {
    fd: std::sync::Arc<std::sync::atomic::AtomicI32>,
}

impl DaemonReport {
    fn ready(&self) {
        let fd = self.fd.swap(-1, std::sync::atomic::Ordering::SeqCst);
        if fd < 0 {
            return;
        }
        let _ = unsafe { libc::write(fd, b"R".as_ptr() as *const _, 1) };
        unsafe { libc::close(fd) };
    }

    fn fail(&self, msg: &str) -> ! {
        let fd = self.fd.swap(-1, std::sync::atomic::Ordering::SeqCst);
        if fd >= 0 {
            let text = format!("E:{msg}");
            let _ = unsafe {
                libc::write(fd, text.as_ptr() as *const _, text.len())
            };
            unsafe { libc::close(fd) };
        }
        std::process::exit(1);
    }
}
