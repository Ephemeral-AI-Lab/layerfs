//! Root-against-root comparison at inode, binding and path level.
//!
//! Changed inodes come from the page-skipping difference of the two inode
//! tables, and changed names from the directories among them, so detecting a
//! change costs work proportional to the change, not to the namespace. Paths
//! are not stored in the canonical tree: a breadth-first walk from the root
//! names the changed inodes and stops once every one is found. Each walk and
//! every listing is bounded by an explicit entry limit; reaching it is
//! reported as truncation, never as a smaller complete answer.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::sorted::{difference, DifferenceWork};
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::{list_after, DirectoryReadWork};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::path::PathName;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::sorted::format::{CompactDirectory, CompactInodes};
use crate::filesystem::sorted::DirectoryRoot;
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::{AuthenticatedObjects, ObjectId};

/// Names one listing request returns.
const LISTING_WINDOW: usize = 256;

/// One checked root's inode table, decoded once.
#[derive(Clone, Copy, Debug)]
pub struct TreeTable {
    /// The decoded root.
    pub root: FilesystemRoot,
    /// Its inode table.
    pub table: InodeTable,
}

impl TreeTable {
    /// Decodes one filesystem root.
    pub fn open(reader: &dyn AuthenticatedObjects, root: FilesystemRootId) -> ContentResult<Self> {
        let root = FilesystemRoot::decode(&reader.read_canonical(root.0)?)?;
        Ok(Self {
            root,
            table: InodeTable {
                root: root.inode_table(),
                root_serial: root.root_inode().serial(),
            },
        })
    }

    /// Serial of the root directory.
    pub fn root_serial(&self) -> u64 {
        self.table.root_serial
    }

    /// Values of many serials in demand order.
    pub fn values(
        &self,
        reader: &dyn AuthenticatedObjects,
        serials: &[u64],
    ) -> ContentResult<Vec<Option<InodeValue>>> {
        lookup_many(reader, self.table, serials, &mut InodeReadWork::default())
    }

    /// One serial's value.
    pub fn value(
        &self,
        reader: &dyn AuthenticatedObjects,
        serial: u64,
    ) -> ContentResult<Option<InodeValue>> {
        Ok(self.values(reader, &[serial])?.pop().flatten())
    }

    /// The binding of one name in one directory serial of this tree.
    pub fn lookup(
        &self,
        reader: &dyn AuthenticatedObjects,
        directory: u64,
        name: &PathName,
    ) -> ContentResult<Option<u64>> {
        match self.value(reader, directory)? {
            Some(value) if value.kind == InodeKind::Directory => {
                crate::filesystem::directory::read::lookup(
                    reader,
                    DirectoryRoot(value.content_root),
                    name,
                )
            }
            _ => Ok(None),
        }
    }
}

/// Values equal apart from the derived reference count.
pub fn same_value(left: &InodeValue, right: &InodeValue) -> bool {
    left.kind == right.kind
        && left.content_root == right.content_root
        && left.metadata_root == right.metadata_root
}

/// Semantic equality of two values: the strict [`same_value`] check, then —
/// when only the metadata forms differ — the decoded portable values, so a
/// root-form row and an inline row of the same logical metadata compare equal.
pub fn same_inode(
    reader: &dyn AuthenticatedObjects,
    left: &InodeValue,
    right: &InodeValue,
    work: &mut crate::filesystem::attributes::AttributeReadWork,
) -> ContentResult<bool> {
    if same_value(left, right) {
        return Ok(true);
    }
    if left.kind != right.kind || left.content_root != right.content_root {
        return Ok(false);
    }
    Ok(
        crate::filesystem::attributes::read_portable(reader, left.metadata_root, left.kind, work)?
            == crate::filesystem::attributes::read_portable(
                reader,
                right.metadata_root,
                right.kind,
                work,
            )?,
    )
}

/// Whether two metadata fields decode to the same portable value, with the
/// strict byte fast path first. Use where the form may differ but the value
/// must mean the same thing.
pub fn same_metadata(
    reader: &dyn AuthenticatedObjects,
    kind: crate::object::inode_leaf::InodeKind,
    left: crate::object::inode_leaf::InodeMetadata,
    right: crate::object::inode_leaf::InodeMetadata,
    work: &mut crate::filesystem::attributes::AttributeReadWork,
) -> ContentResult<bool> {
    if left == right {
        return Ok(true);
    }
    Ok(
        crate::filesystem::attributes::read_portable(reader, left, kind, work)?
            == crate::filesystem::attributes::read_portable(reader, right, kind, work)?,
    )
}

/// Every inode whose value differs between two roots, in serial order.
pub fn inode_changes(
    reader: &dyn AuthenticatedObjects,
    left: &TreeTable,
    right: &TreeTable,
    work: &mut DifferenceWork,
    mut visit: impl FnMut(u64, Option<&InodeValue>, Option<&InodeValue>) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut metadata = crate::filesystem::attributes::AttributeReadWork::default();
    difference::<CompactInodes>(
        reader,
        Some(left.table.root),
        Some(right.table.root),
        work,
        |serial, before, after| match (before, after) {
            (Some(a), Some(b)) if same_inode(reader, a, b, &mut metadata)? => Ok(()),
            _ => visit(*serial, before, after),
        },
    )
}

/// Every inode whose stored value differs between two roots, reference counts
/// included, in serial order. An absent left side is an empty table.
pub fn inode_differences(
    reader: &dyn AuthenticatedObjects,
    left: Option<&TreeTable>,
    right: &TreeTable,
    work: &mut DifferenceWork,
    mut visit: impl FnMut(u64, Option<&InodeValue>, Option<&InodeValue>) -> ContentResult<()>,
) -> ContentResult<()> {
    difference::<CompactInodes>(
        reader,
        left.map(|tree| tree.table.root),
        Some(right.table.root),
        work,
        |serial, before, after| visit(*serial, before, after),
    )
}

/// Every name binding `right` holds that `left` does not, as `(child, parent,
/// name)`: the new bindings of each directory whose listing changed, and every
/// binding of a directory `left` lacks. Work follows the change; with no
/// `left`, it is every binding of `right`.
pub fn new_bindings(
    reader: &dyn AuthenticatedObjects,
    left: Option<&TreeTable>,
    right: &TreeTable,
    mut visit: impl FnMut(u64, u64, &PathName) -> ContentResult<()>,
) -> ContentResult<()> {
    let mut work = DifferenceWork::default();
    let mut directories = Vec::new();
    let listing = |value: Option<&InodeValue>| {
        value
            .filter(|value| value.kind == InodeKind::Directory)
            .map(|value| value.content_root)
    };
    inode_differences(reader, left, right, &mut work, |serial, before, after| {
        if let Some(now) = listing(after) {
            let was = listing(before);
            if was != Some(now) {
                directories.push((serial, was, now));
            }
        }
        Ok(())
    })?;
    for (parent, was, now) in directories {
        match was {
            Some(was) => {
                binding_changes(reader, Some(was), Some(now), &mut work, |name, _, child| {
                    child.map_or(Ok(()), |child| visit(child, parent, name))
                })?
            }
            None => list_directory(reader, now, |name, child| {
                visit(child, parent, name).map(|()| true)
            })?,
        }
    }
    Ok(())
}

/// Every name whose binding differs between two directory contents.
pub fn binding_changes(
    reader: &dyn AuthenticatedObjects,
    left: Option<ObjectId>,
    right: Option<ObjectId>,
    work: &mut DifferenceWork,
    mut visit: impl FnMut(&PathName, Option<u64>, Option<u64>) -> ContentResult<()>,
) -> ContentResult<()> {
    difference::<CompactDirectory>(reader, left, right, work, |name, before, after| {
        visit(name, before.copied(), after.copied())
    })
}

/// Lists one directory's bindings, all windows, in name order.
pub fn list_directory(
    reader: &dyn AuthenticatedObjects,
    directory: ObjectId,
    mut each: impl FnMut(&PathName, u64) -> ContentResult<bool>,
) -> ContentResult<()> {
    let mut after: Option<PathName> = None;
    let mut work = DirectoryReadWork::default();
    loop {
        let page = list_after(
            reader,
            DirectoryRoot(directory),
            after.as_ref(),
            LISTING_WINDOW,
            LISTING_WINDOW * 300,
            &mut work,
        )?;
        for (name, serial) in &page.entries {
            if !each(name, *serial)? {
                return Ok(());
            }
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => return Ok(()),
        }
    }
}

/// Joins a parent path and one name; the root is "/".
pub fn join(parent: &str, name: &PathName) -> String {
    if parent == "/" {
        format!("/{}", name.as_str())
    } else {
        format!("{parent}/{}", name.as_str())
    }
}

/// Paths of the wanted serials, breadth first from the root, first path only.
///
/// The walk ends when every wanted serial is named or `limit` directories have
/// been listed; an unnamed serial is then simply absent from the answer.
pub fn locate(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    wanted: &BTreeSet<u64>,
    limit: usize,
) -> ContentResult<BTreeMap<u64, String>> {
    let mut found = BTreeMap::new();
    if wanted.contains(&tree.root_serial()) {
        found.insert(tree.root_serial(), "/".to_owned());
    }
    let mut queue = VecDeque::from([(tree.root_serial(), "/".to_owned())]);
    let mut listed = 0usize;
    while let Some((directory, path)) = queue.pop_front() {
        if found.len() == wanted.len() || listed >= limit {
            break;
        }
        listed += 1;
        let Some(value) = tree.value(reader, directory)? else {
            continue;
        };
        let mut children = Vec::new();
        list_directory(reader, value.content_root, |name, serial| {
            children.push((join(&path, name), serial));
            Ok(true)
        })?;
        let serials = children
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        for ((child, serial), value) in children.into_iter().zip(tree.values(reader, &serials)?) {
            if wanted.contains(&serial) {
                found.entry(serial).or_insert_with(|| child.clone());
            }
            if value.is_some_and(|value| value.kind == InodeKind::Directory) {
                queue.push_back((serial, child));
            }
        }
    }
    Ok(found)
}

/// Every entry below one directory, depth first, paths relative to `path`.
pub fn subtree(
    reader: &dyn AuthenticatedObjects,
    tree: &TreeTable,
    path: &str,
    directory: u64,
    limit: usize,
    out: &mut Vec<(String, u64, InodeValue)>,
) -> ContentResult<bool> {
    let mut stack = vec![(directory, path.to_owned())];
    while let Some((serial, path)) = stack.pop() {
        let Some(value) = tree.value(reader, serial)? else {
            return Err(ContentError::InvalidRecord("listed directory record"));
        };
        let mut children = Vec::new();
        list_directory(reader, value.content_root, |name, child| {
            children.push((join(&path, name), child));
            Ok(true)
        })?;
        let serials = children
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        let values = tree.values(reader, &serials)?;
        for ((child, serial), value) in children.into_iter().zip(values).rev() {
            let value = value.ok_or(ContentError::InvalidRecord("bound inode record"))?;
            if out.len() >= limit {
                return Ok(false);
            }
            if value.kind == InodeKind::Directory {
                stack.push((serial, child.clone()));
            }
            out.push((child, serial, value));
        }
    }
    Ok(true)
}

/// Kind of one path-level change from the left root to the right root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    /// Present only on the right.
    Added,
    /// Present only on the left.
    Removed,
    /// Present on both with a different inode or value.
    Modified,
}

/// One side of a changed path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntryFacts {
    /// Inode serial.
    pub serial: u64,
    /// Inode value.
    pub value: InodeValue,
}

/// One path-level change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathChange {
    /// Absolute path.
    pub path: String,
    /// Kind of change.
    pub kind: ChangeKind,
    /// The left entry, when present.
    pub left: Option<EntryFacts>,
    /// The right entry, when present.
    pub right: Option<EntryFacts>,
}

/// A bounded path-level comparison.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RootChanges {
    /// Changes in path order.
    pub entries: Vec<PathChange>,
    /// True when `limit` stopped the comparison early.
    pub truncated: bool,
    /// Sorted-tree work.
    pub work: DifferenceWork,
}

/// Path-level comparison of two roots, at most `limit` entries.
pub fn root_changes(
    reader: &dyn AuthenticatedObjects,
    left: FilesystemRootId,
    right: FilesystemRootId,
    limit: usize,
) -> ContentResult<RootChanges> {
    let mut out = RootChanges::default();
    if left == right {
        return Ok(out);
    }
    let (l, r) = (
        TreeTable::open(reader, left)?,
        TreeTable::open(reader, right)?,
    );
    let mut changed = BTreeMap::new();
    inode_changes(reader, &l, &r, &mut out.work, |serial, before, after| {
        changed.insert(serial, (before.copied(), after.copied()));
        Ok(())
    })?;
    let directory =
        |value: &Option<InodeValue>| value.filter(|value| value.kind == InodeKind::Directory);
    let mut wanted = BTreeSet::new();
    for (serial, (before, after)) in &changed {
        match (directory(before), directory(after), before, after) {
            (Some(_), Some(_), _, _) => {
                wanted.insert(*serial);
            }
            (None, None, Some(_), Some(_)) => {
                wanted.insert(*serial);
            }
            _ => {}
        }
    }
    let paths = locate(reader, &r, &wanted, limit)?;
    out.truncated |= paths.len() < wanted.len();
    let mut entries: BTreeMap<String, PathChange> = BTreeMap::new();
    for (serial, (before, after)) in &changed {
        if entries.len() >= limit {
            out.truncated = true;
            break;
        }
        let (Some(before), Some(after)) = (before, after) else {
            continue;
        };
        let Some(path) = paths.get(serial) else {
            continue;
        };
        if before.kind == InodeKind::Directory && after.kind == InodeKind::Directory {
            let mut bindings = Vec::new();
            binding_changes(
                reader,
                Some(before.content_root),
                Some(after.content_root),
                &mut out.work,
                |name, was, now| {
                    bindings.push((join(path, name), was, now));
                    Ok(())
                },
            )?;
            for (child, was, now) in bindings {
                for (tree, serial, kind) in
                    [(&l, was, ChangeKind::Removed), (&r, now, ChangeKind::Added)]
                {
                    let Some(serial) = serial else { continue };
                    let value = tree
                        .value(reader, serial)?
                        .ok_or(ContentError::InvalidRecord("bound inode record"))?;
                    record(
                        &mut entries,
                        child.clone(),
                        kind,
                        EntryFacts { serial, value },
                    );
                    if value.kind == InodeKind::Directory {
                        let mut below = Vec::new();
                        let complete = subtree(reader, tree, &child, serial, limit, &mut below)?;
                        out.truncated |= !complete;
                        for (path, serial, value) in below {
                            record(&mut entries, path, kind, EntryFacts { serial, value });
                        }
                    }
                }
            }
        } else if before.kind != InodeKind::Directory && after.kind != InodeKind::Directory {
            let (serial, before, after) = (*serial, *before, *after);
            record(
                &mut entries,
                path.clone(),
                ChangeKind::Removed,
                EntryFacts {
                    serial,
                    value: before,
                },
            );
            record(
                &mut entries,
                path.clone(),
                ChangeKind::Added,
                EntryFacts {
                    serial,
                    value: after,
                },
            );
        }
    }
    let mut metadata = crate::filesystem::attributes::AttributeReadWork::default();
    let mut kept = Vec::new();
    for entry in entries.into_values() {
        let keep = match (entry.left, entry.right) {
            (Some(a), Some(b)) => {
                a.serial != b.serial || !same_inode(reader, &a.value, &b.value, &mut metadata)?
            }
            _ => true,
        };
        if keep {
            kept.push(entry);
        }
    }
    out.entries = kept.into_iter().take(limit).collect();
    if out.entries.len() >= limit {
        out.truncated = true;
    }
    Ok(out)
}

/// Records one side of a path; both sides make the path Modified.
fn record(
    entries: &mut BTreeMap<String, PathChange>,
    path: String,
    kind: ChangeKind,
    facts: EntryFacts,
) {
    let entry = entries.entry(path.clone()).or_insert(PathChange {
        path,
        kind,
        left: None,
        right: None,
    });
    match kind {
        ChangeKind::Removed => entry.left = Some(facts),
        _ => entry.right = Some(facts),
    }
    entry.kind = match (entry.left, entry.right) {
        (Some(_), Some(_)) => ChangeKind::Modified,
        (Some(_), None) => ChangeKind::Removed,
        _ => ChangeKind::Added,
    };
}
