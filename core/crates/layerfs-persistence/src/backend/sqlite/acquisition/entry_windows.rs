//! Fixed independent-entry SQL inputs; dependency boundaries keep ordered execution.
use super::{
    accounting::{charge, entry_bytes, native_bytes},
    statements::{
        signed, Failure, BIND_NATIVE, ENTRY_DEPENDENCIES, PUT_ENTRIES, PUT_ENTRY, PUT_NATIVE,
    },
    writes::{packed, put_one, well_formed},
};
use crate::backend::{records::BackendError, sqlite::rows, Transaction};
use layerfs_content::inode_leaf::InodeKind;
use layerfs_storage::port::acquisition::{NewEntry, Owner};
use std::collections::BTreeSet;

/// Execution scratch only; every public write window is processed in full.
const INPUT_ROWS: usize = 32;

#[derive(Default)]
struct Input<'a> {
    parent: Option<i64>,
    name: Option<&'a [u8]>,
    position: Option<i64>,
    kind: Option<i64>,
    metadata: Option<&'a [u8]>,
    target: Option<&'a [u8]>,
    path: Option<&'a [u8]>,
    native: Option<[u8; 60]>,
}
impl<'a> Input<'a> {
    /// Only rows whose shape and SQL constraints cannot refuse may be grouped.
    /// Other rows are executed once in their original order, after this prefix.
    fn independent(entry: &'a NewEntry) -> Option<Self> {
        if !well_formed(entry) || entry.key.name.len() > 255 {
            return None;
        }
        let parent = match entry.key.parent {
            Some(parent) => signed(parent).ok()?,
            None => -1,
        };
        let position = entry.position.map(signed).transpose().ok()?;
        let path = entry.native_path.as_deref();
        if path.is_some_and(|path| path.is_empty() || path.len() > 4096)
            || (entry.kind == InodeKind::RegularFile && position == Some(0))
        {
            return None;
        }
        Some(Self {
            parent: Some(parent),
            name: Some(&entry.key.name),
            position,
            kind: Some(i64::from(entry.kind.code())),
            metadata: Some(entry.metadata_root.as_bytes()),
            target: entry
                .target_root
                .as_ref()
                .map(|root| root.as_bytes().as_slice()),
            path,
            native: entry.native.map(packed),
        })
    }
    fn values<'b>(&'b self, values: &mut Vec<&'b dyn rusqlite::ToSql>) {
        values.extend([
            &self.parent as &dyn rusqlite::ToSql,
            &self.name,
            &self.position,
            &self.kind,
            &self.metadata,
            &self.target,
            &self.path,
        ]);
        // rusqlite binds slices as BLOBs; the packed native value stays borrowed.
        // The optional slice itself is supplied separately by the caller.
    }
}

pub(crate) fn append(
    tx: &Transaction<'_>,
    owner: Owner,
    entries: &[NewEntry],
) -> Result<Vec<u64>, Failure> {
    let operation = signed(owner.operation)?;
    let mut native_one = tx.prepare(BIND_NATIVE)?;
    let mut entry_one = tx.prepare(PUT_ENTRY)?;
    let mut dependencies = tx.prepare(ENTRY_DEPENDENCIES)?;
    let mut natives = tx.prepare(PUT_NATIVE)?;
    let mut append = tx.prepare(PUT_ENTRIES)?;
    let (mut offset, mut rows, mut bytes) = (0, 0, 0);
    let mut canonical = Vec::new();
    while offset < entries.len() {
        let mut input: [Input<'_>; INPUT_ROWS] = std::array::from_fn(|_| Input::default());
        let (mut keys, mut directories, mut identities, mut positions) = (
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let mut count = 0;
        for entry in entries[offset..].iter().take(INPUT_ROWS) {
            let Some(bound) = Input::independent(entry) else {
                break;
            };
            if !keys.insert((entry.key.parent, entry.key.name.as_slice()))
                || (entry.kind == InodeKind::Directory
                    && entry.position.is_some()
                    && !directories.insert(entry.position))
            {
                break;
            }
            if let (Some(position), Some(native)) = (entry.position, entry.native) {
                if !identities.insert((native.device, native.inode)) || !positions.insert(position)
                {
                    break;
                }
            }
            input[count] = bound;
            count += 1;
        }
        if count != 0 {
            let packed: [_; INPUT_ROWS] =
                std::array::from_fn(|i| input[i].native.as_ref().map(|n| n.as_slice()));
            let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 8 * INPUT_ROWS);
            values.push(&operation);
            for i in 0..INPUT_ROWS {
                input[i].values(&mut values);
                values.push(&packed[i]);
            }
            let bound_bytes = binding_bytes(&entries[offset..offset + count]);
            let first = dependencies.mapped(&values, bound_bytes, 1, |row| {
                row.get::<_, Option<i64>>(0).map_err(rows::error)
            })?;
            if let Some(slot) = first.first().ok_or(BackendError::Integrity)? {
                count = usize::try_from(*slot).map_err(|_| BackendError::Integrity)?;
                if count >= INPUT_ROWS {
                    return Err(BackendError::Integrity.into());
                }
            }
            // The indexed check and both writes share this transaction. No
            // concurrent owner can invalidate the proven independent prefix.
            if count != 0 {
                drop(values);
                for slot in &mut input[count..] {
                    *slot = Input::default();
                }
                let packed: [_; INPUT_ROWS] =
                    std::array::from_fn(|i| input[i].native.as_ref().map(|n| n.as_slice()));
                let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 8 * INPUT_ROWS);
                values.push(&operation);
                for i in 0..INPUT_ROWS {
                    input[i].values(&mut values);
                    values.push(&packed[i]);
                }
                let bound_bytes = binding_bytes(&entries[offset..offset + count]);
                natives.borrowed(&values, bound_bytes)?;
                append.borrowed(&values, bound_bytes)?;
                for entry in &entries[offset..offset + count] {
                    rows += 1;
                    bytes += entry_bytes(entry);
                    if let (Some(position), Some(_)) = (entry.position, entry.native) {
                        canonical.push(position);
                        rows += 1;
                        bytes += native_bytes(
                            entry
                                .native_path
                                .as_deref()
                                .ok_or(BackendError::Integrity)?,
                        );
                    }
                }
                offset += count;
                continue;
            }
        }
        let (first, added_rows, added_bytes) =
            put_one(&mut native_one, &mut entry_one, operation, &entries[offset])?;
        canonical.extend(first);
        rows += added_rows;
        bytes += added_bytes;
        offset += 1;
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// Actual scalar/BLOB bytes bound, excluding NULL padding and literal slots.
fn binding_bytes(entries: &[NewEntry]) -> u64 {
    8 + entries
        .iter()
        .map(|e| {
            16 + e.position.map_or(0, |_| 8)
                + e.key.name.len() as u64
                + 32
                + e.target_root.map_or(0, |_| 32)
                + e.native_path.as_ref().map_or(0, |p| p.len() as u64)
                + e.native.map_or(0, |_| 60)
        })
        .sum::<u64>()
}
