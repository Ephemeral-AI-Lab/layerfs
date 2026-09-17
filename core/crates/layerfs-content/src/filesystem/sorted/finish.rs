//! Whole-operation finality: root construction, collapse and final emission.
//!
//! The engine emits pages child-first; the last sibling stays private until
//! nothing can join it. A locally final page is not a finalized operation: the
//! callers of this module establish retained membership and only then re-encode
//! the filesystem root. Initial construction needs no provisional empty seed -
//! an optional base root simply starts the same merge with no stored pages.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::sorted::format::{CompactDirectory, CompactInodes, Format};
use crate::filesystem::sorted::merge::Changes;
use crate::filesystem::sorted::page::{empty_page, Engine, SortedWork};
use crate::object::ObjectId;

impl<'o, 'e, F: Format> Engine<'o, 'e, F> {
    /// Merges one optional base tree with strictly sorted final changes.
    pub(crate) fn apply_root(
        objects: &'o mut FilesystemObjects<'e>,
        root: Option<ObjectId>,
        source: impl Iterator<Item = ContentResult<(F::Key, Option<F::Value>)>>,
        scratch_limit: usize,
        observe: &mut dyn FnMut(Option<F::Value>, Option<F::Value>) -> ContentResult<()>,
    ) -> ContentResult<(ObjectId, SortedWork)> {
        let mut changes = Changes::new(source)?;
        if changes.next.is_none() {
            return match root {
                Some(root) => Ok((root, SortedWork::default())),
                None => Err(ContentError::InvalidRecord("empty initial tree")),
            };
        }
        let mut engine = Engine::<F>::new(objects, observe, scratch_limit);
        let read = match root {
            Some(root) => engine.read(root, true)?,
            None => {
                let lease = engine.budget.reserve(std::mem::size_of::<
                    crate::filesystem::sorted::format::Wire<F::Key, F::Value>,
                >())?;
                crate::filesystem::sorted::page::ReadPage {
                    wire: crate::filesystem::sorted::format::Wire {
                        level: 0,
                        count: 0,
                        bytes: 0,
                        entries: Vec::new(),
                        size: crate::filesystem::sorted::format::EMPTY_PAGE_BYTES,
                    },
                    _lease: lease,
                }
            }
        };
        let level = read.wire.level;
        let mut first = None;
        let _frontier = engine.budget.reserve(
            32 * std::mem::size_of::<
                Option<Box<crate::filesystem::sorted::page::Page<F::Key, F::Value>>>,
            >(),
        )?;
        let mut levels: Vec<Option<Box<crate::filesystem::sorted::page::Page<F::Key, F::Value>>>> =
            (0..32).map(|_| None).collect();
        engine.edit(root, read, None, &mut changes, &mut |engine, node| {
            if first.is_none() && levels.iter().all(Option::is_none) {
                first = Some(node);
                return Ok(());
            }
            if let Some(prior) = first.take() {
                append_root(engine, &mut levels, prior)?;
            }
            append_root(engine, &mut levels, node)
        })?;
        let mut root_node = if let Some(first) = first {
            first
        } else {
            let mut last = None;
            for index in usize::from(level) + 1..32 {
                if let Some(page) = levels[index].take() {
                    let node = engine.node(*page)?;
                    if levels[index + 1..].iter().all(Option::is_none) {
                        last = Some(node);
                        break;
                    }
                    append_root(&mut engine, &mut levels, node)?;
                }
            }
            match last {
                Some(node) => node,
                None if F::empty_allowed() => {
                    let mut page = engine.page(0)?;
                    page.origin = root;
                    engine.node(page)?
                }
                None => return Err(ContentError::InvalidRecord("empty inode table")),
            }
        };
        while root_node.level > 0 && root_node.items == 1 {
            let page = engine.materialize(root_node)?;
            let entry = page
                .entries
                .into_iter()
                .next()
                .ok_or(ContentError::NonCanonicalPagePartition)?;
            root_node = Engine::<F>::child(entry, page.level - 1);
        }
        let id = engine
            .persist(root_node)?
            .id
            .ok_or(ContentError::IdentityMismatch)?;
        engine.work.peak_scratch_bytes = engine.budget.peak();
        Ok((id, engine.work))
    }
}

/// Appends one final child to the accumulating right spine.
fn append_root<'o, 'e, F: Format>(
    engine: &mut Engine<'o, 'e, F>,
    levels: &mut [Option<Box<crate::filesystem::sorted::page::Page<F::Key, F::Value>>>],
    mut node: crate::filesystem::sorted::page::Node<F::Key, F::Value>,
) -> ContentResult<()> {
    loop {
        let level = node
            .level
            .checked_add(1)
            .filter(|level| *level <= crate::filesystem::limits::MAXIMUM_TREE_LEVEL)
            .ok_or(ContentError::MappingDepthExceeded)?;
        let entry = engine.entry(node)?;
        let slot = &mut levels[usize::from(level)];
        if slot.is_none() {
            *slot = Some(Box::new(engine.page(level)?));
        }
        match engine.push(slot.as_mut().expect("spine page"), entry)? {
            Some(next) => node = next,
            None => return Ok(()),
        }
    }
}

/// One directory root, before or after an update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryRoot(pub ObjectId);

/// Applies strictly sorted unique final directory bindings.
///
/// `observe` receives each original and final inode serial the merge actually
/// saw, in key order, from the same leaf merge that produced the pages.
/// Observations are provisional until the caller's whole operation succeeds.
pub fn apply_directory_changes(
    objects: &mut FilesystemObjects<'_>,
    base: Option<DirectoryRoot>,
    changes: impl Iterator<Item = ContentResult<(crate::filesystem::path::PathName, Option<u64>)>>,
    scratch_limit: usize,
    observe: &mut dyn FnMut(Option<u64>, Option<u64>) -> ContentResult<()>,
) -> ContentResult<(DirectoryRoot, SortedWork)> {
    let (root, work) = Engine::<CompactDirectory>::apply_root(
        objects,
        base.map(|root| root.0),
        changes,
        scratch_limit,
        observe,
    )?;
    Ok((DirectoryRoot(root), work))
}

/// Applies strictly sorted unique final inode values and tombstones.
pub fn apply_inode_changes(
    objects: &mut FilesystemObjects<'_>,
    base: Option<ObjectId>,
    changes: impl Iterator<Item = ContentResult<(u64, Option<crate::object::inode_leaf::InodeValue>)>>,
    scratch_limit: usize,
) -> ContentResult<(ObjectId, SortedWork)> {
    Engine::<CompactInodes>::apply_root(objects, base, changes, scratch_limit, &mut |_, _| Ok(()))
}

/// Emits the canonical empty directory page as a real, final result.
pub fn emit_empty_directory(objects: &mut FilesystemObjects<'_>) -> ContentResult<DirectoryRoot> {
    let canonical = empty_page::<CompactDirectory>(0)?;
    let object =
        crate::object::FinalizedObject::new(crate::object::ObjectRole::DirectoryLeaf, canonical)?;
    objects.emit(object).map(DirectoryRoot)
}
