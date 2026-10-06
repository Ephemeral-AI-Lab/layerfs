//! Root placement, inode child summaries and kind-specific content/metadata.

use crate::error::{ContentError, ContentResult};
use crate::file::whole_file_payload;
use crate::filesystem::attributes::{read_portable, AttributeReadWork};
use crate::filesystem::directory::decode_directory_page;
use crate::filesystem::inode::codec::filled_page;
use crate::filesystem::inode::{decode_inode_page, lookup, InodePage, InodeReadWork, InodeTable};
use crate::filesystem::{FilesystemRoot, InodeIdentity, InodeScope, SymlinkTarget};
use crate::object::{AuthenticatedObjects, InodeKind, InodeValue};
use crate::policy::ConstructionPolicy;

pub(super) fn page(
    canonical: &[u8],
    reader: &dyn AuthenticatedObjects,
    policy: ConstructionPolicy,
    inode_scope: InodeScope,
    root_serial: u64,
) -> ContentResult<()> {
    match decode_inode_page(canonical)? {
        InodePage::Leaf { entries } => {
            // Reject every placement/identity before any content demand.
            for (serial, value) in &entries {
                InodeIdentity::new(inode_scope, *serial)?;
                value.validate(*serial == root_serial)?;
            }
            for (_, value) in entries {
                content(value, reader, policy)?;
            }
            Ok(())
        }
        InodePage::Branch {
            level,
            subtree_count,
            children,
        } => {
            for (maximum, _) in &children {
                InodeIdentity::new(inode_scope, *maximum)?;
            }
            let mut count = 0_u64;
            let mut previous = 0;
            for (maximum, id) in children {
                let canonical = reader.read_canonical(id)?;
                let child = decode_inode_page(&canonical)?;
                let (rows, first, last) = match &child {
                    InodePage::Leaf { entries } => {
                        for (serial, _) in entries {
                            InodeIdentity::new(inode_scope, *serial)?;
                        }
                        (
                            entries.len(),
                            entries.first().map(|row| row.0),
                            entries.last().map(|row| row.0),
                        )
                    }
                    InodePage::Branch { children, .. } => {
                        for (serial, _) in children {
                            InodeIdentity::new(inode_scope, *serial)?;
                        }
                        (
                            children.len(),
                            children.first().map(|row| row.0),
                            children.last().map(|row| row.0),
                        )
                    }
                };
                if !filled_page(rows, child.level()) {
                    return Err(ContentError::NonCanonicalPagePartition);
                }
                if child.level().checked_add(1) != Some(level) || last != Some(maximum) {
                    return Err(ContentError::InvalidRecord("inode child summary"));
                }
                if first.is_some_and(|first| first <= previous) {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                count = count
                    .checked_add(child.subtree_count())
                    .ok_or(ContentError::LengthOverflow)?;
                previous = maximum;
            }
            if count != subtree_count {
                return Err(ContentError::InvalidRecord("inode subtree summary"));
            }
            Ok(())
        }
    }
}

pub(super) fn root(
    canonical: &[u8],
    reader: &dyn AuthenticatedObjects,
    policy: ConstructionPolicy,
    inode_scope: InodeScope,
    root_serial: u64,
) -> ContentResult<()> {
    let root = FilesystemRoot::decode(canonical)?;
    if root.scope() != inode_scope || root.root_inode().serial() != root_serial {
        return Err(ContentError::ScopeMismatch {
            what: "context filesystem root",
        });
    }
    let value = lookup(
        reader,
        InodeTable {
            root: root.inode_table(),
            root_serial,
        },
        root_serial,
        &mut InodeReadWork::default(),
    )?
    .ok_or(ContentError::InvalidRecord("filesystem root inode"))?;
    value.validate(true)?;
    content(value, reader, policy)
}

fn content(
    value: InodeValue,
    reader: &dyn AuthenticatedObjects,
    policy: ConstructionPolicy,
) -> ContentResult<()> {
    {
        let canonical = reader.read_canonical(value.content_root)?;
        match value.kind {
            InodeKind::RegularFile => {
                match whole_file_payload(&canonical)? {
                    Some(payload) => {
                        let limit = policy.capacities().whole_file_raw_limit;
                        if payload.is_empty() || payload.len() > limit {
                            return Err(ContentError::BoundedCapacityExceeded {
                                what: "context.whole_file",
                                limit: limit as u64,
                                actual: payload.len() as u64,
                            });
                        }
                    }
                    None => {
                        // A no-op edit can retain an existing FileState below
                        // the fresh-construction cutoff; do not reclassify it.
                        super::mapping::state(&canonical, reader)?;
                    }
                }
            }
            InodeKind::Directory => {
                decode_directory_page(&canonical)?;
            }
            InodeKind::Symlink => {
                SymlinkTarget::decode(&canonical)?;
            }
        }
    }
    read_portable(
        reader,
        value.metadata_root,
        value.kind,
        &mut AttributeReadWork::default(),
    )?;
    Ok(())
}
