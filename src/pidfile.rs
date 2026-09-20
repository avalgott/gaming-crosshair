use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::time::Duration;

/// Directory for the PID and log files: $XDG_RUNTIME_DIR when set (it is a
/// mode-0700 per-user directory), otherwise a mode-0700 per-user directory
/// in /tmp. /tmp is world-writable, so bare file paths there are
/// symlink-attackable and pre-creatable by other users; a private directory
/// avoids both.
fn dir() -> std::io::Result<PathBuf> {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(runtime));
    }
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let uid = unsafe { libc::getuid() };
    let dir = PathBuf::from(format!("/tmp/crosshair-{uid}"));
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {
            // Normalize the mode: DirBuilder's mode passes through the
            // umask, and we need exactly 0700.
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = std::fs::symlink_metadata(&dir)?;
            let ours =
                meta.file_type().is_dir() && meta.uid() == uid && meta.mode() & 0o077 == 0;
            if !ours {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "{} exists and is not a private directory owned by you",
                        dir.display()
                    ),
                ));
            }
        }
        Err(e) => return Err(e),
    }
    Ok(dir)
}

/// Candidate directories for the PID and log files. --update (and --stop)
/// may run in an environment that lost XDG_RUNTIME_DIR (cron, ssh,
/// systemd-run), so probe every place a daemon could have put its PID
/// file. /run/user/<uid> is the conventional runtime dir on systemd
/// systems and covers the case where the daemon had XDG_RUNTIME_DIR and we
/// do not; a non-standard XDG_RUNTIME_DIR in the daemon's environment
/// cannot be guessed, and is the one gap.
fn dirs() -> impl Iterator<Item = PathBuf> {
    let uid = unsafe { libc::getuid() };
    let mut dirs = Vec::with_capacity(3);
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(runtime) => dirs.push(PathBuf::from(runtime)),
        None => dirs.push(PathBuf::from(format!("/run/user/{uid}"))),
    }
    dirs.push(PathBuf::from(format!("/tmp/crosshair-{uid}")));
    dirs.dedup();
    dirs.into_iter()
}

/// Path of the PID file, resolved identically by --start and --stop.
pub fn path() -> std::io::Result<PathBuf> {
    Ok(dir()?.join("crosshair.pid"))
}

/// Path of the log file, next to the PID file. The daemon's stderr goes here
/// (config warnings, X11 housekeeping errors).
pub fn log_path() -> std::io::Result<PathBuf> {
    Ok(dir()?.join("crosshair.log"))
}

/// Owns the PID file for the daemon's lifetime. Drop removes the file; the
/// flock is released when the fd closes.
pub struct PidLock(File);

impl Drop for PidLock {
    fn drop(&mut self) {
        // Flush the PID write, then unlink. The flock dies with the fd.
        let _ = self.0.sync_all();
        if let Ok(p) = path() {
            let _ = std::fs::remove_file(p);
        }
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
        .open(path()?)?;
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
/// Every candidate directory is probed (see dirs).
pub fn running_pid() -> Option<i32> {
    dirs()
        .map(|dir| dir.join("crosshair.pid"))
        .find_map(|path| probe(&path))
}

/// Try one PID file path: None if it is absent, stale (nobody holds the
/// flock), or records a dead PID. Stale files are removed.
fn probe(path: &std::path::Path) -> Option<i32> {
    let file = File::open(path).ok()?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        // Nobody holds the lock: the file is stale. Unlink while this
        // descriptor still holds the flock, so a concurrent --start cannot
        // have claimed this inode in the gap between our lock release and
        // the unlink (that gap would let us delete a fresh daemon's PID
        // file).
        let _ = std::fs::remove_file(path);
        drop(file);
        return None;
    }
    // A daemon holds the lock. Its PID must be positive: 0 and negative
    // values are signal specials (kill(0) = our process group, kill(-1) =
    // every process we may signal) and must never leave the file.
    let pid: i32 = std::fs::read_to_string(path).ok()?.trim().parse().ok()?;
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

/// Why the running instance could not be stopped.
pub enum StopError {
    NotRunning,
    /// The daemon ignored SIGTERM for 2 s; carries its pid.
    Timeout(i32),
}

/// SIGTERM the running instance and wait up to 2 s for it to exit. Shared
/// by --stop and --update (which needs in-line control instead of an exit
/// code).
pub fn stop_daemon() -> Result<(), StopError> {
    let Some(pid) = running_pid() else {
        return Err(StopError::NotRunning);
    };
    unsafe { libc::kill(pid, libc::SIGTERM) };
    for _ in 0..40 {
        if !is_alive(pid) {
            if let Ok(p) = path() {
                let _ = std::fs::remove_file(p);
            }
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(StopError::Timeout(pid))
}

/// --stop: SIGTERM the running instance, wait up to 2 s, report. Returns exit code.
pub fn stop() -> i32 {
    match stop_daemon() {
        Ok(()) => {
            println!("crosshair stopped");
            0
        }
        Err(StopError::NotRunning) => {
            eprintln!("crosshair is not running");
            1
        }
        Err(StopError::Timeout(pid)) => {
            eprintln!("crosshair: instance (pid {pid}) did not exit within 2 s");
            1
        }
    }
}
