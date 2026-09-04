//! Bounded final binding/reference input for a semantic snapshot replacement.
use super::delta_spool::{Run, Sorter, SORT_BYTES};
use super::reconcile::{lookup_path_inode, parent_path};
use super::resolve::{namespace, resolve_parent, LogicalCounters};
use crate::object::access::{ObjectRead, ObjectStore};
use crate::tree::directory::{
    directory_apply_sorted_with_budget, directory_page_after, DirectoryStateRoot, NamespaceCounters,
};
use crate::tree::inode::codec::{decode_inode_record, encode_inode_record};
use crate::tree::inode::{
    inode_table_apply_sorted_with_budget, inode_table_lookup, InodeId, InodeKind, InodeRecordV1,
    InodeTableCounters, InodeTableRoot,
};
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::path::Path;

#[derive(Clone, Copy)]
pub struct ReconcileBudget<'a> {
    pub scratch_dir: &'a Path,
    pub memory_bytes: u64,
    pub spool_bytes: u64,
}
const PATH: usize = 4098;
const EVENT: usize = 106;
const BINDING: usize = 354;
const FRAME: usize = 323;

fn id(bytes: &[u8]) -> CoreResult<InodeId> {
    InodeId::from_slice(bytes)
}
fn loaded(
    store: &impl ObjectRead,
    table: InodeTableRoot,
    inode: InodeId,
) -> CoreResult<Option<(ObjectId, InodeRecordV1)>> {
    inode_table_lookup(store, table, inode, &mut InodeTableCounters::default())?
        .map(|object| {
            store
                .with_authenticated_canonical(object, decode_inode_record)
                .map(|record| (object, record))
        })
        .transpose()
}
fn reference(events: &mut Sorter<EVENT>, inode: InodeId, delta: i64) -> CoreResult<()> {
    let mut event = [0; EVENT];
    event[..32].copy_from_slice(inode.as_bytes());
    event[33..41].copy_from_slice(&delta.to_be_bytes());
    events.push(event)
}
fn replacement(
    events: &mut Sorter<EVENT>,
    inode: InodeId,
    record: InodeRecordV1,
    parent: bool,
) -> CoreResult<()> {
    let mut event = [0; EVENT];
    event[..32].copy_from_slice(inode.as_bytes());
    event[32] = if parent { 2 } else { 1 };
    event[33] = record.kind as u8;
    event[34..66].copy_from_slice(record.content_root.as_bytes());
    event[66..98].copy_from_slice(record.metadata_root.as_bytes());
    events.push(event)
}
fn event_record(event: &[u8; EVENT], count: u64) -> CoreResult<InodeRecordV1> {
    Ok(InodeRecordV1 {
        kind: match event[33] {
            1 => InodeKind::RegularFile,
            2 => InodeKind::Directory,
            3 => InodeKind::Symlink,
            _ => return Err(CoreError::InvalidRecord("replacement inode kind")),
        },
        content_root: ObjectId::from_bytes(&event[34..66])?,
        metadata_root: ObjectId::from_bytes(&event[66..98])?,
        namespace_ref_count: count,
    })
}

// Stack frames are anonymous fixed records, so a deep selected subtree does not
// accumulate directory pages, path strings or inode records in memory.
fn subtree(
    store: &impl ObjectRead,
    table: InodeTableRoot,
    inode: InodeId,
    delta: i64,
    copy: bool,
    events: &mut Sorter<EVENT>,
    budget: ReconcileBudget<'_>,
) -> CoreResult<()> {
    let mut stack = Run::<FRAME>::create(budget.scratch_dir)?;
    let mut next = Some((inode, 0usize));
    loop {
        if let Some((inode, path_bytes)) = next.take() {
            let (_, record) = loaded(store, table, inode)?.ok_or(CoreError::MissingObject)?;
            reference(events, inode, delta)?;
            if copy {
                replacement(events, inode, record, false)?;
            }
            if record.kind == InodeKind::Directory {
                for n in 0..stack.count {
                    if stack.at(n)?[..32] == *inode.as_bytes() {
                        return Err(CoreError::InvalidRecord("directory cycle"));
                    }
                }
                if (stack.count + 1)
                    .checked_mul(FRAME as u64)
                    .is_none_or(|n| n > budget.spool_bytes / 8)
                {
                    return Err(CoreError::ObjectLimitExceeded);
                }
                let mut frame = [0; FRAME];
                frame[..32].copy_from_slice(inode.as_bytes());
                frame[32..64].copy_from_slice(record.content_root.as_bytes());
                frame[321..323].copy_from_slice(&(path_bytes as u16).to_be_bytes());
                stack.push(&frame)?;
            }
        }
        if stack.count == 0 {
            break;
        }
        let mut frame = stack.at(stack.count - 1)?;
        let len = u16::from_be_bytes(frame[319..321].try_into().unwrap()) as usize;
        let after = if len == 0 {
            None
        } else {
            Some(CanonicalName::from_bytes(&frame[64..64 + len])?)
        };
        let page = directory_page_after(
            store,
            DirectoryStateRoot(ObjectId::from_bytes(&frame[32..64])?),
            after.as_ref(),
            1,
            8192,
            &mut NamespaceCounters::default(),
        )?;
        let Some((name, child)) = page.entries.into_iter().next() else {
            stack.truncate(stack.count - 1)?;
            continue;
        };
        let path_bytes = u16::from_be_bytes(frame[321..323].try_into().unwrap()) as usize
            + name.as_bytes().len()
            + 1;
        if path_bytes > 4096 {
            return Err(CoreError::PathLimitExceeded);
        }
        frame[64..319].fill(0);
        frame[64..64 + name.as_bytes().len()].copy_from_slice(name.as_bytes());
        frame[319..321].copy_from_slice(&(name.as_bytes().len() as u16).to_be_bytes());
        stack.truncate(stack.count - 1)?;
        stack.push(&frame)?;
        next = Some((child, path_bytes));
    }
    stack.remove()
}

pub fn replace_paths_from_snapshot_bounded<S: ObjectStore>(
    store: &mut S,
    destination_root: ObjectId,
    source_root: ObjectId,
    paths: &[CanonicalPath],
    budget: ReconcileBudget<'_>,
) -> CoreResult<ObjectId> {
    let destination = namespace(store, destination_root)?;
    let source = namespace(store, source_root)?;
    if destination.profile_id != source.profile_id
        || destination.root_directory_inode != source.root_directory_inode
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    let buffer = (budget.memory_bytes / 8).min(SORT_BYTES);
    // Three sorter buffers, two decoded pages and fixed-record cursors coexist
    // with the tree writer. Reserve its scratch from the same caller allowance.
    let scratch = budget
        .memory_bytes
        .checked_sub(3 * buffer + 64 * 1024)
        .ok_or(CoreError::ObjectLimitExceeded)?;
    if scratch < 64 * 1024 {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut selected = Sorter::<PATH>::new(budget.scratch_dir, budget.spool_bytes / 8, buffer)?;
    for original in paths {
        let mut path = original.clone();
        loop {
            if path.is_root() {
                return Ok(source_root);
            }
            match resolve_parent(
                store,
                destination_root,
                &path,
                &mut LogicalCounters::default(),
            ) {
                Ok(_) => break,
                Err(CoreError::MissingObject | CoreError::InvalidRecord(_)) => {
                    path = parent_path(&path)?
                }
                Err(error) => return Err(error),
            }
        }
        let mut record = [0; PATH];
        let bytes = path.as_bytes();
        record[..bytes.len()].copy_from_slice(bytes);
        record[4096..].copy_from_slice(&(bytes.len() as u16).to_be_bytes());
        selected.push(record)?;
    }
    let mut selected = selected.finish()?;
    let mut events = Sorter::<EVENT>::new(budget.scratch_dir, budget.spool_bytes / 2, buffer)?;
    let mut bindings = Sorter::<BINDING>::new(budget.scratch_dir, budget.spool_bytes / 8, buffer)?;
    let destination_table = InodeTableRoot(destination.inode_table_root);
    let source_table = InodeTableRoot(source.inode_table_root);
    let mut previous = None::<CanonicalPath>;
    while let Some(record) = selected.next()? {
        let len = u16::from_be_bytes(record[4096..].try_into().unwrap()) as usize;
        let path = CanonicalPath::from_bytes(&record[..len])?;
        if previous.as_ref().is_some_and(|parent| {
            path == *parent
                || path
                    .as_bytes()
                    .strip_prefix(parent.as_bytes())
                    .is_some_and(|suffix| suffix.first() == Some(&b'/'))
        }) {
            continue;
        }
        let (parent, name) = resolve_parent(
            store,
            destination_root,
            &path,
            &mut LogicalCounters::default(),
        )?;
        let before = lookup_path_inode(store, destination_root, &path)?;
        let after = lookup_path_inode(store, source_root, &path)?;
        if let Some(inode) = before {
            subtree(
                store,
                destination_table,
                inode,
                -1,
                false,
                &mut events,
                budget,
            )?;
        }
        if let Some(inode) = after {
            subtree(store, source_table, inode, 1, true, &mut events, budget)?;
        }
        if before != after {
            let parent_id = loaded(store, destination_table, parent.inode)?
                .ok_or(CoreError::MissingObject)?
                .0;
            let mut row = [0; BINDING];
            row[..32].copy_from_slice(parent.inode.as_bytes());
            row[32..32 + name.as_bytes().len()].copy_from_slice(name.as_bytes());
            if let Some(inode) = after {
                row[287] = 1;
                row[288..320].copy_from_slice(inode.as_bytes());
            }
            row[320..352].copy_from_slice(parent_id.as_bytes());
            row[352..].copy_from_slice(&(name.as_bytes().len() as u16).to_be_bytes());
            bindings.push(row)?;
        }
        previous = Some(path);
    }
    selected.remove()?;
    let mut bindings = bindings.finish()?;
    let mut next = bindings.next()?;
    while let Some(first) = next {
        let inode = id(&first[..32])?;
        let parent = store.with_authenticated_canonical(
            ObjectId::from_bytes(&first[320..352])?,
            decode_inode_record,
        )?;
        let input = std::iter::from_fn(|| {
            let row = next.filter(|row| row[..32] == *inode.as_bytes())?;
            let result = (|| {
                let len = u16::from_be_bytes(row[352..].try_into().unwrap()) as usize;
                Ok((
                    CanonicalName::from_bytes(&row[32..32 + len])?,
                    if row[287] == 0 {
                        None
                    } else {
                        Some(id(&row[288..320])?)
                    },
                ))
            })();
            match bindings.next() {
                Ok(value) => next = value,
                Err(error) => {
                    next = None;
                    return Some(Err(error));
                }
            }
            Some(result)
        });
        let (directory, _) = directory_apply_sorted_with_budget(
            store,
            DirectoryStateRoot(parent.content_root),
            input,
            scratch as usize,
        )?;
        replacement(
            &mut events,
            inode,
            InodeRecordV1 {
                content_root: directory.0,
                ..parent
            },
            true,
        )?;
    }
    bindings.remove()?;
    let mut events = events.finish()?;
    let mut next = events.next()?;
    let mut pairs = Run::<65>::create(budget.scratch_dir)?;
    while let Some(first) = next.take() {
        let inode = id(&first[..32])?;
        let before = loaded(store, destination_table, inode)?;
        let mut delta = 0i64;
        let mut desired = None;
        let mut event = first;
        loop {
            if event[32] == 0 {
                delta = delta
                    .checked_add(i64::from_be_bytes(event[33..41].try_into().unwrap()))
                    .ok_or(CoreError::LengthOverflow)?;
            } else {
                desired = Some(event);
            }
            next = events.next()?;
            if let Some(row) = next.filter(|row| row[..32] == *inode.as_bytes()) {
                event = row;
            } else {
                break;
            }
        }
        let base_count = before.map_or(0, |(_, record)| record.namespace_ref_count);
        let count = if delta >= 0 {
            base_count.checked_add(delta as u64)
        } else {
            base_count.checked_sub(delta.unsigned_abs())
        }
        .ok_or(CoreError::InvalidRecord("namespace reference count"))?;
        let mut pair = [0; 65];
        pair[..32].copy_from_slice(inode.as_bytes());
        if count != 0 || inode == destination.root_directory_inode {
            let record = match desired {
                Some(event) => event_record(&event, count)?,
                None => InodeRecordV1 {
                    namespace_ref_count: count,
                    ..before.ok_or(CoreError::MissingObject)?.1
                },
            };
            record.validate(inode == destination.root_directory_inode)?;
            let object = if before.is_some_and(|(_, old)| old == record) {
                before.unwrap().0
            } else {
                store.put_owned(encode_inode_record(record)?)?
            };
            pair[32] = 1;
            pair[33..].copy_from_slice(object.as_bytes());
        }
        if (pairs.count + 1)
            .checked_mul(65)
            .is_none_or(|n| n > budget.spool_bytes / 8)
        {
            return Err(CoreError::ObjectLimitExceeded);
        }
        pairs.push(&pair)?;
    }
    events.remove()?;
    pairs.rewind()?;
    let input = std::iter::from_fn(|| match pairs.next() {
        Ok(Some(pair)) => Some((|| {
            Ok((
                id(&pair[..32])?,
                if pair[32] == 0 {
                    None
                } else {
                    Some(ObjectId::from_bytes(&pair[33..])?)
                },
            ))
        })()),
        Ok(None) => None,
        Err(error) => Some(Err(error)),
    });
    let (table, _) =
        inode_table_apply_sorted_with_budget(store, destination_table, input, scratch as usize)?;
    pairs.remove()?;
    store.put_owned(crate::tree::directory::codec::encode_namespace_root(
        crate::tree::NamespaceRootV1 {
            inode_table_root: table.0,
            ..destination
        },
    )?)
}
