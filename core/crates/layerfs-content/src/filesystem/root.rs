//! Checked scoped filesystem-root framing and its known result facts.
//!
//! The root is `profile + allocation scope + root inode serial + inode-table
//! root`. Decoding accepts exactly the compact scoped-inline profile this
//! implementation writes; any other profile fails explicitly, before any
//! mutation, instead of being silently re-interpreted or converted.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::identity::{InodeIdentity, InodeScope};
use crate::object::{codec, ObjectId, ObjectRole};

/// Width of a filesystem-root value, excluding the canonical envelope.
pub const ROOT_VALUE_BYTES: usize = 116;
/// Magic of the compact scoped-inline filesystem root.
pub const ROOT_MAGIC: [u8; 8] = *b"LFS6FSR\0";
/// Grammar version of the compact scoped-inline filesystem root.
pub const ROOT_VERSION: u16 = 1;
/// Persisted role of the filesystem root inside its own header.
pub const ROOT_ROLE: u8 = 6;
/// Frozen profile identity of the compact scoped-inline namespace.
pub const PROFILE_DESCRIPTION: &[u8] = b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5";

/// The accepted write profile's identity.
pub fn profile_id() -> ObjectId {
    ObjectId::for_bytes(PROFILE_DESCRIPTION)
}

/// The allocation scope derived from a caller seed, as the reference does.
pub fn scope_for_seed(seed: [u8; 32]) -> InodeScope {
    let mut bytes = b"layerfs/inode-scope/v1\0".to_vec();
    bytes.extend_from_slice(&seed);
    InodeScope::from_object(ObjectId::for_bytes(&bytes))
}

/// Identity of one filesystem root object.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FilesystemRootId(pub ObjectId);

/// One checked filesystem root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FilesystemRoot {
    profile: ObjectId,
    scope: InodeScope,
    root_inode: InodeIdentity,
    inode_table: ObjectId,
}

impl FilesystemRoot {
    /// Builds a root, rejecting a foreign profile and an unrepresentable serial.
    pub fn new(
        profile: ObjectId,
        scope: InodeScope,
        root_serial: u64,
        inode_table: ObjectId,
    ) -> ContentResult<Self> {
        if profile != profile_id() {
            return Err(ContentError::UnsupportedProfile {
                what: "filesystem root profile",
            });
        }
        Ok(Self {
            profile,
            scope,
            root_inode: InodeIdentity::new(scope, root_serial)?,
            inode_table,
        })
    }

    /// The accepted profile.
    pub const fn profile(self) -> ObjectId {
        self.profile
    }

    /// The allocation scope of every inode in this tree.
    pub const fn scope(self) -> InodeScope {
        self.scope
    }

    /// Identity of the root directory inode.
    pub const fn root_inode(self) -> InodeIdentity {
        self.root_inode
    }

    /// Root of the inline inode table.
    pub const fn inode_table(self) -> ObjectId {
        self.inode_table
    }

    /// The same root with a new inode table, keeping profile, scope and serial.
    pub const fn with_inode_table(self, inode_table: ObjectId) -> Self {
        Self {
            inode_table,
            ..self
        }
    }

    /// Encodes the canonical root object.
    pub fn encode(self) -> ContentResult<Vec<u8>> {
        let mut value = Vec::with_capacity(ROOT_VALUE_BYTES);
        value.extend_from_slice(&ROOT_MAGIC);
        value.extend_from_slice(&ROOT_VERSION.to_be_bytes());
        value.extend_from_slice(&[ROOT_ROLE, 0]);
        value.extend_from_slice(self.profile.as_bytes());
        value.extend_from_slice(self.scope.object().as_bytes());
        value.extend_from_slice(&self.root_inode.serial().to_be_bytes());
        value.extend_from_slice(self.inode_table.as_bytes());
        debug_assert_eq!(value.len(), ROOT_VALUE_BYTES);
        codec::encode_bytes_object(&value)
    }

    /// Decodes one canonical root object, rejecting every unsupported profile.
    pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
        let value = codec::decode_bytes_object(canonical)?;
        if value.len() < ROOT_VALUE_BYTES {
            return Err(ContentError::UnexpectedEof);
        }
        if value.len() > ROOT_VALUE_BYTES {
            return Err(ContentError::TrailingBytes);
        }
        if !value.starts_with(&ROOT_MAGIC) {
            return Err(ContentError::UnsupportedFraming);
        }
        let version = u16::from_be_bytes([value[8], value[9]]);
        if version != ROOT_VERSION {
            return Err(ContentError::UnsupportedMappingVersion { version });
        }
        if value[10] != ROOT_ROLE {
            return Err(ContentError::WrongLogicalRole);
        }
        if value[11] != 0 {
            return Err(ContentError::InvalidRecord("filesystem root flags"));
        }
        let profile = ObjectId::from_bytes(&value[12..44])?;
        if profile != profile_id() {
            return Err(ContentError::UnsupportedProfile {
                what: "filesystem root profile",
            });
        }
        Self::new(
            profile,
            InodeScope::from_object(ObjectId::from_bytes(&value[44..76])?),
            u64::from_be_bytes(
                value[76..84]
                    .try_into()
                    .map_err(|_| ContentError::UnexpectedEof)?,
            ),
            ObjectId::from_bytes(&value[84..116])?,
        )
    }

    /// Logical role of the root object.
    pub const fn role(self) -> ObjectRole {
        ObjectRole::FilesystemRoot
    }

    /// Direct canonical references of the root object.
    pub const fn references(self) -> [ObjectId; 1] {
        [self.inode_table]
    }
}
