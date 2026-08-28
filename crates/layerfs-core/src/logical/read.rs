use super::resolver::{resolve, LogicalCounters};
use crate::content::rope::{self, FileStateRoot};
use crate::inode::{InodeKind, InodeRecordV1};
use crate::namespace::{directory_page_after, DirectoryPage, DirectoryStateRoot};
use crate::namespace_codec::decode_symlink;
use crate::object::access::ObjectRead;
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::io::Write;
use std::ops::Range;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Stat {
    pub kind: InodeKind,
    pub namespace_ref_count: u64,
    pub content_root: ObjectId,
    pub metadata_root: ObjectId,
}

impl From<InodeRecordV1> for Stat {
    fn from(record: InodeRecordV1) -> Self {
        Self {
            kind: record.kind,
            namespace_ref_count: record.namespace_ref_count,
            content_root: record.content_root,
            metadata_root: record.metadata_root,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListPage {
    pub entries: Vec<(CanonicalName, crate::inode::InodeId)>,
    pub continuation: Option<CanonicalName>,
}

pub fn stat<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
) -> CoreResult<(Stat, LogicalCounters)> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    Ok((resolved.record.into(), counters))
}

pub fn list<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
    exclusive_after: Option<&CanonicalName>,
    max_entries: usize,
    max_bytes: usize,
) -> CoreResult<(ListPage, LogicalCounters)> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    if resolved.record.kind != InodeKind::Directory {
        return Err(CoreError::WrongLogicalRole);
    }
    let DirectoryPage {
        entries,
        continuation,
    } = directory_page_after(
        store,
        DirectoryStateRoot(resolved.record.content_root),
        exclusive_after,
        max_entries,
        max_bytes,
        &mut counters.namespace,
    )?;
    Ok((
        ListPage {
            entries,
            continuation,
        },
        counters,
    ))
}

pub fn read_range<S: ObjectRead, W: Write>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
    range: Range<u64>,
    sink: W,
) -> CoreResult<LogicalCounters> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    if resolved.record.kind != InodeKind::RegularFile {
        return Err(CoreError::WrongLogicalRole);
    }
    counters.rope = rope::read_range(
        store,
        FileStateRoot(resolved.record.content_root),
        range,
        sink,
    )?;
    Ok(counters)
}

pub fn stream<S: ObjectRead, W: Write>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
    sink: W,
) -> CoreResult<LogicalCounters> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    if resolved.record.kind != InodeKind::RegularFile {
        return Err(CoreError::WrongLogicalRole);
    }
    counters.rope = rope::read_all(store, FileStateRoot(resolved.record.content_root), sink)?;
    Ok(counters)
}

pub fn readlink<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
) -> CoreResult<(Vec<u8>, LogicalCounters)> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    if resolved.record.kind != InodeKind::Symlink {
        return Err(CoreError::WrongLogicalRole);
    }
    let target = store.with_authenticated_canonical(resolved.record.content_root, |canonical| {
        decode_symlink(canonical).map(|symlink| symlink.target)
    })?;
    Ok((target, counters))
}
