//! Bounded keyset reads with direct typed row mapping.
//!
//! Single-statement reads carry the live owner as the outer SQL relation. A
//! valid empty relation returns one NULL payload sentinel; an absent owner
//! returns no rows. Two-pass path windows retain their explicit owned snapshot.
use super::statements::{
    signed, Failure, BINDINGS_FIRST, BINDINGS_NEXT, DIRECTORIES, DIRECTORY_PATH, DIRECTORY_SIZES,
    ENTRIES_FIRST, ENTRIES_NEXT, FILE_ROOTS, JOB, JOBS, JOB_SIZES, UNPLACED_FIRST, UNPLACED_NEXT,
};
use crate::backend::{records::BackendError, Session, Transaction};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{
    Binding, Directory, Entry, EntryKey, FileRoot, Job, Limits, NativeIdentity, Owner, Phase,
    Unplaced, EVIDENCE_BYTES,
};
use rusqlite::{types::FromSql, Row, ToSql};

/// Largest public column bytes one row of each read can carry.
const ENTRY_ROW_BYTES: usize = 255 + 64 + 40;
const BINDING_ROW_BYTES: usize = 255 + 24;
const UNPLACED_ROW_BYTES: usize = 255 + 60 + 8;
const FILE_ROOT_ROW_BYTES: usize = 48;

fn window(limits: Limits, row_bytes: usize) -> i64 {
    let limits = limits.clamped();
    limits.rows.min((limits.bytes / row_bytes).max(1)) as i64
}

fn get<T: FromSql>(row: &Row<'_>, index: usize) -> Result<T, BackendError> {
    row.get(index).map_err(super::super::rows::error)
}

fn number(value: i64) -> Result<u64, BackendError> {
    u64::try_from(value).map_err(|_| BackendError::Integrity)
}

fn kind(row: &Row<'_>, index: usize) -> Result<InodeKind, BackendError> {
    let code = u8::try_from(get::<i64>(row, index)?).map_err(|_| BackendError::Integrity)?;
    InodeKind::from_code(code).map_err(|_| BackendError::Integrity)
}

fn root(row: &Row<'_>, index: usize) -> Result<ObjectId, BackendError> {
    ObjectId::from_bytes(super::super::rows::blob(row, index)?).map_err(|_| BackendError::Integrity)
}

fn optional_root(row: &Row<'_>, index: usize) -> Result<Option<ObjectId>, BackendError> {
    match row.get_ref(index).map_err(super::super::rows::error)? {
        rusqlite::types::ValueRef::Null => Ok(None),
        _ => root(row, index).map(Some),
    }
}

fn exact<const N: usize>(bytes: &[u8]) -> Result<[u8; N], BackendError> {
    <[u8; N]>::try_from(bytes).map_err(|_| BackendError::Integrity)
}

fn native(device: &[u8], inode: &[u8], evidence: &[u8]) -> Result<NativeIdentity, BackendError> {
    Ok(NativeIdentity {
        device: u64::from_be_bytes(exact(device)?),
        inode: u64::from_be_bytes(exact(inode)?),
        evidence: exact::<EVIDENCE_BYTES>(evidence)?,
    })
}

fn after(position: Option<u64>) -> Result<i64, Failure> {
    position.map_or(Ok(-1), signed)
}

/// Preserve the existing owner-before-input-refusal order on malformed cursor
/// values. The normal path has no additional SQL or framing.
fn checked(session: &Session, owner: Owner, value: Result<i64, Failure>) -> Result<i64, Failure> {
    match value {
        Ok(value) => Ok(value),
        Err(error) => session.run(false, |tx| {
            super::accounting::owner(tx, owner)?;
            Err(error)
        }),
    }
}

/// `sql` must drive its payload LEFT JOIN from the operation's PK/epoch match.
/// Column zero is the phase, and column one is a non-null payload key unless
/// the valid owner has an empty window. A stale owner never accesses payload.
fn owned<T>(
    session: &Session,
    sql: &str,
    values: &[&dyn ToSql],
    bytes: u64,
    capacity: usize,
    mut decode: impl FnMut(&Row<'_>) -> Result<T, BackendError>,
) -> Result<Vec<T>, Failure> {
    let mut live = false;
    let rows = session.read_mapped(sql, values, bytes, capacity, |row| {
        let phase = u8::try_from(get::<i64>(row, 0)?).map_err(|_| BackendError::Integrity)?;
        Phase::from_code(phase).ok_or(BackendError::Integrity)?;
        live = true;
        if matches!(
            row.get_ref(1).map_err(super::super::rows::error)?,
            rusqlite::types::ValueRef::Null
        ) {
            Ok(None)
        } else {
            decode(row).map(Some).map_err(Failure::from)
        }
    })?;
    if !live {
        return Err(Failure::Stale);
    }
    Ok(rows)
}

/// Inspect only lengths for at most 512 indexed keys, then fetch the prefix
/// whose actual columns fit. Both statements share the checked transaction.
fn path_window(
    tx: &Transaction<'_>,
    sql: &str,
    operation: i64,
    after: i64,
    limits: Limits,
) -> Result<i64, Failure> {
    let limits = limits.clamped();
    let sizes = tx.mapped(
        sql,
        &[&operation, &after, &(limits.rows as i64)],
        24,
        limits.rows,
        |row| get::<i64>(row, 0),
    )?;
    let (mut rows, mut bytes) = (0_i64, 0_usize);
    for size in sizes {
        let size = usize::try_from(size).map_err(|_| BackendError::Integrity)?;
        let next = bytes.checked_add(size).ok_or(BackendError::Capacity)?;
        if rows != 0 && next > limits.bytes {
            break;
        }
        rows += 1;
        bytes = next;
    }
    Ok(rows)
}

pub(crate) fn unplaced_children(
    session: &Session,
    owner: Owner,
    parent: u64,
    after_name: Option<&[u8]>,
    limits: Limits,
) -> Result<Vec<Unplaced>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let parent = checked(session, owner, signed(parent))?;
    let limit = window(limits, UNPLACED_ROW_BYTES);
    let decode = |row: &Row<'_>| {
        let identity = match row.get_ref(3).map_err(super::super::rows::error)? {
            rusqlite::types::ValueRef::Null => None,
            _ => {
                let packed: [u8; 60] = exact(super::super::rows::blob(row, 3)?)?;
                Some(native(&packed[..8], &packed[8..16], &packed[16..])?)
            }
        };
        Ok(Unplaced {
            name: super::super::rows::blob(row, 1)?.to_vec(),
            kind: kind(row, 2)?,
            native: identity,
        })
    };
    match after_name {
        None => owned(
            session,
            UNPLACED_FIRST,
            &[&operation, &epoch, &parent, &limit],
            32,
            limit as usize,
            decode,
        ),
        Some(name) => owned(
            session,
            UNPLACED_NEXT,
            &[&operation, &epoch, &parent, &name, &limit],
            32 + name.len() as u64,
            limit as usize,
            decode,
        ),
    }
}

pub(crate) fn directories(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Directory>, Failure> {
    let (operation, after) = (signed(owner.operation)?, after(after_position)?);
    let limit = path_window(tx, DIRECTORY_SIZES, operation, after, limits)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    Ok(tx.mapped(
        DIRECTORIES,
        &[&operation, &after, &limit],
        24,
        limit as usize,
        |row| {
            Ok(Directory {
                position: number(get(row, 0)?)?,
                native_path: super::super::rows::blob(row, 1)?.to_vec(),
            })
        },
    )?)
}

pub(crate) fn directory_path(
    session: &Session,
    owner: Owner,
    position: u64,
) -> Result<Option<Vec<u8>>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let position = checked(session, owner, signed(position))?;
    let rows = owned(
        session,
        DIRECTORY_PATH,
        &[&operation, &epoch, &position],
        24,
        1,
        |row| Ok(super::super::rows::blob(row, 1)?.to_vec()),
    )?;
    Ok(rows.into_iter().next())
}

fn job_row(row: &Row<'_>, start: usize) -> Result<Job, BackendError> {
    Ok(Job {
        position: number(get(row, start)?)?,
        aliases: number(get(row, start + 5)?)?,
        native: native(
            super::super::rows::blob(row, start + 1)?,
            super::super::rows::blob(row, start + 2)?,
            super::super::rows::blob(row, start + 3)?,
        )?,
        native_path: super::super::rows::blob(row, start + 4)?.to_vec(),
    })
}

pub(crate) fn jobs(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Job>, Failure> {
    let (operation, after) = (signed(owner.operation)?, after(after_position)?);
    let limit = path_window(tx, JOB_SIZES, operation, after, limits)?;
    if limit == 0 {
        return Ok(Vec::new());
    }
    Ok(tx.mapped(
        JOBS,
        &[&operation, &after, &limit],
        24,
        limit as usize,
        |row| job_row(row, 0),
    )?)
}

pub(crate) fn job(session: &Session, owner: Owner, position: u64) -> Result<Option<Job>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let position = checked(session, owner, signed(position))?;
    let rows = owned(
        session,
        JOB,
        &[&operation, &epoch, &position],
        24,
        1,
        |row| job_row(row, 1),
    )?;
    Ok(rows.into_iter().next())
}

pub(crate) fn file_roots(
    session: &Session,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<FileRoot>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let after = checked(session, owner, after(after_position))?;
    let limit = window(limits, FILE_ROOT_ROW_BYTES);
    owned(
        session,
        FILE_ROOTS,
        &[&operation, &epoch, &after, &limit],
        32,
        limit as usize,
        |row| {
            Ok(FileRoot {
                position: number(get(row, 1)?)?,
                aliases: number(get(row, 2)?)?,
                root: optional_root(row, 3)?,
            })
        },
    )
}

fn entry_row(row: &Row<'_>) -> Result<Entry, BackendError> {
    let parent: i64 = get(row, 1)?;
    let position = get::<Option<i64>>(row, 3)?.ok_or(BackendError::Integrity)?;
    Ok(Entry {
        key: EntryKey {
            parent: (parent >= 0).then(|| number(parent)).transpose()?,
            name: super::super::rows::blob(row, 2)?.to_vec(),
        },
        position: number(position)?,
        kind: kind(row, 4)?,
        canonical: get::<Option<i64>>(row, 5)?.map(number).transpose()?,
        metadata_root: root(row, 6)?,
        content_root: optional_root(row, 7)?,
    })
}

pub(crate) fn entries(
    session: &Session,
    owner: Owner,
    after_key: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Entry>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let limit = window(limits, ENTRY_ROW_BYTES);
    match after_key {
        None => owned(
            session,
            ENTRIES_FIRST,
            &[&operation, &epoch, &limit],
            24,
            limit as usize,
            entry_row,
        ),
        Some(key) => {
            let parent = checked(session, owner, key.parent.map_or(Ok(-1), signed))?;
            owned(
                session,
                ENTRIES_NEXT,
                &[&operation, &epoch, &parent, &key.name.as_slice(), &limit],
                32 + key.name.len() as u64,
                limit as usize,
                entry_row,
            )
        }
    }
}

pub(crate) fn bindings(
    session: &Session,
    owner: Owner,
    after_key: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Binding>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let limit = window(limits, BINDING_ROW_BYTES);
    let decode = |row: &Row<'_>| {
        let parent: i64 = get(row, 1)?;
        let position = get::<Option<i64>>(row, 3)?.ok_or(BackendError::Integrity)?;
        Ok(Binding {
            key: EntryKey {
                parent: (parent >= 0).then(|| number(parent)).transpose()?,
                name: super::super::rows::blob(row, 2)?.to_vec(),
            },
            position: number(position)?,
            canonical: get::<Option<i64>>(row, 4)?.map(number).transpose()?,
        })
    };
    match after_key {
        None => owned(
            session,
            BINDINGS_FIRST,
            &[&operation, &epoch, &limit],
            24,
            limit as usize,
            decode,
        ),
        Some(key) => {
            let parent = checked(session, owner, key.parent.map_or(Ok(-1), signed))?;
            owned(
                session,
                BINDINGS_NEXT,
                &[&operation, &epoch, &parent, &key.name.as_slice(), &limit],
                32 + key.name.len() as u64,
                limit as usize,
                decode,
            )
        }
    }
}
