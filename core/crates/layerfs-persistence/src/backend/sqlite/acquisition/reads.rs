//! Bounded keyset read units in acquisition, position and name order.
//!
//! A window's row count is derived from the byte limit and the largest row its
//! statement can return, so the byte bound holds without fetching past it.
use super::statements::{
    signed, unsigned, Failure, DIRECTORIES, DIRECTORY_PATH, ENTRIES_FIRST, ENTRIES_NEXT,
    FILE_ROOTS, JOB, JOBS, UNPLACED_FIRST, UNPLACED_NEXT,
};
use crate::backend::{
    records::{BackendError, Record},
    Transaction,
};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{
    Directory, Entry, EntryKey, FileRoot, Job, Limits, NativeIdentity, Owner, Unplaced,
    EVIDENCE_BYTES,
};

/// Largest column bytes one row of each read can carry.
const ENTRY_ROW_BYTES: usize = 255 + 64 + 40;
const UNPLACED_ROW_BYTES: usize = 255 + 60 + 8;
const PATH_ROW_BYTES: usize = 4096 + 60 + 16;
const FILE_ROOT_ROW_BYTES: usize = 48;

/// Rows a window may fetch: the row limit, held within the byte limit.
fn window(limits: Limits, row_bytes: usize) -> i64 {
    let limits = limits.clamped();
    limits.rows.min((limits.bytes / row_bytes).max(1)) as i64
}

/// The position after which a keyset window starts; before the first when absent.
fn after(position: Option<u64>) -> Result<i64, Failure> {
    position.map_or(Ok(-1), signed)
}

fn kind(row: &Record, index: usize) -> Result<InodeKind, Failure> {
    let code = u8::try_from(row.get::<i64>(index)?).map_err(|_| BackendError::Integrity)?;
    Ok(InodeKind::from_code(code).map_err(|_| BackendError::Integrity)?)
}

fn root(bytes: &[u8]) -> Result<ObjectId, Failure> {
    Ok(ObjectId::from_bytes(bytes).map_err(|_| BackendError::Integrity)?)
}

fn optional_root(row: &Record, index: usize) -> Result<Option<ObjectId>, Failure> {
    row.get::<Option<Vec<u8>>>(index)?
        .map(|bytes| root(&bytes))
        .transpose()
}

fn exact<const N: usize>(bytes: &[u8]) -> Result<[u8; N], Failure> {
    Ok(<[u8; N]>::try_from(bytes).map_err(|_| BackendError::Integrity)?)
}

fn native(device: &[u8], inode: &[u8], evidence: &[u8]) -> Result<NativeIdentity, Failure> {
    Ok(NativeIdentity {
        device: u64::from_be_bytes(exact(device)?),
        inode: u64::from_be_bytes(exact(inode)?),
        evidence: exact::<EVIDENCE_BYTES>(evidence)?,
    })
}

pub(crate) fn unplaced_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    after_name: Option<&[u8]>,
    limits: Limits,
) -> Result<Vec<Unplaced>, Failure> {
    let (operation, parent) = (signed(owner.operation)?, signed(parent)?);
    let limit = window(limits, UNPLACED_ROW_BYTES);
    let rows = match after_name {
        None => tx.borrowed(UNPLACED_FIRST, &[&operation, &parent, &limit], 24)?,
        Some(name) => tx.borrowed(
            UNPLACED_NEXT,
            &[&operation, &parent, &name, &limit],
            24 + name.len() as u64,
        )?,
    };
    rows.into_iter()
        .map(|mut row| {
            let stored = row.get::<Option<Vec<u8>>>(2)?;
            let identity = stored
                .map(|packed| {
                    let packed: [u8; 60] = exact(&packed)?;
                    native(&packed[..8], &packed[8..16], &packed[16..])
                })
                .transpose()?;
            Ok(Unplaced {
                kind: kind(&row, 1)?,
                native: identity,
                name: row.take_bytes(0)?,
            })
        })
        .collect()
}

pub(crate) fn directories(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Directory>, Failure> {
    let rows = tx.borrowed(
        DIRECTORIES,
        &[
            &signed(owner.operation)?,
            &after(after_position)?,
            &window(limits, PATH_ROW_BYTES),
        ],
        24,
    )?;
    rows.into_iter()
        .map(|mut row| {
            Ok(Directory {
                position: unsigned(row.get::<i64>(0)?)?,
                native_path: row.take_bytes(1)?,
            })
        })
        .collect()
}

pub(crate) fn directory_path(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Vec<u8>>, Failure> {
    let mut rows = tx.borrowed(
        DIRECTORY_PATH,
        &[&signed(owner.operation)?, &signed(position)?],
        16,
    )?;
    rows.first_mut()
        .map(|row| row.take_bytes(0).map_err(Failure::from))
        .transpose()
}

fn job_row(mut row: Record) -> Result<Job, Failure> {
    Ok(Job {
        position: unsigned(row.get::<i64>(0)?)?,
        native: native(
            &row.get::<Vec<u8>>(1)?,
            &row.get::<Vec<u8>>(2)?,
            &row.get::<Vec<u8>>(3)?,
        )?,
        native_path: row.take_bytes(4)?,
    })
}

pub(crate) fn jobs(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Job>, Failure> {
    let rows = tx.borrowed(
        JOBS,
        &[
            &signed(owner.operation)?,
            &after(after_position)?,
            &window(limits, PATH_ROW_BYTES),
        ],
        24,
    )?;
    rows.into_iter().map(job_row).collect()
}

pub(crate) fn job(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Job>, Failure> {
    let rows = tx.borrowed(JOB, &[&signed(owner.operation)?, &signed(position)?], 16)?;
    rows.into_iter().next().map(job_row).transpose()
}

pub(crate) fn file_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<FileRoot>, Failure> {
    let rows = tx.borrowed(
        FILE_ROOTS,
        &[
            &signed(owner.operation)?,
            &after(after_position)?,
            &window(limits, FILE_ROOT_ROW_BYTES),
        ],
        24,
    )?;
    rows.iter()
        .map(|row| {
            Ok(FileRoot {
                position: unsigned(row.get::<i64>(0)?)?,
                aliases: unsigned(row.get::<i64>(1)?)?,
                root: optional_root(row, 2)?,
            })
        })
        .collect()
}

pub(crate) fn entries(
    tx: &Transaction<'_>,
    owner: Owner,
    after_key: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Entry>, Failure> {
    let operation = signed(owner.operation)?;
    let limit = window(limits, ENTRY_ROW_BYTES);
    let rows = match after_key {
        None => tx.borrowed(ENTRIES_FIRST, &[&operation, &limit], 16)?,
        Some(key) => {
            let parent = key.parent.map_or(Ok(-1), signed)?;
            tx.borrowed(
                ENTRIES_NEXT,
                &[&operation, &parent, &key.name.as_slice(), &limit],
                24 + key.name.len() as u64,
            )?
        }
    };
    rows.into_iter()
        .map(|mut row| {
            let parent = row.get::<i64>(0)?;
            // Every entry a consumer streams has been placed.
            let position = row.get::<Option<i64>>(2)?.ok_or(BackendError::Integrity)?;
            Ok(Entry {
                position: unsigned(position)?,
                kind: kind(&row, 3)?,
                canonical: row.get::<Option<i64>>(4)?.map(unsigned).transpose()?,
                metadata_root: root(&row.get::<Vec<u8>>(5)?)?,
                content_root: optional_root(&row, 6)?,
                key: EntryKey {
                    parent: (parent >= 0).then(|| unsigned(parent)).transpose()?,
                    name: row.take_bytes(1)?,
                },
            })
        })
        .collect()
}
