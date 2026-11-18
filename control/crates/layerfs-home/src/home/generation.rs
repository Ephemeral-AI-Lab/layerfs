//! The home's generation (call surface 2.0, `home.get`): a number in
//! `generation` that moves whenever a record of the home changes, so a
//! client that polls `home.get` knows when to read the records again. It
//! is counted under an exclusive `flock` on its own file, a leaf lock: it is
//! taken last and nothing else is taken while it is held, so a record
//! writer moves it whether it holds the home lock, an entity lock or none.
//! Reading it takes no lock. Moving it is best effort: a count that cannot
//! be written changes no outcome.
use super::flock;
use std::fs::OpenOptions;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// `generation`, in the home.
pub fn file(home: &Path) -> PathBuf {
    home.join("generation")
}

/// The home's generation: 0 until its first record change is counted.
pub fn read(home: &Path) -> i64 {
    std::fs::read_to_string(file(home))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

/// A record of `home` was written or removed: the generation moves by one.
/// Nothing is counted in a folder that is not a LayerFS home (a machine's
/// worker root keeps records too), and the home is never created here.
pub fn moved(home: &Path) {
    if !home.join("layerfs.json").is_file() {
        return;
    }
    let opened = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(file(home));
    let Ok(file) = opened else {
        return;
    };
    if flock::lock(&file).is_err() {
        return;
    }
    let mut held = [0u8; 20];
    let read = file.read_at(&mut held, 0).unwrap_or(0);
    let now: i64 = std::str::from_utf8(&held[..read])
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0);
    // one write at the start: a number only grows, so no reader sees a
    // shorter one over a longer one
    let _ = file.write_all_at(now.saturating_add(1).to_string().as_bytes(), 0);
    flock::unlock(&file);
}

/// [`moved`] for a caller that holds only a record's path or its folder
/// (a Sandbox's folder, a Workspace's record, a project's entry): the home
/// is the nearest folder above `path`, at most five levels up, that holds
/// `layerfs.json`. Under a machine's worker root there is none, and nothing
/// is counted.
pub fn moved_above(path: &Path) {
    let home = (path.ancestors().skip(1).take(5)).find(|dir| dir.join("layerfs.json").is_file());
    if let Some(home) = home {
        moved(home);
    }
}
