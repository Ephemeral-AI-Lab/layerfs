//! Local reference agreement and role-directed child validation.

use layerfs_telemetry::timer::TimingScope;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::{InodeIdentity, InodeScope};
use crate::object::{AuthenticatedObjects, FinalizedObject, ObjectRole};
use crate::policy::ConstructionPolicy;

pub(crate) fn validate(
    object: &FinalizedObject,
    provider: &dyn AuthenticatedObjects,
    policy: ConstructionPolicy,
    inode_scope: InodeScope,
    root_serial: u64,
    timing: TimingScope<'_>,
) -> ContentResult<()> {
    timing.run(|scope| {
        let policy = policy.validated()?;
        InodeIdentity::new(inode_scope, root_serial)?;
        let references = scope.child("content.context.local").run(|_| {
            crate::object::admission::decode_references(
                object.role(),
                object.canonical(),
                policy,
                inode_scope,
            )
        })?;
        if references.as_slice() != object.references() {
            return Err(ContentError::InvalidRecord("object reference list"));
        }
        let reader = super::read::Reader::new(provider, scope);
        let canonical = object.canonical();
        match object.role() {
            ObjectRole::WholeFile | ObjectRole::Chunk | ObjectRole::Symlink => Ok(()),
            ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => {
                super::mapping::node(canonical, &reader)
            }
            ObjectRole::FileState => super::mapping::state(canonical, &reader).map(|_| ()),
            ObjectRole::InodeLeaf | ObjectRole::InodeBranch => {
                super::inode::page(canonical, &reader, policy, inode_scope, root_serial)
            }
            ObjectRole::DirectoryLeaf | ObjectRole::DirectoryBranch => {
                super::namespace::directory(canonical, &reader, inode_scope, root_serial)
            }
            ObjectRole::AttributeLeaf | ObjectRole::AttributeBranch => {
                super::namespace::attributes(canonical, &reader)
            }
            ObjectRole::FilesystemRoot => {
                super::inode::root(canonical, &reader, policy, inode_scope, root_serial)
            }
        }
    })
}
