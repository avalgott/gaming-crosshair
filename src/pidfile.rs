use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

/// Path of the PID file, resolved identically by --start and --stop.
pub fn path() -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime).join("crosshair.pid");
    }
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/crosshair-{uid}.pid"))
}

/// Path of the log file, next to the PID file. The daemon's stderr goes here
/// (config warnings, X11 housekeeping errors).
pub fn log_path() -> PathBuf {
    let mut p = path();
    p.set_extension("log");
    p
}

/// PID of a live running instance, or None. Stale PID files are removed.
pub fn running_pid() -> Option<i32> {
    let text = std::fs::read_to_string(path()).ok()?;
    let pid: i32 = text.trim().parse().ok()?;
    if is_alive(pid) {
        Some(pid)
    } else {
        let _ = std::fs::remove_file(path());
        None
    }
}

/// True if a process with this PID exists (ESRCH = gone; EPERM = exists).
fn is_alive(pid: i32) -> bool {
    let ret = unsafe { libc::kill(pid, 0) };
    if ret == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Write our own PID to the PID file, atomically: the file is created with
/// O_EXCL, so two racing instances cannot both claim it. A stale file
/// (dead process) is removed and retried once; a live foreign PID is an
/// error.
pub fn claim() -> std::io::Result<()> {
    for _ in 0..2 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path())
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                return Ok(());
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if running_pid().is_some() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::AlreadyExists,
                        "crosshair is already running",
                    ));
                }
                // running_pid() removed the stale file — retry the create.
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "crosshair is already running",
    ))
}

/// --stop: SIGTERM the running instance, wait up to 2 s, report. Returns exit code.
pub fn stop() -> i32 {
    let Some(pid) = running_pid() else {
        eprintln!("crosshair is not running");
        return 1;
    };
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..40 {
        if !is_alive(pid) {
            let _ = std::fs::remove_file(path());
            println!("crosshair stopped");
            return 0;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("crosshair: instance (pid {pid}) did not exit within 2 s");
    1
}
