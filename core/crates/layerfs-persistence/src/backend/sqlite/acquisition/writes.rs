//! Bounded write units: begin, append, place, record roots and advance.
use super::accounting::{charge, entry_bytes, native_bytes, NATIVE_BYTES, ROOT_BYTES};
use super::statements::{
    signed, unsigned, Failure, ADVANCE, BEGIN, BIND_NATIVE, CLAIM_EPOCH, COMPLETE_FILE,
    PLACE_ENTRY, PUT_ENTRY, SET_DIRECTORY_ROOT,
};
use crate::backend::{records::BackendError, Transaction};
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_storage::port::acquisition::{Begin, NativeIdentity, NewEntry, Owner, Phase, Placed};

/// Parent position stored for the source root, which has no parent.
const ROOT_PARENT: i64 = -1;

/// Inserts the operation row; the first operation of a session names its epoch.
pub(crate) fn begin(tx: &Transaction<'_>, epoch: u64, facts: &Begin) -> Result<Owner, Failure> {
    let known = (epoch != 0).then(|| signed(epoch)).transpose()?;
    let (device, inode) = (
        facts.source_device.to_be_bytes(),
        facts.source_inode.to_be_bytes(),
    );
    let inserted = tx.borrowed(
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
    let operation = inserted
        .first()
        .ok_or(BackendError::Integrity)?
        .get::<i64>(0)?;
    let epoch = match known {
        Some(epoch) => epoch,
        None => tx
            .borrowed(CLAIM_EPOCH, &[&operation], 8)?
            .first()
            .ok_or(BackendError::Integrity)?
            .get::<i64>(0)?,
    };
    Ok(Owner {
        operation: unsigned(operation)?,
        epoch: unsigned(epoch)?,
    })
}

/// Binds one positioned path to its native identity and returns the canonical
/// position with whether this path created the identity's row.
fn bind(
    tx: &Transaction<'_>,
    operation: i64,
    position: u64,
    native: &NativeIdentity,
    path: &[u8],
) -> Result<(i64, bool), Failure> {
    let own = signed(position)?;
    let (device, inode) = (native.device.to_be_bytes(), native.inode.to_be_bytes());
    let bound = tx.borrowed(
        BIND_NATIVE,
        &[
            &operation,
            &own,
            &device.as_slice(),
            &inode.as_slice(),
            &native.evidence.as_slice(),
            &path,
        ],
        76 + path.len() as u64,
    )?;
    // No row: the identity exists with different evidence and was not changed.
    let canonical = bound
        .first()
        .ok_or(Failure::Changed(position))?
        .get::<i64>(0)?;
    Ok((canonical, canonical == own))
}

/// Appends one window of entries and binds its positioned regular files.
pub(crate) fn put_entries(
    tx: &Transaction<'_>,
    owner: Owner,
    entries: &[NewEntry],
) -> Result<Vec<u64>, Failure> {
    let operation = signed(owner.operation)?;
    let (mut rows, mut bytes) = (0_i64, 0_i64);
    let mut canonical = Vec::new();
    for entry in entries {
        let parent = match entry.key.parent {
            Some(parent) => signed(parent)?,
            None => ROOT_PARENT,
        };
        let position = entry.position.map(signed).transpose()?;
        let path = entry.native_path.as_deref();
        let well_formed = match entry.kind {
            InodeKind::Directory => path.is_some() && entry.native.is_none(),
            InodeKind::RegularFile => path.is_some() && entry.native.is_some(),
            InodeKind::Symlink => path.is_none() && entry.native.is_none(),
        } && (entry.kind == InodeKind::Symlink) == entry.target_root.is_some()
            && entry.key.parent.is_some() != (entry.position == Some(0))
            && entry.key.parent.is_some() != entry.key.name.is_empty()
            && (entry.key.parent.is_some() || entry.kind == InodeKind::Directory);
        if !well_formed {
            return Err(Failure::Bounds);
        }
        let mut first = None;
        if let (Some(own), Some(native), Some(path)) = (entry.position, &entry.native, path) {
            let (bound, created) = bind(tx, operation, own, native, path)?;
            first = Some(bound);
            canonical.push(unsigned(bound)?);
            if created {
                rows += 1;
                bytes += native_bytes(path);
            }
        }
        // A regular file's path lives with its identity; only directories keep one.
        let stored_path = path.filter(|_| entry.kind == InodeKind::Directory);
        let unplaced = entry.native.filter(|_| entry.position.is_none());
        let unplaced = unplaced.map(packed);
        tx.borrowed(
            PUT_ENTRY,
            &[
                &operation,
                &parent,
                &entry.key.name.as_slice(),
                &position,
                &i64::from(entry.kind.code()),
                &first,
                &entry.metadata_root.as_bytes().as_slice(),
                &entry
                    .target_root
                    .as_ref()
                    .map(|root| root.as_bytes().as_slice()),
                &stored_path,
                &unplaced.as_ref().map(|native| native.as_slice()),
            ],
            entry_bytes(entry) as u64 + 40,
        )?;
        rows += 1;
        bytes += entry_bytes(entry);
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// Device, inode and evidence as one stored value.
fn packed(native: NativeIdentity) -> [u8; NATIVE_BYTES as usize] {
    let mut out = [0; NATIVE_BYTES as usize];
    out[..8].copy_from_slice(&native.device.to_be_bytes());
    out[8..16].copy_from_slice(&native.inode.to_be_bytes());
    out[16..].copy_from_slice(&native.evidence);
    out
}

/// Assigns positions to one window of unpositioned children of `parent`.
pub(crate) fn place_children(
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
        if let Some((native, path)) = &child.native {
            let (bound, created) = bind(tx, operation, child.position, native, path)?;
            first = Some(bound);
            canonical.push(unsigned(bound)?);
            if created {
                rows += 1;
                bytes += native_bytes(path);
            }
            bytes -= NATIVE_BYTES;
        }
        let updated = tx.borrowed(
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
        let kind = updated
            .first()
            .ok_or(Failure::Changed(child.position))?
            .get::<i64>(0)?;
        let regular = kind == i64::from(InodeKind::RegularFile.code());
        if regular != child.native.is_some() {
            return Err(Failure::Changed(child.position));
        }
    }
    charge(tx, owner, rows, bytes)?;
    Ok(canonical)
}

/// Records each `(position, root)` through `statement`, exactly once per row.
fn record_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    statement: &str,
    roots: &[(u64, ObjectId)],
) -> Result<(), Failure> {
    let operation = signed(owner.operation)?;
    for (position, root) in roots {
        let recorded = tx.borrowed(
            statement,
            &[&operation, &signed(*position)?, &root.as_bytes().as_slice()],
            48,
        )?;
        if recorded.is_empty() {
            return Err(Failure::Changed(*position));
        }
    }
    charge(tx, owner, 0, roots.len() as i64 * ROOT_BYTES)?;
    Ok(())
}

pub(crate) fn complete_files(
    tx: &Transaction<'_>,
    owner: Owner,
    roots: &[(u64, ObjectId)],
) -> Result<(), Failure> {
    record_roots(tx, owner, COMPLETE_FILE, roots)
}

pub(crate) fn set_directory_roots(
    tx: &Transaction<'_>,
    owner: Owner,
    roots: &[(u64, ObjectId)],
) -> Result<(), Failure> {
    record_roots(tx, owner, SET_DIRECTORY_ROOT, roots)
}

pub(crate) fn advance(tx: &Transaction<'_>, owner: Owner, phase: Phase) -> Result<(), Failure> {
    let updated = tx.borrowed(
        ADVANCE,
        &[
            &signed(owner.operation)?,
            &signed(owner.epoch)?,
            &i64::from(phase.code()),
        ],
        24,
    )?;
    if updated.is_empty() {
        return Err(Failure::Stale);
    }
    Ok(())
}
