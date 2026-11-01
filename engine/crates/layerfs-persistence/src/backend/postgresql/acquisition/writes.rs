//! Bounded write units: begin, append, place, record roots and advance, with
//! the owner check and exact charges every unit shares.
//!
//! A window of entries is appended in array statements once a check over
//! the whole window shows its rows cannot conflict with each other or with
//! stored rows; the first row that might conflict runs alone, in order, so
//! its outcome and failure are exactly a sequential append's.
use super::reads::get;
use crate::backend::{
    postgresql::session::transaction::Transaction,
    records::BackendError,
    sqlite::acquisition::{
        accounting::{entry_bytes, host_bytes, HOST_BYTES, ROOT_BYTES},
        metadata_blob,
        statements::{signed, unsigned, Failure},
        writes::{packed, well_formed},
    },
};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{
    AcquisitionWork, Begin, HostIdentity, NewEntry, Owner, Phase, Placed,
};
use std::collections::BTreeSet;

/// Parent position stored for the source root, which has no parent.
const ROOT_PARENT: i64 = -1;

const OWNER: &str = "SELECT phase,held_rows,held_bytes,peak_rows,peak_bytes,removed_rows,removed_bytes FROM init_operation WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint";
const CHARGE: &str = "UPDATE init_operation SET held_rows=held_rows+$3::bigint,held_bytes=held_bytes+$4::bigint,peak_rows=greatest(peak_rows,held_rows+$3::bigint),peak_bytes=greatest(peak_bytes,held_bytes+$4::bigint) WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING held_rows";
const CREDIT: &str = "UPDATE init_operation SET held_rows=held_rows-$3::bigint,held_bytes=held_bytes-$4::bigint,removed_rows=removed_rows+$3::bigint,removed_bytes=removed_bytes+$4::bigint WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING held_rows";
const BEGIN: &str = "INSERT INTO init_operation(owner_epoch,phase,source_device,source_inode,stack_id,scope) VALUES(coalesce($1::bigint,1),1,$2::bytea,$3::bytea,$4::bytea,$5::bytea) RETURNING operation_id";
const CLAIM_EPOCH: &str =
    "UPDATE init_operation SET owner_epoch=operation_id WHERE operation_id=$1::bigint RETURNING owner_epoch";
const ADVANCE: &str = "UPDATE init_operation SET phase=$3::bigint WHERE operation_id=$1::bigint AND owner_epoch=$2::bigint RETURNING phase";
const BIND_HOST_FILE: &str = "INSERT INTO init_host_file(operation_id,canonical_position,device,inode,evidence,host_path) VALUES($1::bigint,$2::bigint,$3::bytea,$4::bytea,$5::bytea,$6::bytea) ON CONFLICT(operation_id,device,inode) DO UPDATE SET aliases=init_host_file.aliases+1 WHERE init_host_file.evidence=excluded.evidence RETURNING canonical_position";
const PUT_ENTRY: &str = "INSERT INTO init_entry(operation_id,parent_position,name,position,kind,canonical_position,metadata_root,content_root,host_path,host_identity) VALUES($1::bigint,$2::bigint,$3::bytea,$4::bigint,$5::bigint,$6::bigint,$7::bytea,$8::bytea,$9::bytea,$10::bytea)";
const PLACE_ENTRY: &str = "UPDATE init_entry SET position=$4::bigint,canonical_position=$5::bigint,host_identity=NULL WHERE operation_id=$1::bigint AND parent_position=$2::bigint AND name=$3::bytea AND position IS NULL RETURNING kind";
/// The first slot of a window whose row a stored row could refuse or change.
const DEPENDENCIES: &str = "SELECT min(s.slot) FROM unnest($2::bigint[],$3::bigint[],$4::bytea[],$5::bigint[],$6::bigint[],$7::bytea[],$8::bytea[]) AS s(slot,parent,name,position,kind,device,inode) \
 WHERE EXISTS(SELECT 1 FROM init_entry e WHERE e.operation_id=$1::bigint AND e.parent_position=s.parent AND e.name=s.name) \
 OR (s.kind=2 AND s.position IS NOT NULL AND EXISTS(SELECT 1 FROM init_entry e WHERE e.operation_id=$1::bigint AND e.kind=2 AND e.position=s.position)) \
 OR (s.device IS NOT NULL AND (EXISTS(SELECT 1 FROM init_host_file h WHERE h.operation_id=$1::bigint AND h.device=s.device AND h.inode=s.inode) \
  OR EXISTS(SELECT 1 FROM init_host_file h WHERE h.operation_id=$1::bigint AND h.canonical_position=s.position)))";
const PUT_HOST_FILES: &str = "INSERT INTO init_host_file(operation_id,canonical_position,device,inode,evidence,host_path) SELECT $1::bigint,* FROM unnest($2::bigint[],$3::bytea[],$4::bytea[],$5::bytea[],$6::bytea[])";
const PUT_ENTRIES: &str = "INSERT INTO init_entry(operation_id,parent_position,name,position,kind,canonical_position,metadata_root,content_root,host_path,host_identity) SELECT $1::bigint,* FROM unnest($2::bigint[],$3::bytea[],$4::bigint[],$5::bigint[],$6::bigint[],$7::bytea[],$8::bytea[],$9::bytea[],$10::bytea[])";
const FILE_ROOTS: &str = "UPDATE init_host_file AS t SET file_root=r.root FROM unnest($2::bigint[],$3::bytea[]) AS r(position,root) WHERE t.operation_id=$1::bigint AND t.canonical_position=r.position AND t.file_root IS NULL RETURNING t.canonical_position";
const DIRECTORY_ROOTS: &str = "UPDATE init_entry AS t SET content_root=r.root FROM unnest($2::bigint[],$3::bytea[]) AS r(position,root) WHERE t.operation_id=$1::bigint AND t.position=r.position AND t.kind=2 AND t.content_root IS NULL RETURNING t.position";

fn identity(owner: Owner) -> Result<(i64, i64), Failure> {
    Ok((signed(owner.operation)?, signed(owner.epoch)?))
}

/// The operation's phase and charges; `Stale` when the owner is not live.
pub(super) fn owner(
    tx: &Transaction<'_>,
    owner: Owner,
) -> Result<(Phase, AcquisitionWork), Failure> {
    let (operation, epoch) = identity(owner)?;
    let rows = tx.typed(OWNER, &[&operation, &epoch], 16)?;
    let row = rows.first().ok_or(Failure::Stale)?;
    let phase = u8::try_from(get::<i64>(row, 0)?)
        .ok()
        .and_then(Phase::from_code)
        .ok_or(BackendError::Integrity)?;
    let mut counts = [0_u64; 6];
    for (index, count) in counts.iter_mut().enumerate() {
        *count = unsigned(get::<i64>(row, index + 1)?)?;
    }
    let [held_rows, held_bytes, peak_rows, peak_bytes, removed_rows, removed_bytes] = counts;
    Ok((
        phase,
        AcquisitionWork {
            held_rows,
            held_bytes,
            peak_rows,
            peak_bytes,
            removed_rows,
            removed_bytes,
        },
    ))
}

fn adjust(
    tx: &Transaction<'_>,
    sql: &str,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    let (operation, epoch) = identity(owner)?;
    let updated = tx.typed(sql, &[&operation, &epoch, &rows, &bytes], 32)?;
    unsigned(get::<i64>(updated.first().ok_or(Failure::Stale)?, 0)?)
}

/// Adds signed row and byte deltas; returns the rows now held.
pub(super) fn charge(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    adjust(tx, CHARGE, owner, rows, bytes)
}

/// Moves removed rows and bytes out of the held charge; returns rows now held.
pub(super) fn credit(
    tx: &Transaction<'_>,
    owner: Owner,
    rows: i64,
    bytes: i64,
) -> Result<u64, Failure> {
    adjust(tx, CREDIT, owner, rows, bytes)
}

/// Inserts the operation row; the first operation of a session names its epoch.
pub(super) fn begin(tx: &Transaction<'_>, epoch: u64, facts: &Begin) -> Result<Owner, Failure> {
    let known = (epoch != 0).then(|| signed(epoch)).transpose()?;
    let (device, inode) = (
        facts.source_device.to_be_bytes(),
        facts.source_inode.to_be_bytes(),
    );
    let inserted = tx.typed(
        BEGIN,
        &[
            &known,
            &device.as_slice(),
            &inode.as_slice(),
            &facts.stack.as_slice(),
            &facts.scope.as_bytes().as_slice(),
        ],
        72,
    )?;
    let operation: i64 = get(inserted.first().ok_or(BackendError::Integrity)?, 0)?;
    let epoch = match known {
        Some(epoch) => epoch,
        None => get(
            tx.typed(CLAIM_EPOCH, &[&operation], 8)?
                .first()
                .ok_or(BackendError::Integrity)?,
            0,
        )?,
    };
    Ok(Owner {
        operation: unsigned(operation)?,
        epoch: unsigned(epoch)?,
    })
}

/// Binds one positioned path to its host identity and returns the canonical
/// position with whether this path created the identity's row.
fn bind(
    tx: &Transaction<'_>,
    operation: i64,
    position: u64,
    host: &HostIdentity,
    path: &[u8],
) -> Result<(i64, bool), Failure> {
    let own = signed(position)?;
    let (device, inode) = (host.device.to_be_bytes(), host.inode.to_be_bytes());
    let bound = tx.typed(
        BIND_HOST_FILE,
        &[
            &operation,
            &own,
            &device.as_slice(),
            &inode.as_slice(),
            &host.evidence.as_slice(),
            &path,
        ],
        76 + path.len() as u64,
    )?;
    // No row: the identity exists with different evidence and was not changed.
    let canonical: i64 = get(bound.first().ok_or(Failure::Changed(position))?, 0)?;
    Ok((canonical, canonical == own))
}

/// Appends one window of entries and binds its positioned regular files.
pub(super) fn put_entries(
    tx: &Transaction<'_>,
    owner: Owner,
    entries: &[NewEntry],
) -> Result<Vec<u64>, Failure> {
    let operation = signed(owner.operation)?;
    let (mut offset, mut rows, mut bytes) = (0, 0_i64, 0_i64);
    let mut canonical = Vec::new();
    while offset < entries.len() {
        let window = &entries[offset..];
        let mut count = independent(window);
        if count != 0 {
            count = first_dependent(tx, operation, &window[..count])?;
        }
        if count != 0 {
            append(tx, operation, &window[..count])?;
            for entry in &window[..count] {
                rows += 1;
                bytes += entry_bytes(entry);
                if let (Some(position), Some(_)) = (entry.position, entry.host) {
                    canonical.push(position);
                    rows += 1;
                    bytes += host_bytes(entry.host_path.as_deref().ok_or(BackendError::Integrity)?);
                }
            }
            offset += count;
            continue;
        }
        let (first, added_rows, added_bytes) = put_one(tx, operation, &entries[offset])?;
        canonical.extend(first);
        rows += added_rows;
        bytes += added_bytes;
        offset += 1;
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// The leading rows no statement can refuse and no other row of the window
/// can conflict with: shapes and constraints hold, and keys, directory
/// positions, host identities and canonical positions are distinct.
fn independent(entries: &[NewEntry]) -> usize {
    let (mut keys, mut directories, mut identities, mut positions) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    for (count, entry) in entries.iter().enumerate() {
        let path = entry.host_path.as_deref();
        let alone = well_formed(entry)
            && entry.key.name.len() <= 255
            && entry.key.parent.is_none_or(|parent| signed(parent).is_ok())
            && entry
                .position
                .is_none_or(|position| signed(position).is_ok())
            && !path.is_some_and(|path| path.is_empty() || path.len() > 4096)
            && !(entry.kind == InodeKind::RegularFile && entry.position == Some(0));
        if !alone
            || !keys.insert((entry.key.parent, entry.key.name.as_slice()))
            || (entry.kind == InodeKind::Directory
                && entry.position.is_some()
                && !directories.insert(entry.position))
        {
            return count;
        }
        if let (Some(position), Some(host)) = (entry.position, entry.host) {
            if !identities.insert((host.device, host.inode)) || !positions.insert(position) {
                return count;
            }
        }
    }
    entries.len()
}

/// The number of leading rows of an independent window that no stored row
/// conflicts with, by one indexed check in this transaction.
fn first_dependent(
    tx: &Transaction<'_>,
    operation: i64,
    window: &[NewEntry],
) -> Result<usize, Failure> {
    let slots: Vec<i64> = (0..window.len() as i64).collect();
    let parents = parents(window);
    let names: Vec<&[u8]> = window.iter().map(|e| e.key.name.as_slice()).collect();
    let positions: Vec<Option<i64>> = window
        .iter()
        .map(|e| e.position.map(|p| p as i64))
        .collect();
    let kinds: Vec<i64> = window.iter().map(|e| i64::from(e.kind.code())).collect();
    let bound = |e: &NewEntry| e.host.filter(|_| e.position.is_some());
    let devices: Vec<Option<[u8; 8]>> = window
        .iter()
        .map(|e| bound(e).map(|h| h.device.to_be_bytes()))
        .collect();
    let inodes: Vec<Option<[u8; 8]>> = window
        .iter()
        .map(|e| bound(e).map(|h| h.inode.to_be_bytes()))
        .collect();
    let devices: Vec<Option<&[u8]>> = devices.iter().map(|d| d.as_ref().map(|d| &d[..])).collect();
    let inodes: Vec<Option<&[u8]>> = inodes.iter().map(|d| d.as_ref().map(|d| &d[..])).collect();
    let bytes = 8 + names.iter().map(|n| 48 + n.len() as u64).sum::<u64>();
    let found = tx.typed(
        DEPENDENCIES,
        &[
            &operation, &slots, &parents, &names, &positions, &kinds, &devices, &inodes,
        ],
        bytes,
    )?;
    let first: Option<i64> = get(found.first().ok_or(BackendError::Integrity)?, 0)?;
    match first {
        None => Ok(window.len()),
        Some(slot) => usize::try_from(slot)
            .ok()
            .filter(|slot| *slot < window.len())
            .ok_or(Failure::Backend(BackendError::Integrity)),
    }
}

fn parents(window: &[NewEntry]) -> Vec<i64> {
    window
        .iter()
        .map(|e| e.key.parent.map_or(ROOT_PARENT, |p| p as i64))
        .collect()
}

/// Inserts an independent window: its first-path host files, then its entries.
fn append(tx: &Transaction<'_>, operation: i64, window: &[NewEntry]) -> Result<(), Failure> {
    let hosts: Vec<(i64, [u8; HOST_BYTES as usize], &[u8])> = window
        .iter()
        .filter_map(|e| match (e.position, e.host, e.host_path.as_deref()) {
            (Some(position), Some(host), Some(path)) => Some((position as i64, packed(host), path)),
            _ => None,
        })
        .collect();
    if !hosts.is_empty() {
        let positions: Vec<i64> = hosts.iter().map(|h| h.0).collect();
        let devices: Vec<&[u8]> = hosts.iter().map(|h| &h.1[..8]).collect();
        let inodes: Vec<&[u8]> = hosts.iter().map(|h| &h.1[8..16]).collect();
        let evidence: Vec<&[u8]> = hosts.iter().map(|h| &h.1[16..]).collect();
        let paths: Vec<&[u8]> = hosts.iter().map(|h| h.2).collect();
        let bytes = 8 + hosts.iter().map(|h| 68 + h.2.len() as u64).sum::<u64>();
        tx.typed(
            PUT_HOST_FILES,
            &[&operation, &positions, &devices, &inodes, &evidence, &paths],
            bytes,
        )?;
    }
    let regular = |e: &NewEntry| e.kind == InodeKind::RegularFile;
    let names: Vec<&[u8]> = window.iter().map(|e| e.key.name.as_slice()).collect();
    let positions: Vec<Option<i64>> = window
        .iter()
        .map(|e| e.position.map(|p| p as i64))
        .collect();
    let kinds: Vec<i64> = window.iter().map(|e| i64::from(e.kind.code())).collect();
    let canonical: Vec<Option<i64>> = window
        .iter()
        .map(|e| e.position.filter(|_| regular(e)).map(|p| p as i64))
        .collect();
    let metadata: Vec<[u8; 32]> = window.iter().map(|e| metadata_blob(&e.metadata)).collect();
    let metadata: Vec<&[u8]> = metadata.iter().map(|m| &m[..]).collect();
    let targets: Vec<Option<&[u8]>> = window
        .iter()
        .map(|e| {
            e.target_root
                .as_ref()
                .map(|root| root.as_bytes().as_slice())
        })
        .collect();
    let paths: Vec<Option<&[u8]>> = window
        .iter()
        .map(|e| {
            e.host_path
                .as_deref()
                .filter(|_| e.kind == InodeKind::Directory)
        })
        .collect();
    let unplaced: Vec<Option<[u8; HOST_BYTES as usize]>> = window
        .iter()
        .map(|e| e.host.filter(|_| e.position.is_none()).map(packed))
        .collect();
    let unplaced: Vec<Option<&[u8]>> = unplaced
        .iter()
        .map(|h| h.as_ref().map(|h| &h[..]))
        .collect();
    let bytes = 8 + window
        .iter()
        .map(|e| 48 + entry_bytes(e) as u64)
        .sum::<u64>();
    tx.typed(
        PUT_ENTRIES,
        &[
            &operation,
            &parents(window),
            &names,
            &positions,
            &kinds,
            &canonical,
            &metadata,
            &targets,
            &paths,
            &unplaced,
        ],
        bytes,
    )?;
    Ok(())
}

/// One entry alone, in the original host-then-entry order.
fn put_one(
    tx: &Transaction<'_>,
    operation: i64,
    entry: &NewEntry,
) -> Result<(Option<u64>, i64, i64), Failure> {
    let (mut rows, mut bytes) = (0_i64, 0_i64);
    let parent = match entry.key.parent {
        Some(parent) => signed(parent)?,
        None => ROOT_PARENT,
    };
    let position = entry.position.map(signed).transpose()?;
    let path = entry.host_path.as_deref();
    if !well_formed(entry) {
        return Err(Failure::Bounds);
    }
    let mut first = None;
    if let (Some(own), Some(host), Some(path)) = (entry.position, &entry.host, path) {
        let (bound, created) = bind(tx, operation, own, host, path)?;
        first = Some(bound);
        if created {
            rows += 1;
            bytes += host_bytes(path);
        }
    }
    // A regular file's path lives with its identity; only directories keep one.
    let stored_path = path.filter(|_| entry.kind == InodeKind::Directory);
    let unplaced = entry.host.filter(|_| entry.position.is_none()).map(packed);
    tx.typed(
        PUT_ENTRY,
        &[
            &operation,
            &parent,
            &entry.key.name.as_slice(),
            &position,
            &i64::from(entry.kind.code()),
            &first,
            &metadata_blob(&entry.metadata).as_slice(),
            &entry
                .target_root
                .as_ref()
                .map(|root| root.as_bytes().as_slice()),
            &stored_path,
            &unplaced.as_ref().map(|host| host.as_slice()),
        ],
        entry_bytes(entry) as u64 + 40,
    )?;
    rows += 1;
    bytes += entry_bytes(entry);
    Ok((first.map(unsigned).transpose()?, rows, bytes))
}

/// Assigns positions to one window of unpositioned children of `parent`.
pub(super) fn place_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    placed: &[Placed],
) -> Result<Vec<u64>, Failure> {
    let (operation, parent) = (signed(owner.operation)?, signed(parent)?);
    let (mut rows, mut bytes) = (0_i64, 0_i64);
    let mut canonical = Vec::new();
    for child in placed {
        let position = signed(child.position)?;
        let mut first = None;
        if let Some((host, path)) = &child.host {
            let (bound, created) = bind(tx, operation, child.position, host, path)?;
            first = Some(bound);
            canonical.push(unsigned(bound)?);
            if created {
                rows += 1;
                bytes += host_bytes(path);
            }
            bytes -= HOST_BYTES;
        }
        let updated = tx.typed(
            PLACE_ENTRY,
            &[
                &operation,
                &parent,
                &child.name.as_slice(),
                &position,
                &first,
            ],
            32 + child.name.len() as u64,
        )?;
        // The child must exist unpositioned, and be regular exactly when bound.
        let kind: i64 = get(updated.first().ok_or(Failure::Changed(child.position))?, 0)?;
        let regular = kind == i64::from(InodeKind::RegularFile.code());
        if regular != child.host.is_some() {
            return Err(Failure::Changed(child.position));
        }
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// Which roots a window records.
#[derive(Clone, Copy)]
pub(super) enum Roots {
    /// Constructed file roots, by canonical position.
    Files,
    /// Directory content roots, by directory position.
    Directories,
}

/// Records each root once. A window stops before a repeated position, so
/// the repeat meets its already-filled row as a sequential update would; an
/// earlier missing row still wins.
pub(super) fn record_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    which: Roots,
    roots: &[(u64, ObjectId)],
) -> Result<(), Failure> {
    let operation = signed(owner.operation)?;
    let sql = match which {
        Roots::Files => FILE_ROOTS,
        Roots::Directories => DIRECTORY_ROOTS,
    };
    let mut offset = 0;
    while offset < roots.len() {
        let (mut positions, mut objects) = (Vec::new(), Vec::new());
        let mut distinct = BTreeSet::new();
        for (position, object) in &roots[offset..] {
            let position = match signed(*position) {
                Ok(position) => position,
                Err(_) if !positions.is_empty() => break,
                Err(error) => return Err(error),
            };
            if !distinct.insert(position) {
                break;
            }
            positions.push(position);
            objects.push(object.as_bytes().as_slice());
        }
        let count = positions.len();
        let mut recorded = tx
            .typed(
                sql,
                &[&operation, &positions, &objects],
                8 + 40 * count as u64,
            )?
            .iter()
            .map(|row| get::<i64>(row, 0))
            .collect::<Result<Vec<_>, _>>()?;
        // RETURNING order is not input order; check identities, not a count.
        recorded.sort_unstable();
        for (position, _) in &roots[offset..offset + count] {
            if recorded.binary_search(&signed(*position)?).is_err() {
                return Err(Failure::Changed(*position));
            }
        }
        offset += count;
    }
    charge(tx, owner, 0, roots.len() as i64 * ROOT_BYTES)?;
    Ok(())
}

pub(super) fn advance(tx: &Transaction<'_>, owner: Owner, phase: Phase) -> Result<(), Failure> {
    let (operation, epoch) = identity(owner)?;
    let updated = tx.typed(ADVANCE, &[&operation, &epoch, &i64::from(phase.code())], 24)?;
    if updated.is_empty() {
        return Err(Failure::Stale);
    }
    Ok(())
}
