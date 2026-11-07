//! `project.export`: exports a project's committed Branch snapshot from its
//! Store to a local folder on the caller's host, with live progress (`prepare`,
//! `export`, `finalize`) across `native` SQLite, PostgreSQL and worker-backed
//! SQLite Stores (#53).
use super::frame::ExportFrame;
use super::staged;
use super::tree::{header, Reader, Record};
use crate::lease;
use crate::open_args::{OpenArgs, PgSelection};
use crate::place::{self, Executor, Worker};
use crate::project::discovery::home_of;
use crate::project::record::ProjectRecord;
use crate::record::{StoreRecord, StoreRef};
use crate::resolve::{self, Target};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::{hex, node_text, short, unhex};
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::names::ProjectRef;
use layerfs_home::outcome::{failed, refused, Outcome, View};
use layerfs_content::compare::{memo::Memo, tree::TreeTable, EntryFacts};
use layerfs_content::file::FileView;
use layerfs_content::filesystem::{
    attributes::{read_portable, AttributeReadWork},
    directory::read::{list_after, DirectoryReadWork},
    path::PathName,
    sorted::DirectoryRoot,
    symlink::SymlinkTarget,
    FilesystemRootId,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError};
use layerfs_history::{HistoryName, NodeId};
use layerfs_store_service::history::{lookup, MAIN};
use layerfs_store_service::read::cached::Cached;
use layerfs_store_service::store::StorePorts;
use layerfs_store_service::StoreService;
use layerfs_telemetry::timer::Timing;
use std::fs;
use std::io::Write;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Maximum size of one streamed `tree::Record` part (`2 MiB`).
const PART_LIMIT: usize = 2 << 20;
/// Maximum size of one `Record::Write` piece (`256 KiB`).
const PIECE_LIMIT: usize = 256 << 10;
/// Smallest room left in a part worth filling before flushing (`64 KiB`).
const MIN_ROOM: usize = 64 << 10;
/// Directory entries read per B-tree page wave.
const DIRECTORY_WINDOW: usize = 64;
const DIRECTORY_BYTES: usize = 8192;
/// The least time between two `export` progress events: at most five a
/// second however fast entries are written, so the event count follows
/// time, not bytes or entries.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// One `project.export(project, destination, { branch, on_progress })`.
pub struct ProjectExport {
    /// The project, `<store>/<project>`.
    pub project: ProjectRef,
    /// The local folder it is exported to: absolute, absent or empty.
    pub destination: String,
    /// The Branch whose snapshot is exported; `main` when absent.
    pub branch: Option<String>,
}

/// Summary of the pinned snapshot and exported tree counts.
struct SnapshotSummary {
    snapshot: String,
    node: String,
    root: String,
    counts: Counts,
}

/// Runs `project.export(project, destination, { branch, on_progress })`.
pub fn run(home: &Path, context: &mut Events<'_>, request: &ProjectExport) -> Outcome {
    let home = home.to_path_buf();
    let project = request.project.to_string();
    let project = project.as_str();
    let destination_text = request.destination.as_str();
    let branch = request.branch.as_deref().unwrap_or(MAIN);
    validate_branch_arg(branch)?;
    let destination = check_destination(&home, destination_text)?;
    let target = resolve::target(&home, project)?;
    let project_record = ProjectRecord::read(&home, &target.store, &target.project)?
        .ok_or_else(|| refused("NoSuchProject", format!("no project {project}")))?;
    target.record.unblocked()?;
    let (lease, record) = lease::claim(&home, &target.store)?;
    let store_ref = record.store_ref()?.clone();
    let summary = match &store_ref {
        StoreRef::Postgres { .. } => export_postgres(
            context,
            &home,
            &target,
            &record,
            branch,
            &destination,
            lease,
        )?,
        StoreRef::Sqlite { .. } => match place::plan(&home, &store_ref)? {
            Executor::Local => export_local_sqlite(
                context,
                &home,
                &target,
                &record,
                branch,
                &destination,
                lease,
            )?,
            Executor::Worker(worker) => export_on_worker(
                context,
                worker,
                &target,
                &record,
                branch,
                &destination,
                lease,
            )?,
        },
    };
    Ok(build_result(
        project_record,
        &record,
        branch,
        destination_text,
        summary,
    ))
}

/// Rejects invalid branch names and 66-char hex Commit/Layer identities before
/// any effect.
fn validate_branch_arg(branch: &str) -> Result<()> {
    if branch == MAIN {
        return Ok(());
    }
    if branch.len() == 66 && unhex(branch).is_some() {
        return Err(refused(
            "InvalidInput",
            format!("`{branch}` is a Commit or Layer identity; `branch` must name a Branch"),
        ));
    }
    HistoryName::new(branch).map_err(|_| {
        refused(
            "InvalidInput",
            format!("`{branch}` is not a valid Branch name"),
        )
    })?;
    Ok(())
}

/// Checks `destination` on the caller's host before reaching the Store or
/// creating any staging directory.
fn check_destination(home: &Path, text: &str) -> Result<PathBuf> {
    let dest = PathBuf::from(text);
    if !dest.is_absolute() {
        return Err(refused(
            "InvalidInput",
            "the destination must be an absolute path",
        ));
    }
    if dest
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must not contain `.` or `..` components",
                dest.display()
            ),
        ));
    }
    let Some(parent) = dest.parent() else {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must name a directory path with a parent",
                dest.display()
            ),
        ));
    };
    if dest.file_name().is_none() {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} must name a directory, not the filesystem root",
                dest.display()
            ),
        ));
    }
    if overlaps_home(home, &dest, parent) {
        return Err(refused(
            "InvalidInput",
            format!(
                "destination {} is inside the Store home ({})",
                dest.display(),
                home.display()
            ),
        ));
    }
    check_destination_state(&dest, parent)?;
    Ok(dest)
}

fn overlaps_home(home: &Path, dest: &Path, parent: &Path) -> bool {
    if dest.starts_with(home) || home.starts_with(dest) {
        return true;
    }
    let Ok(canon_home) = fs::canonicalize(home) else {
        return false;
    };
    if let Ok(canon_parent) = fs::canonicalize(parent) {
        if let Some(name) = dest.file_name() {
            let resolved = canon_parent.join(name);
            if resolved.starts_with(&canon_home) || canon_home.starts_with(&resolved) {
                return true;
            }
        }
    }
    false
}

/// Verifies that `dest` is either absent (with an existing real parent
/// directory) or an existing empty real directory (not a symlink or file).
fn check_destination_state(dest: &Path, parent: &Path) -> Result<()> {
    match fs::symlink_metadata(dest) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_dir() {
                return Err(refused(
                    "Exists",
                    format!(
                        "destination {} already exists and is not a directory",
                        dest.display()
                    ),
                ));
            }
            let mut entries = fs::read_dir(dest)
                .map_err(|e| refused("Failed", format!("{}: {e}", dest.display())))?;
            if entries.next().transpose().map_err(failed)?.is_some() {
                return Err(refused(
                    "NotEmpty",
                    format!("destination {} is not empty", dest.display()),
                ));
            }
            check_parent_is_real_dir(parent)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => check_parent_is_real_dir(parent),
        Err(e) => Err(refused("Failed", format!("{}: {e}", dest.display()))),
    }
}

fn check_parent_is_real_dir(parent: &Path) -> Result<()> {
    let meta = match fs::metadata(parent) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(refused(
                "NotFound",
                format!(
                    "destination parent directory {} does not exist",
                    parent.display()
                ),
            ))
        }
        Err(e) => return Err(refused("Failed", format!("{}: {e}", parent.display()))),
    };
    if !meta.is_dir() {
        return Err(refused(
            "NotDirectory",
            format!("destination parent {} is not a directory", parent.display()),
        ));
    }
    // refuse intermediate symlinks when all ancestor components are under a
    // non-system path or if the immediate parent itself is a symlink when
    // destination already exists; note `/var` -> `/private/var` on macOS is a
    // system root symlink, so we only check `metadata(parent).is_dir()` above.
    Ok(())
}

/// Hidden sibling staging directory `<parent>/.layerfs-export-<id>` that
/// cleans itself up (including `0o555` subdirectories) unless committed.
struct StagingGuard {
    staging: PathBuf,
    committed: bool,
}

impl StagingGuard {
    fn create(dest: &Path) -> Result<Self> {
        let parent = dest
            .parent()
            .ok_or_else(|| refused("InvalidInput", "destination has no parent"))?;
        let random = layerfs_home::helpers::random()?;
        let staging = parent.join(format!(".layerfs-export-{}", hex(&random[..8])));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&staging)
            .map_err(|e| {
                let code = if e.raw_os_error() == Some(libc::ENOSPC) {
                    "Capacity"
                } else {
                    "Failed"
                };
                refused(code, format!("{}: {e}", staging.display()))
            })?;
        Ok(Self {
            staging,
            committed: false,
        })
    }

    fn path(&self) -> &Path {
        &self.staging
    }

    fn publish(mut self, dest: &Path) -> Result<()> {
        let parent = dest
            .parent()
            .ok_or_else(|| refused("InvalidInput", "destination has no parent"))?;
        check_destination_state(dest, parent)?;
        fs::rename(&self.staging, dest).map_err(|e| {
            let code = match e.raw_os_error() {
                Some(libc::ENOTEMPTY | libc::EEXIST) => "NotEmpty",
                Some(libc::ENOSPC) => "Capacity",
                _ => "Failed",
            };
            refused(code, format!("{}: {e}", dest.display()))
        })?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for StagingGuard {
    fn drop(&mut self) {
        if !self.committed {
            let _ = staged::remove(&self.staging);
        }
    }
}

/// Local writer and throttled progress emitter for `prepare`, `export` and
/// `finalize`.
struct LocalWriter {
    guard: StagingGuard,
    files: u64,
    directories: u64,
    symlinks: u64,
    bytes: u64,
    last_emit: Instant,
    emitted_final_counts: bool,
}

impl LocalWriter {
    fn start(context: &mut Events<'_>, dest: &Path) -> Result<Self> {
        let guard = StagingGuard::create(dest)?;
        let mut writer = Self {
            guard,
            files: 0,
            directories: 0,
            symlinks: 0,
            bytes: 0,
            last_emit: Instant::now(),
            emitted_final_counts: false,
        };
        writer.emit_phase(context, "prepare", SdkValue::Null);
        writer.emit_export(context);
        Ok(writer)
    }

    fn emit_phase(&self, context: &mut Events<'_>, phase: &str, done: SdkValue) {
        context.progress(self.phase_event(phase, done));
    }

    fn phase_event(&self, phase: &str, done: SdkValue) -> SdkValue {
        SdkValue::map([
            ("phase", SdkValue::text(phase)),
            ("files", SdkValue::Int(self.files as i64)),
            ("directories", SdkValue::Int(self.directories as i64)),
            ("symlinks", SdkValue::Int(self.symlinks as i64)),
            ("bytes", SdkValue::Int(self.bytes as i64)),
            ("done", done),
            ("total", SdkValue::Null),
        ])
    }

    fn emit_export(&mut self, context: &mut Events<'_>) {
        self.emit_phase(context, "export", SdkValue::Int(self.bytes as i64));
        self.last_emit = Instant::now();
        self.emitted_final_counts = true;
    }

    fn maybe_emit_export(&mut self, context: &mut Events<'_>) {
        self.emitted_final_counts = false;
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.emit_export(context);
        }
    }

    fn write_part(&mut self, context: &mut Events<'_>, part: &[u8]) -> Result<()> {
        let mut reader = Reader::new(part);
        while let Some(record) = reader.next_record()? {
            staged::write_exported(self.guard.path(), record)?;
            match record {
                Record::Directory { .. } => {
                    self.directories += 1;
                    self.maybe_emit_export(context);
                }
                Record::Write { offset, bytes, .. } => {
                    if offset == 0 {
                        self.files += 1;
                    }
                    self.bytes += bytes.len() as u64;
                    self.maybe_emit_export(context);
                }
                Record::Link { .. } => {
                    self.symlinks += 1;
                    self.maybe_emit_export(context);
                }
                Record::Meta { .. } => {}
            }
        }
        Ok(())
    }

    fn finish(
        mut self,
        context: &mut Events<'_>,
        dest: &Path,
        expected: Counts,
    ) -> Result<Counts> {
        if self.files != expected.files
            || self.directories != expected.directories
            || self.symlinks != expected.symlinks
            || self.bytes != expected.bytes
        {
            return Err(refused(
                "Failed",
                format!(
                    "exported counts ({} files, {} directories, {} symlinks, {} bytes) do not match snapshot ({} files, {} directories, {} symlinks, {} bytes)",
                    self.files,
                    self.directories,
                    self.symlinks,
                    self.bytes,
                    expected.files,
                    expected.directories,
                    expected.symlinks,
                    expected.bytes,
                ),
            ));
        }
        if !self.emitted_final_counts {
            self.emit_export(context);
        }
        // `finalize` is the last event and means the folder is in place: a
        // publish that fails emits none
        let finalize = self.phase_event("finalize", SdkValue::Int(self.bytes as i64));
        self.guard.publish(dest)?;
        context.progress(finalize);
        Ok(expected)
    }
}

fn export_local_sqlite(
    context: &mut Events<'_>,
    home: &Path,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let store_file = paths::store_file(home, &target.store);
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(target.store.clone()),
        project: Some(target.project.clone()),
        store_path: Some(store_file.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    export_in_process(context, &open, &target.project, branch, dest)
}

fn export_postgres(
    context: &mut Events<'_>,
    home: &Path,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let url = crate::record::url(record)?;
    let key = record.postgresql_key(home, &target.store);
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(target.store.clone()),
        project: Some(target.project.clone()),
        store_path: Some(key.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: Some(PgSelection {
            url: Some(url),
            ..PgSelection::default()
        }),
    };
    export_in_process(context, &open, &target.project, branch, dest)
}

fn export_in_process(
    context: &mut Events<'_>,
    open_args: &OpenArgs,
    project: &str,
    branch: &str,
    dest: &Path,
) -> Result<SnapshotSummary> {
    crate::service::with_service(open_args, |service| {
        let pinned = pin_snapshot(service, project, branch)?;
        let mut writer = LocalWriter::start(context, dest)?;
        let counts = stream_snapshot(&pinned, &mut |part| writer.write_part(context, part))?;
        let counts = writer.finish(context, dest, counts)?;
        Ok(SnapshotSummary {
            snapshot: pinned.label,
            node: pinned.node,
            root: pinned.root,
            counts,
        })
    })
}

fn export_on_worker(
    context: &mut Events<'_>,
    mut worker: Worker,
    target: &Target,
    record: &StoreRecord,
    branch: &str,
    dest: &Path,
    _lease: lease::Lease,
) -> Result<SnapshotSummary> {
    let request = ExportFrame {
        store: layerfs_home::names::StoreName::parse(&target.store)?,
        project: target.project.clone(),
        branch: Some(branch.to_owned()),
        store_record: Some(record.to_record().clone()),
    }
    .encode();
    let mut writer: Option<LocalWriter> = None;
    // a local failure refuses the event: the connection closes and the
    // worker stops streaming at its next part
    let value = worker.call_until("project.export", request, &mut |event| {
        let part = match event.get("part") {
            Some(SdkValue::Bytes(part)) => Some(part),
            _ if event.get_text("phase") == Some("prepare") => None,
            _ => return Ok(()),
        };
        let w = match writer.as_mut() {
            Some(w) => w,
            None => writer.insert(LocalWriter::start(context, dest)?),
        };
        match part {
            Some(part) => w.write_part(context, part),
            None => Ok(()),
        }
    })?;
    let expected = worker_counts(&value)?;
    let w = match writer {
        Some(w) => w,
        None => LocalWriter::start(context, dest)?,
    };
    let counts = w.finish(context, dest, expected)?;
    Ok(SnapshotSummary {
        snapshot: value.get_text("snapshot").unwrap_or("").to_owned(),
        node: value.get_text("node").unwrap_or("").to_owned(),
        root: value.get_text("root").unwrap_or("").to_owned(),
        counts,
    })
}

fn worker_counts(value: &SdkValue) -> Result<Counts> {
    let take = |field: &str| -> Result<u64> {
        value
            .get_int(field)
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| {
                refused(
                    "Failed",
                    format!("worker project.export reply is missing valid `{field}` count"),
                )
            })
    };
    Ok(Counts {
        files: take("files")?,
        directories: take("directories")?,
        symlinks: take("symlinks")?,
        bytes: take("bytes")?,
    })
}

/// Worker side of `project.export`: opens the SQLite Store under `root`, pins
/// the Branch snapshot once and streams bounded `tree::Record` parts back to
/// the caller through `events`.
pub fn in_worker(
    root: &Path,
    request: &ExportFrame,
    events: &mut dyn FnMut(SdkValue) -> Result<()>,
) -> Outcome {
    let root = home_of(Some(root))?;
    let store = request.store.as_str();
    let project = request.project.as_str();
    let branch = request.branch.as_deref().unwrap_or(MAIN);
    validate_branch_arg(branch)?;
    let _lease = lease::hold_locked(&root, store)?;
    let open = OpenArgs {
        home: Some(root.clone()),
        store: Some(store.to_owned()),
        project: Some(project.to_owned()),
        store_path: Some(paths::store_file(&root, store).display().to_string()),
        store_record: request.store_record.clone(),
        postgresql: None,
    };
    crate::service::with_service(&open, |service| {
        let pinned = pin_snapshot(service, project, branch)?;
        events(SdkValue::map([("phase", SdkValue::text("prepare"))]))?;
        let counts = stream_snapshot(&pinned, &mut |part| {
            events(SdkValue::map([("part", SdkValue::Bytes(part.to_vec()))]))
        })?;
        Ok(SdkValue::map([
            ("snapshot", SdkValue::text(&pinned.label)),
            ("node", SdkValue::text(&pinned.node)),
            ("root", SdkValue::text(&pinned.root)),
            ("files", SdkValue::Int(counts.files as i64)),
            ("directories", SdkValue::Int(counts.directories as i64)),
            ("symlinks", SdkValue::Int(counts.symlinks as i64)),
            ("bytes", SdkValue::Int(counts.bytes as i64)),
        ]))
    })
}

struct PinnedSnapshot {
    reader: std::sync::Arc<StorePorts>,
    label: String,
    node: String,
    root: String,
    tree: TreeTable,
}

/// Resolves `(project, branch)` once into an immutable `(NodeId, ObjectId,
/// TreeTable)` snapshot.
fn pin_snapshot(service: &StoreService, project: &str, branch: &str) -> Result<PinnedSnapshot> {
    let stack = lookup::project(&service.store, project)?;
    let node = if branch == MAIN {
        NodeId::Layer(stack.head_layer)
    } else {
        let branch_rec = lookup::branch(&service.store, &stack, branch)?;
        lookup::branch_node(&branch_rec)
    };
    let label = format!("{branch}@{}", short(node));
    let root = lookup::root(&service.store, node)?;
    let reader = service.store.ports(stack.scope);
    let tree = TreeTable::open(reader.as_ref(), FilesystemRootId(root)).map_err(lookup::content)?;
    Ok(PinnedSnapshot {
        reader,
        label,
        node: node_text(node),
        root: hex(root.as_bytes()),
        tree,
    })
}

#[derive(Default)]
struct Counts {
    files: u64,
    directories: u64,
    symlinks: u64,
    bytes: u64,
}

struct DirFrame {
    rel: Vec<u8>,
    entry: EntryFacts,
    after: Option<PathName>,
    pending: std::vec::IntoIter<(PathName, u64, Option<InodeValue>)>,
    exhausted: bool,
}

/// Walks the pinned snapshot depth-first through `Cached` + `Memo`, encoding
/// `tree::Record`s into bounded parts and passing each part to `on_part`.
fn stream_snapshot(
    pinned: &PinnedSnapshot,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<Counts> {
    let cached = Cached::new(&pinned.reader)?;
    let memo = Memo::new(&cached);
    let root_serial = pinned.tree.root_serial();
    let root_value = pinned
        .tree
        .value(&memo, root_serial)
        .map_err(lookup::content)?
        .ok_or_else(|| lookup::failed("missing root inode"))?;
    if root_value.kind != InodeKind::Directory {
        return Err(lookup::failed("snapshot root is not a directory"));
    }
    let mut part = Vec::with_capacity(PART_LIMIT.min(256 << 10));
    let mut counts = Counts::default();
    let mut stack = vec![DirFrame {
        rel: Vec::new(),
        entry: EntryFacts {
            serial: root_serial,
            value: root_value,
        },
        after: None,
        pending: Vec::new().into_iter(),
        exhausted: false,
    }];

    while let Some(frame) = stack.last_mut() {
        if frame.pending.len() == 0 && !frame.exhausted {
            let page = list_after(
                &memo,
                DirectoryRoot(frame.entry.value.content_root),
                frame.after.as_ref(),
                DIRECTORY_WINDOW,
                DIRECTORY_BYTES,
                &mut DirectoryReadWork::default(),
            )
            .map_err(lookup::content)?;
            frame.exhausted = page.continuation.is_none();
            frame.after = page.continuation;
            let serials: Vec<u64> = page.entries.iter().map(|(_, s)| *s).collect();
            let values = pinned
                .tree
                .values(&memo, &serials)
                .map_err(lookup::content)?;
            frame.pending = page
                .entries
                .into_iter()
                .zip(values)
                .map(|((name, serial), val)| (name, serial, val))
                .collect::<Vec<_>>()
                .into_iter();
        }
        let Some((name, serial, value)) = frame.pending.next() else {
            let finished = stack.pop().expect("active directory frame");
            emit_meta(
                &memo,
                &finished.rel,
                &finished.entry.value,
                &mut part,
                on_part,
            )?;
            continue;
        };
        let value = value.ok_or_else(|| lookup::failed("missing bound inode"))?;
        let mut rel = frame.rel.clone();
        if !rel.is_empty() {
            rel.push(b'/');
        }
        rel.extend_from_slice(name.as_bytes());
        match value.kind {
            InodeKind::Directory => {
                counts.directories += 1;
                push_record(Record::Directory { path: &rel }, &mut part, on_part)?;
                stack.push(DirFrame {
                    rel,
                    entry: EntryFacts { serial, value },
                    after: None,
                    pending: Vec::new().into_iter(),
                    exhausted: false,
                });
            }
            InodeKind::Symlink => {
                counts.symlinks += 1;
                let canonical = memo
                    .read_canonical(value.content_root)
                    .map_err(lookup::content)?;
                let target = SymlinkTarget::decode(&canonical).map_err(lookup::content)?;
                push_record(
                    Record::Link {
                        path: &rel,
                        target: target.as_bytes(),
                    },
                    &mut part,
                    on_part,
                )?;
                emit_meta(&memo, &rel, &value, &mut part, on_part)?;
            }
            InodeKind::RegularFile => {
                counts.files += 1;
                let file_bytes = emit_file(&cached, &rel, &value, &mut part, on_part)?;
                counts.bytes += file_bytes;
                emit_meta(&memo, &rel, &value, &mut part, on_part)?;
            }
        }
    }
    flush_part(&mut part, on_part)?;
    Ok(counts)
}

fn emit_meta(
    reader: &dyn AuthenticatedObjects,
    rel: &[u8],
    value: &InodeValue,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let meta = read_portable(
        reader,
        value.metadata_root,
        value.kind,
        &mut AttributeReadWork::default(),
    )
    .map_err(lookup::content)?;
    push_record(
        Record::Meta {
            path: rel,
            mode: meta.mode,
            seconds: meta.mtime_seconds,
            nanoseconds: meta.mtime_nanoseconds,
        },
        part,
        on_part,
    )
}

fn emit_file(
    reader: &dyn AuthenticatedObjects,
    rel: &[u8],
    value: &InodeValue,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<u64> {
    let canonical = reader
        .read_canonical(value.content_root)
        .map_err(lookup::content)?;
    let view = FileView::from_canonical(value.content_root, canonical).map_err(lookup::content)?;
    let len = view.logical_len();
    if len == 0 {
        push_record(
            Record::Write {
                path: rel,
                offset: 0,
                bytes: &[],
            },
            part,
            on_part,
        )?;
        return Ok(0);
    }
    if let Some(payload) = view.whole_file_bytes().map_err(lookup::content)? {
        write_pieces(rel, 0, payload, part, on_part)?;
        return Ok(len);
    }
    let mut sink = PieceSink {
        rel,
        offset: 0,
        part,
        on_part,
        error: None,
    };
    let (read_res, _) = Timing::disabled("export.file.read", |scope| {
        view.read_range(reader, 0..len, &mut sink, scope)
    });
    if let Some(err) = sink.error {
        return Err(err);
    }
    read_res.map_err(|e| match e {
        ContentError::Io => refused("Failed", "writing exported file stream failed"),
        other => lookup::content(other),
    })?;
    Ok(len)
}

struct PieceSink<'a, 'b> {
    rel: &'a [u8],
    offset: u64,
    part: &'a mut Vec<u8>,
    on_part: &'a mut (dyn FnMut(&[u8]) -> Result<()> + 'b),
    error: Option<SdkRefusal>,
}

impl Write for PieceSink<'_, '_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if let Err(err) = write_pieces(self.rel, self.offset, buf, self.part, self.on_part) {
            self.error = Some(err);
            return Err(std::io::Error::other("export sink failed"));
        }
        self.offset += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn write_pieces(
    rel: &[u8],
    mut offset: u64,
    mut bytes: &[u8],
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let fixed = header(rel) + 8 + 4;
    while !bytes.is_empty() {
        let mut room = PART_LIMIT.saturating_sub(part.len() + fixed);
        if room < MIN_ROOM.min(bytes.len()) {
            flush_part(part, on_part)?;
            room = PART_LIMIT.saturating_sub(fixed);
        }
        let take = bytes.len().min(PIECE_LIMIT).min(room.max(1));
        let (piece, rest) = bytes.split_at(take);
        push_record(
            Record::Write {
                path: rel,
                offset,
                bytes: piece,
            },
            part,
            on_part,
        )?;
        offset += take as u64;
        bytes = rest;
    }
    Ok(())
}

fn push_record(
    record: Record<'_>,
    part: &mut Vec<u8>,
    on_part: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    if !part.is_empty() && part.len() + record.size() > PART_LIMIT {
        flush_part(part, on_part)?;
    }
    record.encode(part);
    if part.len() >= PART_LIMIT {
        flush_part(part, on_part)?;
    }
    Ok(())
}

fn flush_part(part: &mut Vec<u8>, on_part: &mut dyn FnMut(&[u8]) -> Result<()>) -> Result<()> {
    if !part.is_empty() {
        on_part(part)?;
        part.clear();
    }
    Ok(())
}

fn build_result(
    project: ProjectRecord,
    record: &StoreRecord,
    branch: &str,
    destination: &str,
    summary: SnapshotSummary,
) -> SdkValue {
    let mut out = project.to_sdk(View::Full, record);
    for (k, v) in [
        ("branch", SdkValue::text(branch)),
        ("snapshot", SdkValue::Text(summary.snapshot)),
        ("node", SdkValue::Text(summary.node)),
        ("root", SdkValue::Text(summary.root)),
        ("destination", SdkValue::text(destination)),
        ("files", SdkValue::Int(summary.counts.files as i64)),
        (
            "directories",
            SdkValue::Int(summary.counts.directories as i64),
        ),
        ("symlinks", SdkValue::Int(summary.counts.symlinks as i64)),
        ("bytes", SdkValue::Int(summary.counts.bytes as i64)),
    ] {
        out = layerfs_home::home::with_field(out, k, v);
    }
    out
}
