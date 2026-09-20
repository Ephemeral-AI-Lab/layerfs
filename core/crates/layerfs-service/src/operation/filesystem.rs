//! Existing-identity updates only; every supplied root is retained beforehand.
use super::{failure::content, read::id};
use layerfs_bridge::contract::*;
use layerfs_content::{
    filesystem::root::FilesystemRootId,
    object::inode_leaf::{InodeKind, InodeValue},
};
use layerfs_content::{
    AuthenticatedObjects, DirectoryUpdate, FilesystemInput, FilesystemObjects, FilesystemRead,
    FilesystemResources, FinalizedConsumer, InodeScope, InodeUpdate, PathName,
};
use layerfs_telemetry::timer::{Active, TimingScope};
pub fn update(
    provider: &dyn AuthenticatedObjects,
    op: &Operation,
    consumer: &mut dyn FinalizedConsumer,
    scope: &TimingScope<'_, Active>,
) -> Result<(Root, u64), Failure> {
    let Operation::UpdatePreparedFilesystem {
        base,
        scope: allocation,
        root_serial,
        directories,
        inodes,
    } = op
    else {
        return Err(Code::Unsupported.into());
    };
    let mut fs = FilesystemRead::new(provider, FilesystemRootId(id(base))).map_err(content)?;
    if fs.root().scope().object() != id(allocation)
        || fs.root().root_inode().serial() != *root_serial
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
    if fs
        .lookup_inodes(&serials)
        .map_err(content)?
        .iter()
        .any(Option::is_none)
    {
        return Err(Code::InvalidInput.into());
    }
    let mut values = Vec::with_capacity(inodes.len());
    for i in inodes {
        let kind = InodeKind::from_code(i.kind).map_err(content)?;
        // Directory content is changed through changed-name records, never wholesale
        // replacement with a caller's unrelated directory tree.
        let old = fs.lookup_inodes(&[i.serial]).map_err(content)?[0].ok_or(Code::InvalidInput)?;
        if old.kind != kind || (kind == InodeKind::Directory && old.content_root != id(&i.content))
        {
            return Err(Code::InvalidInput.into());
        }
        provider.read_canonical(id(&i.content)).map_err(content)?;
        provider.read_canonical(id(&i.metadata)).map_err(content)?;
        values.push(InodeUpdate {
            serial: i.serial,
            value: InodeValue {
                kind,
                namespace_ref_count: old.namespace_ref_count,
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
    let input = FilesystemInput {
        base: Some(FilesystemRootId(id(base))),
        scope: InodeScope::from_object(id(allocation)),
        root_serial: *root_serial,
        directories: &directories,
        inodes: &values,
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let mut objects = FilesystemObjects::new(provider, consumer);
    let result = layerfs_content::filesystem::update::update_filesystem_timed(
        &mut objects,
        &input,
        None,
        &layerfs_content::filesystem::FilesystemPhases::new(scope),
    )
    .map_err(content)?;
    Ok((*result.root.0.as_bytes(), 0))
}
