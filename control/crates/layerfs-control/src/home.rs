//! `home.init` and `home.get`: setting a home up on purpose, and reading what
//! this build would use with it without changing anything. `init` opens the
//! home as every call does (created when empty; a folder that is not a
//! LayerFS 2.0 home is refused with the fix that moves it aside) and says which it did
//! (`outcome`: `created`, `adopted` or `unchanged`). A home that lost its
//! `layerfs.json` is not created again: `--adopt` writes the marker for the
//! data already there, taking the id its records keep, and audits it read
//! only (layerfs-project#752, c6).
//! `get` reads files only: no process, Docker, network, lock or log line, so a
//! client can poll it, and it answers for a home that does not exist yet.
//! What it answers is the call surface's (2.0): the home's `state` with
//! its `reason` and `fix`, the build's `version`, the home's `generation`,
//! and, only when asked, the platforms this computer supports (a probe)
//! and the tree of everything in the home ([`crate::home_tree`]).
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::layout::{detect, HomeFile, Layout, LAYOUT_VERSION};
use layerfs_home::home::{generation, paths, read_record, with_field};
use layerfs_home::names::{matches_provider_name, provider_prefix_hex};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_home::state::{Reason, State};
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::record::SandboxRecord;
use std::path::{Path, PathBuf};

/// The identity of the build making the call (`LAYERFS_BUILD_COMMIT` and
/// `LAYERFS_BUILD_DIRTY` when it was compiled), compared with the Linux
/// bundle's and the native daemon's; `commit` is `None` when it was built
/// without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Build<'a> {
    /// The 40-digit commit, if known.
    pub commit: Option<&'a str>,
    /// Whether the tree had uncommitted changes.
    pub dirty: bool,
}

/// The text a binary carries to name its build (`layerfs-daemon`):
/// the marker, then `<commit>:clean`, `<commit>:dirty` or `unknown`.
pub const BUILD_MARKER: &[u8] = b"layerfs-build-identity:";

/// What a home is, before anything opens it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Found {
    Missing,
    Current,
    Foreign,
    WorkerRoot,
    /// A home's data with no `layerfs.json`: `--adopt` writes it.
    DataWithoutMarker,
}

/// The sections `home.get` adds when asked (`--with`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct With {
    /// `platforms`: what this computer supports, probed.
    pub platforms: bool,
    /// `tree`: everything in the home, from its records.
    pub tree: bool,
}

/// The one command that makes a missing home.
const INIT: &str = "layerfs home init";

/// The home's state and layout, read only; a home the system refuses to
/// read is refused, as `detect` does.
fn state(home: &Path) -> Result<(Found, Option<i64>), layerfs_bridge::control::SdkRefusal> {
    Ok(match detect(home)? {
        Layout::Empty => (Found::Missing, None),
        Layout::Foreign => (Found::Foreign, None),
        Layout::WorkerRoot => (Found::WorkerRoot, None),
        Layout::DataWithoutMarker => (Found::DataWithoutMarker, None),
        Layout::Current(file) => (Found::Current, Some(file.layout)),
    })
}

/// `home.init`: the home ready for calls, as the first call of any other
/// kind makes it: `{ home, unchanged, outcome, layout, version }`, and for
/// `--adopt` also `{ id, id_source, warnings }`. Refused inside a machine's
/// worker root this process serves (`InvalidInput`), for a folder that is
/// not a LayerFS 2.0 home (`Unsupported`, with the fix that moves it
/// aside), and for a home that lost its marker without `--adopt`
/// (`Unsupported`, with `layerfs home init --adopt`; never a move).
/// `--adopt` takes the home lock, re-detects the folder under it, writes
/// `layerfs.json` atomically and never rewrites, moves or initializes the
/// data (layerfs-project#752, c6).
pub fn init(home: &Path, adopt: bool) -> Outcome {
    if layerfs_home::home::guest::is_root(home) {
        return Err(refused(
            "InvalidInput",
            "a machine's worker root is not a home; run `layerfs home init` on the host",
        ));
    }
    match state(home)? {
        // a current home is left as it is, with or without `--adopt`
        (Found::Current, layout) => unchanged(home, layout.unwrap_or(LAYOUT_VERSION)),
        // an empty home is created, `--adopt` or not
        (Found::Missing, _) => created(home),
        (Found::Foreign, _) => Err(layerfs_home::home::layout::foreign(home)),
        (Found::WorkerRoot, _) => Err(layerfs_home::home::layout::worker_root(home)),
        // the data is there but the marker is not: `--adopt` writes it,
        // and without `--adopt` nothing is written, not even the home lock
        (Found::DataWithoutMarker, _) if !adopt => {
            Err(layerfs_home::home::layout::data_without_marker(home))
        }
        (Found::DataWithoutMarker, _) => adopted(home),
    }
}

/// The answer of `home.init` for a home that was there already.
fn unchanged(home: &Path, layout: i64) -> Outcome {
    Ok(SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("unchanged", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("unchanged")),
        ("layout", SdkValue::Int(layout)),
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
    ]))
}

/// The answer of `home.init` for a home it made: an empty folder, created
/// as the first call of any other kind makes it.
fn created(home: &Path) -> Outcome {
    let file = layerfs_home::home::open(home)?;
    #[cfg(feature = "machine")]
    layerfs_machine::ensure_built_ins(home, &file.id)?;
    Ok(SdkValue::map([
        ("home", SdkValue::text(home.display().to_string())),
        ("unchanged", SdkValue::Bool(false)),
        ("outcome", SdkValue::text("created")),
        ("layout", SdkValue::Int(file.layout)),
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
    ]))
}

/// `home.init --adopt`: the home lock, then detection again under it (a
/// home adopted meanwhile is left as it is), then `layerfs.json` and the
/// read only audit. The data is never rewritten, moved or initialized.
fn adopted(home: &Path) -> Outcome {
    let _lock = layerfs_home::home::lock::take(home)?;
    match state(home)? {
        (Found::Current, layout) => unchanged(home, layout.unwrap_or(LAYOUT_VERSION)),
        (Found::Missing, _) => created(home),
        (Found::Foreign, _) => Err(layerfs_home::home::layout::foreign(home)),
        (Found::WorkerRoot, _) => Err(layerfs_home::home::layout::worker_root(home)),
        (Found::DataWithoutMarker, _) => {
            let (id, id_source, mut warnings) = adopted_id(home)?;
            let file = HomeFile {
                layout: LAYOUT_VERSION,
                id,
            };
            layerfs_home::home::layout::write(home, &file)?;
            warnings.extend(audit(home));
            Ok(SdkValue::map([
                ("home", SdkValue::text(home.display().to_string())),
                ("unchanged", SdkValue::Bool(false)),
                ("outcome", SdkValue::text("adopted")),
                ("layout", SdkValue::Int(file.layout)),
                ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
                ("id", SdkValue::text(&file.id)),
                ("id_source", SdkValue::text(id_source)),
                (
                    "warnings",
                    SdkValue::List(warnings.into_iter().map(|w| SdkValue::text(w)).collect()),
                ),
            ]))
        }
    }
}

/// The id `--adopt` writes: the longest hex prefix the home's records agree
/// on plus a random tail, so the machines and Sandboxes they name keep
/// their provider names; a fresh random id only when no record names one.
/// A record that cannot be read, or whose provider name is not one this
/// build parses, and records that disagree are refused: this build cannot
/// tell which home the data belongs to, so it writes nothing
/// (layerfs-project#752, c4, c6).
fn adopted_id(home: &Path) -> Result<(String, &'static str, Vec<String>), SdkRefusal> {
    let names = provider_names(home)?;
    let mut prefixes: Vec<String> = Vec::new();
    for (path, recorded) in &names {
        let Some(hex) = provider_prefix_hex(recorded) else {
            return Err(refused(
                "Failed",
                format!(
                    "{} names provider {recorded:?}, not a `layerfs-<hex>-` or `lfs-<hex>-` name, so this build cannot tell this home's id; repair or remove that record by hand",
                    path.display()
                ),
            ));
        };
        if !prefixes.iter().any(|known| known == hex) {
            prefixes.push(hex.to_owned());
        }
    }
    let mut warnings = Vec::new();
    let (id, id_source) = match prefixes.iter().max_by_key(|hex| hex.len()) {
        None => {
            warnings.push(
                "no machine or Sandbox record names this home, so its id is new; an older backup's `layerfs-exported-from` will not match it".to_owned(),
            );
            (fresh_id()?, "fresh")
        }
        Some(longest) => {
            if let Some(other) = prefixes
                .iter()
                .find(|hex| !longest.starts_with(hex.as_str()))
            {
                return Err(conflicting(home, longest, other));
            }
            let mut id = longest.clone();
            id.push_str(&random_hex()?[..32 - longest.len()]);
            // c4's predicate: every recorded name must still name this id
            for (_, recorded) in &names {
                if !matches_provider_name(&id, recorded) {
                    return Err(conflicting(
                        home,
                        longest,
                        provider_prefix_hex(recorded).unwrap_or_default(),
                    ));
                }
            }
            warnings.push(
                "this build's records keep only an 8- or 16-hex prefix of the home id; the adopted id keeps that prefix and a random tail, so an older backup's full `layerfs-exported-from` will not match it; a record that stored the full home id would make a later adopt exact".to_owned(),
            );
            (id, "records")
        }
    };
    Ok((id, id_source, warnings))
}

/// The provider names the home's records keep: each machine record's
/// `vm_name` and each Sandbox record's daemon name, with the record's path.
/// These are the fields the doctor's `home.layout` cross-check reads
/// (layerfs-project#752, c4); a record with no provider name (`native`, a
/// machine made before c2, a Sandbox that never started) names none.
fn provider_names(home: &Path) -> Result<Vec<(PathBuf, String)>, SdkRefusal> {
    let mut names = Vec::new();
    for name in paths::names(&home.join("machine")).map_err(failed)? {
        let path = paths::machine_record(home, &name);
        let Some(value) = read_record(&path)? else {
            continue;
        };
        let record = MachineRecord::from_record(&name, value);
        if let Some(recorded) = record.text("vm_name") {
            names.push((path, recorded.to_owned()));
        }
    }
    for name in paths::names(&home.join("sandbox")).map_err(failed)? {
        let Some(record) = SandboxRecord::read(home, &name)? else {
            continue;
        };
        let Some(recorded) = record.daemon().and_then(|daemon| daemon.name()) else {
            continue;
        };
        names.push((paths::sandbox_record(home, &name), recorded.to_owned()));
    }
    Ok(names)
}

/// The home id prefixes its provider names keep (layerfs-project#752, c4,
/// c6): each machine record's `vm_name` and each Sandbox record's daemon
/// `name`, as the hex run it starts with (`16` digits since c2, `8` before),
/// without duplicates. `--adopt` uses them to reuse the longest agreed
/// prefix; a record that cannot be read or whose name is not a provider name
/// this build parses is refused.
pub fn recorded_prefixes(home: &Path) -> Result<Vec<String>, SdkRefusal> {
    let mut prefixes: Vec<String> = Vec::new();
    for (path, recorded) in provider_names(home)? {
        let Some(hex) = provider_prefix_hex(&recorded) else {
            return Err(refused(
                "Failed",
                format!(
                    "{} names provider {recorded:?}, not a `layerfs-<hex>-` or `lfs-<hex>-` name",
                    path.display()
                ),
            ));
        };
        if !prefixes.iter().any(|known| known == hex) {
            prefixes.push(hex.to_owned());
        }
    }
    Ok(prefixes)
}

/// The refusal of a home whose records disagree on its id: nothing is
/// written; a person chooses the home the records belong to, or removes the
/// other's records.
fn conflicting(home: &Path, first: &str, second: &str) -> SdkRefusal {
    refused(
        "Failed",
        format!(
            "{}: this home's records disagree on its id (prefixes {first:?} and {second:?}); this build writes nothing: choose the home the records belong to, or remove the other's records by hand",
            home.display()
        ),
    )
}

/// A random 32-hex id.
fn fresh_id() -> Result<String, SdkRefusal> {
    Ok(random_hex()?[..32].to_owned())
}

/// 64 random hex digits (32 random bytes).
fn random_hex() -> Result<String, SdkRefusal> {
    Ok(layerfs_bridge::sdk_values::hex(
        &layerfs_home::helpers::random()?,
    ))
}

/// The parts a home keeps at its top level, as this build writes them.
const PARTS: [&str; 7] = [
    "machine", "store", "sandbox", "backup", "secret", "trace", "log",
];

/// The read only audit after `home.init --adopt` wrote the marker: each
/// expected part (missing, unreadable, or not 0700), each `*/record.json`
/// (missing or undecodable), a temporary a killed `home.init` left, and an
/// `incoming/init-*` copy a worker's root keeps. No doctor, no open, no
/// ensure: every finding is a warning for the caller
/// (layerfs-project#752, c6).
fn audit(home: &Path) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Some(mode) = bits(home) {
        if mode != 0o700 {
            warnings.push(format!("{}: mode {mode:04o}, not 0700", home.display()));
        }
    }
    for part in PARTS {
        let dir = home.join(part);
        match std::fs::read_dir(&dir) {
            Ok(_) => {
                if let Some(mode) = bits(&dir) {
                    if mode != 0o700 {
                        warnings.push(format!("{}: mode {mode:04o}, not 0700", dir.display()));
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                warnings.push(format!("{}: missing", dir.display()));
            }
            Err(error) => warnings.push(format!("{}: cannot be read: {error}", dir.display())),
        }
    }
    for path in record_files(home) {
        match read_record(&path) {
            Ok(Some(_)) => {}
            Ok(None) => warnings.push(format!("{}: no record", path.display())),
            Err(refusal) => warnings.push(format!("{}: {}", path.display(), refusal.message)),
        }
    }
    if let Ok(entries) = std::fs::read_dir(home) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(".layerfs.json.") && name.ends_with(".tmp") {
                warnings.push(format!(
                    "{}: a temporary a killed `home init` left",
                    entry.path().display()
                ));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(home.join("incoming")) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("init-") && entry.path().is_dir() {
                warnings.push(format!(
                    "{}: a worker's init copy; check this is the home you meant",
                    entry.path().display()
                ));
            }
        }
    }
    warnings
}

/// Every `record.json` a home's entity folders keep: one per machine, Store,
/// Sandbox and bucket, and one per project under a Store.
fn record_files(home: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for kind in ["machine", "sandbox", "backup"] {
        let Ok(entries) = std::fs::read_dir(home.join(kind)) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                paths.push(entry.path().join("record.json"));
            }
        }
    }
    let Ok(entries) = std::fs::read_dir(home.join("store")) else {
        return paths;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        paths.push(dir.join("record.json"));
        if let Ok(projects) = std::fs::read_dir(dir.join("project")) {
            for project in projects.flatten() {
                if project.path().is_dir() {
                    paths.push(project.path().join("record.json"));
                }
            }
        }
    }
    paths
}

/// A folder's permission bits, `0o700` for a private one.
#[cfg(unix)]
fn bits(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    Some(std::fs::metadata(path).ok()?.permissions().mode() & 0o777)
}

#[cfg(not(unix))]
fn bits(_path: &Path) -> Option<u32> {
    None
}

/// `home.get`: the home's path, `state` (with `reason`, `fix` and the
/// sentence behind them as `message` when it is broken), the build's
/// `version`, the home's `generation`, `layout` and `mode`, the Linux
/// bundle and native daemon this build would use and whether they come from
/// the same build, and the cache root. A home it cannot read is answered
/// `broken`, not refused. `with` adds `platforms` and `tree`; the tree is
/// null unless the home is ready (there are no records of this layout to
/// read).
pub fn get(home: &Path, build: Build<'_>, with: With) -> Outcome {
    let shown = home.display().to_string();
    let (state, layout, message) = match state(home) {
        Ok((Found::Current, layout)) => (State::Ready, layout, None),
        Ok((Found::Missing, layout)) => (
            State::broken(Reason::Missing, Some(INIT)),
            layout,
            Some(format!("no LayerFS home at {shown}; `{INIT}` creates it")),
        ),
        // a `layerfs.json` this build cannot parse, or of no layout it
        // knows, is not a 2.0 home's (MIG): `outdated` with the fix that
        // moves the folder aside (C11 gives a broken state its fix; RC-8)
        Ok((Found::Foreign, layout)) => (
            State::broken(
                Reason::Outdated,
                Some(&layerfs_home::home::layout::foreign_fix(home)),
            ),
            layout,
            Some(layerfs_home::home::layout::foreign(home).message),
        ),
        // a machine's worker root is not a home: `unsupported`, naming the
        // real home instead of moving the data disk (layerfs-project#752, c5)
        Ok((Found::WorkerRoot, layout)) => (
            State::broken(
                Reason::Unsupported,
                Some(layerfs_home::home::layout::worker_root_fix()),
            ),
            layout,
            Some(layerfs_home::home::layout::worker_root(home).message),
        ),
        // a home that lost its `layerfs.json` is not missing and is never
        // moved: `missing` with the adopt fix, which writes the marker and
        // keeps the data (layerfs-project#752, c6)
        Ok((Found::DataWithoutMarker, layout)) => (
            State::broken(
                Reason::Missing,
                Some(layerfs_home::home::layout::data_without_marker_fix()),
            ),
            layout,
            Some(layerfs_home::home::layout::data_without_marker(home).message),
        ),
        // `detect` refuses only a read the system refused (permissions,
        // I/O), which may be a real 2.0 home: `failed` with the OS error,
        // never the move aside (RC-8)
        Err(refusal) => (
            State::broken(Reason::Failed, None),
            None,
            Some(refusal.message),
        ),
    };
    let ready = state == State::Ready;
    let cache = match layerfs_machine::bundle::cache_root() {
        Ok(path) => SdkValue::map([("path", SdkValue::text(path.display().to_string()))]),
        Err(_) => SdkValue::Null,
    };
    let mut value = state.write(SdkValue::map([
        ("home", SdkValue::text(shown)),
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
        (
            "generation",
            match ready {
                true => SdkValue::Int(generation::read(home)),
                false => SdkValue::Null,
            },
        ),
        ("message", SdkValue::opt_text(message)),
        ("layout", layout.map_or(SdkValue::Null, SdkValue::Int)),
        ("mode", mode(home).map_or(SdkValue::Null, SdkValue::text)),
        ("bundle", bundle_value(build)),
        ("daemon", daemon_value(home, build)),
        ("cache", cache),
    ]));
    if with.platforms {
        value = with_field(value, "platforms", platforms(home)?);
    }
    if with.tree {
        let tree = match ready {
            true => crate::home_tree::tree(home)?,
            false => SdkValue::Null,
        };
        value = with_field(value, "tree", tree);
    }
    Ok(value)
}

/// `--with platforms`: `{ host, items }`, one row per platform this build
/// knows with what it is missing here ([`layerfs_machine::platforms`]).
#[cfg(feature = "machine")]
fn platforms(home: &Path) -> Outcome {
    layerfs_machine::platforms(home)
}

/// Without the `machine` feature (the guest worker's build) there are no
/// machine adapters to ask.
#[cfg(not(feature = "machine"))]
fn platforms(_home: &Path) -> Outcome {
    Err(refused(
        "Unsupported",
        "this build has no machine adapters, so it lists no platforms",
    ))
}

/// The home folder's permission bits, as `0700`; none while there is no
/// folder. Whether they are owner-only is the home doctor's to say.
#[cfg(unix)]
fn mode(home: &Path) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(home).ok()?;
    Some(format!("{:04o}", meta.permissions().mode() & 0o7777))
}

#[cfg(not(unix))]
fn mode(_home: &Path) -> Option<String> {
    None
}

/// Whether a part with `commit`/`dirty` comes from `build`: `false` when the
/// commits differ, `true` when they are equal and both trees were clean, and
/// null when either commit is unknown or a tree was dirty (it cannot tell).
fn matches(build: Build<'_>, commit: Option<&str>, dirty: Option<bool>) -> SdkValue {
    match (build.commit, commit) {
        (Some(ours), Some(theirs)) if ours != theirs => SdkValue::Bool(false),
        (Some(_), Some(_)) if !build.dirty && dirty == Some(false) => SdkValue::Bool(true),
        _ => SdkValue::Null,
    }
}

fn short(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

fn optional(text: Option<&str>) -> SdkValue {
    text.map_or(SdkValue::Null, SdkValue::text)
}

fn bundle_value(build: Build<'_>) -> SdkValue {
    let Ok((dir, source)) = layerfs_machine::bundle_place("linux") else {
        return SdkValue::Null;
    };
    let manifest = read_record(&dir.join("bundle.json")).ok().flatten();
    let origin = manifest.as_ref().and_then(|m| m.get("source"));
    let commit = origin.and_then(|s| s.get_text("head"));
    let dirty = match origin.and_then(|s| s.get("dirty")) {
        Some(SdkValue::Bool(dirty)) => Some(*dirty),
        _ => None,
    };
    let matched = matches(build, commit, dirty);
    SdkValue::map([
        ("path", SdkValue::text(dir.display().to_string())),
        ("source", SdkValue::text(source)),
        ("present", SdkValue::Bool(manifest.is_some())),
        ("commit", optional(commit)),
        ("dirty", dirty.map_or(SdkValue::Null, SdkValue::Bool)),
        ("matches", matched),
    ])
}

/// The daemon the native machine runs, as its Sandbox launch chooses it: the
/// one its record names (`recorded`), else the one beside this host's
/// bundle (`default`), with the build it names; read from files only.
pub struct NativeDaemon {
    /// Where it is.
    pub path: std::path::PathBuf,
    /// `recorded` or `default`.
    pub source: &'static str,
    /// Whether it is a file.
    pub present: bool,
    /// The commit and dirty flag stamped into it, when it names them.
    pub stamp: Option<(String, bool)>,
}

impl NativeDaemon {
    /// The native machine's daemon in `home` (a record that cannot be read
    /// leaves the default), or `None` when the user cache is unknown.
    pub fn of(home: &Path) -> Option<Self> {
        use layerfs_machine::record::MachineRecord;
        let recorded = MachineRecord::read(home, "native")
            .ok()
            .flatten()
            .and_then(|r| r.daemon().map(std::path::PathBuf::from));
        let (path, source) = match recorded {
            Some(path) => (path, "recorded"),
            None => (
                layerfs_machine::bundle_dir(std::env::consts::OS)
                    .ok()?
                    .join("layerfs-daemon"),
                "default",
            ),
        };
        // only a regular file is read (a FIFO or device would block or never end)
        let present = path.is_file();
        let stamp = present
            .then(|| std::fs::read(&path).ok())
            .flatten()
            .and_then(|bytes| stamp(&bytes));
        Some(Self {
            present,
            path,
            source,
            stamp,
        })
    }

    /// Whether it comes from `build` (as [`matches`] says).
    pub fn matches(&self, build: Build<'_>) -> SdkValue {
        let (commit, dirty) = match &self.stamp {
            Some((commit, dirty)) => (Some(commit.as_str()), Some(*dirty)),
            None => (None, None),
        };
        matches(build, commit, dirty)
    }

    /// `DaemonMissing`'s message.
    pub fn missing(&self) -> String {
        format!(
            "no native daemon at {}; the native machine needs this build's `layerfs-daemon`: `layerfs machine update native --daemon <path>`",
            self.path.display()
        )
    }

    /// `DaemonMismatch`'s message, when its commit differs from `build`'s.
    pub fn mismatch(&self, build: Build<'_>) -> Option<String> {
        let theirs = self.stamp.as_ref().map(|(commit, _)| commit.as_str())?;
        let ours = build.commit?;
        (self.matches(build) == SdkValue::Bool(false)).then(|| {
            format!(
                "the native daemon at {} was built from {}, this build from {}; record this build's daemon with `layerfs machine update native --daemon <path>`",
                self.path.display(),
                short(theirs),
                short(ours)
            )
        })
    }
}

/// `refusal` of a Sandbox start whose daemon (its detail's `daemon`) is the
/// native machine's and of another build than `build`, or names no build
/// at all (one older than the build marker): it says so, and its `fix`
/// records this build's daemon when [`own_daemon`] finds one, else runs
/// `layerfs home doctor`, which names the daemon and its build. Any other
/// refusal comes back as it is; one whose detail names no `daemon` (no
/// start failed) without reading anything.
pub fn native_start(home: &Path, build: Build<'_>, refusal: SdkRefusal) -> SdkRefusal {
    let Some(started) = refusal
        .detail
        .get_text("daemon")
        .map(std::path::PathBuf::from)
    else {
        return refusal;
    };
    let Some(ours) = build.commit else {
        return refusal;
    };
    let Some(daemon) = NativeDaemon::of(home).filter(|d| d.path == started) else {
        return refusal;
    };
    let said = match &daemon.stamp {
        Some((theirs, _)) if daemon.matches(build) == SdkValue::Bool(false) => format!(
            "The native machine's daemon {} was built from {}, this build from {}: \
             it cannot be expected to read what this build writes",
            daemon.path.display(),
            short(theirs),
            short(ours)
        ),
        Some(_) => return refusal,
        None if daemon.present => format!(
            "The native machine's daemon {} names no build: it is not this build ({}) \
             and may not read what this build writes",
            daemon.path.display(),
            short(ours)
        ),
        None => return refusal,
    };
    let fix = match own_daemon(build) {
        Some(path) => format!(
            "layerfs machine update native --daemon {}",
            crate::doctor::shell(&path)
        ),
        None => "layerfs home doctor".to_owned(),
    };
    let message = format!("{}. {said}", refusal.message);
    layerfs_home::outcome::with_fix(SdkRefusal { message, ..refusal }, fix)
}

/// This build's own `layerfs-daemon`: the one beside the running program
/// (a runtime's `bin/`), else the default beside this host's bundle, the
/// first that is a file stamped with `build`'s commit. Read from files only.
pub fn own_daemon(build: Build<'_>) -> Option<std::path::PathBuf> {
    let ours = build.commit?;
    let beside = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("layerfs-daemon")));
    let bundle = layerfs_machine::bundle_dir(std::env::consts::OS)
        .ok()
        .map(|dir| dir.join("layerfs-daemon"));
    [beside, bundle].into_iter().flatten().find(|path| {
        path.is_file()
            && std::fs::read(path)
                .ok()
                .and_then(|bytes| stamp(&bytes))
                .is_some_and(|(commit, _)| commit == ours)
    })
}

/// The native machine's daemon (macOS only; null elsewhere): the one its
/// record names, else the default beside the macOS bundle.
#[cfg(target_os = "macos")]
fn daemon_value(home: &Path, build: Build<'_>) -> SdkValue {
    let Some(daemon) = NativeDaemon::of(home) else {
        return SdkValue::Null;
    };
    let (commit, dirty) = match &daemon.stamp {
        Some((commit, dirty)) => (Some(commit.as_str()), Some(*dirty)),
        None => (None, None),
    };
    SdkValue::map([
        ("path", SdkValue::text(daemon.path.display().to_string())),
        ("source", SdkValue::text(daemon.source)),
        ("present", SdkValue::Bool(daemon.present)),
        ("commit", optional(commit)),
        ("dirty", dirty.map_or(SdkValue::Null, SdkValue::Bool)),
        ("matches", daemon.matches(build)),
    ])
}

#[cfg(not(target_os = "macos"))]
fn daemon_value(_home: &Path, _build: Build<'_>) -> SdkValue {
    SdkValue::Null
}

/// The build a binary names after [`BUILD_MARKER`]: its commit and whether
/// the tree was dirty; `None` without the marker or for `unknown`.
pub fn stamp(bytes: &[u8]) -> Option<(String, bool)> {
    let at = bytes
        .windows(BUILD_MARKER.len())
        .position(|w| w == BUILD_MARKER)?
        + BUILD_MARKER.len();
    let rest = bytes.get(at..at + 46)?;
    let (commit, state) = rest.split_at(40);
    if !commit.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let dirty = match state {
        b":dirty" => true,
        b":clean" => false,
        _ => return None,
    };
    Some((String::from_utf8_lossy(commit).into_owned(), dirty))
}
