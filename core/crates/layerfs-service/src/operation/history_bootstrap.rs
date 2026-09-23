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
//! Every serial is assigned by the service from one reservation the C5 catalog
//! consumed before this function was called. No caller supplies a serial, and no
//! scope is imported: the scope arrives as a checked C1 value.

use crate::operation::failure::{content, storage};
use layerfs_bridge::contract::{Code, Failure, MAX_FILE};
use layerfs_content::filesystem::attributes::read::{read_portable, AttributeReadWork};
use layerfs_content::filesystem::attributes::{
    build::build_attribute_tree, codec::AttributeEntry, keys::AttributeKey,
    portable::PortableMetadata, value::emit_value,
};
use layerfs_content::filesystem::references::FileBacking;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::{
    DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemResources, InodeScope,
    InodeUpdate, PathName,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, FileView, ObjectId};
use layerfs_history::{ManifestEntry, RecordKind};
use layerfs_storage::{SaveHandoff, Store};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::{
    fs,
    io::Write,
    os::unix::fs::DirBuilderExt,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

static NEXT_ORDERING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// One bounded progress byte on a long native import, separate from result data.
pub(crate) struct ImportProgress<'a> {
    deadline: Instant,
    last: Instant,
    output: Option<&'a mut dyn Write>,
}
impl<'a> ImportProgress<'a> {
    pub fn disabled(deadline: Instant) -> Self {
        Self {
            deadline,
            last: Instant::now(),
            output: None,
        }
    }
    pub fn with_output(deadline: Instant, output: &'a mut dyn Write) -> Self {
        Self {
            deadline,
            last: Instant::now(),
            output: Some(output),
        }
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn tick(&mut self) -> Result<(), Failure> {
        let now = Instant::now();
        if now >= self.deadline {
            return Err(Code::Deadline.into());
        }
        if now.duration_since(self.last) >= Duration::from_secs(1) {
            if let Some(output) = self.output.as_deref_mut() {
                output.write_all(&[0])?;
                output.flush()?;
            }
            self.last = now;
        }
        Ok(())
    }
}

/// Internal import row. Its parent index is not restricted by the old wire manifest.
pub(crate) struct PreparedEntry {
    pub parent: usize,
    pub name: Vec<u8>,
    pub kind: RecordKind,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub content: Option<ObjectId>,
    pub target: Option<Vec<u8>>,
}

impl From<&ManifestEntry> for PreparedEntry {
    fn from(entry: &ManifestEntry) -> Self {
        Self {
            parent: usize::from(entry.parent),
            name: entry.name.clone(),
            kind: entry.kind,
            mode: entry.mode,
            mtime_seconds: entry.mtime_seconds,
            mtime_nanoseconds: entry.mtime_nanoseconds,
            content: entry.content,
            target: entry.target.clone(),
        }
    }
}

/// Builds and saves one bounded logical namespace, returning its published root.
#[expect(clippy::too_many_arguments, reason = "import provenance is explicit")]
pub(crate) fn build_namespace(
    store: &Store,
    provider: &dyn AuthenticatedObjects,
    scope: InodeScope,
    root_serial: u64,
    entries: &[PreparedEntry],
    files_just_imported: bool,
    progress: &mut ImportProgress<'_>,
    timer: &TimingScope<'_, Active>,
) -> Result<ObjectId, Failure> {
    let count = u64::try_from(entries.len()).map_err(|_| Code::Capacity)?;
    let last = root_serial
        .checked_add(count)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(Code::Capacity)?;
    progress.tick()?;
    let serials: Vec<u64> = (root_serial..last).collect();
    let (metadata, content_roots) = prerequisites(
        store,
        provider,
        entries,
        files_just_imported,
        progress,
        timer,
    )?;
    let inodes: Vec<InodeUpdate> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            Ok(InodeUpdate {
                serial: serials[index],
                value: InodeValue {
                    kind: kind_of(entry.kind),
                    namespace_ref_count: 0,
                    content_root: match entry.kind {
                        RecordKind::RegularFile => entry.content.ok_or(Code::InvalidInput)?,
                        RecordKind::Symlink => content_roots[index],
                        RecordKind::Directory => content_roots[index],
                    },
                    metadata_root: metadata[index],
                },
            })
        })
        .collect::<Result<_, Failure>>()?;
    let directories = directory_updates(entries, &serials)?;
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &serials,
        resources: FilesystemResources::default(),
    };
    input.check().map_err(content)?;
    progress.tick()?;
    let scratch = loop {
        let path = store.path().with_extension(format!(
            "ordering-{}-{}",
            std::process::id(),
            NEXT_ORDERING_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => break path,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let mut save = match store.begin_save(timer.child("history.begin_tree_save")) {
        Ok(save) => save,
        Err(error) => {
            let mut failure = storage(error);
            if fs::remove_dir(&scratch).is_err() {
                failure.cleanup = Some(Code::Io);
            }
            return Err(failure);
        }
    };
    let mut handoff = SaveHandoff::new(&mut save);
    let mut backing = FileBacking::new(&scratch);
    let built = {
        let mut objects = FilesystemObjects::new(provider, &mut handoff);
        layerfs_content::build_filesystem(&mut objects, &input, Some(&mut backing)).map_err(content)
    };
    let scratch_cleanup = fs::remove_dir(&scratch);
    let retained = handoff.take_failure();
    drop(handoff);
    let built = match retained {
        Some(error) => Err(storage(error)),
        None => built,
    };
    let built = match (built, scratch_cleanup) {
        (Err(mut failure), Err(_)) => {
            failure.cleanup = Some(Code::Io);
            Err(failure)
        }
        (Ok(_), Err(error)) => Err(error.into()),
        (result, Ok(())) => result,
    };
    let built = built.and_then(|value| {
        progress.tick()?;
        Ok(value)
    });
    match built {
        Ok(result) => {
            save.finish(timer.child("history.finish_tree_save"))
                .map_err(storage)?;
            Ok(result.root.0)
        }
        Err(mut error) => {
            if !error.unknown {
                if let Err(cleanup) = save.abort(timer.child("history.abort_tree_save")) {
                    let cleanup = storage(cleanup);
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
            }
            Err(error)
        }
    }
}

/// Builds and saves the attribute trees and symlink targets the tree refers to.
///
/// Returns one metadata root per entry and one content root per entry; a
/// directory's and a regular file's content root come from elsewhere, so their
/// slots carry the metadata root as a harmless placeholder that the caller
/// overwrites.
fn prerequisites(
    store: &Store,
    provider: &dyn AuthenticatedObjects,
    entries: &[PreparedEntry],
    files_just_imported: bool,
    progress: &mut ImportProgress<'_>,
    timer: &TimingScope<'_, Active>,
) -> Result<(Vec<ObjectId>, Vec<ObjectId>), Failure> {
    let mut save = store
        .begin_save(timer.child("history.begin_prerequisite_save"))
        .map_err(storage)?;
    let built = {
        let mut handoff = SaveHandoff::new(&mut save);
        let result = timer.child("history.prerequisites").run(|_| {
            let mut objects = FilesystemObjects::new(provider, &mut handoff);
            let mut metadata = Vec::with_capacity(entries.len());
            let mut content_roots = Vec::with_capacity(entries.len());
            // Adjacent equal portable fields have the same canonical metadata root.
            // One slot avoids an entry-count-sized cache on heterogeneous imports.
            let mut previous_metadata = None;
            for entry in entries {
                progress.tick()?;
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
                        SymlinkTarget::new(entry.target.clone().ok_or(Code::InvalidInput)?)
                            .map_err(content)?,
                    )
                    .map_err(content)?,
                    RecordKind::RegularFile => {
                        let root = entry.content.ok_or(Code::InvalidInput)?;
                        if !files_just_imported {
                            let view = Timing::disabled("history.role", |scope| {
                                FileView::open(provider, root, scope.child("file"))
                            })
                            .0
                            .map_err(content)?;
                            if view.logical_len() > MAX_FILE {
                                return Err(Code::Capacity.into());
                            }
                        }
                        root
                    }
                    RecordKind::Directory => metadata_root,
                };
                metadata.push(metadata_root);
                content_roots.push(content_root);
            }
            Ok((metadata, content_roots))
        });
        let retained = handoff.take_failure();
        drop(handoff);
        match retained {
            Some(error) => Err(storage(error)),
            None => result,
        }
    };
    let built = built.and_then(|value| {
        progress.tick()?;
        Ok(value)
    });
    match built {
        Ok(values) => {
            save.finish(timer.child("history.finish_prerequisite_save"))
                .map_err(storage)?;
            Ok(values)
        }
        Err(mut error) => {
            if !error.unknown {
                if let Err(cleanup) = save.abort(timer.child("history.abort_prerequisite_save")) {
                    let cleanup = storage(cleanup);
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
            }
            Err(error)
        }
    }
}

/// Builds the typed portable fields through the caller's existing save owner.
/// The constructors only emit; they never read these unpublished new objects.
pub(crate) fn build_metadata(
    objects: &mut FilesystemObjects<'_>,
    kind: InodeKind,
    value: PortableMetadata,
) -> Result<ObjectId, Failure> {
    value.validate(kind).map_err(content)?;
    let mode = emit_value(objects, &value.mode_bytes(kind).map_err(content)?).map_err(content)?;
    let mtime = emit_value(objects, &value.mtime_bytes().map_err(content)?).map_err(content)?;
    build_attribute_tree(
        objects,
        vec![
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).map_err(content)?,
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).map_err(content)?,
                value_root: mtime,
            }),
        ]
        .into_iter(),
    )
    .map(|(root, _)| root)
    .map_err(content)
}

/// Groups the manifest's entries into sorted final directory bindings.
///
/// The root directory is always stated, even when it has no children. A build
/// retains a directory's content root only for the parents the operation
/// states, so an unstated root would be dropped from the inode table; the empty
/// statement is also what gives the one-directory namespace its real empty page.
fn directory_updates(
    entries: &[PreparedEntry],
    serials: &[u64],
) -> Result<Vec<DirectoryUpdate>, Failure> {
    let mut bindings = std::collections::BTreeMap::new();
    for (entry, serial) in entries.iter().zip(serials) {
        if entry.kind == RecordKind::Directory {
            bindings.insert(*serial, Vec::new());
        }
    }
    for (index, entry) in entries.iter().enumerate().skip(1) {
        let name = PathName::from_bytes(&entry.name).map_err(content)?;
        let parent = serials[entry.parent];
        bindings
            .get_mut(&parent)
            .ok_or(Code::InvalidInput)?
            .push((name, Some(serials[index])));
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

/// Bounded semantic-role validation of one already published inode value.
///
/// A stored root's presence is not provenance. A regular file's content root
/// must open as a file representation, a symlink's must decode as a stored
/// target, and every kind's metadata root must decode as an attribute tree whose
/// typed portable fields are valid for that kind. Nothing here walks a tree.
pub(crate) fn validate_inode_role(
    provider: &dyn AuthenticatedObjects,
    kind: InodeKind,
    content_root: ObjectId,
    metadata_root: ObjectId,
    timer: &TimingScope<'_, Active>,
) -> Result<(), Failure> {
    match kind {
        InodeKind::RegularFile => {
            let view = FileView::open(provider, content_root, timer.child("history.role_file"))
                .map_err(content)?;
            if view.logical_len() > MAX_FILE {
                return Err(Code::Capacity.into());
            }
        }
        InodeKind::Symlink => {
            let canonical = provider.read_canonical(content_root).map_err(content)?;
            SymlinkTarget::decode(&canonical).map_err(content)?;
        }
        InodeKind::Directory => {
            provider.read_canonical(content_root).map_err(content)?;
        }
    }
    let mut work = AttributeReadWork::default();
    read_portable(provider, metadata_root, kind, &mut work).map_err(content)?;
    Ok(())
}
