//! Scoped inode identity and the caller-authorized serial contract.
//!
//! An inode identity is an allocation scope plus an unsigned serial. The compact
//! profile stores the serial in the inode table and in directory bindings, and
//! the scope in the filesystem root; comparing serials from different scopes is
//! meaningless, so this module keeps the scope next to the serial in every public
//! identity value.
//!
//! C1 never allocates. New serials arrive from the caller's allocator, which owns
//! uniqueness, scope separation and the lifecycle of an exposed serial: the
//! reference allocator durably burns a reserved range even when the work that
//! requested it fails, and a caller that reuses an exposed serial would silently
//! reinterpret retained records. That precondition is stated here, enforced where
//! it can be (no zero serial, in-range values) and never hidden inside a store.

use crate::error::{ContentError, ContentResult};
use crate::object::ObjectId;

/// Largest accepted inode serial, matching the reference grammar's `i64` bound.
pub const MAXIMUM_INODE_SERIAL: u64 = i64::MAX as u64;

/// Allocation scope of one filesystem tree.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InodeScope(ObjectId);

impl InodeScope {
    /// Wraps one scope identity.
    pub const fn from_object(id: ObjectId) -> Self {
        Self(id)
    }

    /// The scope identity as stored in the filesystem root.
    pub const fn object(self) -> ObjectId {
        self.0
    }
}

/// One authorized inode identity inside an explicit scope.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InodeIdentity {
    scope: InodeScope,
    serial: u64,
}

impl InodeIdentity {
    /// Checks and wraps one serial inside `scope`.
    pub fn new(scope: InodeScope, serial: u64) -> ContentResult<Self> {
        if serial == 0 || serial > MAXIMUM_INODE_SERIAL {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        Ok(Self { scope, serial })
    }

    /// The scope this identity belongs to.
    pub const fn scope(self) -> InodeScope {
        self.scope
    }

    /// The serial inside the scope.
    pub const fn serial(self) -> u64 {
        self.serial
    }

    /// Rejects an identity that does not belong to `scope`.
    pub fn check_scope(self, scope: InodeScope) -> ContentResult<Self> {
        if self.scope == scope {
            Ok(self)
        } else {
            Err(ContentError::ScopeMismatch {
                what: "inode identity",
            })
        }
    }
}
