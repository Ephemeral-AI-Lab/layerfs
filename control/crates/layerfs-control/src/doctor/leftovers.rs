//! What interrupted operations left behind (layerfs-project#180, `home
//! leftovers`): one bounded walk of one side's LayerFS folders, the home on
//! `native` or a machine's worker root, and how each entry it finds is
//! named.
//!
//! The walk ([`walk`]) is a read: it takes no lock, lease or daemon
//! connection, follows no link and opens no file but a run folder's small
//! `owner` marker and the Sandbox records. It looks only where LayerFS puts
//! its own entries: the run root (`machine/native/run`) with its image
//! cache, each Store folder's two staged-snapshot names and `incoming/`.
//! A Store's files, a lock or lease
//! file and an overlay database are never opened, and a received image part
//! is not listed (`machine images` shows it and `machine prune` removes it,
//! under the copy's lock); a run folder's
//! `workspaces/` (its Workspace mount points) is never entered or `stat`ed,
//! and a directory on another device (a mount) is not descended into. The
//! names an integration keeps in a home (`secret/`, `runtime/`, `job/`,
//! `stat/`, `preset/`) are neither listed nor read: the walk never lists the
//! home itself. It stops at [`Bounds`] and then answers `complete: false`.
//!
//! Each entry is `{ kind, name, path, bytes, modified }`: `bytes` is the
//! size of the file, or of every file under the folder (`null` when a bound
//! ended the walk before it was measured), and `modified` the newest
//! modification time (`mtime`, seconds since 1970) of the entry and of what
//! was measured under it. An image's `<hex>.2.lock` file and a Workspace's
//! `<id>.unmount` marker are LayerFS's own and never listed.
//!
//! An entry's id is `<machine>:<kind>:<name>` ([`id`], [`parse`]), its kind
//! in snake_case. The path is never part of it: the side that removes
//! derives the path from the kind and the name ([`Kind::path`]), whose
//! grammar admits no separator, no `..` and no leading dot, and finds the
//! entry again under its guard ([`super::leftover_prune`]). An
//! `export_container` is not on a disk: it is an image export container in
//! the host's Docker (`native:export_container:<container>`,
//! `layerfs_sandbox::image_exports`), listed by `home leftovers` beside
//! `native`'s walk.
use layerfs_bridge::control::SdkValue;
use layerfs_home::home::paths;
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox_backend::rootfs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Where one side's walk stops: after this many directory entries, or this
/// long, whichever comes first.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    /// Directory entries listed or measured.
    pub entries: usize,
    /// Wall time.
    pub within: Duration,
}

/// The bounds `home leftovers` walks each side with.
pub const BOUNDS: Bounds = Bounds {
    entries: 20_000,
    within: Duration::from_secs(2),
};

/// The two names a `backup.create` stages a snapshot under in a Store's
/// folder: the copy fetched from a worker, and the one staged beside the
/// Store's file.
pub const STAGED: [&str; 2] = ["backup.staging", "store.sqlite.backup"];

/// A run folder's marker naming the record folder that owns it.
pub const OWNER: &str = "owner";

/// One kind of leftover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A Sandbox run folder no Sandbox record names.
    RunFolder,
    /// An empty private root `<name>.root` whose run folder is gone.
    EmptyRoot,
    /// A folder copy `incoming/init-<id>` of a `project.init`.
    InitCopy,
    /// A staged snapshot of a `backup.create` ([`STAGED`]).
    BackupStaging,
    /// An image unpack's temporary directory `.tmp-<hex>.2-<id>`.
    ImageTmp,
    /// An image export container a killed copy left in the host's Docker.
    ExportContainer,
}

impl Kind {
    /// Every kind, in the order the specification lists them.
    pub const ALL: [Kind; 6] = [
        Kind::RunFolder,
        Kind::EmptyRoot,
        Kind::InitCopy,
        Kind::BackupStaging,
        Kind::ImageTmp,
        Kind::ExportContainer,
    ];

    /// Its word in an entry's `kind` and inside its id.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::RunFolder => "run_folder",
            Kind::EmptyRoot => "empty_root",
            Kind::InitCopy => "init_copy",
            Kind::BackupStaging => "backup_staging",
            Kind::ImageTmp => "image_tmp",
            Kind::ExportContainer => "export_container",
        }
    }

    /// The kind named `text`.
    pub fn named(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// Whether `name` is a name this kind gives.
    pub fn admits(self, name: &str) -> bool {
        match self {
            Kind::ExportContainer => layerfs_sandbox::image_exports::admits(name),
            _ => self.path(Path::new("/"), name).is_some(),
        }
    }

    /// Where entry `name` of this kind is under side root `root`, or `None`
    /// when `name` is not a name this kind gives (and for an
    /// `export_container`, which is in Docker). This is the only way a
    /// path comes from a fix id, and every name it admits is made of plain
    /// path components (letters, digits, `_`, `.`, `-`, no leading dot), so
    /// the path stays under the folder its kind names.
    pub fn path(self, root: &Path, name: &str) -> Option<PathBuf> {
        let run = paths::run_root(root);
        match self {
            Kind::RunFolder => run_name(name).then(|| run.join(name)),
            Kind::EmptyRoot => run_name(name).then(|| run.join(format!("{name}.root"))),
            Kind::InitCopy => (name.len() == 16 && hex(name))
                .then(|| layerfs_store::project::staged::folder(root, name)),
            Kind::BackupStaging => {
                let (store, file) = name.split_once('/')?;
                (layerfs_home::names::name("Store", store).is_ok() && STAGED.contains(&file))
                    .then(|| paths::store_dir(root, store).join(file))
            }
            Kind::ImageTmp => {
                let entry = format!(".tmp-{name}");
                rootfs::temporary_digest(&entry)
                    .is_some()
                    .then(|| rootfs::cache(&run).join(&entry))
            }
            Kind::ExportContainer => None,
        }
    }
}

/// Lowercase hex digits only.
fn hex(text: &str) -> bool {
    text.bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// One plain path component: letters, digits, `_`, `.` and `-`, not empty
/// and not starting with a dot (so never `.`, `..` or a hidden entry).
fn plain(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// A run folder's name: a plain component that is neither the image cache
/// nor a private root.
fn run_name(name: &str) -> bool {
    plain(name) && name != "images" && !name.ends_with(".root")
}

/// The id of entry `name` of `kind` on `machine`.
pub fn id(machine: &str, kind: Kind, name: &str) -> String {
    format!("{machine}:{}:{name}", kind.as_str())
}

/// The machine, kind and name an id holds; `None` unless every part is one
/// a [`walk`] gives.
pub fn parse(id: &str) -> Option<(&str, Kind, &str)> {
    let (machine, rest) = id.split_once(':')?;
    let (kind, name) = rest.split_once(':')?;
    let kind = Kind::named(kind)?;
    layerfs_home::names::name("machine", machine).ok()?;
    (kind.admits(name) && (kind != Kind::ExportContainer || machine == "native"))
        .then_some((machine, kind, name))
}

/// Whether `directory` is a mount point, from its own device, its parent's
/// and whether this process's mount table lists it: a directory on another
/// device than its parent is one whatever the table says.
pub fn is_mount(device: u64, parent_device: u64, listed: bool) -> bool {
    listed || device != parent_device
}

/// The Sandbox name a run folder belongs to: the folder its `owner` marker
/// names (`<root>/sandbox/<name>`), else what follows the prefix LayerFS
/// gives run folders (`layerfs-<8 hex>-`, or the earlier `lfs-<8 hex>-`).
pub fn sandbox_of(folder: &Path) -> Option<String> {
    if let Some(owner) = owner(folder) {
        let named = owner.file_name()?.to_str()?.to_owned();
        let parent = owner.parent()?.file_name()?;
        return (parent == "sandbox").then_some(named);
    }
    let name = folder.file_name()?.to_str()?;
    ["layerfs-", "lfs-"].into_iter().find_map(|prefix| {
        let rest = name.strip_prefix(prefix)?;
        let (home, sandbox) = (rest.get(..8)?, rest.get(8..)?.strip_prefix('-')?);
        (hex(home) && !sandbox.is_empty()).then(|| sandbox.to_owned())
    })
}

/// The record folder a run folder's `owner` marker names: read through no
/// link, at most 4096 bytes.
pub fn owner(folder: &Path) -> Option<PathBuf> {
    use std::io::Read;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::OpenOptionsExt;
    let mut marker = Vec::new();
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(folder.join(OWNER))
        .and_then(|file| file.take(4096).read_to_end(&mut marker))
        .ok()?;
    let path = PathBuf::from(std::ffi::OsString::from_vec(marker));
    path.is_absolute().then_some(path)
}

/// The Sandbox records of one side, as the walk and a fix read them: each
/// record's name and the name of the run folder its daemon record holds.
pub fn records(root: &Path) -> Vec<(String, Option<String>)> {
    let names = paths::names(&root.join("sandbox")).unwrap_or_default();
    (names.into_iter())
        .filter_map(|name| {
            let record = SandboxRecord::read(root, &name).ok()??;
            let folder = (record.daemon())
                .and_then(|daemon| daemon.directory())
                .and_then(|directory| Path::new(directory).file_name()?.to_str())
                .map(str::to_owned);
            Some((name, folder))
        })
        .collect()
}

/// Whether run folder `folder` belongs to a Sandbox someone records: a
/// record of this side names it; or its Sandbox ([`sandbox_of`]) has a
/// record here that names no run folder yet (a create in flight), or is one
/// the caller's home records (`known`: a machine's Sandboxes are recorded
/// in the home before their worker records them).
pub fn recorded(folder: &Path, records: &[(String, Option<String>)], known: &[String]) -> bool {
    let name = folder.file_name().and_then(|name| name.to_str());
    if (records.iter()).any(|(_, run)| run.as_deref().is_some_and(|run| Some(run) == name)) {
        return true;
    }
    sandbox_of(folder).is_some_and(|sandbox| {
        known.contains(&sandbox)
            || (records.iter()).any(|(record, run)| *record == sandbox && run.is_none())
    })
}

/// One side's walk in progress.
struct Walk<'a> {
    root: &'a Path,
    left: usize,
    deadline: Instant,
    complete: bool,
    now: i64,
    found: Vec<SdkValue>,
}

impl Walk<'_> {
    /// Counts one more entry; `false` once a bound has ended the walk.
    fn more(&mut self) -> bool {
        if self.left == 0 || Instant::now() >= self.deadline {
            self.complete = false;
            return false;
        }
        self.left -= 1;
        true
    }

    /// The entries of real directory `dir` (a link is not entered, an
    /// absent one is empty): each name and whether it is a directory, from
    /// the listing alone.
    fn list(&mut self, dir: &Path) -> Vec<(String, bool)> {
        let mut listed = Vec::new();
        if !std::fs::symlink_metadata(dir).is_ok_and(|found| found.is_dir()) {
            return listed;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return listed;
        };
        for entry in entries.flatten() {
            if !self.more() {
                break;
            }
            let directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
            if let Ok(name) = entry.file_name().into_string() {
                listed.push((name, directory));
            }
        }
        listed.sort();
        listed
    }

    /// The bytes of every file under `path` and the newest modification
    /// time seen, through no link and into no other device; `skip` names a
    /// child of `path` that is not entered. The bytes are `None` when a
    /// bound ended the walk first.
    fn measure(&mut self, path: &Path, skip: Option<&str>) -> (Option<i64>, i64) {
        let Ok(top) = std::fs::symlink_metadata(path) else {
            return (None, self.now);
        };
        let (device, mut newest, mut bytes) = (top.dev(), top.mtime(), 0u64);
        if !top.is_dir() {
            return (i64::try_from(top.len()).ok(), newest);
        }
        let mut pending = vec![(path.to_owned(), true)];
        while let Some((dir, first)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !self.more() {
                    return (None, newest);
                }
                if first && skip.is_some_and(|skip| entry.file_name() == skip) {
                    continue;
                }
                let Ok(found) = std::fs::symlink_metadata(entry.path()) else {
                    continue;
                };
                newest = newest.max(found.mtime());
                if !found.is_dir() {
                    bytes = bytes.saturating_add(found.len());
                } else if found.dev() == device {
                    pending.push((entry.path(), false));
                }
            }
        }
        (i64::try_from(bytes).ok(), newest)
    }

    fn found(&mut self, kind: Kind, name: &str, skip: Option<&str>) {
        let Some(path) = kind.path(self.root, name) else {
            return;
        };
        let (bytes, newest) = self.measure(&path, skip);
        // a run folder's `owner` marker, which `home prune` needs (and needs
        // quiet for a minute): when it was written, in seconds, else null
        let owner = match kind {
            Kind::RunFolder => std::fs::symlink_metadata(path.join(OWNER))
                .ok()
                .filter(|found| found.is_file())
                .map_or(SdkValue::Null, |found| SdkValue::Int(found.mtime())),
            _ => SdkValue::Null,
        };
        self.found.push(SdkValue::map([
            ("kind", SdkValue::text(kind.as_str())),
            ("name", SdkValue::text(name)),
            ("path", SdkValue::text(path.display().to_string())),
            ("bytes", bytes.map_or(SdkValue::Null, SdkValue::Int)),
            ("modified", SdkValue::Int(newest)),
            ("owner_written", owner),
        ]));
    }

    /// The run root: run folders no record names, empty private roots whose
    /// run folder is gone, and the image cache.
    fn run_root(&mut self, known: &[String]) {
        let run = paths::run_root(self.root);
        let listed = self.list(&run);
        if listed.is_empty() {
            return;
        }
        let records = records(self.root);
        for (name, directory) in &listed {
            if !directory {
                continue;
            }
            if name == "images" {
                self.images(&run.join(name));
            } else if let Some(base) = name.strip_suffix(".root") {
                let sibling = listed.iter().any(|(other, _)| other == base);
                let empty = std::fs::read_dir(run.join(name))
                    .is_ok_and(|mut entries| entries.next().is_none());
                if !sibling && empty {
                    self.found(Kind::EmptyRoot, base, None);
                }
            } else if run_name(name) && !recorded(&run.join(name), &records, known) {
                // its Workspace mount points are never entered or `stat`ed
                self.found(Kind::RunFolder, name, Some("workspaces"));
            }
        }
    }

    /// The image cache: an unpack's temporaries.
    fn images(&mut self, cache: &Path) {
        for (name, _) in self.list(cache) {
            if let Some(rest) = name.strip_prefix(".tmp-") {
                self.found(Kind::ImageTmp, rest, None);
            }
        }
    }

    /// Each Store folder's staged snapshots, by their two names: nothing
    /// else of a Store's folder is listed or opened.
    fn stores(&mut self) {
        for (store, directory) in self.list(&self.root.join("store")) {
            for file in STAGED {
                let path = paths::store_dir(self.root, &store).join(file);
                let staged = std::fs::symlink_metadata(path).is_ok_and(|found| found.is_file());
                if directory && staged && self.more() {
                    self.found(Kind::BackupStaging, &format!("{store}/{file}"), None);
                }
            }
        }
    }

    /// The folder copies of `project.init` under `incoming/`.
    fn incoming(&mut self) {
        for (name, directory) in self.list(&self.root.join("incoming")) {
            if let (true, Some(copy)) = (directory, name.strip_prefix("init-")) {
                self.found(Kind::InitCopy, copy, None);
            }
        }
    }
}

/// One side's walk under `root`, inside `bounds`: `{ leftovers: [{ kind,
/// name, path, bytes, modified }], complete }`.
/// `known` names the Sandboxes the caller's home records on this machine.
pub fn walk(root: &Path, known: &[String], bounds: Bounds) -> SdkValue {
    let mut walk = Walk {
        root,
        left: bounds.entries,
        deadline: Instant::now() + bounds.within,
        complete: true,
        now: layerfs_home::home::now_ms() / 1000,
        found: Vec::new(),
    };
    walk.run_root(known);
    walk.stores();
    walk.incoming();
    SdkValue::map([
        ("leftovers", SdkValue::List(walk.found)),
        ("complete", SdkValue::Bool(walk.complete)),
    ])
}

/// [`walk`] in a machine's worker, as its `doctor.sandbox` operation answers
/// `{ leftovers: true, known, timeout_ms }`: the worker's root, inside the
/// time the caller gives (at most [`BOUNDS`]'s).
pub(super) fn walk_in_worker(root: &Path, args: &SdkValue) -> SdkValue {
    let known: Vec<String> = match args.get("known") {
        Some(SdkValue::List(names)) => (names.iter())
            .filter_map(|name| match name {
                SdkValue::Text(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let within = (args.get_int("timeout_ms"))
        .and_then(|ms| u64::try_from(ms).ok())
        .map_or(BOUNDS.within, |ms| {
            Duration::from_millis(ms).min(BOUNDS.within)
        });
    walk(root, &known, Bounds { within, ..BOUNDS })
}
