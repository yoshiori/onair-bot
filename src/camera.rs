//! Camera-in-use detection.
//!
//! Instead of shelling out to `lsof`, this scans `/proc/<pid>/fd` directly for
//! open file descriptors pointing at any `/dev/video*` device. The result is a
//! plain boolean ("in use" / "not in use"), debounced so that the brief device
//! probes camera apps perform on startup do not flicker the light.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Symlink target prefix that identifies a V4L2 video device.
const VIDEO_DEVICE_PREFIX: &str = "/dev/video";

/// List the non-ignored processes currently holding a `/dev/video*` device open.
///
/// `proc_root` is normally `/proc`; it is a parameter so the scan can be tested
/// against a fixture directory tree. `ignore` holds process-name prefixes
/// (matched against `/proc/<pid>/comm`) to skip — typically background media
/// services such as `pipewire` that keep the device open without capturing.
///
/// Processes owned by other users are silently skipped: just like `lsof` run
/// as a normal user, this only sees the invoking user's own processes.
pub fn scan_camera_holders(proc_root: &Path, ignore: &[String]) -> io::Result<Vec<String>> {
    let mut holders = Vec::new();

    for entry in fs::read_dir(proc_root)? {
        let Ok(entry) = entry else { continue };

        let file_name = entry.file_name();
        let Some(pid) = file_name.to_str() else {
            continue;
        };
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }

        let proc_dir = entry.path();
        let comm = read_comm(&proc_dir);

        if let Some(comm) = &comm {
            if ignore
                .iter()
                .any(|prefix| comm.starts_with(prefix.as_str()))
            {
                continue;
            }
        }

        if process_holds_video(&proc_dir) {
            let label = comm.unwrap_or_else(|| "unknown".to_string());
            holders.push(format!("{label} (pid {pid})"));
        }
    }

    holders.sort();
    Ok(holders)
}

/// Read and trim `/proc/<pid>/comm` (the process name).
fn read_comm(proc_dir: &Path) -> Option<String> {
    fs::read_to_string(proc_dir.join("comm"))
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// Whether any file descriptor of the process points at a video device.
fn process_holds_video(proc_dir: &Path) -> bool {
    let Ok(fds) = fs::read_dir(proc_dir.join("fd")) else {
        return false;
    };
    for fd in fds.flatten() {
        if let Ok(target) = fs::read_link(fd.path()) {
            if target.to_string_lossy().starts_with(VIDEO_DEVICE_PREFIX) {
                return true;
            }
        }
    }
    false
}

/// Confirms a state change only after it has been observed for a number of
/// consecutive readings, smoothing out single-poll flicker.
#[derive(Debug)]
pub struct Debouncer {
    state: bool,
    pending: Option<bool>,
    pending_count: u32,
    threshold: u32,
}

impl Debouncer {
    /// Create a debouncer starting from a known state. A `threshold` of 1
    /// commits every change immediately.
    pub fn new(initial: bool, threshold: u32) -> Self {
        Self {
            state: initial,
            pending: None,
            pending_count: 0,
            threshold: threshold.max(1),
        }
    }

    /// The currently committed (debounced) state.
    pub fn state(&self) -> bool {
        self.state
    }

    /// Feed one raw reading. Returns `Some(new_state)` when a change is
    /// committed, `None` while unchanged or still settling.
    pub fn observe(&mut self, reading: bool) -> Option<bool> {
        if reading == self.state {
            self.pending = None;
            self.pending_count = 0;
            return None;
        }

        if self.pending == Some(reading) {
            self.pending_count += 1;
        } else {
            self.pending = Some(reading);
            self.pending_count = 1;
        }

        if self.pending_count >= self.threshold {
            self.state = reading;
            self.pending = None;
            self.pending_count = 0;
            Some(self.state)
        } else {
            None
        }
    }
}

/// Watches camera usage over time, emitting debounced on/off transitions.
pub struct CameraWatcher {
    proc_root: PathBuf,
    ignore: Vec<String>,
    debouncer: Debouncer,
}

impl CameraWatcher {
    /// Create a watcher, taking an immediate (non-debounced) initial reading
    /// so the daemon can sync the light to reality on startup.
    pub fn new(proc_root: PathBuf, ignore: Vec<String>, debounce: u32) -> io::Result<Self> {
        let initial = !scan_camera_holders(&proc_root, &ignore)?.is_empty();
        Ok(Self {
            proc_root,
            ignore,
            debouncer: Debouncer::new(initial, debounce),
        })
    }

    /// The current committed camera state (`true` = in use).
    pub fn state(&self) -> bool {
        self.debouncer.state()
    }

    /// Poll once. Returns `Some(true/false)` on a debounced transition.
    ///
    /// A failed scan is logged and treated as "no change" rather than a
    /// transition, so a transient `/proc` read error never toggles the light.
    pub fn poll(&mut self) -> Option<bool> {
        let holders = match scan_camera_holders(&self.proc_root, &self.ignore) {
            Ok(holders) => holders,
            Err(error) => {
                log::warn!("camera scan failed, keeping previous state: {error}");
                return None;
            }
        };

        let in_use = !holders.is_empty();
        log::debug!("camera reading: in_use={in_use}, holders={holders:?}");
        self.debouncer.observe(in_use)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// Build a fake `/proc/<pid>` with the given `comm` and a set of fd
    /// symlink targets.
    fn fake_process(proc_root: &Path, pid: u32, comm: &str, fd_targets: &[&str]) {
        let proc_dir = proc_root.join(pid.to_string());
        let fd_dir = proc_dir.join("fd");
        fs::create_dir_all(&fd_dir).unwrap();
        fs::write(proc_dir.join("comm"), format!("{comm}\n")).unwrap();
        for (index, target) in fd_targets.iter().enumerate() {
            symlink(target, fd_dir.join(index.to_string())).unwrap();
        }
    }

    fn ignore_list() -> Vec<String> {
        vec!["pipewire".to_string(), "wireplumber".to_string()]
    }

    #[test]
    fn detects_a_process_holding_a_video_device() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 100, "firefox", &["/dev/video0", "socket:[42]"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert_eq!(holders, ["firefox (pid 100)"]);
    }

    #[test]
    fn reports_nothing_when_no_video_device_is_open() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 100, "bash", &["/dev/null", "pipe:[7]"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert!(holders.is_empty());
    }

    #[test]
    fn ignores_background_media_services() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 100, "pipewire", &["/dev/video0"]);
        fake_process(tmp.path(), 101, "wireplumber", &["/dev/video1"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert!(holders.is_empty());
    }

    #[test]
    fn detects_any_video_device_not_just_video0() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 100, "zoom", &["/dev/video2"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert_eq!(holders, ["zoom (pid 100)"]);
    }

    #[test]
    fn skips_non_pid_entries_and_processes_without_fds() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("acpi")).unwrap();
        fs::write(tmp.path().join("uptime"), "123").unwrap();
        fake_process(tmp.path(), 100, "obs", &["/dev/video0"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert_eq!(holders, ["obs (pid 100)"]);
    }

    #[test]
    fn results_are_sorted_for_stable_logging() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 200, "zoom", &["/dev/video0"]);
        fake_process(tmp.path(), 100, "firefox", &["/dev/video0"]);

        let holders = scan_camera_holders(tmp.path(), &ignore_list()).unwrap();
        assert_eq!(holders, ["firefox (pid 100)", "zoom (pid 200)"]);
    }

    #[test]
    fn debouncer_with_threshold_one_commits_immediately() {
        let mut debouncer = Debouncer::new(false, 1);
        assert_eq!(debouncer.observe(true), Some(true));
        assert_eq!(debouncer.observe(false), Some(false));
    }

    #[test]
    fn debouncer_ignores_a_single_poll_flicker() {
        let mut debouncer = Debouncer::new(false, 2);
        // A one-poll probe spike does not reach the threshold.
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(false), None);
        assert!(!debouncer.state());
    }

    #[test]
    fn debouncer_commits_a_sustained_change() {
        let mut debouncer = Debouncer::new(false, 3);
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(true), Some(true));
        assert!(debouncer.state());
        // Further identical readings produce no new transition.
        assert_eq!(debouncer.observe(true), None);
    }

    #[test]
    fn debouncer_resets_pending_count_when_reading_flips_back() {
        let mut debouncer = Debouncer::new(false, 3);
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(true), None);
        // Flip back to the committed state: the pending streak is cleared.
        assert_eq!(debouncer.observe(false), None);
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(true), None);
        assert_eq!(debouncer.observe(true), Some(true));
    }

    #[test]
    fn watcher_takes_an_immediate_initial_reading() {
        let tmp = TempDir::new().unwrap();
        fake_process(tmp.path(), 100, "firefox", &["/dev/video0"]);

        let watcher = CameraWatcher::new(tmp.path().to_path_buf(), ignore_list(), 2).unwrap();
        assert!(watcher.state());
    }
}
