//! Production namespace initialization and bounded inode role validation.
//!
//! The fixture under `examples/` is not an implementation: this module builds a
//! real bounded namespace through public C1 constructors and saves it through
//! public C2 operations, with one prerequisite save for attributes and symlink
//! targets and one save for the filesystem tree.
//!
//! The split is deliberate. The filesystem build consumes roots that are already
//! published, so nothing here assumes a combined unpublished reader/sink that
//! could read an object this same operation is still producing. If the tree save
//! fails, the prerequisite objects may become unreferenced; that is the same
//! bounded ownership the ordinary save path has, and no cleanup is guessed.
//!
//! Every serial is assigned by Init from one reservation the C5 catalog
//! consumed before this function was called. No caller supplies a serial, and no
//! scope is imported: the scope arrives as a checked C1 value.

use crate::error::ProjectError as Failure;
use crate::error::{content, storage};
use crate::metadata::build_metadata;
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::references::FileBacking;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::{
    check_input, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources,
    InodeScope, InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, FileView, ObjectId};
use layerfs_history::RecordKind;
use layerfs_storage::Storage;
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::{
    fs,
    os::unix::fs::DirBuilderExt,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_ORDERING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// One bounded progress byte on a long native import, separate from result data.
pub(crate) struct ImportProgress {
    deadline: Instant,
    pub(crate) work: crate::NamespaceWork,
}
impl ImportProgress {
    pub fn new(deadline: Instant) -> Self {
        Self {
            deadline,
            work: crate::NamespaceWork::default(),
        }
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn tick(&mut self) -> Result<(), Failure> {
        if Instant::now() >= self.deadline {
            return Err(Failure::Deadline);
        }
        Ok(())
    }
}

/// Internal import row. Its parent index is not restricted by the old wire manifest.
pub(crate) struct PreparedEntry {
    /// Canonical first regular-file entry for a native hard-link alias.
    pub alias: Option<usize>,
    pub parent: usize,
    pub name: Vec<u8>,
    pub kind: RecordKind,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub content: Option<ObjectId>,
    pub target: Option<Vec<u8>>,
}

/// Builds and saves one bounded logical namespace, returning its published root.
#[expect(clippy::too_many_arguments, reason = "import provenance is explicit")]
pub(crate) fn build_namespace(
    scratch_parent: &Path,
    store: &Storage,
    provider: &dyn AuthenticatedObjects,
    scope: InodeScope,
    root_serial: u64,
    entries: &[PreparedEntry],
    files_just_imported: bool,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> Result<ObjectId, Failure> {
    let count = u64::try_from(entries.len()).map_err(|_| Failure::Capacity)?;
    root_serial
        .checked_add(count)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(Failure::Capacity)?;
    progress.tick()?;
    let serials: Vec<u64> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.alias.is_none())
        .map(|(index, _)| root_serial + index as u64)
        .collect();
    let inodes = prerequisites(
        store,
        provider,
        entries,
        root_serial,
        files_just_imported,
        progress,
        timer,
    )?;
    let directories = directory_updates(entries, root_serial)?;
    progress.work.serial_capacity_bytes = serials.capacity() * std::mem::size_of::<u64>();
    progress.work.inode_capacity_bytes = inodes.capacity() * std::mem::size_of::<InodeUpdate>();
    progress.work.directory_bindings = directories.iter().map(|d| d.changes.len()).sum();
    progress.work.directory_capacity_bytes =
        directories.capacity() * std::mem::size_of::<DirectoryUpdate>();
    progress.work.change_capacity_bytes = directories
        .iter()
        .map(|d| d.changes.capacity() * std::mem::size_of::<(PathName, Option<u64>)>())
        .sum();
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &serials,
        resources: FilesystemResources::default(),
    };
    check_input(&input).map_err(content)?;
    progress.tick()?;
    let scratch = scratch_parent.join(format!(
        "ordering-{}-{}",
        std::process::id(),
        NEXT_ORDERING_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    let save = store.begin_save().map_err(storage)?;
    fs::DirBuilder::new().mode(0o700).create(&scratch)?;
    let mut sink = save.sink();
    let mut backing = FileBacking::new(&scratch);
    let built = {
        let mut objects = FilesystemObjects::new(provider, &mut sink);
        layerfs_content::build_filesystem(&mut objects, &input, Some(&mut backing)).map_err(content)
    };
    let built = match save.take_failure() {
        Some(error) => Err(storage(error)),
        None => built,
    };
    let cleanup = fs::remove_dir(&scratch);
    let built = match (built, cleanup) {
        (Err(error), Ok(())) => Err(error),
        (result, Err(cleanup)) => Err(Failure::Cleanup {
            cause: result.err().map(Box::new),
            error: cleanup,
        }),
        (Ok(result), Ok(())) => Ok(result),
    };
    let built = built.and_then(|value| {
        progress.tick()?;
        Ok(value)
    });
    let result = built?;
    timer
        .child("history.finish_tree_save")
        .run(|_| save.finish().map_err(storage))?;
    Ok(result.root.0)
}

/// Builds and saves the attribute trees and symlink targets the tree refers to.
///
/// Constructs each typed inode directly as its prerequisite roots are emitted.
fn prerequisites(
    store: &Storage,
    provider: &dyn AuthenticatedObjects,
    entries: &[PreparedEntry],
    root_serial: u64,
    files_just_imported: bool,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> Result<Vec<InodeUpdate>, Failure> {
    let save = store.begin_save().map_err(storage)?;
    let built = {
        let mut handoff = save.sink();
        let result = timer.child("history.prerequisites").run(|_| {
            let mut objects = FilesystemObjects::new(provider, &mut handoff);
            let mut inodes = Vec::with_capacity(entries.len());
            // Adjacent equal portable fields have the same canonical metadata root.
            // One slot avoids an entry-count-sized cache on heterogeneous imports.
            let mut previous_metadata = None;
            for (index, entry) in entries.iter().enumerate() {
                progress.tick()?;
                if entry.alias.is_some() {
                    continue;
                }
                let kind = kind_of(entry.kind);
                let value = PortableMetadata {
                    mode: entry.mode,
                    mtime_seconds: entry.mtime_seconds,
                    mtime_nanoseconds: entry.mtime_nanoseconds,
                };
                let key = (
                    kind.code(),
                    entry.mode,
                    entry.mtime_seconds,
                    entry.mtime_nanoseconds,
                );
                let metadata_root = match previous_metadata {
                    Some((previous, root)) if previous == key => root,
                    _ => {
                        let root = build_metadata(&mut objects, kind, value)?;
                        previous_metadata = Some((key, root));
                        root
                    }
                };
                let content_root = match entry.kind {
                    RecordKind::Symlink => emit_symlink(
                        &mut objects,
                        SymlinkTarget::new(entry.target.clone().ok_or(Failure::InvalidInput)?)
                            .map_err(content)?,
                    )
                    .map_err(content)?,
                    RecordKind::RegularFile => {
                        let root = entry.content.ok_or(Failure::InvalidInput)?;
                        if !files_just_imported {
                            Timing::disabled("history.role", |scope| {
                                FileView::open(provider, root, scope.child("file"))
                            })
                            .0
                            .map_err(content)?;
                        }
                        root
                    }
                    RecordKind::Directory => metadata_root,
                };
                inodes.push(InodeUpdate {
                    serial: root_serial + index as u64,
                    value: InodeValue {
                        kind,
                        namespace_ref_count: 0,
                        content_root,
                        metadata_root,
                    },
                });
            }
            Ok(inodes)
        });
        let retained = save.take_failure();
        match retained {
            Some(error) => Err(storage(error)),
            None => result,
        }
    };
    let built = built.and_then(|value| {
        progress.tick()?;
        Ok(value)
    });
    let values = built?;
    timer
        .child("history.finish_prerequisite_save")
        .run(|_| save.finish().map_err(storage))?;
    Ok(values)
}

/// Groups the manifest's entries into sorted final directory bindings.
///
/// The root directory is always stated, even when it has no children. A build
/// retains a directory's content root only for the parents the operation
/// states, so an unstated root would be dropped from the inode table; the empty
/// statement is also what gives the one-directory namespace its real empty page.
fn directory_updates(
    entries: &[PreparedEntry],
    root_serial: u64,
) -> Result<Vec<DirectoryUpdate>, Failure> {
    let mut bindings = std::collections::BTreeMap::new();
    for (index, entry) in entries.iter().enumerate() {
        if entry.kind == RecordKind::Directory {
            bindings.insert(root_serial + index as u64, Vec::new());
        }
    }
    for (index, entry) in entries.iter().enumerate().skip(1) {
        let name = PathName::from_bytes(&entry.name).map_err(content)?;
        let parent = root_serial + entry.parent as u64;
        bindings
            .get_mut(&parent)
            .ok_or(Failure::InvalidInput)?
            .push((
                name,
                Some(root_serial + entry.alias.unwrap_or(index) as u64),
            ));
    }
    let mut directories: Vec<DirectoryUpdate> = bindings
        .into_iter()
        .map(|(parent, changes)| DirectoryUpdate { parent, changes })
        .collect();
    for directory in &mut directories {
        directory.changes.sort_by(|a, b| a.0.cmp(&b.0));
        directory.check().map_err(content)?;
    }
    Ok(directories)
}

fn kind_of(kind: RecordKind) -> InodeKind {
    match kind {
        RecordKind::Directory => InodeKind::Directory,
        RecordKind::RegularFile => InodeKind::RegularFile,
        RecordKind::Symlink => InodeKind::Symlink,
    }
}
