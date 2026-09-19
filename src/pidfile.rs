use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
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

/// Owns the PID file for the daemon's lifetime. Drop removes the file; the
/// flock is released when the fd closes.
pub struct PidLock(File);

impl Drop for PidLock {
    fn drop(&mut self) {
        // Flush the PID write, then unlink. The flock dies with the fd.
        let _ = self.0.sync_all();
        let _ = std::fs::remove_file(path());
    }
}

/// Write our PID to the PID file and hold an exclusive flock on it for the
/// process lifetime. The flock, not the file contents, is the source of
/// truth: a stale file (dead daemon) carries no lock, and a reused PID
/// cannot hold our lock, so --stop can never signal an unrelated process.
pub fn claim() -> std::io::Result<PidLock> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path())?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret != 0 {
        let e = std::io::Error::last_os_error();
        return Err(if e.raw_os_error() == Some(libc::EWOULDBLOCK) {
            std::io::Error::new(std::io::ErrorKind::AlreadyExists, "already running")
        } else {
            e
        });
    }
    // The lock is ours: rewrite the file with our PID. A previous dead
    // instance may have left different contents behind.
    file.set_len(0)?;
    writeln!(file, "{}", std::process::id())?;
    Ok(PidLock(file))
}

/// PID of a live running instance, or None. Stale PID files are removed.
pub fn running_pid() -> Option<i32> {
    let file = File::open(path()).ok()?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        // Nobody holds the lock: the file is stale. Drop the file first
        // (releasing the lock), then remove the path.
        drop(file);
        let _ = std::fs::remove_file(path());
        return None;
    }
    // A daemon holds the lock. Its PID must be positive: 0 and negative
    // values are signal specials (kill(0) = our process group, kill(-1) =
    // every process we may signal) and must never leave the file.
    let pid: i32 = std::fs::read_to_string(path()).ok()?.trim().parse().ok()?;
    if pid <= 0 || !is_alive(pid) {
        return None;
    }
    Some(pid)
}

/// True if a process with this PID exists (ESRCH = gone; EPERM = exists).
fn is_alive(pid: i32) -> bool {
    let ret = unsafe { libc::kill(pid, 0) };
    if ret == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
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
