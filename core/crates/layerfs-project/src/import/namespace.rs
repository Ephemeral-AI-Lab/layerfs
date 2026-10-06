//! Production namespace initialization from backed acquisition order.
//!
//! This module builds a real namespace through public C1 constructors and saves
//! it through public C2 operations, with one prerequisite save for attributes
//! and symlink targets and one save for the filesystem tree.
//!
//! The split is deliberate. The tree consumes roots that are already published,
//! so nothing here assumes a combined unpublished reader/sink that could read
//! an object this same operation is still producing. If the tree save fails,
//! the prerequisite objects may become unreferenced; that is the same bounded
//! ownership the ordinary save path has, and no cleanup is guessed.
//!
//! The scan already supplies every directory's bindings contiguously in name
//! order and every inode in serial order, so each directory and the inode table
//! are streamed straight into their sorted constructors. Every root an entry
//! owns is written to a scratch run in position order and merged back by
//! position. Each final count is the retained bindings of that serial: one per
//! entry plus one per native alias. No whole-namespace update, count array or
//! content map is resident.
//!
//! Every serial is assigned by Init from one reservation the C5 catalog
//! consumed before this function was called. No caller supplies a serial, and no
//! scope is imported: the scope arrives as a checked C1 value.

use crate::error::ProjectError as Failure;
use crate::error::{content, storage};
use crate::metadata::build_metadata;
use crate::runs::{Cursor, Run, Writer};
use crate::scan::{Identities, Scanned};
use crate::scratch::{decode_pair, Entry, Job, Rooted, Scratch};
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::filesystem::directory::update::{build_directory, empty_directory};
use layerfs_content::filesystem::inode::update::build_table;
use layerfs_content::filesystem::root::{profile_id, FilesystemRoot};
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::{FilesystemObjects, InodeScope, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{Save, Storage};
use layerfs_telemetry::timer::{Active, TimingScope};
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

/// Builds one logical namespace and returns its root with the tree save still
/// open, so the caller releases operation scratch before anything is published.
#[expect(clippy::too_many_arguments, reason = "import provenance is explicit")]
pub(crate) fn build_namespace<'a>(
    store: &'a Storage,
    provider: &dyn AuthenticatedObjects,
    scope: InodeScope,
    root_serial: u64,
    acquired: &Acquired<'_>,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> Result<(ObjectId, Save<'a>), Failure> {
    root_serial
        .checked_add(acquired.scanned.entries)
        .filter(|end| *end <= i64::MAX as u64)
        .ok_or(Failure::Capacity)?;
    progress.tick()?;
    let owned = prerequisites(store, provider, acquired, scratch, progress, timer)?;
    progress.tick()?;
    let save = store.begin_save().map_err(storage)?;
    let built = {
        let mut sink = save.sink();
        let mut objects = FilesystemObjects::new(provider, &mut sink);
        tree(
            &mut objects,
            scope,
            root_serial,
            acquired,
            &owned,
            scratch,
            progress,
        )
    };
    let root = match save.take_failure() {
        Some(error) => Err(storage(error)),
        None => built,
    }?;
    Ok((root, save))
}

/// Every backed stream the namespace is assembled from.
pub(crate) struct Acquired<'r> {
    pub scanned: &'r Scanned,
    pub identities: &'r Identities,
    /// Constructed regular-file roots in position order.
    pub contents: &'r Run,
}

/// Roots the prerequisite save published, each in position order.
struct Owned {
    /// Metadata root of every entry that is not a later native path.
    metadata: Run,
    /// Target root of every symlink.
    targets: Run,
}

/// Position of the next later native path, consumed when it is `id`.
fn alias_of(aliases: &mut Cursor<'_, Job>, id: u64) -> Result<Option<u64>, Failure> {
    let canonical = aliases.peek()?.filter(|alias| alias.id == id);
    let canonical = canonical.map(|alias| alias.canonical);
    if canonical.is_some() {
        aliases.take();
    }
    Ok(canonical)
}

/// The root `id` owns in one position-ordered stream, when it owns one.
fn root_of(roots: &mut Cursor<'_, Rooted>, id: u64) -> Result<Option<ObjectId>, Failure> {
    let root = roots.peek()?.filter(|rooted| rooted.id == id);
    let root = root.map(|rooted| rooted.root);
    if root.is_some() {
        roots.take();
    }
    Ok(root)
}

/// Builds and saves the attribute trees and symlink targets the tree refers to.
fn prerequisites(
    store: &Storage,
    provider: &dyn AuthenticatedObjects,
    acquired: &Acquired<'_>,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
    timer: &TimingScope<'_, Active>,
) -> Result<Owned, Failure> {
    let save = store.begin_save().map_err(storage)?;
    let built = {
        let mut handoff = save.sink();
        let result = timer.child("history.prerequisites").run(|_| {
            let mut objects = FilesystemObjects::new(provider, &mut handoff);
            let mut entries = Cursor::new(&acquired.scanned.entry_run, Entry::decode);
            let mut aliases = Cursor::new(&acquired.identities.aliases, Job::alias);
            let mut metadata = Writer::new(scratch.backing())?;
            let mut targets = Writer::new(scratch.backing())?;
            // Adjacent equal portable fields have the same canonical metadata root.
            // One slot avoids an entry-count-sized cache on heterogeneous imports.
            let mut previous_metadata = None;
            while let Some(entry) = entries.next()? {
                progress.tick()?;
                if alias_of(&mut aliases, entry.id)?.is_some() {
                    continue;
                }
                let key = (entry.kind.code(), entry.mode, entry.mtime, entry.nanos);
                let metadata_root = match previous_metadata {
                    Some((previous, root)) if previous == key => root,
                    _ => {
                        let value = PortableMetadata {
                            mode: entry.mode,
                            mtime_seconds: entry.mtime,
                            mtime_nanoseconds: entry.nanos,
                        };
                        let root = build_metadata(&mut objects, entry.kind, value)?;
                        previous_metadata = Some((key, root));
                        root
                    }
                };
                metadata.push(&Rooted::record(entry.id, metadata_root))?;
                if entry.kind == InodeKind::Symlink {
                    let target = SymlinkTarget::new(entry.target.ok_or(Failure::InvalidInput)?)
                        .map_err(content)?;
                    let root = emit_symlink(&mut objects, target).map_err(content)?;
                    targets.push(&Rooted::record(entry.id, root))?;
                }
            }
            progress.work.entry_capacity_bytes = entries.resident_bytes();
            Ok(Owned {
                metadata: metadata.finish()?,
                targets: targets.finish()?,
            })
        });
        match save.take_failure() {
            Some(error) => Err(storage(error)),
            None => result,
        }
    };
    let owned = built.and_then(|owned| progress.tick().map(|()| owned))?;
    timer
        .child("history.finish_prerequisite_save")
        .run(|_| save.finish().map_err(storage))?;
    Ok(owned)
}

/// Streams every directory, then the inode table, then the filesystem root.
fn tree(
    objects: &mut FilesystemObjects<'_>,
    scope: InodeScope,
    root_serial: u64,
    acquired: &Acquired<'_>,
    owned: &Owned,
    scratch: &mut Scratch,
    progress: &mut ImportProgress,
) -> Result<ObjectId, Failure> {
    let (scanned, identities) = (acquired.scanned, acquired.identities);
    let mut entries = Cursor::new(&scanned.entry_run, Entry::decode);
    let mut aliases = Cursor::new(&identities.aliases, Job::alias);
    let mut directories = Writer::new(scratch.backing())?;
    // Entry zero is the root; every later entry is one binding of its parent.
    entries.next()?.ok_or(Failure::InvalidInput)?;
    while let Some(parent) = entries.peek()?.map(|entry| entry.parent) {
        progress.tick()?;
        let mut failure = None;
        let mut bound = 0usize;
        let bindings = std::iter::from_fn(|| {
            let mut next = || -> Result<Option<(Vec<u8>, u64)>, Failure> {
                if entries.peek()?.is_none_or(|entry| entry.parent != parent) {
                    return Ok(None);
                }
                let Some(entry) = entries.take() else {
                    return Ok(None);
                };
                let canonical = alias_of(&mut aliases, entry.id)?;
                Ok(Some((entry.name, canonical.unwrap_or(entry.id))))
            };
            match next() {
                Ok(binding) => {
                    let (name, id) = binding?;
                    bound += 1;
                    Some(PathName::from_bytes(&name).map(|name| (name, root_serial + id)))
                }
                Err(error) => {
                    failure = Some(error);
                    None
                }
            }
        });
        let built = build_directory(objects, bindings);
        if let Some(error) = failure {
            return Err(error);
        }
        directories.push(&Rooted::record(parent, built.map_err(content)?.0 .0))?;
        progress.work.directory_bindings += bound;
    }
    let entry_bytes = entries.resident_bytes() + aliases.resident_bytes();
    let directories = directories.finish()?;
    // A directory that bound nothing has the one canonical empty page.
    let empty = if scanned.childless {
        Some(empty_directory(objects).map_err(content)?.0)
    } else {
        None
    };
    progress.tick()?;
    let mut kinds = Cursor::new(&scanned.entry_run, Entry::kind_of);
    let mut aliases = Cursor::new(&identities.aliases, Job::alias);
    let mut counts = Cursor::new(&identities.counts, decode_pair);
    let mut metadata = Cursor::new(&owned.metadata, Rooted::decode);
    let mut targets = Cursor::new(&owned.targets, Rooted::decode);
    let mut files = Cursor::new(acquired.contents, Rooted::decode);
    let mut bound = Cursor::new(&directories, Rooted::decode);
    let mut failure = None;
    let deadline = progress.deadline();
    let rows = std::iter::from_fn(|| {
        let mut next = || -> Result<Option<(u64, InodeValue)>, Failure> {
            loop {
                let Some((id, kind)) = kinds.next()? else {
                    return Ok(None);
                };
                if Instant::now() >= deadline {
                    return Err(Failure::Deadline);
                }
                if alias_of(&mut aliases, id)?.is_some() {
                    continue;
                }
                let mut count = u64::from(id != 0);
                if let Some((_, later)) = counts.peek()?.filter(|(first, _)| *first == id) {
                    count += later;
                    counts.take();
                }
                let content_root = match kind {
                    InodeKind::Directory => root_of(&mut bound, id)?.or(empty),
                    InodeKind::RegularFile => root_of(&mut files, id)?,
                    InodeKind::Symlink => root_of(&mut targets, id)?,
                };
                return Ok(Some((
                    root_serial + id,
                    InodeValue {
                        kind,
                        namespace_ref_count: count,
                        content_root: content_root.ok_or(Failure::InvalidInput)?,
                        metadata_root: root_of(&mut metadata, id)?.ok_or(Failure::InvalidInput)?,
                    },
                )));
            }
        };
        match next() {
            Ok(row) => row.map(Ok),
            Err(error) => {
                failure = Some(error);
                None
            }
        }
    });
    let built = build_table(objects, rows);
    if let Some(error) = failure {
        return Err(error);
    }
    let (table, _) = built.map_err(content)?;
    let work = &mut progress.work;
    work.entry_capacity_bytes = work.entry_capacity_bytes.max(entry_bytes);
    work.inode_capacity_bytes = kinds.resident_bytes()
        + aliases.resident_bytes()
        + counts.resident_bytes()
        + metadata.resident_bytes()
        + targets.resident_bytes()
        + files.resident_bytes()
        + bound.resident_bytes();
    let root = FilesystemRoot::new(profile_id(), scope, root_serial, table).map_err(content)?;
    let object = FinalizedObject::new(ObjectRole::FilesystemRoot, root.encode().map_err(content)?)
        .map_err(content)?
        .with_references(vec![table]);
    objects.emit(object).map_err(content)
}
