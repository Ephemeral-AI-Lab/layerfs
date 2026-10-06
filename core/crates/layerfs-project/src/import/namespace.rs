//! Production namespace initialization from backed acquisition order.
//!
//! This module builds a real namespace through public C1 constructors and saves
//! it through public C2 operations. Scan attributes and symlink targets are
//! published prerequisites. Directory roots, regular-file objects and the
//! inode table enter one assembly Save in dependency order; fresh sorted
//! constructors do not reread those unpublished roots. A canonical regular
//! inode pulls its completion from bounded worker state, and no completed-file
//! root is written to acquisition backing. Earlier published reference-closed
//! batches survive later failure under the ordinary Save custody contract.
//!
//! The backing supplies every directory's bindings contiguously in name order
//! and every inode in serial order, so each directory and the inode table are
//! streamed straight into their sorted constructors through one read window.
//! Each final count is the retained bindings of that serial: one per entry plus
//! one per native alias. No whole-namespace update, count array or content map
//! is resident.
//!
//! Every serial is assigned by Init from one reservation the C5 catalog
//! consumed before this function was called. No caller supplies a serial, and no
//! scope is imported: the scope arrives as a checked C1 value.

use crate::backing::Backing;
use crate::error::ProjectError as Failure;
use crate::error::{content, malformed, storage};
use crate::files::{self, FileCompletions};
use crate::scan::Scanned;
use layerfs_content::filesystem::directory::update::{build_directory, empty_directory};
use layerfs_content::filesystem::inode::update::build_table;
use layerfs_content::filesystem::root::{profile_id, FilesystemRoot};
use layerfs_content::filesystem::{FilesystemObjects, InodeScope, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::port::acquisition::WRITE_WINDOW_ROWS;
use layerfs_storage::{Save, Storage};
use std::time::Instant;

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

/// Builds one namespace through the caller's assembly Save. Its filesystem
/// root is accepted last, after every file completion and alias check, so no
/// later accept can publish that root before cleanup and Save::finish.
pub(crate) fn build_namespace(
    store: &Storage,
    save: &Save<'_>,
    provider: &dyn AuthenticatedObjects,
    allocation: (InodeScope, u64),
    scanned: &Scanned,
    backing: &Backing<'_>,
    progress: &mut ImportProgress,
) -> Result<ObjectId, Failure> {
    let (scope, root_serial) = allocation;
    root_serial
        .checked_add(scanned.entries)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(Failure::Capacity)?;
    progress.tick()?;
    let built = {
        let mut sink = save.sink();
        let mut objects = FilesystemObjects::new(provider, &mut sink);
        directories(&mut objects, root_serial, backing, progress).and_then(|()| {
            let table = files::with_completions(
                backing,
                scanned.aliases,
                store,
                save,
                progress,
                |files, progress| {
                    table(&mut objects, root_serial, scanned, backing, files, progress)
                },
            )?;
            filesystem_root(&mut objects, scope, root_serial, table)
        })
    };
    let sink_failure = save.take_failure().map(storage);
    let root = match (built, sink_failure) {
        // A direct stream accept retains its original storage error outside
        // SaveSink. A later sink refusal must not turn uncertainty into a
        // definite Aborted result that would initiate acquisition cleanup.
        (Err(error), _) if error.unknown_outcome() => Err(error),
        (_, Some(error)) => Err(error),
        (result, None) => result,
    }?;
    Ok(root)
}

/// Streams every directory's bindings into its constructor and records its root.
fn directories(
    objects: &mut FilesystemObjects<'_>,
    root_serial: u64,
    backing: &Backing<'_>,
    progress: &mut ImportProgress,
) -> Result<(), Failure> {
    let mut entries = backing.bindings();
    let mut roots: Vec<(u64, ObjectId)> = Vec::new();
    // Entry zero is the root; every later entry is one binding of its parent.
    let root = entries.next()?.ok_or_else(malformed)?;
    if root.key.parent.is_some() || root.position != 0 {
        return Err(malformed());
    }
    while let Some(parent) = entries.peek()?.map(|entry| entry.key.parent) {
        let parent = parent.ok_or_else(malformed)?;
        progress.tick()?;
        let mut failure = None;
        let mut bound = 0usize;
        let bindings = std::iter::from_fn(|| {
            if failure.is_some() {
                return None;
            }
            let mut next = || -> Result<Option<(Vec<u8>, u64)>, Failure> {
                let same = |held: Option<u64>| held == Some(parent);
                if entries.peek()?.is_none_or(|entry| !same(entry.key.parent)) {
                    return Ok(None);
                }
                let Some(entry) = entries.next()? else {
                    return Ok(None);
                };
                // A later native path binds the serial of its identity's first.
                Ok(Some((
                    entry.key.name,
                    entry.canonical.unwrap_or(entry.position),
                )))
            };
            match next() {
                Ok(binding) => {
                    let (name, id) = binding?;
                    bound += 1;
                    Some(PathName::from_bytes(&name).map(|name| (name, root_serial + id)))
                }
                Err(error) => {
                    failure = Some(error);
                    // End construction at this exact row. Returning None
                    // would finalize the successful prefix after a refusal.
                    Some(Err(layerfs_content::ContentError::OutputRejected))
                }
            }
        });
        let built = build_directory(objects, bindings);
        if let Some(error) = failure {
            return Err(error);
        }
        roots.push((parent, built.map_err(content)?.0 .0));
        progress.work.directory_bindings += bound;
        if roots.len() == WRITE_WINDOW_ROWS {
            backing.set_directory_roots(&mut roots)?;
        }
    }
    backing.set_directory_roots(&mut roots)
}

/// Streams every inode into its sorted table. A canonical regular entry waits
/// only for its own admitted completion; later paths share its retained count.
fn table(
    objects: &mut FilesystemObjects<'_>,
    root_serial: u64,
    scanned: &Scanned,
    backing: &Backing<'_>,
    files: &mut FileCompletions<'_, '_, '_>,
    progress: &mut ImportProgress,
) -> Result<ObjectId, Failure> {
    // A directory that bound nothing has the one canonical empty page.
    let empty = if scanned.childless {
        Some(empty_directory(objects).map_err(content)?.0)
    } else {
        None
    };
    progress.tick()?;
    let mut entries = backing.entries();
    let mut failure = None;
    // Entries stream by key; acquisition order makes that position order too.
    let mut expected = 0u64;
    let deadline = progress.deadline();
    let rows = std::iter::from_fn(|| {
        if failure.is_some() {
            return None;
        }
        let mut next = || -> Result<Option<(u64, InodeValue)>, Failure> {
            loop {
                let Some(entry) = entries.next()? else {
                    return Ok(None);
                };
                if entry.position != expected {
                    return Err(malformed());
                }
                expected += 1;
                if Instant::now() >= deadline {
                    return Err(Failure::Deadline);
                }
                let id = entry.position;
                let mut count = u64::from(id != 0);
                let content_root = match entry.kind {
                    InodeKind::Directory => entry.content_root.or(empty),
                    InodeKind::Symlink => entry.content_root,
                    InodeKind::RegularFile => {
                        if entry.canonical.ok_or_else(malformed)? != id {
                            continue;
                        }
                        let (root, aliases) = files.root(id)?;
                        count = count.checked_add(aliases).ok_or(Failure::Capacity)?;
                        Some(root)
                    }
                };
                return Ok(Some((
                    root_serial + id,
                    InodeValue {
                        kind: entry.kind,
                        namespace_ref_count: count,
                        content_root: content_root.ok_or_else(malformed)?,
                        metadata_root: entry.metadata_root,
                    },
                )));
            }
        };
        match next() {
            Ok(row) => row.map(Ok),
            Err(error) => {
                failure = Some(error);
                Some(Err(layerfs_content::ContentError::OutputRejected))
            }
        }
    });
    let built = build_table(objects, rows);
    if let Some(error) = failure {
        return Err(error);
    }
    let (table, _) = built.map_err(content)?;
    if expected != scanned.entries {
        return Err(malformed());
    }
    Ok(table)
}

/// The last accepted object of the assembly Save. Worker draining and source
/// alias validation have completed before this final reference is constructed.
fn filesystem_root(
    objects: &mut FilesystemObjects<'_>,
    scope: InodeScope,
    root_serial: u64,
    table: ObjectId,
) -> Result<ObjectId, Failure> {
    let root = FilesystemRoot::new(profile_id(), scope, root_serial, table).map_err(content)?;
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().map_err(content)?)
        .map_err(content)?
        .with_references(vec![table]);
    objects.emit(object).map_err(content)
}
