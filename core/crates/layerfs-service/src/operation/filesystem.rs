//! Existing inode values, directory metadata and fresh declarations under one save.
//!
//! The prepared-update surface is shared by the legacy content operation and by
//! history staging. It is the same production body in both cases: one builder,
//! one ownership rule, one place where a supplied root is checked against the
//! base tree. Callers validate fresh file/symlink roles; history validates every inode role.
use super::{
    failure::content, history_bootstrap::build_metadata, metadata::patch_portable, read::id,
};
use layerfs_bridge::contract::*;
use layerfs_content::{
    filesystem::{attributes::PortableMetadata, root::FilesystemRootId},
    object::inode_leaf::{InodeKind, InodeValue},
};
use layerfs_content::{
    AuthenticatedObjects, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FinalizedConsumer, InodeScope, InodeUpdate, PathName,
};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::time::Instant;
/// One checked prepared update: an existing root plus its final changes.
#[derive(Clone, Copy)]
pub(crate) struct PreparedUpdate<'a> {
    /// Immutable base root the update is applied to.
    pub(crate) base: Root,
    /// Allocation scope the base root records.
    pub(crate) scope: Root,
    /// Root directory serial of the base tree.
    pub(crate) root_serial: u64,
    /// Final directory bindings.
    pub(crate) directories: &'a [DirectoryChange],
    /// Typed final inode values.
    pub(crate) inodes: &'a [InodeChange],
    /// Qualified fresh declarations; unbound directories may be omitted by C1.
    pub(crate) new_directories: &'a [DirectoryMetadata],
    /// Portable metadata patches to existing directories, including the root.
    pub(crate) directory_metadata: &'a [DirectoryMetadata],
    /// Fresh regular identities, a subset of the role-validated final inode values.
    pub(crate) new_file_serials: &'a [u64],
    /// Fresh symlink identities, a subset of the role-validated final inode values.
    pub(crate) new_symlink_serials: &'a [u64],
}

pub(crate) fn update(
    provider: &dyn AuthenticatedObjects,
    update: &PreparedUpdate<'_>,
    consumer: &mut dyn FinalizedConsumer,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<(Root, u64), Failure> {
    let PreparedUpdate {
        base,
        scope: allocation,
        root_serial,
        directories,
        inodes,
        new_directories,
        directory_metadata,
        new_file_serials,
        new_symlink_serials,
    } = *update;
    let mut fs = FilesystemRead::new(provider, FilesystemRootId(id(&base))).map_err(content)?;
    if fs.root().scope().object() != id(&allocation)
        || fs.root().root_inode().serial() != root_serial
    {
        return Err(Code::InvalidInput.into());
    }
    let mut serials = Vec::new();
    for d in directories {
        serials.push(d.parent);
        for (_, s) in &d.changes {
            if let Some(s) = s {
                serials.push(*s);
            }
        }
    }
    for inode in inodes {
        serials.push(inode.serial);
    }
    serials.extend(directory_metadata.iter().map(|directory| directory.serial));
    if fs
        .lookup_inodes(&serials)
        .map_err(content)?
        .iter()
        .zip(&serials)
        .any(|(found, serial)| {
            found.is_none()
                != (new_directories
                    .binary_search_by_key(serial, |d| d.serial)
                    .is_ok()
                    || new_file_serials.binary_search(serial).is_ok()
                    || new_symlink_serials.binary_search(serial).is_ok())
        })
    {
        return Err(Code::InvalidInput.into());
    }
    let mut values =
        Vec::with_capacity(inodes.len() + new_directories.len() + directory_metadata.len());
    for i in inodes {
        let kind = InodeKind::from_code(i.kind).map_err(content)?;
        // Directory content is changed through changed-name records, never wholesale
        // replacement with a caller's unrelated directory tree.
        let references = if new_file_serials.binary_search(&i.serial).is_ok()
            || new_symlink_serials.binary_search(&i.serial).is_ok()
        {
            // Both callers checked these roots with validate_inode_role before
            // construction. C1 derives the count from retained bindings.
            0
        } else {
            let old =
                fs.lookup_inodes(&[i.serial]).map_err(content)?[0].ok_or(Code::InvalidInput)?;
            if old.kind != kind
                || (kind == InodeKind::Directory && old.content_root != id(&i.content))
            {
                return Err(Code::InvalidInput.into());
            }
            provider.read_canonical(id(&i.content)).map_err(content)?;
            provider.read_canonical(id(&i.metadata)).map_err(content)?;
            old.namespace_ref_count
        };
        values.push(InodeUpdate {
            serial: i.serial,
            value: InodeValue {
                kind,
                namespace_ref_count: references,
                content_root: id(&i.content),
                metadata_root: id(&i.metadata),
            },
        });
    }
    let directories = directories
        .iter()
        .map(|d| {
            Ok(DirectoryUpdate {
                parent: d.parent,
                changes: d
                    .changes
                    .iter()
                    .map(|(n, s)| Ok((PathName::from_bytes(n).map_err(content)?, *s)))
                    .collect::<Result<_, Failure>>()?,
            })
        })
        .collect::<Result<Vec<_>, Failure>>()?;
    let mut objects = FilesystemObjects::new(provider, consumer);
    for directory in directory_metadata {
        if Instant::now() >= deadline {
            return Err(Code::Deadline.into());
        }
        let old =
            fs.lookup_inodes(&[directory.serial]).map_err(content)?[0].ok_or(Code::InvalidInput)?;
        if old.kind != InodeKind::Directory {
            return Err(Code::InvalidInput.into());
        }
        let metadata_root = patch_portable(
            &mut objects,
            old.metadata_root,
            InodeKind::Directory,
            PortableMetadata {
                mode: directory.mode,
                mtime_seconds: directory.mtime_seconds,
                mtime_nanoseconds: directory.mtime_nanoseconds,
            },
        )
        .map_err(content)?;
        values.push(InodeUpdate {
            serial: directory.serial,
            value: InodeValue {
                metadata_root,
                ..old
            },
        });
    }
    let mut new_inodes = Vec::with_capacity(
        new_directories.len() + new_file_serials.len() + new_symlink_serials.len(),
    );
    new_inodes.extend_from_slice(new_file_serials);
    new_inodes.extend_from_slice(new_symlink_serials);
    for directory in new_directories {
        if Instant::now() >= deadline {
            return Err(Code::Deadline.into());
        }
        let metadata_root = build_metadata(
            &mut objects,
            InodeKind::Directory,
            PortableMetadata {
                mode: directory.mode,
                mtime_seconds: directory.mtime_seconds,
                mtime_nanoseconds: directory.mtime_nanoseconds,
            },
        )?;
        new_inodes.push(directory.serial);
        values.push(InodeUpdate {
            serial: directory.serial,
            value: InodeValue {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                // Every declaration has a binding record, including empty ones.
                // C1 replaces this placeholder with its newly built directory root.
                content_root: metadata_root,
                metadata_root,
            },
        });
    }
    new_inodes.sort_unstable();
    if !new_directories.is_empty() || !directory_metadata.is_empty() {
        values.sort_unstable_by_key(|value| value.serial);
    }
    let input = FilesystemInput {
        base: Some(FilesystemRootId(id(&base))),
        scope: InodeScope::from_object(id(&allocation)),
        root_serial,
        directories: &directories,
        inodes: &values,
        new_inodes: &new_inodes,
        resources: FilesystemResources::default(),
    };
    let result = layerfs_content::filesystem::update::update_filesystem_timed(
        &mut objects,
        &input,
        None,
        &layerfs_content::filesystem::FilesystemPhases::new(scope),
    )
    .map_err(content)?;
    Ok((*result.root.0.as_bytes(), 0))
}
