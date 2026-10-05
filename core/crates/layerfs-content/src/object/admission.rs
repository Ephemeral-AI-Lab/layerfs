//! Untrusted object admission through the owning canonical role decoders.

use layerfs_telemetry::timer::TimingScope;

use crate::error::{ContentError, ContentResult};
use crate::file::{mapping, whole_file_payload};
use crate::filesystem::attributes::{decode_attribute_page, AttributePage};
use crate::filesystem::directory::{decode_directory_page, DirectoryPage};
use crate::filesystem::inode::{decode_inode_page, InodePage};
use crate::filesystem::{FilesystemRoot, InodeScope, SymlinkTarget};
use crate::object::{codec, InodeKind, ObjectId, ObjectRole};
use crate::policy::ConstructionPolicy;

pub(super) fn references(
    id: ObjectId,
    role: ObjectRole,
    canonical: &[u8],
    policy: ConstructionPolicy,
    inode_scope: InodeScope,
    timing: TimingScope<'_>,
) -> ContentResult<Vec<ObjectId>> {
    timing.run(|scope| {
        let policy = policy.validated()?;
        // Check the object/field ceiling before hashing or role allocations.
        codec::decode_bytes_object(canonical)?;
        scope.child("content.admission.hash").run(|_| {
            if ObjectId::for_bytes(canonical) == id {
                Ok(())
            } else {
                Err(ContentError::IdentityMismatch)
            }
        })?;
        scope
            .child("content.admission.decode")
            .run(|_| decode_references(role, canonical, policy, inode_scope))
    })
}

fn decode_references(
    role: ObjectRole,
    canonical: &[u8],
    policy: ConstructionPolicy,
    inode_scope: InodeScope,
) -> ContentResult<Vec<ObjectId>> {
    match role {
        ObjectRole::WholeFile => {
            let payload = whole_file_payload(canonical)?.ok_or(ContentError::WrongLogicalRole)?;
            let limit = policy.capacities().whole_file_raw_limit;
            if payload.is_empty() || payload.len() > limit {
                return Err(ContentError::BoundedCapacityExceeded {
                    what: "admission.whole_file",
                    limit: limit as u64,
                    actual: payload.len() as u64,
                });
            }
            Ok(Vec::new())
        }
        ObjectRole::Chunk => {
            mapping::decode_chunk_payload(codec::decode_bytes_object(canonical)?)?;
            Ok(Vec::new())
        }
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => {
            // Root fill is the locally possible context. Dependency closure
            // must check actual child/non-root fill, levels and summaries.
            let node = mapping::decode_node(canonical)?;
            let actual = if node.level() == 0 {
                ObjectRole::ExtentLeaf
            } else {
                ObjectRole::ExtentBranch
            };
            require_role(role, actual)?;
            Ok(node.references())
        }
        ObjectRole::FileState => {
            let state = mapping::decode_file_state(canonical)?;
            if (state.logical_len == 0 && (state.extent_count != 0 || state.tree_level != 0))
                || (state.logical_len > 0
                    && (state.extent_count == 0 || state.extent_count > state.logical_len))
            {
                return Err(ContentError::InvalidRecord("file state summary"));
            }
            Ok(vec![state.mapping_root])
        }
        ObjectRole::InodeLeaf | ObjectRole::InodeBranch => {
            match decode_inode_page(canonical)? {
                InodePage::Leaf { entries } => {
                    require_role(role, ObjectRole::InodeLeaf)?;
                    let mut refs = Vec::with_capacity(entries.len() * 2);
                    for (_, value) in entries {
                        // The root serial is contextual and not encoded in a
                        // page. Refuse values legal in neither possible context.
                        value.validate(
                            value.kind == InodeKind::Directory && value.namespace_ref_count == 0,
                        )?;
                        refs.extend([value.content_root, value.metadata_root]);
                    }
                    Ok(refs)
                }
                InodePage::Branch { children, .. } => {
                    require_role(role, ObjectRole::InodeBranch)?;
                    Ok(children.into_iter().map(|(_, id)| id).collect())
                }
            }
        }
        ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch => {
            match decode_directory_page(canonical)? {
                DirectoryPage::Leaf { .. } => {
                    require_role(role, ObjectRole::DirectoryLeaf)?;
                    // Serials are namespace bindings, not canonical object IDs.
                    Ok(Vec::new())
                }
                DirectoryPage::Branch {
                    children,
                    subtree_count,
                    ..
                } => {
                    require_role(role, ObjectRole::DirectoryBranch)?;
                    if subtree_count < children.len() as u64 {
                        return Err(ContentError::InvalidRecord("directory subtree summary"));
                    }
                    Ok(children.into_iter().map(|(_, id)| id).collect())
                }
            }
        }
        ObjectRole::FilesystemRoot => {
            let root = FilesystemRoot::decode(canonical)?;
            if root.scope() != inode_scope {
                return Err(ContentError::ScopeMismatch {
                    what: "admitted filesystem root",
                });
            }
            // Profile and allocation scope are identities, not stored children.
            Ok(root.references().to_vec())
        }
        ObjectRole::AttributeLeaf | ObjectRole::AttributeBranch => {
            match decode_attribute_page(canonical)? {
                AttributePage::Leaf { entries, .. } => {
                    require_role(role, ObjectRole::AttributeLeaf)?;
                    Ok(entries.into_iter().map(|entry| entry.value_root).collect())
                }
                AttributePage::Branch {
                    children,
                    subtree_count,
                    ..
                } => {
                    require_role(role, ObjectRole::AttributeBranch)?;
                    if subtree_count < children.len() as u64 {
                        return Err(ContentError::InvalidRecord("attribute subtree summary"));
                    }
                    Ok(children.into_iter().map(|(_, id)| id).collect())
                }
            }
        }
        ObjectRole::Symlink => {
            SymlinkTarget::decode(canonical)?;
            Ok(Vec::new())
        }
    }
}

fn require_role(supplied: ObjectRole, decoded: ObjectRole) -> ContentResult<()> {
    if supplied == decoded {
        Ok(())
    } else {
        Err(ContentError::WrongLogicalRole)
    }
}
