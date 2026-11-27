//! Bounded keyset reads of one live operation, each one fenced autocommit
//! statement (`Session::read_statement`, #818 I3a): no `BEGIN`/`COMMIT`
//! round trips and no separate owner select.
//!
//! Every read joins its window `LATERAL` to the operation's live owner row
//! ([`owned`]), so a released or foreign operation returns no row (`Stale`,
//! as the owner select in a read transaction answered), and an empty window
//! returns one row whose leading marker is NULL. The statement's snapshot
//! is the one snapshot the read transaction had; the Store's generation is
//! its joined trailing column. Every window is an index range scan in key
//! order, so a window's cost does not grow with the operation. A path
//! window sums its rows' sizes over the rows it read (at most the row
//! limit) and keeps the leading rows that fit, in the same statement, where
//! a sizing read and a second read did before. Rows are put in key order
//! here, whatever order the joined statement returns them in.
use crate::acquisition::{
    failure::{signed, Failure},
    payload::metadata_parse,
};
use crate::backend::{
    error::BackendError,
    postgresql::{
        history::statements,
        session::{rows, transaction::Transaction},
    },
};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{
    AcquisitionWork, Binding, Directory, Entry, EntryKey, FileRoot, HostIdentity, Job, Limits,
    Owner, Unplaced, EVIDENCE_BYTES,
};
use postgres::{types::FromSql, Row};

/// Largest public column bytes one row of each read can carry.
const ENTRY_ROW_BYTES: usize = 255 + 64 + 40;
const BINDING_ROW_BYTES: usize = 255 + 24;
const UNPLACED_ROW_BYTES: usize = 255 + 60 + 8;
const FILE_ROOT_ROW_BYTES: usize = 48;

/// Each read's statement key (`history::statements::qualified`): its window
/// joined `LATERAL` to the live owner row, whose first column is the marker
/// `1`; `$1` is the operation and `$2` its owner epoch, so a window's own
/// parameters start at `$3`. `{s}` names the Store's schema.
const UNPLACED: &str = "acquisition_unplaced";
/// The leading directories whose paths (and 8 bytes each) fit `$5` bytes,
/// the first always, among at most `$4` after `$3`.
const DIRECTORIES: &str = "acquisition_directories";
const DIRECTORY_PATH: &str = "acquisition_directory_path";
/// The leading jobs whose paths (and 68 bytes each) fit `$5` bytes, the
/// first always, among at most `$4` after `$3`.
const JOBS: &str = "acquisition_jobs";
const JOB: &str = "acquisition_job";
const FILE_ROOTS: &str = "acquisition_file_roots";
const ENTRIES: &str = "acquisition_entries";
const BINDINGS: &str = "acquisition_bindings";

/// One column, with a conversion failure as `Integrity`.
pub(super) fn get<'a, T: FromSql<'a>>(row: &'a Row, index: usize) -> Result<T, BackendError> {
    row.try_get(index).map_err(|e| rows::error(&e))
}

/// The statement text of `key` in the Store's schema: its window joined to
/// the live owner row, no row when the owner is not live, one row with a NULL
/// marker for an empty window.
pub(super) fn owned(tx: &Transaction<'_>, key: &str) -> Result<String, BackendError> {
    let sql = statements::qualified(key).ok_or(BackendError::Integrity)?;
    Ok(sql.replace("{s}", tx.schema))
}

/// Sends one owned read and returns its window's rows: `Stale` when the
/// owner is not live, `Integrity` when its row does not decode (the owner
/// columns follow the window's in every row), none for an empty window.
fn read(
    tx: &Transaction<'_>,
    owner: Owner,
    key: &str,
    params: &[&(dyn postgres::types::ToSql + Sync)],
    bytes: u64,
) -> Result<Vec<Row>, Failure> {
    let (operation, epoch) = (signed(owner.operation)?, signed(owner.epoch)?);
    let mut all: Vec<&(dyn postgres::types::ToSql + Sync)> = vec![&operation, &epoch];
    all.extend_from_slice(params);
    let found = tx.typed(&owned(tx, key)?, &all, 16 + bytes)?;
    let first = found.first().ok_or(Failure::Stale)?;
    let owner_at = (first.columns().iter())
        .position(|column| column.name() == "owner_phase")
        .ok_or(BackendError::Integrity)?;
    super::accounting::decoded(first, owner_at)?;
    if get::<Option<i32>>(first, 0)?.is_none() {
        if found.len() != 1 {
            return Err(BackendError::Integrity.into());
        }
        return Ok(Vec::new());
    }
    Ok(found)
}

/// `items` in the order of their keys.
fn ordered<K: Ord, T>(mut items: Vec<(K, T)>) -> Vec<T> {
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items.into_iter().map(|(_, item)| item).collect()
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

/// The row and byte bounds of a path window.
fn path_limits(limits: Limits) -> (i64, i64) {
    let limits = limits.clamped();
    (
        limits.rows as i64,
        i64::try_from(limits.bytes).unwrap_or(i64::MAX),
    )
}

pub(super) fn unplaced_children(
    tx: &Transaction<'_>,
    owner: Owner,
    parent: u64,
    after_name: Option<&[u8]>,
    limits: Limits,
) -> Result<Vec<Unplaced>, Failure> {
    let parent = signed(parent)?;
    let limit = window(limits, UNPLACED_ROW_BYTES);
    // A child's name is never empty, so the empty name precedes every one.
    let after_name = after_name.unwrap_or(&[]);
    let found = read(
        tx,
        owner,
        UNPLACED,
        &[&parent, &after_name, &limit],
        16 + after_name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let identity = match get::<Option<&[u8]>>(row, 3)? {
            None => None,
            Some(packed) => {
                let packed: [u8; 60] = exact(packed)?;
                Some(host(&packed[..8], &packed[8..16], &packed[16..])?)
            }
        };
        let name = get::<Vec<u8>>(row, 1)?;
        out.push((
            name.clone(),
            Unplaced {
                name,
                kind: kind(row, 2)?,
                host: identity,
            },
        ));
    }
    Ok(ordered(out))
}

pub(super) fn directories(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Directory>, Failure> {
    let after = after(after_position)?;
    let (rows, bytes) = path_limits(limits);
    let found = read(tx, owner, DIRECTORIES, &[&after, &rows, &bytes], 24)?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let position = number(get(row, 1)?)?;
        out.push((
            position,
            Directory {
                position,
                host_path: get(row, 2)?,
            },
        ));
    }
    Ok(ordered(out))
}

pub(super) fn directory_path(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Vec<u8>>, Failure> {
    let position = signed(position)?;
    let found = read(tx, owner, DIRECTORY_PATH, &[&position], 8)?;
    Ok(match found.first() {
        Some(row) => Some(get(row, 1)?),
        None => None,
    })
}

fn job_row(row: &Row) -> Result<Job, BackendError> {
    Ok(Job {
        position: number(get(row, 1)?)?,
        host: host(get(row, 2)?, get(row, 3)?, get(row, 4)?)?,
        host_path: get(row, 5)?,
    })
}

pub(super) fn jobs(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<Job>, Failure> {
    let after = after(after_position)?;
    let (rows, bytes) = path_limits(limits);
    let found = read(tx, owner, JOBS, &[&after, &rows, &bytes], 24)?;
    let jobs = found
        .iter()
        .map(|row| job_row(row).map(|job| (job.position, job)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ordered(jobs))
}

pub(super) fn job(
    tx: &Transaction<'_>,
    owner: Owner,
    position: u64,
) -> Result<Option<Job>, Failure> {
    let position = signed(position)?;
    let found = read(tx, owner, JOB, &[&position], 8)?;
    Ok(found.first().map(job_row).transpose()?)
}

pub(super) fn file_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    after_position: Option<u64>,
    limits: Limits,
) -> Result<Vec<FileRoot>, Failure> {
    let after = after(after_position)?;
    let limit = window(limits, FILE_ROOT_ROW_BYTES);
    let found = read(tx, owner, FILE_ROOTS, &[&after, &limit], 16)?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let position = number(get(row, 1)?)?;
        out.push((
            position,
            FileRoot {
                position,
                aliases: number(get(row, 2)?)?,
                root: optional_root(row, 3)?,
            },
        ));
    }
    Ok(ordered(out))
}

/// A row's stored key (`parent_position`, `name`) and its entry key and
/// position.
#[allow(clippy::type_complexity)]
fn key(row: &Row) -> Result<((i64, Vec<u8>), EntryKey, u64), BackendError> {
    let parent: i64 = get(row, 1)?;
    let name: Vec<u8> = get(row, 2)?;
    let position = get::<Option<i64>>(row, 3)?.ok_or(BackendError::Integrity)?;
    Ok((
        (parent, name.clone()),
        EntryKey {
            parent: (parent >= 0).then(|| number(parent)).transpose()?,
            name,
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
    let (parent, name) = after_key(after)?;
    let limit = window(limits, ENTRY_ROW_BYTES);
    let found = read(
        tx,
        owner,
        ENTRIES,
        &[&parent, &name, &limit],
        16 + name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let (stored, key, position) = key(row)?;
        let kind = kind(row, 4)?;
        out.push((
            stored,
            Entry {
                key,
                position,
                kind,
                canonical: get::<Option<i64>>(row, 5)?.map(number).transpose()?,
                metadata: metadata_parse(get(row, 6)?, kind)?,
                content_root: optional_root(row, 7)?,
            },
        ));
    }
    Ok(ordered(out))
}

pub(super) fn bindings(
    tx: &Transaction<'_>,
    owner: Owner,
    after: Option<&EntryKey>,
    limits: Limits,
) -> Result<Vec<Binding>, Failure> {
    let (parent, name) = after_key(after)?;
    let limit = window(limits, BINDING_ROW_BYTES);
    let found = read(
        tx,
        owner,
        BINDINGS,
        &[&parent, &name, &limit],
        16 + name.len() as u64,
    )?;
    let mut out = Vec::with_capacity(found.len());
    for row in &found {
        let (stored, key, position) = key(row)?;
        out.push((
            stored,
            Binding {
                key,
                position,
                canonical: get::<Option<i64>>(row, 4)?.map(number).transpose()?,
            },
        ));
    }
    Ok(ordered(out))
}

/// The operation's charges: the owner row alone (`accounting::owner`).
pub(super) fn work(tx: &Transaction<'_>, owner: Owner) -> Result<AcquisitionWork, Failure> {
    Ok(super::accounting::owner(tx, owner)?.1)
}
