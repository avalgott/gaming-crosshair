//! Self-update: fetch the latest GitHub release, download and verify the
//! new binary, and atomically replace the running one. Also hosts the
//! version check the calibration panel uses for its "Update available"
//! link.
//!
//! CROSSHAIR_UPDATE_API and CROSSHAIR_UPDATE_DL override the endpoints
//! (undocumented hooks used by the tests and available to self-hosted
//! mirrors).

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

const API_URL: &str = "https://api.github.com/repos/avalgott/gaming-crosshair/releases/latest";
const DL_BASE: &str = "https://github.com/avalgott/gaming-crosshair/releases/download";
const USER_AGENT: &str = concat!("crosshair/", env!("CARGO_PKG_VERSION"));

/// Entry point for `crosshair --update`. Errors are printed by main.
pub fn run() -> Result<(), String> {
    let exe = current_exe_path()?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    check_writable_dir(dir)?;

    let agent = build_agent(Duration::from_secs(60));
    let tag = fetch_latest(&agent)?;
    match version_cmp(env!("CARGO_PKG_VERSION"), &tag)? {
        std::cmp::Ordering::Less => {}
        _ => {
            println!(
                "crosshair v{} is up to date (latest: {tag})",
                env!("CARGO_PKG_VERSION")
            );
            return Ok(());
        }
    }

    // Download and verify BEFORE touching the daemon: a failed download
    // must leave the overlay running, and the stop-to-rename gap is the
    // only downtime.
    let tmp = exe.with_file_name(format!(".crosshair.new.{}", std::process::id()));
    download_and_verify(&agent, &tag, &tmp)?;

    let was_running = match crate::pidfile::stop_daemon() {
        Ok(()) => true,
        Err(crate::pidfile::StopError::NotRunning) => false,
        Err(crate::pidfile::StopError::Timeout(pid)) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "refusing to replace the binary while the overlay (pid {pid}) ignores SIGTERM"
            ));
        }
    };

    install_in_place(&tmp, &exe)?;

    if was_running {
        restart(&exe, &tag)?;
    }
    println!("crosshair updated to {tag} (SHA-256 verified)");
    Ok(())
}

/// Version check for the calibration panel: the tag of a newer release, or
/// None. Every failure is silent (offline, GitHub down, API error, missing
/// checksum): the panel must simply show no link. Uses a short timeout so
/// the link appears quickly or never.
pub fn latest_if_newer() -> Option<String> {
    let agent = build_agent(Duration::from_secs(8));
    let tag = fetch_latest(&agent).ok()?;
    (version_cmp(env!("CARGO_PKG_VERSION"), &tag).ok()? == std::cmp::Ordering::Less)
        .then_some(tag)
}

fn api_url() -> String {
    std::env::var("CROSSHAIR_UPDATE_API").unwrap_or_else(|_| API_URL.to_string())
}

/// Base URL for one release's assets (the sidecar lives next to the binary).
fn dl_base(tag: &str) -> String {
    std::env::var("CROSSHAIR_UPDATE_DL")
        .map(|base| format!("{base}/{tag}"))
        .unwrap_or_else(|_| format!("{DL_BASE}/{tag}"))
}

/// The running binary, with a guard: `cargo run` artifacts live under
/// target/ and must not be clobbered by an update. current_exe() resolves
/// symlinks, so a symlinked install updates the real file.
fn current_exe_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("cannot locate the running binary: {e}"))?;
    if exe.components().any(|c| c.as_os_str() == "target") {
        return Err(format!(
            "{} is a cargo build artifact and cannot self-update; install crosshair first (install.sh)",
            exe.display()
        ));
    }
    Ok(exe)
}

/// Create and remove a probe file: a read-only install dir (a
/// sudo-installed /usr/local/bin, for one) fails here, before any network
/// I/O, with a hint instead of a confusing error later.
fn check_writable_dir(dir: &Path) -> Result<(), String> {
    let probe = dir.join(format!(".crosshair.probe.{}", std::process::id()));
    File::create(&probe).map_err(|e| {
        format!(
            "{} is not writable ({e}); re-install with install.sh",
            dir.display()
        )
    })?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

fn build_agent(timeout: Duration) -> ureq::Agent {
    let config = ureq::config::Config::builder()
        .timeout_global(Some(timeout))
        .build();
    ureq::Agent::new_with_config(config)
}

/// The tag of the latest stable release, plus a promise that it ships a
/// checksum sidecar (a pre-CI release has none: failing loudly beats
/// skipping verification). GitHub requires a User-Agent on API calls.
fn fetch_latest(agent: &ureq::Agent) -> Result<String, String> {
    let mut resp = agent
        .get(&api_url())
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| match e {
            ureq::Error::StatusCode(403) | ureq::Error::StatusCode(429) => {
                "GitHub API rate limit reached (60 requests/hour unauthenticated); try again later"
                    .to_string()
            }
            ureq::Error::StatusCode(code) => format!("GitHub API error: HTTP {code}"),
            other => format!("cannot reach the GitHub API: {other}"),
        })?;
    let body: serde_json::Value = resp
        .body_mut()
        .read_json()
        .map_err(|e| format!("invalid API response: {e}"))?;
    let tag = body["tag_name"]
        .as_str()
        .filter(|t| !t.is_empty())
        .ok_or_else(|| "release has no tag_name".to_string())?
        .to_string();
    let has_sidecar = body["assets"]
        .as_array()
        .map(|assets| {
            assets
                .iter()
                .any(|a| a["name"].as_str() == Some("crosshair.sha256"))
        })
        .unwrap_or(false);
    if !has_sidecar {
        return Err(format!("release {tag} ships no checksum; refusing to install"));
    }
    Ok(tag)
}

/// Compare two versions by their first three dot-separated numeric
/// components (leading v/V stripped). Prerelease suffixes (v0.3.0-rc1)
/// compare by number only, so -rc and the final release count as equal:
/// `releases/latest` never surfaces prereleases or drafts anyway, so only
/// hand-rolled tags could hit this.
fn version_cmp(a: &str, b: &str) -> Result<std::cmp::Ordering, String> {
    let parse = |s: &str| -> Result<Vec<u64>, String> {
        let s = s.trim().trim_start_matches(['v', 'V']);
        let mut parts = Vec::with_capacity(3);
        for part in s.split('.') {
            if parts.len() == 3 {
                break;
            }
            let num: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if num.is_empty() {
                break;
            }
            parts.push(num.parse().map_err(|_| format!("invalid version {s:?}"))?);
        }
        if parts.is_empty() {
            return Err(format!("invalid version {s:?}"));
        }
        Ok(parts)
    };
    Ok(parse(a)?.cmp(&parse(b)?))
}

/// Download the new binary and its checksum sidecar, verify, and write the
/// temp file. On any failure the temp file is removed and the overlay is
/// still running.
fn download_and_verify(agent: &ureq::Agent, tag: &str, dest: &Path) -> Result<(), String> {
    let base = dl_base(tag);
    let binary_url = format!("{base}/crosshair");
    let bin = download_bytes(agent, &binary_url)?;
    let sidecar_url = format!("{base}/crosshair.sha256");
    let sidecar = download_text(agent, &sidecar_url)?;
    let expected = parse_sha256(&sidecar).map_err(|e| format!("{sidecar_url}: {e}"))?;
    let digest = Sha256::digest(&bin);
    if digest[..] != expected[..] {
        let _ = std::fs::remove_file(dest);
        return Err(format!("checksum mismatch for {binary_url} (aborting, file removed)"));
    }
    let mut file = File::create(dest)
        .map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    file.write_all(&bin)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", dest.display()))
}

fn download_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    // read_to_vec caps at 10 MB by default: ample for a ~1.5 MB binary,
    // and a growth spurt beyond it fails loudly instead of filling disk.
    let mut resp = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| download_error(url, &e))?;
    resp.body_mut()
        .read_to_vec()
        .map_err(|e| format!("cannot read {url}: {e}"))
}

fn download_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    let mut resp = agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| download_error(url, &e))?;
    resp.body_mut()
        .read_to_string()
        .map_err(|e| format!("cannot read {url}: {e}"))
}

fn download_error(url: &str, e: &ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(code) => format!("download failed: HTTP {code} ({url})"),
        other => format!("download failed: {other} ({url})"),
    }
}

/// The sidecar's first whitespace-delimited token, as 32 raw bytes. The CI
/// workflow writes `sha256sum` output there, which also names the file, so
/// only the leading token is the digest.
fn parse_sha256(sidecar: &str) -> Result<[u8; 32], String> {
    let hex = sidecar.split_whitespace().next().unwrap_or("");
    if hex.len() != 64 {
        return Err("malformed checksum file".to_string());
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(chunk).map_err(|_| "malformed checksum file")?;
        out[i] = u8::from_str_radix(s, 16).map_err(|_| "malformed checksum file")?;
    }
    Ok(out)
}

/// Replace the running binary: chmod (the umask must not strip the
/// executable bit), rename over it (atomic; any process still executing
/// keeps the old inode alive), then fsync the directory so the rename
/// survives a crash. The temp file lives in the target's own directory, so
/// the rename is always same-filesystem.
fn install_in_place(tmp: &Path, exe: &Path) -> Result<(), String> {
    std::fs::set_permissions(tmp, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("cannot chmod {}: {e}", tmp.display()))?;
    std::fs::rename(tmp, exe)
        .map_err(|e| format!("cannot replace {}: {e}", exe.display()))?;
    if let Some(dir) = exe.parent()
        && let Ok(dir) = File::open(dir)
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Bring the overlay back after the swap. The new binary detaches itself
/// like any --start; status() waits for the original process, which exits
/// 0 only once the daemon holds the PID file.
fn restart(exe: &Path, tag: &str) -> Result<(), String> {
    let detail = match std::process::Command::new(exe).arg("--start").status() {
        Ok(status) if status.success() => return Ok(()),
        Ok(status) => status.to_string(),
        Err(e) => e.to_string(),
    };
    Err(format!(
        "the binary was updated to {tag} but the overlay failed to restart ({detail}); run `crosshair --start`"
    ))
}
