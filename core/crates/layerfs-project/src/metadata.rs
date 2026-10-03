//! Canonical portable attribute construction, adapted from the existing Init path.
use crate::{error::content, ProjectError};
use layerfs_content::filesystem::attributes::{
    build::build_attribute_tree, codec::AttributeEntry, value::emit_value,
};
use layerfs_content::filesystem::attributes::{AttributeKey, PortableMetadata};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{FilesystemObjects, ObjectId};

/// Builds the typed portable fields through the caller's existing save owner.
/// The constructors only emit; they never read these unpublished new objects.
pub(crate) fn build_metadata(
    objects: &mut FilesystemObjects<'_>,
    kind: InodeKind,
    value: PortableMetadata,
) -> Result<ObjectId, ProjectError> {
    value.validate(kind).map_err(content)?;
    let mode = emit_value(objects, &value.mode_bytes(kind).map_err(content)?).map_err(content)?;
    let mtime = emit_value(objects, &value.mtime_bytes().map_err(content)?).map_err(content)?;
    build_attribute_tree(
        objects,
        vec![
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).map_err(content)?,
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).map_err(content)?,
                value_root: mtime,
            }),
        ]
        .into_iter(),
    )
    .map(|(root, _)| root)
    .map_err(content)
}
