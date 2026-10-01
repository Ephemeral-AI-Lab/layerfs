//! Shared scalar semantic algorithm with prospectively selected fact acquisition.
use super::binding::{Bindings, Headers};
use super::fact_access::FactAccess;
use super::prefetch;
use super::{
    charge_site, check_new_identities, declared_limit, FilesystemTopology, ValidationState,
    ValidationWork,
};
use crate::filesystem::rows::{DirectoryHeader, PreparedBindingRows};
use crate::filesystem::PathName;
use crate::object::{inode_leaf::InodeKind, AuthenticatedObjects};
use crate::{ContentError, ContentResult};

pub(super) fn initial(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    work: &mut ValidationWork,
) -> ContentResult<FilesystemTopology> {
    let declared = declared_limit(input);
    let rows = input
        .directory_rows()
        .max(input.inode_rows())
        .max(input.new_rows());
    if rows > declared {
        return Err(ContentError::ObjectLimitExceeded {
            limit: declared,
            actual: rows,
        });
    }
    let topology =
        FilesystemTopology::load(reader, input.base(), input.scope(), input.root_serial())?;
    // The allocator precondition is checked before anything else: a serial the
    // caller calls new must not already exist in the base it addresses.
    let allocation_before = work.inode_pages_read;
    check_new_identities(reader, input, topology, work)?;
    charge_site(work, allocation_before, |sites| &mut sites.allocation);
    Ok(topology)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn checked<S: ?Sized>(
    reader: &dyn AuthenticatedObjects,
    input: &dyn PreparedBindingRows,
    topology: FilesystemTopology,
    work: &mut ValidationWork,
    mut facts: ValidationState,
    state: &mut S,
    access: FactAccess<S>,
    mut claim: impl FnMut(&mut S, u64, &DirectoryHeader, u32, bool, PathName) -> ContentResult<()>,
) -> ContentResult<(FilesystemTopology, ValidationState)> {
    let declared = declared_limit(input);
    // Count and finish the selected scalar source before the first prefetch grouped base
    // demand. Replay it through one fixed raw64 producer; no demand union lives.
    if let Some(table) = topology.table {
        let prefetch_before = work.inode_pages_read;
        let outcome = prefetch::read_with(
            reader, input, table, &mut facts, declared, work, state, access,
        );
        charge_site(work, prefetch_before, |sites| &mut sites.prefetch);
        outcome?;
    }
    let bindings_before = work.inode_pages_read;
    let mut rows = Headers::new(input)?;
    let mut names = 0usize;
    while let Some(header) = rows.next()? {
        names = names.saturating_add(header.binding_count() as usize);
        if names > declared {
            return Err(ContentError::ObjectLimitExceeded {
                limit: declared,
                actual: names,
            });
        }
        if header.parent() == input.root_serial() && input.base().is_none() {
            // The root directory of a new filesystem is built by this operation.
        } else if header.parent() == input.root_serial() {
            // A root directory update is legal; the root's own count stays zero.
        } else if input.is_new(header.parent())? {
            // A directory this operation allocates starts empty; its value must
            // still declare the directory kind it will have.
            let value = input
                .value_for(header.parent())?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else if let Some(table) = topology.table {
            let record = access
                .lookup(&mut facts, state, reader, table, header.parent(), work)?
                .ok_or(ContentError::InvalidRecord("missing base inode"))?;
            if record.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        } else {
            let value = input
                .value_for(header.parent())?
                .ok_or(ContentError::InvalidRecord("directory parent value"))?;
            if value.kind != InodeKind::Directory {
                return Err(ContentError::InvalidRecord("directory parent kind"));
            }
        }
        let mut bindings = Bindings::new(input, header)?;
        let mut ordinal = 0_u32;
        while let Some((name, binding)) = bindings.next()? {
            let selected_ordinal = ordinal;
            ordinal = ordinal.checked_add(1).ok_or(ContentError::LengthOverflow)?;
            let Some(child) = binding else {
                continue;
            };
            if child == input.root_serial() {
                return Err(ContentError::InvalidRecord("root directory binding"));
            }
            // The kind comes from the stored record for an existing inode and
            // from the caller's typed value for one this operation allocates.
            let stored = match topology.table {
                Some(table) => access.lookup(&mut facts, state, reader, table, child, work)?,
                None => None,
            };
            let previous = match stored {
                Some(record) => Some(record),
                None => input.value_for(child)?,
            };
            let previous = previous.ok_or(ContentError::InvalidRecord("binding kind"))?;
            // Only a regular file may carry several bindings, so a file that
            // already has one keeps gaining them.
            let kind = stored.map_or(previous.kind, |record| record.kind);
            if kind == InodeKind::RegularFile {
                continue;
            }
            // A same-batch duplicate is refused here, before the base-listing
            // alias pass: a directory or a symlink has one binding outside the
            // root and the batch already names it twice.
            claim(
                state,
                child,
                &header,
                selected_ordinal,
                stored.is_some(),
                name,
            )?;
        }
    }
    drop(rows);
    charge_site(work, bindings_before, |sites| &mut sites.bindings);
    Ok((topology, facts))
}
