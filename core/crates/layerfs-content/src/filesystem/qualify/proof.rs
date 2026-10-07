//! The required context and the proof a completed qualification returns.
use crate::error::{ContentError, ContentResult};
use crate::filesystem::directory::read::DirectoryReadWork;
use crate::filesystem::identity::InodeScope;
use crate::filesystem::inode::read::InodeReadWork;
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::object::ObjectId;

/// The exact root and context a caller requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RootContext {
    /// Identity of the root object.
    pub root: FilesystemRootId,
    /// Allocation scope every inode of the tree must belong to.
    pub scope: InodeScope,
    /// Namespace profile the root must declare.
    pub profile: ObjectId,
    /// Root directory serial, when the caller already holds one.
    pub root_serial: Option<u64>,
}

impl RootContext {
    /// Refuses a decoded root that is not the one this context names.
    pub(super) fn check(&self, root: FilesystemRoot) -> ContentResult<()> {
        if root.scope() != self.scope || root.profile() != self.profile {
            return Err(ContentError::ScopeMismatch {
                what: "qualified root scope/profile",
            });
        }
        if self
            .root_serial
            .is_some_and(|serial| serial != root.root_inode().serial())
        {
            return Err(ContentError::ScopeMismatch {
                what: "qualified root serial",
            });
        }
        Ok(())
    }
}

/// Proof that one exact immutable root passed whole-root qualification.
///
/// Only [`super::qualify_root`] constructs it. It carries no authority: it
/// states a fact about canonical bytes, and [`Self::admit`] answers only for
/// the root and context it was computed under.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QualifiedRoot {
    pub(super) id: FilesystemRootId,
    pub(super) root: FilesystemRoot,
    pub(super) inodes: u64,
    pub(super) directories: u64,
    pub(super) bindings: u64,
}

impl QualifiedRoot {
    /// Identity of the qualified root object.
    pub const fn id(&self) -> FilesystemRootId {
        self.id
    }
    /// The decoded qualified root.
    pub const fn root(&self) -> FilesystemRoot {
        self.root
    }
    /// Inodes in the table, including the root directory.
    pub const fn inodes(&self) -> u64 {
        self.inodes
    }
    /// Directory inodes, including the root directory.
    pub const fn directories(&self) -> u64 {
        self.directories
    }
    /// Name bindings in the whole namespace.
    pub const fn bindings(&self) -> u64 {
        self.bindings
    }
    /// The qualified root, for exactly this root and context and no other.
    pub fn admit(&self, context: &RootContext) -> ContentResult<FilesystemRoot> {
        if context.root != self.id {
            return Err(ContentError::ScopeMismatch {
                what: "qualified root identity",
            });
        }
        context.check(self.root)?;
        Ok(self.root)
    }
}

/// Work one qualification performed, including a refused one's progress.
/// Qualification resets this structure at entry; prior observations cannot
/// contribute to the namespace closure or to a successful proof's counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QualificationWork {
    /// Inode-table pages read by the sequential pass.
    pub inode: InodeReadWork,
    /// Directory pages read by the reachability pass.
    pub directory: DirectoryReadWork,
    /// Inode rows recorded.
    pub inodes: u64,
    /// Directories listed.
    pub directories: u64,
    /// Bindings counted.
    pub bindings: u64,
    /// Point record reads.
    pub record_reads: u64,
    /// Guarded record batches applied.
    pub record_batches: u64,
    /// Record changes in those batches.
    pub record_changes: u64,
    /// Largest number of changes in one batch.
    pub peak_batch_changes: u64,
}
