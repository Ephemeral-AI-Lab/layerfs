//! Bounded keyset reads of one live operation, inside the provider's read
//! transaction after its owner check. Every window is an index range scan
//! in key order, so a window's cost does not grow with the operation.
use crate::backend::{
    postgresql::session::{rows, transaction::Transaction},
    records::BackendError,
    sqlite::acquisition::{
        metadata_parse,
        statements::{signed, Failure},
    },
};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{
    Binding, Directory, Entry, EntryKey, FileRoot, HostIdentity, Job, Limits, Owner, Unplaced,
    EVIDENCE_BYTES,
};
use postgres::{types::FromSql, Row};

/// Largest public column bytes one row of each read can carry.
const ENTRY_ROW_BYTES: usize = 255 + 64 + 40;
const BINDING_ROW_BYTES: usize = 255 + 24;
const UNPLACED_ROW_BYTES: usize = 255 + 60 + 8;
const FILE_ROOT_ROW_BYTES: usize = 48;

const UNPLACED: &str = "SELECT name,kind,host_identity FROM init_entry WHERE operation_id=$1::bigint AND parent_position=$2::bigint AND position IS NULL AND name>$3::bytea ORDER BY name LIMIT $4::bigint";
const DIRECTORY_SIZES: &str = "SELECT (octet_length(host_path)+8)::bigint FROM init_entry WHERE operation_id=$1::bigint AND kind=2 AND position>$2::bigint ORDER BY position LIMIT $3::bigint";
const DIRECTORIES: &str = "SELECT position,host_path FROM init_entry WHERE operation_id=$1::bigint AND kind=2 AND position>$2::bigint ORDER BY position LIMIT $3::bigint";
const DIRECTORY_PATH: &str = "SELECT host_path FROM init_entry WHERE operation_id=$1::bigint AND kind=2 AND position=$2::bigint";
const JOB_SIZES: &str = "SELECT (octet_length(host_path)+68)::bigint FROM init_host_file WHERE operation_id=$1::bigint AND canonical_position>$2::bigint ORDER BY canonical_position LIMIT $3::bigint";
const JOBS: &str = "SELECT canonical_position,device,inode,evidence,host_path FROM init_host_file WHERE operation_id=$1::bigint AND canonical_position>$2::bigint ORDER BY canonical_position LIMIT $3::bigint";
const JOB: &str = "SELECT canonical_position,device,inode,evidence,host_path FROM init_host_file WHERE operation_id=$1::bigint AND canonical_position=$2::bigint";
const FILE_ROOTS: &str = "SELECT canonical_position,aliases,file_root FROM init_host_file WHERE operation_id=$1::bigint AND canonical_position>$2::bigint ORDER BY canonical_position LIMIT $3::bigint";
const ENTRIES: &str = "SELECT parent_position,name,position,kind,canonical_position,metadata_root,content_root FROM init_entry WHERE operation_id=$1::bigint AND (parent_position,name)>($2::bigint,$3::bytea) ORDER BY parent_position,name LIMIT $4::bigint";
const BINDINGS: &str = "SELECT parent_position,name,position,canonical_position FROM init_entry WHERE operation_id=$1::bigint AND (parent_position,name)>($2::bigint,$3::bytea) ORDER BY parent_position,name LIMIT $4::bigint";

/// One column, with a conversion failure as `Integrity`.
pub(super) fn get<'a, T: FromSql<'a>>(row: &'a Row, index: usize) -> Result<T, BackendError> {
    row.try_get(index).map_err(|e| rows::error(&e))
}

fn window(limits: Limits, row_bytes: usize) -> i64 {
    let limits = limits.clamped();
    limits.rows.min((limits.bytes / row_bytes).max(1)) as i64
}

fn number(value: i64) -> Result<u64, BackendError> {
    u64::try_from(value).map_err(|_| BackendError::Integrity)
}

fn kind(row: &Row, index: usize) -> Result<InodeKind, BackendError> {
    let code = u8::try_from(get::<i64>(row, index)?).map_err(|_| BackendError::Integrity)?;
    InodeKind::from_code(code).map_err(|_| BackendError::Integrity)
}

fn optional_root(row: &Row, index: usize) -> Result<Option<ObjectId>, BackendError> {
    get::<Option<&[u8]>>(row, index)?
        .map(|bytes| ObjectId::from_bytes(bytes).map_err(|_| BackendError::Integrity))
        .transpose()
}

fn exact<const N: usize>(bytes: &[u8]) -> Result<[u8; N], BackendError> {
    <[u8; N]>::try_from(bytes).map_err(|_| BackendError::Integrity)
}

fn host(device: &[u8], inode: &[u8], evidence: &[u8]) -> Result<HostIdentity, BackendError> {
    Ok(HostIdentity {
        device: u64::from_be_bytes(exact(device)?),
        inode: u64::from_be_bytes(exact(inode)?),
        evidence: exact::<EVIDENCE_BYTES>(evidence)?,
    })
}

fn after(position: Option<u64>) -> Result<i64, Failure> {
    position.map_or(Ok(-1), signed)
}

/// The key a window resumes after; the source root sorts first of all.
fn after_key(key: Option<&EntryKey>) -> Result<(i64, &[u8]), Failure> {
    match key {
        None => Ok((-2, &[])),
        Some(key) => Ok((key.parent.map_or(Ok(-1), signed)?, key.name.as_slice())),
    }
}

/// The leading rows of a path window whose actual columns fit `limits`.
fn path_window(
    tx: &Transaction<'_>,
    sql: &str,
    operation: i64,
    after: i64,
    limits: Limits,
) -> Result<i64, Failure> {
    let limits = limits.clamped();
    let sizes = tx.typed(sql, &[&operation, &after, &(limits.rows as i64)], 24)?;
    let (mut rows, mut bytes) = (0_i64, 0_usize);
    for size in &sizes {
        let size = usize::try_from(get::<i64>(size, 0)?).map_err(|_| BackendError::Integrity)?;
        let next = bytes.checked_add(size).ok_or(BackendError::Capacity)?;
        if rows != 0 && next > limits.bytes {
            break;
        }
        rows += 1;
        bytes = next;
    }
    Ok(rows)
}

pub(super) fn unplaced_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    after_name: Option<&[u8]>,
    limits: Limits,
) -> Result<Vec<Unplaced>, Failure> {
    let (operation, parent) = (signed(owner.operation)?, signed(parent)?);
    let limit = window(limits, UNPLACED_ROW_BYTES);
    // A child's name is never empty, so the empty name precedes every one.
    let after_name = after_name.unwrap_or(&[]);
    let found = tx.typed(
        UNPLACED,
        &[&operation, &parent, &after_name, &limit],
        32 + after_name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let identity = match get::<Option<&[u8]>>(row, 2)? {
            None => None,
            Some(packed) => {
                let packed: [u8; 60] = exact(packed)?;
                Some(host(&packed[..8], &packed[8..16], &packed[16..])?)
            }
        };
        out.push(Unplaced {
            name: get::<Vec<u8>>(row, 0)?,
            kind: kind(row, 1)?,
            host: identity,
        });
    }
    Ok(out)
}

pub(super) fn directories(
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
    let found = tx.typed(DIRECTORIES, &[&operation, &after, &limit], 24)?;
    found
        .iter()
        .map(|row| {
            Ok(Directory {
                position: number(get(row, 0)?)?,
                host_path: get(row, 1)?,
            })
        })
        .collect()
}

pub(super) fn directory_path(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Vec<u8>>, Failure> {
    let (operation, position) = (signed(owner.operation)?, signed(position)?);
    let found = tx.typed(DIRECTORY_PATH, &[&operation, &position], 16)?;
    Ok(match found.first() {
        Some(row) => Some(get(row, 0)?),
        None => None,
    })
}

fn job_row(row: &Row) -> Result<Job, BackendError> {
    Ok(Job {
        position: number(get(row, 0)?)?,
        host: host(get(row, 1)?, get(row, 2)?, get(row, 3)?)?,
        host_path: get(row, 4)?,
    })
}

pub(super) fn jobs(
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
    let found = tx.typed(JOBS, &[&operation, &after, &limit], 24)?;
    Ok(found.iter().map(job_row).collect::<Result<_, _>>()?)
}

pub(super) fn job(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Job>, Failure> {
    let (operation, position) = (signed(owner.operation)?, signed(position)?);
    let found = tx.typed(JOB, &[&operation, &position], 16)?;
    Ok(found.first().map(job_row).transpose()?)
}

pub(super) fn file_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<FileRoot>, Failure> {
    let (operation, after) = (signed(owner.operation)?, after(after_position)?);
    let limit = window(limits, FILE_ROOT_ROW_BYTES);
    let found = tx.typed(FILE_ROOTS, &[&operation, &after, &limit], 24)?;
    found
        .iter()
        .map(|row| {
            Ok(FileRoot {
                position: number(get(row, 0)?)?,
                aliases: number(get(row, 1)?)?,
                root: optional_root(row, 2)?,
            })
        })
        .collect()
}

fn key(row: &Row) -> Result<(EntryKey, u64), BackendError> {
    let parent: i64 = get(row, 0)?;
    let position = get::<Option<i64>>(row, 2)?.ok_or(BackendError::Integrity)?;
    Ok((
        EntryKey {
            parent: (parent >= 0).then(|| number(parent)).transpose()?,
            name: get(row, 1)?,
        },
        number(position)?,
    ))
}

pub(super) fn entries(
    tx: &Transaction<'_>,
    owner: Owner,
    after: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Entry>, Failure> {
    let operation = signed(owner.operation)?;
    let (parent, name) = after_key(after)?;
    let limit = window(limits, ENTRY_ROW_BYTES);
    let found = tx.typed(
        ENTRIES,
        &[&operation, &parent, &name, &limit],
        32 + name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let (key, position) = key(row)?;
        let kind = kind(row, 3)?;
        out.push(Entry {
            key,
            position,
            kind,
            canonical: get::<Option<i64>>(row, 4)?.map(number).transpose()?,
            metadata: metadata_parse(get(row, 5)?, kind)?,
            content_root: optional_root(row, 6)?,
        });
    }
    Ok(out)
}

pub(super) fn bindings(
    tx: &Transaction<'_>,
    owner: Owner,
    after: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Binding>, Failure> {
    let operation = signed(owner.operation)?;
    let (parent, name) = after_key(after)?;
    let limit = window(limits, BINDING_ROW_BYTES);
    let found = tx.typed(
        BINDINGS,
        &[&operation, &parent, &name, &limit],
        32 + name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let (key, position) = key(row)?;
        out.push(Binding {
            key,
            position,
            canonical: get::<Option<i64>>(row, 3)?.map(number).transpose()?,
        });
    }
    Ok(out)
}
