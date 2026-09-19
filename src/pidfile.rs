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
pub fn running_pid() -> Option<i32> {
    let file = File::open(path().ok()?).ok()?;
    let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        // Nobody holds the lock: the file is stale. Unlink while this
        // descriptor still holds the flock, so a concurrent --start cannot
        // have claimed this inode in the gap between our lock release and
        // the unlink (that gap would let us delete a fresh daemon's PID
        // file).
        if let Ok(p) = path() {
            let _ = std::fs::remove_file(p);
        }
        drop(file);
        return None;
    }
    // A daemon holds the lock. Its PID must be positive: 0 and negative
    // values are signal specials (kill(0) = our process group, kill(-1) =
    // every process we may signal) and must never leave the file.
    let pid: i32 = std::fs::read_to_string(path().ok()?).ok()?.trim().parse().ok()?;
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
            if let Ok(p) = path() {
                let _ = std::fs::remove_file(p);
            }
            println!("crosshair stopped");
            return 0;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    eprintln!("crosshair: instance (pid {pid}) did not exit within 2 s");
    1
}
