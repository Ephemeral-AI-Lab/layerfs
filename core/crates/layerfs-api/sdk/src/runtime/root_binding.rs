//! Authorized demand validation of a bound root's actual directory inode.
use super::{Authorization, RuntimeError, RuntimeResult};
use layerfs_content::filesystem::{
    attributes::{read_portable, AttributeReadWork},
    directory::decode_directory_page,
    inode::read::{lookup, InodeReadWork, InodeTable},
    FilesystemRoot,
};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{BranchSnapshot, WorkspaceId};
use layerfs_storage::Reader;
use std::cell::RefCell;

/// Every demanded descendant is authorized before its provider read. The first
/// exact adapter failure survives Content's narrower provider error boundary.
struct RootReader<'a, 's> {
    reader: Reader<'s>,
    authority: &'a dyn Authorization,
    peer: [u8; 32],
    workspace: WorkspaceId,
    snapshot: &'a BranchSnapshot,
    failure: RefCell<Option<RuntimeError>>,
}
impl AuthenticatedObjects for RootReader<'_, '_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if self.failure.borrow().is_some() {
            return Err(ContentError::ProviderFailure {
                what: "root binding demand already failed",
            });
        }
        let result = self
            .authority
            .workspace(self.peer, self.workspace, self.snapshot.branch.id)
            .and_then(|()| {
                self.authority
                    .objects(self.peer, self.workspace, self.snapshot.branch.id, ids)
            })
            .and_then(|()| self.reader.read_objects(ids).map_err(Into::into));
        match result {
            Ok(values) => Ok(values),
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                Err(ContentError::ProviderFailure {
                    what: "authorized root binding demand",
                })
            }
        }
    }
}

pub(super) fn validate(
    reader: Reader<'_>,
    authority: &dyn Authorization,
    peer: [u8; 32],
    workspace: WorkspaceId,
    snapshot: &BranchSnapshot,
) -> RuntimeResult<FilesystemRoot> {
    let reader = RootReader {
        reader,
        authority,
        peer,
        workspace,
        snapshot,
        failure: RefCell::new(None),
    };
    let result = validate_root(&reader);
    if let Some(error) = reader.failure.into_inner() {
        return Err(error);
    }
    result.map_err(Into::into)
}

fn validate_root(reader: &RootReader<'_, '_>) -> ContentResult<FilesystemRoot> {
    let root = FilesystemRoot::decode(&reader.read_canonical(reader.snapshot.effective_root)?)?;
    if root.scope().object() != reader.snapshot.scope || root.profile() != reader.snapshot.profile {
        return Err(ContentError::ScopeMismatch {
            what: "Branch root scope/profile",
        });
    }
    let serial = root.root_inode().serial();
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
