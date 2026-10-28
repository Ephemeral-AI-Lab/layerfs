//! Fixed independent-entry SQL inputs; dependency boundaries keep ordered execution.
use super::{
    accounting::{charge, entry_bytes, host_bytes},
    statements::{
        signed, Failure, BIND_HOST_FILE, ENTRY_DEPENDENCIES, PUT_ENTRIES, PUT_ENTRY, PUT_HOST_FILE,
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
    metadata: Option<Vec<u8>>,
    target: Option<&'a [u8]>,
    path: Option<&'a [u8]>,
    host: Option<[u8; 60]>,
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
        let path = entry.host_path.as_deref();
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
            metadata: Some(super::metadata_blob(&entry.metadata).to_vec()),
            target: entry
                .target_root
                .as_ref()
                .map(|root| root.as_bytes().as_slice()),
            path,
            host: entry.host.map(packed),
        })
    }
}

pub(crate) fn append(
    tx: &Transaction<'_>,
    owner: Owner,
    entries: &[NewEntry],
) -> Result<Vec<u64>, Failure> {
    let operation = signed(owner.operation)?;
    let mut host_one = tx.prepare(BIND_HOST_FILE)?;
    let mut entry_one = tx.prepare(PUT_ENTRY)?;
    let mut dependencies = tx.prepare(ENTRY_DEPENDENCIES)?;
    let mut host_files = tx.prepare(PUT_HOST_FILE)?;
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
            if let (Some(position), Some(host)) = (entry.position, entry.host) {
                if !identities.insert((host.device, host.inode)) || !positions.insert(position) {
                    break;
                }
            }
            input[count] = bound;
            count += 1;
        }
        if count != 0 {
            let packed: [_; INPUT_ROWS] =
                std::array::from_fn(|i| input[i].host.as_ref().map(|n| n.as_slice()));
            let identities: [_; INPUT_ROWS] = std::array::from_fn(|i| {
                packed[i]
                    .filter(|_| input[i].position.is_some())
                    .map(|n| &n[..16])
            });
            let details: [_; INPUT_ROWS] = std::array::from_fn(|i| {
                if input[i].kind == Some(i64::from(InodeKind::Directory.code())) {
                    input[i].path
                } else {
                    packed[i].filter(|_| input[i].position.is_none())
                }
            });
            // One binding allocation and one packed-host view survive the
            // dependency check. Writes bind only the accepted prefix, without
            // clearing/rebuilding the independently classified input rows.
            let absent: Option<&[u8]> = None;
            let mut values: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + 7 * INPUT_ROWS);
            values.push(&operation);
            for i in 0..INPUT_ROWS {
                values.extend([
                    &input[i].parent as &dyn rusqlite::ToSql,
                    &input[i].name,
                    &input[i].position,
                    &input[i].kind,
                    &identities[i],
                ]);
            }
            let bound_bytes = dependency_bytes(&entries[offset..offset + count]);
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
                if input[..count]
                    .iter()
                    .any(|row| row.host.is_some() && row.position.is_some())
                {
                    values.clear();
                    values.push(&operation);
                    for i in 0..count {
                        let row = &input[i];
                        if row.host.is_some() && row.position.is_some() {
                            values.extend([
                                &row.position as &dyn rusqlite::ToSql,
                                &packed[i],
                                &row.path,
                            ]);
                        }
                    }
                    values.resize(1 + 3 * INPUT_ROWS, &absent);
                    host_files.borrowed(
                        &values,
                        host_binding_bytes(&entries[offset..offset + count]),
                    )?;
                }
                values.clear();
                values.push(&operation);
                for i in 0..count {
                    let row = &input[i];
                    values.extend([
                        &row.parent as &dyn rusqlite::ToSql,
                        &row.name,
                        &row.position,
                        &row.kind,
                        &row.metadata,
                        &row.target,
                        &details[i],
                    ]);
                }
                values.resize(1 + 7 * INPUT_ROWS, &absent);
                append.borrowed(
                    &values,
                    entry_binding_bytes(&entries[offset..offset + count]),
                )?;
                for entry in &entries[offset..offset + count] {
                    rows += 1;
                    bytes += entry_bytes(entry);
                    if let (Some(position), Some(_)) = (entry.position, entry.host) {
                        canonical.push(position);
                        rows += 1;
                        bytes +=
                            host_bytes(entry.host_path.as_deref().ok_or(BackendError::Integrity)?);
                    }
                }
                offset += count;
                continue;
            }
        }
        let (first, added_rows, added_bytes) =
            put_one(&mut host_one, &mut entry_one, operation, &entries[offset])?;
        canonical.extend(first);
        rows += added_rows;
        bytes += added_bytes;
        offset += 1;
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// Actual scalar/BLOB bytes bound, excluding NULL padding and literal slots.
fn dependency_bytes(entries: &[NewEntry]) -> u64 {
    8 + entries
        .iter()
        .map(|e| {
            16 + e.position.map_or(0, |_| 8)
                + e.key.name.len() as u64
                + if e.host.is_some() && e.position.is_some() {
                    16
                } else {
                    0
                }
        })
        .sum::<u64>()
}

fn host_binding_bytes(entries: &[NewEntry]) -> u64 {
    8 + entries
        .iter()
        .filter(|e| e.host.is_some() && e.position.is_some())
        .map(|e| 68 + e.host_path.as_ref().map_or(0, |p| p.len() as u64))
        .sum::<u64>()
}

fn entry_binding_bytes(entries: &[NewEntry]) -> u64 {
    8 + entries
        .iter()
        .map(|e| {
            48 + e.position.map_or(0, |_| 8)
                + e.key.name.len() as u64
                + e.target_root.map_or(0, |_| 32)
                + if e.kind == InodeKind::Directory {
                    e.host_path.as_ref().map_or(0, |p| p.len() as u64)
                } else if e.host.is_some() && e.position.is_none() {
                    60
                } else {
                    0
                }
        })
        .sum::<u64>()
}
