use super::namespace;
use crate::object::access::ObjectRead;
use crate::tree::directory::{directory_page_after, DirectoryStateRoot, NamespaceCounters};
use crate::tree::inode::codec::decode_inode_record;
use crate::tree::inode::{
    inode_table_lookup, inode_table_lookup_pair, InodeId, InodeKind, InodeRecordV1,
    InodeTableCounters, InodeTableRoot,
};
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeSummary {
    pub kind: InodeKind,
    pub content_root: ObjectId,
    pub metadata_root: ObjectId,
    pub namespace_ref_count: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiffAspects {
    pub node_type: bool,
    pub content: bool,
    pub metadata: bool,
    pub directory_membership: bool,
    pub hard_links: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffEntry {
    Add {
        path: CanonicalPath,
        after: NodeSummary,
    },
    Remove {
        path: CanonicalPath,
        before: NodeSummary,
    },
    Modify {
        path: CanonicalPath,
        before: NodeSummary,
        after: NodeSummary,
        aspects: DiffAspects,
    },
}

pub fn diff_roots<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    mut visitor: impl FnMut(DiffEntry) -> CoreResult<()>,
) -> CoreResult<()> {
    if old == new {
        return Ok(());
    }
    let old_namespace = namespace(store, old)?;
    let new_namespace = namespace(store, new)?;
    if old_namespace.profile_id != new_namespace.profile_id
        || old_namespace.root_directory_inode != new_namespace.root_directory_inode
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    let old = RootView {
        table: InodeTableRoot(old_namespace.inode_table_root),
    };
    let new = RootView {
        table: InodeTableRoot(new_namespace.inode_table_root),
    };
    diff_node(
        store,
        old,
        new,
        CanonicalPath::root(),
        Some(old_namespace.root_directory_inode),
        Some(new_namespace.root_directory_inode),
        true,
        &mut visitor,
    )
}

#[derive(Clone, Copy)]
struct RootView {
    table: InodeTableRoot,
}

#[derive(Clone, Copy)]
struct LoadedNode {
    inode: InodeId,
    record: InodeRecordV1,
    summary: NodeSummary,
}

#[allow(clippy::too_many_arguments)]
fn diff_node<S: ObjectRead>(
    store: &S,
    old: RootView,
    new: RootView,
    path: CanonicalPath,
    old_inode: Option<InodeId>,
    new_inode: Option<InodeId>,
    is_root: bool,
    visitor: &mut impl FnMut(DiffEntry) -> CoreResult<()>,
) -> CoreResult<()> {
    let (before, after) = load_pair(store, old, new, old_inode, new_inode, is_root)?;
    match (before, after) {
        (Some(before), Some(after)) => {
            if before.summary != after.summary || before.inode != after.inode {
                visitor(DiffEntry::Modify {
                    path: path.clone(),
                    before: before.summary,
                    after: after.summary,
                    aspects: aspects(before, after),
                })?;
            }
            match (before.record.kind, after.record.kind) {
                (InodeKind::Directory, InodeKind::Directory) => diff_directories(
                    store,
                    old,
                    new,
                    &path,
                    before.record.content_root,
                    after.record.content_root,
                    visitor,
                ),
                (InodeKind::Directory, _) => walk_directory(
                    store,
                    old,
                    &path,
                    before.record.content_root,
                    false,
                    visitor,
                ),
                (_, InodeKind::Directory) => {
                    walk_directory(store, new, &path, after.record.content_root, true, visitor)
                }
                _ => Ok(()),
            }
        }
        (Some(before), None) => {
            visitor(DiffEntry::Remove {
                path: path.clone(),
                before: before.summary,
            })?;
            if before.record.kind == InodeKind::Directory {
                walk_directory(
                    store,
                    old,
                    &path,
                    before.record.content_root,
                    false,
                    visitor,
                )?;
            }
            Ok(())
        }
        (None, Some(after)) => {
            visitor(DiffEntry::Add {
                path: path.clone(),
                after: after.summary,
            })?;
            if after.record.kind == InodeKind::Directory {
                walk_directory(store, new, &path, after.record.content_root, true, visitor)?;
            }
            Ok(())
        }
        (None, None) => Err(CoreError::InvalidRecord("missing Diff node")),
    }
}

fn load_pair(
    store: &impl ObjectRead,
    old: RootView,
    new: RootView,
    old_inode: Option<InodeId>,
    new_inode: Option<InodeId>,
    is_root: bool,
) -> CoreResult<(Option<LoadedNode>, Option<LoadedNode>)> {
    let (old_record, new_record) = match (old_inode, new_inode) {
        (Some(old_inode), Some(new_inode)) if old_inode == new_inode => inode_table_lookup_pair(
            store,
            old.table,
            new.table,
            old_inode,
            &mut InodeTableCounters::default(),
        )?,
        (old_inode, new_inode) => (
            old_inode
                .map(|inode| record_id(store, old, inode))
                .transpose()?,
            new_inode
                .map(|inode| record_id(store, new, inode))
                .transpose()?,
        ),
    };
    if let (Some(old_inode), Some(new_inode), Some(old_record), Some(new_record)) =
        (old_inode, new_inode, old_record, new_record)
    {
        if old_record == new_record {
            let record = load_record(store, old_record, is_root)?;
            return Ok((
                Some(loaded(old_inode, record)),
                Some(loaded(new_inode, record)),
            ));
        }
    }
    Ok((
        old_inode
            .zip(old_record)
            .map(|(inode, id)| load_record(store, id, is_root).map(|record| loaded(inode, record)))
            .transpose()?,
        new_inode
            .zip(new_record)
            .map(|(inode, id)| load_record(store, id, is_root).map(|record| loaded(inode, record)))
            .transpose()?,
    ))
}

fn record_id(store: &impl ObjectRead, view: RootView, inode: InodeId) -> CoreResult<ObjectId> {
    inode_table_lookup(store, view.table, inode, &mut InodeTableCounters::default())?
        .ok_or(CoreError::MissingObject)
}

fn load_record(store: &impl ObjectRead, id: ObjectId, is_root: bool) -> CoreResult<InodeRecordV1> {
    let record = store.with_authenticated_canonical(id, decode_inode_record)?;
    record.validate(is_root)?;
    Ok(record)
}

fn loaded(inode: InodeId, record: InodeRecordV1) -> LoadedNode {
    LoadedNode {
        inode,
        summary: NodeSummary {
            kind: record.kind,
            content_root: record.content_root,
            metadata_root: record.metadata_root,
            namespace_ref_count: record.namespace_ref_count,
        },
        record,
    }
}

fn aspects(before: LoadedNode, after: LoadedNode) -> DiffAspects {
    DiffAspects {
        node_type: before.record.kind != after.record.kind,
        content: before.record.content_root != after.record.content_root,
        metadata: before.record.metadata_root != after.record.metadata_root,
        directory_membership: before.record.kind == InodeKind::Directory
            && before.record.content_root != after.record.content_root,
        hard_links: before.inode != after.inode
            || before.record.namespace_ref_count != after.record.namespace_ref_count,
    }
}

fn diff_directories<S: ObjectRead>(
    store: &S,
    old: RootView,
    new: RootView,
    parent: &CanonicalPath,
    old_root: ObjectId,
    new_root: ObjectId,
    visitor: &mut impl FnMut(DiffEntry) -> CoreResult<()>,
) -> CoreResult<()> {
    if old_root == new_root {
        let mut entries = DirectoryCursor::new(store, old_root);
        while let Some((name, inode)) = entries.next()? {
            diff_node(
                store,
                old,
                new,
                child(parent, &name)?,
                Some(inode),
                Some(inode),
                false,
                visitor,
            )?;
        }
        return Ok(());
    }

    let mut old_entries = DirectoryCursor::new(store, old_root);
    let mut new_entries = DirectoryCursor::new(store, new_root);
    let mut before = old_entries.next()?;
    let mut after = new_entries.next()?;
    loop {
        match (before.take(), after.take()) {
            (None, None) => return Ok(()),
            (Some((old_name, old_inode)), Some((new_name, new_inode))) if old_name == new_name => {
                diff_node(
                    store,
                    old,
                    new,
                    child(parent, &old_name)?,
                    Some(old_inode),
                    Some(new_inode),
                    false,
                    visitor,
                )?;
                before = old_entries.next()?;
                after = new_entries.next()?;
            }
            (Some((old_name, old_inode)), Some((new_name, new_inode))) if old_name < new_name => {
                diff_node(
                    store,
                    old,
                    new,
                    child(parent, &old_name)?,
                    Some(old_inode),
                    None,
                    false,
                    visitor,
                )?;
                before = old_entries.next()?;
                after = Some((new_name, new_inode));
            }
            (Some((old_name, old_inode)), Some((new_name, new_inode))) => {
                diff_node(
                    store,
                    old,
                    new,
                    child(parent, &new_name)?,
                    None,
                    Some(new_inode),
                    false,
                    visitor,
                )?;
                before = Some((old_name, old_inode));
                after = new_entries.next()?;
            }
            (Some((name, inode)), None) => {
                diff_node(
                    store,
                    old,
                    new,
                    child(parent, &name)?,
                    Some(inode),
                    None,
                    false,
                    visitor,
                )?;
                before = old_entries.next()?;
            }
            (None, Some((name, inode))) => {
                diff_node(
                    store,
                    old,
                    new,
                    child(parent, &name)?,
                    None,
                    Some(inode),
                    false,
                    visitor,
                )?;
                after = new_entries.next()?;
            }
        }
    }
}

fn walk_directory<S: ObjectRead>(
    store: &S,
    view: RootView,
    parent: &CanonicalPath,
    root: ObjectId,
    added: bool,
    visitor: &mut impl FnMut(DiffEntry) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut entries = DirectoryCursor::new(store, root);
    while let Some((name, inode)) = entries.next()? {
        let path = child(parent, &name)?;
        if added {
            diff_node(store, view, view, path, None, Some(inode), false, visitor)?;
        } else {
            diff_node(store, view, view, path, Some(inode), None, false, visitor)?;
        }
    }
    Ok(())
}

struct DirectoryCursor<'a, S> {
    store: &'a S,
    root: DirectoryStateRoot,
    after: Option<CanonicalName>,
    buffered: VecDeque<(CanonicalName, InodeId)>,
    done: bool,
}

impl<'a, S: ObjectRead> DirectoryCursor<'a, S> {
    fn new(store: &'a S, root: ObjectId) -> Self {
        Self {
            store,
            root: DirectoryStateRoot(root),
            after: None,
            buffered: VecDeque::new(),
            done: false,
        }
    }

    fn next(&mut self) -> CoreResult<Option<(CanonicalName, InodeId)>> {
        loop {
            if let Some(entry) = self.buffered.pop_front() {
                return Ok(Some(entry));
            }
            if self.done {
                return Ok(None);
            }
            let page = directory_page_after(
                self.store,
                self.root,
                self.after.as_ref(),
                32,
                16 * 1024,
                &mut NamespaceCounters::default(),
            )?;
            self.after = page.continuation;
            self.done = self.after.is_none();
            self.buffered = page.entries.into();
        }
    }
}

fn child(parent: &CanonicalPath, name: &CanonicalName) -> CoreResult<CanonicalPath> {
    if parent.is_root() {
        CanonicalPath::from_bytes(name.as_bytes())
    } else {
        let mut bytes = parent.as_bytes().to_vec();
        bytes.push(b'/');
        bytes.extend_from_slice(name.as_bytes());
        CanonicalPath::from_bytes(&bytes)
    }
}
