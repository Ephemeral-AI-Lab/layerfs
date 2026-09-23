//! Check that published roots have the declared inode roles.
use crate::error::content;
use layerfs_bridge::contract::{Code, Failure, MAX_FILE};
use layerfs_content::filesystem::attributes::read::{read_portable, AttributeReadWork};
use layerfs_content::filesystem::symlink::SymlinkTarget;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, FileView, ObjectId};
use layerfs_telemetry::timer::{Active, TimingScope};

pub(crate) fn validate_inode_role(
    provider: &dyn AuthenticatedObjects,
    kind: InodeKind,
    content_root: ObjectId,
    metadata_root: ObjectId,
    timer: &TimingScope<'_, Active>,
) -> Result<(), Failure> {
    match kind {
        InodeKind::RegularFile => {
            let view = FileView::open(provider, content_root, timer.child("history.role_file"))
                .map_err(content)?;
            if view.logical_len() > MAX_FILE {
                return Err(Code::Capacity.into());
            }
        }
        InodeKind::Symlink => {
            let canonical = provider.read_canonical(content_root).map_err(content)?;
            SymlinkTarget::decode(&canonical).map_err(content)?;
        }
        InodeKind::Directory => {
            provider.read_canonical(content_root).map_err(content)?;
        }
    }
    let mut work = AttributeReadWork::default();
    read_portable(provider, metadata_root, kind, &mut work).map_err(content)?;
    Ok(())
}
