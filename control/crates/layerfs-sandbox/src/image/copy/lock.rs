//! The locks of copying an image to a machine, kept in the home under
//! `run/image-lock/<machine>/` (#175 R01): [`ImageLock`], one copy of an image
//! on a machine at a time, and [`PruneGate`], which `machine.prune` holds while
//! it reads those locks and removes, so a copy cannot start writing meanwhile.
use crate::image::cache;
use layerfs_home::events::Events;
use layerfs_home::home::{self, flock, private_dir, read_record, write_record};
use layerfs_home::outcome::{failed, refused};
use layerfs_home::process;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// How often a waiting copy looks at the lock again: one look every 200 ms
/// reacts to a released or taken-over lock, and to a killed waiter, at once.
const LOCK_POLL: Duration = Duration::from_millis(200);
/// How often a waiting copy says so: the longest silence of a wait is this.
const WAIT_REPORT: Duration = Duration::from_secs(1);

/// `id`'s 64 hex digits, the name of its files under the cache and the lock.
fn hex_of(id: &str) -> Option<&str> {
    cache::digest(id).then(|| &id["sha256:".len()..])
}

/// The lock of one image on one machine, in the home:
/// `<home>/run/image-lock/<machine>/<hex>.lock`, an exclusive `flock` held
/// for the whole copy, with its owner recorded beside it in `<hex>.json`
/// (pid, its kernel start time and boot, the machine and the image; #175
/// R01 and #176's intent).
///
/// The lock is the exclusion and the record is not: a waiter reads the record
/// only to say whom it waits for. An owner that dies releases the kernel
/// lock, so the next copy takes it over and resumes the staged part; a record
/// whose pid another process now holds does not keep the lock, because the
/// lock is not that process's. There is no deadline: a live owner is waited
/// for as long as it lives and the wait says `{phase: "lock"}` every second.
/// Interrupting a waiting copy today means killing it, which leaves nothing
/// to undo — a released kernel lock and at most its record, which the next
/// copy replaces; a wait a shared Cancel can end is X01's, not this card's.
pub struct ImageLock {
    file: File,
    record: Option<PathBuf>,
}

impl Drop for ImageLock {
    fn drop(&mut self) {
        // the copy is over; a copy that died left its record behind for the
        // next owner to replace
        if let Some(record) = &self.record {
            let _ = std::fs::remove_file(record);
        }
        flock::unlock(&self.file);
    }
}

impl ImageLock {
    /// The folder of machine `machine`'s image locks in `home`.
    pub fn dir(home: &Path, machine: &str) -> PathBuf {
        home.join("run").join("image-lock").join(machine)
    }

    /// The lock file of `hex` on `machine`.
    pub fn path(home: &Path, machine: &str, hex: &str) -> PathBuf {
        Self::dir(home, machine).join(format!("{hex}.lock"))
    }

    /// The owner record beside it.
    pub fn record_path(home: &Path, machine: &str, hex: &str) -> PathBuf {
        Self::dir(home, machine).join(format!("{hex}.json"))
    }

    /// The owner record of the copy of image `hex` on `machine`, while the
    /// process it names lives: who holds the lock, for a listing. It reads
    /// the record only and never takes or tests the kernel lock, so a
    /// listing cannot make a copy wait or a prune's guard refuse. (A
    /// prune's own guard writes no record and is not seen here.)
    pub fn holder(home: &Path, machine: &str, hex: &str) -> Option<SdkValue> {
        let record = read_record(&Self::record_path(home, machine, hex)).ok()??;
        process::recorded_alive(&record).then_some(record)
    }

    /// Opens (creating if absent) the lock file `name` of `machine`.
    fn open_file(home: &Path, machine: &str, name: &str) -> Result<File> {
        private_dir(&Self::dir(home, machine))?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(Self::dir(home, machine).join(name))
            .map_err(failed)
    }

    /// Opens `<hex>.lock` and tries an exclusive `flock` without waiting.
    fn try_lock_file(home: &Path, machine: &str, id: &str) -> Result<Option<(File, String)>> {
        let hex = hex_of(id).ok_or_else(|| refused("InvalidInput", "image id"))?;
        let file = Self::open_file(home, machine, &format!("{hex}.lock"))?;
        if !flock::try_lock(&file, true).map_err(failed)? {
            return Ok(None);
        }
        Ok(Some((file, hex.to_owned())))
    }

    /// Takes the lock of image `id` on `machine` without waiting or writing an
    /// owner record (`machine.prune` guard); returns `Ok(None)` when another
    /// live owner currently holds it.
    pub fn try_hold(home: &Path, machine: &str, id: &str) -> Result<Option<Self>> {
        Ok(Self::try_lock_file(home, machine, id)?.map(|(file, _)| Self { file, record: None }))
    }

    /// Takes the lock of image `id` on `machine` without waiting; returns
    /// `Ok(None)` when another live copy currently holds it.
    pub fn try_take(home: &Path, machine: &str, id: &str) -> Result<Option<Self>> {
        let Some((file, hex)) = Self::try_lock_file(home, machine, id)? else {
            return Ok(None);
        };
        Self::with_record(file, home, machine, &hex, id).map(Some)
    }

    /// Takes the lock of image `id` on `machine`, waiting while a live copy
    /// holds it and recording this copy as the owner. `events` say
    /// `{phase: "lock"}` once a second meanwhile; the copy it waits for is
    /// the `busy` item of `machine images` (its pid is not sent).
    pub fn take(home: &Path, machine: &str, id: &str, events: &mut Events<'_>) -> Result<Self> {
        let hex = hex_of(id).ok_or_else(|| refused("InvalidInput", "image id"))?;
        let file = Self::open_file(home, machine, &format!("{hex}.lock"))?;
        let mut reported: Option<Instant> = None;
        loop {
            if flock::try_lock(&file, true).map_err(failed)? {
                break;
            }
            if reported.is_none_or(|at| at.elapsed() >= WAIT_REPORT) {
                events.phase("lock", None, None, Vec::new());
                reported = Some(Instant::now());
            }
            std::thread::sleep(LOCK_POLL);
        }
        // a prune of this machine that is under way finishes first
        PruneGate::pass(home, machine)?;
        Self::with_record(file, home, machine, hex, id)
    }

    /// Records the phase this copy is in (`export`, `send`, `unpack`) in its
    /// owner record, which `machine images` shows as the busy item's
    /// `op.phase`. A copy that writes no record (a prune's guard) records
    /// nothing; a record that cannot be written changes nothing else.
    pub fn at(&self, phase: &str) {
        let Some(path) = &self.record else {
            return;
        };
        if let Ok(Some(record)) = read_record(path) {
            let _ = write_record(
                path,
                &home::with_field(record, "phase", SdkValue::text(phase)),
            );
        }
    }

    /// Wraps `file` with its owner record path and writes the owner record.
    fn with_record(file: File, home: &Path, machine: &str, hex: &str, id: &str) -> Result<Self> {
        let path = Self::record_path(home, machine, hex);
        let now = home::now_ms();
        let mut fields = process::identity();
        fields.extend([
            ("machine", SdkValue::text(machine)),
            ("image", SdkValue::text(id)),
            ("since", SdkValue::text(cache::utc(now))),
            ("since_ms", SdkValue::Int(now)),
        ]);
        let record = SdkValue::Map(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        );
        write_record(&path, &record)?;
        Ok(Self {
            file,
            record: Some(path),
        })
    }
}

/// The prune gate of `machine`: an exclusive `flock` on the folder of its
/// copy locks, `<home>/run/image-lock/<machine>/`, so holding it creates no
/// file (a full disk still prunes). `machine.prune` holds it from before it
/// reads the copy locks until its removals are done; a copy passes it once,
/// right after taking its [`ImageLock`] and before it writes anything to the
/// machine. So a copy is either holding its lock when the prune reads them
/// (and is left alone), or waits until the prune is over, on a worker machine
/// too, where the files a prune removes are not beside the locks.
pub struct PruneGate(Option<File>);

impl Drop for PruneGate {
    fn drop(&mut self) {
        if let Some(folder) = &self.0 {
            flock::unlock(folder);
        }
    }
}

impl PruneGate {
    /// Holds the gate of `machine`, waiting while a copy passes or another
    /// prune holds it. A folder that cannot be made holds no copy and lets
    /// none start, so there is nothing to wait for then.
    pub fn hold(home: &Path, machine: &str) -> Result<Self> {
        let dir = ImageLock::dir(home, machine);
        if private_dir(&dir).is_err() && !dir.is_dir() {
            return Ok(Self(None));
        }
        let folder = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
            .open(&dir)
            .map_err(failed)?;
        flock::lock(&folder).map_err(failed)?;
        Ok(Self(Some(folder)))
    }

    /// Waits until no prune of `machine` holds the gate.
    pub fn pass(home: &Path, machine: &str) -> Result<()> {
        Self::hold(home, machine).map(drop)
    }
}
