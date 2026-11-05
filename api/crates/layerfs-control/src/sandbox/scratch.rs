//! A Sandbox's own scratch, and the token that says a cleaner may remove it
//! (#549).
//!
//! The scratch is the one directory a Sandbox's commands write throwaway
//! files into: `<sandbox run directory>/tmp` for a local Sandbox. It is not
//! the Store, not a Workspace, and not the host's `/tmp`, and a cleaner may
//! not guess any of those from a path alone. The Sandbox record therefore
//! owns the scratch: it carries the absolute root, when the Sandbox claimed
//! it (`createdAt`) and a `generation`. A cleaner may remove a path only
//! when it is under that exact root and the record still carries the
//! generation the plan was made with; a rebuilt Sandbox has a new
//! generation, so a stale plan aborts instead of deleting the new
//! Sandbox's files.
//!
//! That `generation` is a **scratch token**, minted by one start. It is not
//! the record/CAS generation #548 (B5) defines, and the two must not be
//! treated as one: this token answers only "is this still the Sandbox that
//! claimed this directory?". A record may later carry both, as two separate
//! fields and two separate spaces; neither may stand in for the other.
//!
//! The policy is domain logic: it decides, and it does not know Docker, a
//! worker, a daemon or the Store engine. The two ways in are
//! [`clean`] (one settled close) and [`sweep`] (one bounded, machine-keyed
//! pass over the home's records); both use [`keep`], so they cannot drift.
//! An `Unresolved` record is never swept: its intent's owner is gone but its
//! outcome is not known, so only an explicit owner (the settled close that
//! clears the intent) may remove it.
use super::record::SandboxRecord;
use crate::common::home;
use crate::common::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The record field that holds the scratch ownership.
pub const FIELD: &str = "scratch";
/// The scratch's absolute root inside [`FIELD`].
pub const ROOT: &str = "root";
/// When the Sandbox claimed the scratch, inside [`FIELD`].
pub const CREATED_AT: &str = "createdAt";
/// The ownership token, inside [`FIELD`].
pub const GENERATION: &str = "generation";

/// The leaf every scratch root ends in: the runtime's `tmp`, never a Store
/// directory, a Workspace or the host's `/tmp` itself.
const LEAF: &str = "tmp";
/// Roots one sweep may remove. A pass is bounded by construction: it visits
/// records in name order and charges the batch only for a root it may
/// remove, so records of other machines, kept records and already-gone roots
/// cannot crowd out a later settled one. A removed root is gone next pass,
/// so a home with more than this many settled records is covered over
/// successive passes.
const MAX_SWEEP: usize = 64;

/// One Sandbox's scratch ownership, as its record holds it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Scratch {
    root: PathBuf,
    created_at: i64,
    generation: u64,
}

impl Scratch {
    /// Ownership of `root`, claimed at `created_at` with `generation`.
    /// Refused `InvalidInput` when `root` is not an absolute scratch path
    /// (absolute, no `.` or `..`, its leaf `tmp`, not the host's `/tmp`).
    pub fn new(root: PathBuf, created_at: i64, generation: u64) -> Result<Self, SdkRefusal> {
        if generation == 0 {
            return Err(refused(
                "InvalidInput",
                "a Sandbox scratch generation is a positive token",
            ));
        }
        check_root(&root)?;
        Ok(Self {
            root,
            created_at,
            generation,
        })
    }

    /// A fresh claim of `root`: now, and a new random generation, so a
    /// rebuilt Sandbox never reuses a token.
    pub(crate) fn claim(root: &Path) -> Result<Self, SdkRefusal> {
        let bytes = super::registry::random()?;
        // an i64-safe, non-zero token: the record keeps a JSON integer
        let token = u64::from_be_bytes(bytes[..8].try_into().expect("eight bytes"));
        let generation = 1 + token % (i64::MAX as u64 - 1);
        Self::new(root.to_path_buf(), home::now_ms(), generation)
    }

    /// The scratch a record `value` owns, or `None` when it recorded none
    /// (a Docker Sandbox's `/tmp` is its container's, not this host's).
    pub fn of(value: &SdkValue) -> Result<Option<Self>, SdkRefusal> {
        let scratch = match value.get(FIELD) {
            None | Some(SdkValue::Null) => return Ok(None),
            Some(scratch @ SdkValue::Map(_)) => scratch,
            Some(_) => return Err(refused("Failed", "a Sandbox scratch record is not a map")),
        };
        let root = scratch
            .get_text(ROOT)
            .ok_or_else(|| refused("Failed", "a Sandbox scratch record has no absolute root"))?;
        let created_at = scratch
            .get_int(CREATED_AT)
            .ok_or_else(|| refused("Failed", "a Sandbox scratch record has no createdAt"))?;
        let generation = scratch
            .get_int(GENERATION)
            .ok_or_else(|| refused("Failed", "a Sandbox scratch record has no generation"))?;
        let generation = u64::try_from(generation).map_err(|_| {
            refused(
                "Failed",
                "a Sandbox scratch generation is not a positive token",
            )
        })?;
        Self::new(PathBuf::from(root), created_at, generation).map(Some)
    }

    /// The ownership as the record keeps it.
    pub fn to_record(&self) -> SdkValue {
        SdkValue::map([
            (ROOT, SdkValue::text(self.root.to_string_lossy())),
            (CREATED_AT, SdkValue::Int(self.created_at)),
            (GENERATION, SdkValue::Int(self.generation as i64)),
        ])
    }

    /// Its absolute root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// When the Sandbox claimed it.
    pub fn created_at(&self) -> i64 {
        self.created_at
    }

    /// The token a cleaner re-checks immediately before deleting. It is this
    /// scratch's own token, distinct from #548 (B5)'s record generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}

/// Who asks a cleaner to remove a scratch. Both go through [`keep`], so the
/// automatic and the explicit paths cannot make different decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Owner {
    /// One settled close: it stops the runtime, so the scratch may go even
    /// when the record is `unresolved` (the close is the explicit owner
    /// that clears the intent).
    Close,
    /// A periodic pass: it deletes only a settled (`gone`) record's scratch
    /// and keeps `running`, `creating`, `closing` and `unresolved` ones.
    Sweep,
}

/// Why a cleaner kept a scratch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kept {
    /// The record carries no scratch.
    NoScratch,
    /// The root is outside the home that owns the record.
    Outside,
    /// The root is not a scratch directory: not `tmp`, under a Store or
    /// under a Workspace, or reached through a symbolic link.
    NotScratch,
    /// The Sandbox may still be running.
    Live,
    /// Its outcome is not known (`Unresolved`): an automatic pass keeps it.
    Unresolved,
    /// The record's generation changed after the plan was made.
    Stale,
    /// The record's ownership could not be read, so it authorizes nothing.
    Malformed,
}

impl Kept {
    /// Its stable label, as a report or a refusal detail shows it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoScratch => "no_scratch",
            Self::Outside => "outside_home",
            Self::NotScratch => "not_scratch",
            Self::Live => "live",
            Self::Unresolved => "unresolved",
            Self::Stale => "stale_generation",
            Self::Malformed => "malformed",
        }
    }
}

/// What one [`clean`] did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Cleaned {
    /// The record owns no scratch: nothing to do.
    None,
    /// The scratch at `root` was removed (or was already gone).
    Deleted(PathBuf),
    /// The scratch at `root` was kept, for the given reason.
    Kept(PathBuf, Kept),
    /// The record's ownership could not be read: nothing was deleted.
    Malformed,
}

/// One scratch root a pass planned or removed, with the record that owns
/// it: an operator report names the Sandbox, not only a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Owned {
    sandbox: String,
    root: PathBuf,
}

impl Owned {
    /// The root `root` of Sandbox `sandbox`.
    pub fn new(sandbox: &str, root: PathBuf) -> Self {
        Self {
            sandbox: sandbox.to_owned(),
            root,
        }
    }

    /// The record's name, as its folder holds it.
    pub fn sandbox(&self) -> &str {
        &self.sandbox
    }

    /// Its absolute scratch root.
    pub fn root(&self) -> &Path {
        &self.root
    }
}

/// What one [`sweep`] did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    /// The roots a dry run would delete, in record-name order, with the
    /// record that owns each.
    pub planned: Vec<Owned>,
    /// The roots an apply pass deleted, with the record that owned each.
    pub deleted: Vec<Owned>,
    /// The roots it kept, and why.
    pub kept: Vec<(Owned, Kept)>,
}

/// Whether one sweep should delete (it may) or apply (it does).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// List what an apply pass would delete; change nothing.
    DryRun,
    /// Delete what a dry run lists.
    Apply,
}

/// Removes the scratch `record` owns on behalf of `owner`, re-checking the
/// record's generation immediately before deleting. A stale generation, an
/// `Unresolved` record under [`Owner::Sweep`], and any root that is not the
/// Sandbox's own scratch are kept.
pub fn clean(home: &Path, record: &SandboxRecord, owner: Owner) -> Result<Cleaned, SdkRefusal> {
    let scratch = match record.scratch() {
        Ok(Some(scratch)) => scratch,
        Ok(None) => return Ok(Cleaned::None),
        // a malformed ownership authorizes no delete, but it must not stop
        // the explicit close that owns the record
        Err(_) => return Ok(Cleaned::Malformed),
    };
    let root = scratch.root().to_path_buf();
    // nothing at the root is already the desired state, even when the path
    // would no longer pass the checks (a removed run directory); any other
    // error is not "gone" and must not be reported as deleted
    match fs::symlink_metadata(&root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Cleaned::Deleted(root))
        }
        Err(error) => return Err(failed(error)),
        Ok(_) => {}
    }
    if let Some(why) = keep(home, &root, record.state(false), owner) {
        return Ok(Cleaned::Kept(root, why));
    }
    // the whole point of the token: re-read immediately before the delete
    let now = SandboxRecord::read(home, record.name())?;
    let current = match &now {
        Some(now) => now.scratch()?,
        None => None,
    };
    if current.map(|current| current.generation()) != Some(scratch.generation()) {
        return Ok(Cleaned::Kept(root, Kept::Stale));
    }
    remove_root(&root)?;
    Ok(Cleaned::Deleted(root))
}

/// Removes the scratch a settled close owns, without ever failing the close
/// (#549): the runtime is already stopped, so a cleaner's error must not
/// turn `sandbox.close` into an error after it succeeded. The failure is
/// appended to `log/scratch.jsonl` for people, and nothing is deleted.
pub fn clean_on_close(home: &Path, record: &SandboxRecord) {
    let Err(refusal) = clean(home, record, Owner::Close) else {
        return;
    };
    let root = record
        .scratch()
        .ok()
        .flatten()
        .map(|scratch| scratch.root().to_string_lossy().into_owned())
        .unwrap_or_default();
    let _ = crate::common::home::log_line(
        home,
        "scratch.jsonl",
        &SdkValue::map([
            ("at", SdkValue::Int(crate::common::home::now_ms())),
            ("sandbox", SdkValue::text(record.name())),
            ("scratch", SdkValue::text(root)),
            ("code", SdkValue::text(&refusal.code)),
            ("message", SdkValue::text(&refusal.message)),
        ]),
    );
}

/// One bounded, machine-keyed pass over the home's Sandbox records: it
/// deletes a settled record's scratch, and keeps a live or `Unresolved`
/// one. `machine` is the machine the pass runs on, never an isolation
/// class; records of other machines are left for that machine's pass, and
/// they are filtered out before the bounded batch so they cannot crowd a
/// settled record out of it. The batch is charged only for a root this pass
/// may remove: a kept record and an already-gone root do not consume it.
/// Since a removed root is gone next pass, a home with more than [`MAX_SWEEP`]
/// settled records is covered over successive passes. The report lists
/// exactly what [`Mode::DryRun`] would delete and what [`Mode::Apply`] did.
pub fn sweep(home: &Path, machine: &str, mode: Mode) -> Result<Report, SdkRefusal> {
    let mut report = Report::default();
    let dir = home.join("sandbox");
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(error) => return Err(failed(error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(failed)?;
        if entry.file_type().map_err(failed)?.is_dir() {
            names.push(entry.file_name());
        }
    }
    names.sort();
    // Filter by machine before the bounded batch, and charge the batch only
    // for a root this pass may remove. Another machine's records, a record
    // this pass must keep and an already-gone root cannot use up the batch,
    // so the settled record past them is still reached (#549).
    let mut removing = 0;
    for name in names {
        let Some(name) = name.to_str() else { continue };
        // a name that is not a Sandbox is not this pass's business
        if crate::common::names::SandboxName::parse(name).is_err() {
            continue;
        }
        let Some(record) = SandboxRecord::read(home, name)? else {
            continue;
        };
        if record.machine() != machine {
            continue;
        }
        let scratch = match record.scratch() {
            Ok(Some(scratch)) => scratch,
            // a malformed ownership is never guessed at
            Ok(None) => continue,
            Err(_) => {
                report
                    .kept
                    .push((Owned::new(name, dir.join(name)), Kept::Malformed));
                continue;
            }
        };
        let root = scratch.root().to_path_buf();
        if fs::symlink_metadata(&root).is_err() {
            continue;
        }
        if let Some(why) = keep(home, &root, record.state(false), Owner::Sweep) {
            report.kept.push((Owned::new(name, root), why));
            continue;
        }
        if removing == MAX_SWEEP {
            break;
        }
        removing += 1;
        match mode {
            Mode::DryRun => report.planned.push(Owned::new(name, root)),
            Mode::Apply => {
                // the generation may have moved since the plan: re-read
                let now = SandboxRecord::read(home, name)?;
                let current = match &now {
                    Some(now) => now.scratch()?,
                    None => None,
                };
                if current.map(|current| current.generation()) != Some(scratch.generation()) {
                    report.kept.push((Owned::new(name, root), Kept::Stale));
                    continue;
                }
                remove_root(&root)?;
                report.deleted.push(Owned::new(name, root));
            }
        }
    }
    Ok(report)
}

/// `Some(reason)` when `owner` must keep `root`, `None` when it may remove
/// it. This is the one predicate both [`clean`] and [`sweep`] use.
fn keep(home: &Path, root: &Path, state: &str, owner: Owner) -> Option<Kept> {
    if !owns(home, root) {
        return Some(match root.starts_with(home) {
            true => Kept::NotScratch,
            false => Kept::Outside,
        });
    }
    match owner {
        // a settled close stopped the runtime; the explicit owner is the
        // call that clears an uncertain intent too
        Owner::Close => None,
        Owner::Sweep => match state {
            "gone" => None,
            "unresolved" => Some(Kept::Unresolved),
            _ => Some(Kept::Live),
        },
    }
}

/// Whether `root` is the scratch of a Sandbox in `home`: absolute, no
/// `.`/`..`, its leaf `tmp`, not the host's `/tmp`, strictly under a home
/// that is not a Store or a Workspace folder, and with no symbolic link
/// leading outside it.
fn owns(home: &Path, root: &Path) -> bool {
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return false;
    }
    if root.file_name() != Some(OsStr::new(LEAF)) || host_tmp(root) {
        return false;
    }
    if !root.starts_with(home) || root == home {
        return false;
    }
    if root.starts_with(home.join("store")) || root.starts_with(home.join("sandbox")) {
        return false;
    }
    let real_home = match fs::canonicalize(home) {
        Ok(real_home) => real_home,
        Err(_) => return false,
    };
    let real_root = match fs::canonicalize(root) {
        Ok(real_root) => real_root,
        Err(_) => return false,
    };
    if !real_root.starts_with(&real_home) || real_root == real_home {
        return false;
    }
    !matches!(
        fs::symlink_metadata(root),
        Ok(meta) if meta.file_type().is_symlink()
    )
}

/// The host's `/tmp` (and its aliases on macOS and Linux), never a scratch.
fn host_tmp(root: &Path) -> bool {
    ["/tmp", "/private/tmp", "/var/tmp"]
        .iter()
        .any(|host| root == Path::new(host))
}

/// The path checks every ownership token passes, whichever record it came
/// from.
fn check_root(root: &Path) -> Result<(), SdkRefusal> {
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, Component::CurDir | Component::ParentDir))
    {
        return Err(refused(
            "InvalidInput",
            format!(
                "a Sandbox scratch root is an absolute path, not {}",
                root.display()
            ),
        ));
    }
    if root.file_name() != Some(OsStr::new(LEAF)) {
        return Err(refused(
            "InvalidInput",
            format!(
                "a Sandbox scratch root ends in /{LEAF}, not {}",
                root.display()
            ),
        ));
    }
    if host_tmp(root) {
        return Err(refused(
            "InvalidInput",
            format!(
                "{} is the host's temporary directory, never a Sandbox scratch",
                root.display()
            ),
        ));
    }
    Ok(())
}

/// Removes `root` once, refusing to follow a symbolic link at it.
fn remove_root(root: &Path) -> Result<(), SdkRefusal> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failed(error)),
        Ok(meta) if meta.file_type().is_symlink() => Err(refused(
            "Unavailable",
            format!(
                "Sandbox scratch {} is a symbolic link; it is not followed",
                root.display()
            ),
        )),
        Ok(_) => fs::remove_dir_all(root).map_err(failed),
    }
}
