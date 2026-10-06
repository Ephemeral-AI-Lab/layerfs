//! Authorized demand validation of a bound root's actual directory inode.
use super::{authorized_objects::AuthorizedObjects, Authorization, Binding, RuntimeResult};
use layerfs_content::filesystem::{
    attributes::{read_portable, AttributeReadWork},
    directory::decode_directory_page,
    inode::read::{lookup, InodeReadWork, InodeTable},
    FilesystemRoot,
};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_storage::Reader;

pub(super) fn validate(
    reader: Reader<'_>,
    authority: &dyn Authorization,
    peer: [u8; 32],
    workspace: WorkspaceId,
    snapshot: &BranchSnapshot,
) -> RuntimeResult<FilesystemRoot> {
    let provider = AuthorizedObjects::new(authority, peer, workspace, snapshot.branch.id, |ids| {
        reader.read_objects(ids)
    });
    let result = validate_root(&provider, snapshot.effective_root, snapshot, None);
    provider.runtime(result)
}

pub(super) fn validate_candidate(
    reader: Reader<'_>,
    authority: &dyn Authorization,
    binding: &Binding,
    candidate: ObjectId,
) -> RuntimeResult<FilesystemRoot> {
    let provider = AuthorizedObjects::new(
        authority,
        binding.peer,
        binding.workspace,
        binding.snapshot.branch.id,
        |ids| reader.read_objects(ids),
    );
    let result = validate_root(
        &provider,
        candidate,
        &binding.snapshot,
        Some(binding.root_serial),
    );
    provider.runtime(result)
}

fn validate_root(
    reader: &dyn AuthenticatedObjects,
    candidate: ObjectId,
    snapshot: &BranchSnapshot,
    expected_serial: Option<u64>,
) -> ContentResult<FilesystemRoot> {
    let root = FilesystemRoot::decode(&reader.read_canonical(candidate)?)?;
    if root.scope().object() != snapshot.scope || root.profile() != snapshot.profile {
        return Err(ContentError::ScopeMismatch {
            what: "Branch root scope/profile",
        });
    }
    let serial = root.root_inode().serial();
    if expected_serial.is_some_and(|expected| serial != expected) {
        return Err(ContentError::ScopeMismatch {
            what: "candidate root serial",
        });
    }
    let value = lookup(
        reader,
        InodeTable {
            root: root.inode_table(),
            root_serial: serial,
        },
        serial,
        &mut InodeReadWork::default(),
    )?
    .ok_or(ContentError::InvalidRecord("missing filesystem root inode"))?;
    value.validate(true)?;
    // Check the directory root page and the two portable metadata demand paths,
    // never enumerate their complete subtrees. Owning readers check descended
    // child summaries, fill and typed value grammar under bounded read windows.
    decode_directory_page(&reader.read_canonical(value.content_root)?)?;
    read_portable(
        reader,
        value.metadata_root,
        value.kind,
        &mut AttributeReadWork::default(),
    )?;
    Ok(root)
}
