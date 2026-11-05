//! Frozen typed history identities and their domain-separated derivation.
//!
//! The encodings are the reference product's: a one-byte tag followed by the
//! derived or authority-supplied body, at a fixed width per entity. Commit and
//! Layer identities are derived with the reference's own domain separators, so a
//! Commit of the same `(root, parent, base Layer)` and a Layer of the same
//! `(stack, parent, root)` reproduce the reference identity exactly.
//!
//! What a hash cannot carry is provenance: a Layer's source Branch and source
//! Commit are deliberately outside its identity. Two records that derive the
//! same identity must therefore agree on every identity-bearing field, and a
//! disagreement is an integrity failure rather than an overwrite. Creation-time
//! annotations such as a Commit's author, labels and time are not identity-bearing;
//! the first stored record wins. The catalog enforces both rules.
//!
//! Identity creation is an authority decision. Stack and Branch IDs and Workspace
//! incarnations arrive as explicit checked inputs; this crate never reads a
//! clock, a PID or `/dev/urandom` to invent one, so a fixture is reproducible and
//! a caller cannot claim an identity it did not receive.

use crate::error::{HistoryError, HistoryResult};
use layerfs_content::ObjectId;

/// Tag byte of a Branch identity.
pub const BRANCH_TAG: u8 = 0x11;
/// Tag byte of a Commit identity.
pub const COMMIT_TAG: u8 = 0x12;
/// Tag byte of a LayerStack identity.
pub const LAYER_STACK_TAG: u8 = 0x31;
/// Tag byte of a Layer identity.
pub const LAYER_TAG: u8 = 0x32;
/// Encoded width of a Branch identity.
pub const BRANCH_BYTES: usize = 17;
/// Encoded width of a Commit identity.
pub const COMMIT_BYTES: usize = 33;
/// Encoded width of a LayerStack identity.
pub const LAYER_STACK_BYTES: usize = 17;
/// Encoded width of a Layer identity.
pub const LAYER_BYTES: usize = 33;
/// Encoded width of a Workspace incarnation.
pub const WORKSPACE_BYTES: usize = 32;
/// Largest bytes of one authority-local history name.
pub const NAME_MAX_BYTES: usize = 63;
/// Identity format version frozen by this implementation.
pub const IDENTITY_FORMAT: i64 = 1;

macro_rules! tagged_id {
    ($name:ident, $bytes:expr, $tag:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; $bytes]);

        impl $name {
            /// Wraps one already-encoded identity, checking its tag.
            pub fn from_bytes(bytes: [u8; $bytes]) -> HistoryResult<Self> {
                if bytes[0] != $tag {
                    return Err(HistoryError::Integrity(concat!(stringify!($name), " tag")));
                }
                Ok(Self(bytes))
            }

            /// Wraps one raw slice of the exact width, checking its tag.
            pub fn from_slice(bytes: &[u8]) -> HistoryResult<Self> {
                Self::from_bytes(
                    bytes.try_into().map_err(|_| {
                        HistoryError::Integrity(concat!(stringify!($name), " width"))
                    })?,
                )
            }

            /// The encoded identity.
            pub const fn to_bytes(self) -> [u8; $bytes] {
                self.0
            }

            /// The encoded identity as a slice.
            pub fn as_slice(&self) -> &[u8] {
                &self.0
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(concat!(stringify!($name), "("))?;
                write_hex(&self.0, formatter)?;
                formatter.write_str(")")
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write_hex(&self.0, formatter)
            }
        }
    };
}

tagged_id!(
    BranchId,
    BRANCH_BYTES,
    BRANCH_TAG,
    "One Branch inside one LayerStack."
);
tagged_id!(
    CommitId,
    COMMIT_BYTES,
    COMMIT_TAG,
    "One immutable Commit: root, optional parent and base Layer."
);
tagged_id!(
    LayerStackId,
    LAYER_STACK_BYTES,
    LAYER_STACK_TAG,
    "One LayerStack: a linear Layer publication timeline."
);
tagged_id!(
    LayerId,
    LAYER_BYTES,
    LAYER_TAG,
    "One immutable Layer publication."
);

/// One Workspace producer incarnation.
///
/// The value is an opaque namespace supplied by the application authority. It
/// carries no live handle and no connection state.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorkspaceId([u8; WORKSPACE_BYTES]);

impl WorkspaceId {
    /// Wraps an authority-supplied incarnation, rejecting the all-zero value.
    pub fn from_authority(bytes: [u8; WORKSPACE_BYTES]) -> HistoryResult<Self> {
        if bytes.iter().all(|byte| *byte == 0) {
            return Err(HistoryError::InvalidInput("Workspace incarnation"));
        }
        Ok(Self(bytes))
    }

    /// Wraps one raw slice of the exact width.
    pub fn from_slice(bytes: &[u8]) -> HistoryResult<Self> {
        Self::from_authority(
            bytes
                .try_into()
                .map_err(|_| HistoryError::InvalidInput("Workspace incarnation width"))?,
        )
    }

    /// The encoded incarnation.
    pub const fn to_bytes(self) -> [u8; WORKSPACE_BYTES] {
        self.0
    }

    /// The encoded incarnation as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for WorkspaceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("WorkspaceId(")?;
        write_hex(&self.0, formatter)?;
        formatter.write_str(")")
    }
}

/// Identity of one history catalog binding.
///
/// It is derived from the configured authority binding key, so it is stable
/// across a reopen and identical for two handles bound to the same authority. A
/// cursor binds this value and the catalog incarnation, which is what makes a
/// cursor from another catalog - or from a recreated one - fail.
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogId([u8; 32]);

impl CatalogId {
    /// Derives the catalog identity of one authority binding key.
    pub fn derive(binding_key: &[u8]) -> HistoryResult<Self> {
        if binding_key.is_empty() || binding_key.len() > 128 {
            return Err(HistoryError::InvalidInput("history binding key"));
        }
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/history/catalog/v1\0");
        hasher.update(binding_key);
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    /// Wraps one already-derived identity.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The encoded identity.
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// The encoded identity as a slice.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl std::fmt::Debug for CatalogId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CatalogId(")?;
        write_hex(&self.0, formatter)?;
        formatter.write_str(")")
    }
}

/// One checked authority-local history name.
///
/// The grammar is the reference's: 1 to 63 ASCII bytes, lowercase letters,
/// digits and `.`, `_`, `-`, beginning and ending alphanumeric. It is
/// deliberately narrow so that a name is comparable across a catalog file, a
/// configuration value and a wire record without locale or case rules.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HistoryName(String);

impl HistoryName {
    /// Checks and owns one name.
    pub fn new(value: &str) -> HistoryResult<Self> {
        let bytes = value.as_bytes();
        if bytes.is_empty() || bytes.len() > NAME_MAX_BYTES {
            return Err(HistoryError::InvalidInput("history name length"));
        }
        if !bytes.is_ascii() {
            return Err(HistoryError::InvalidInput("history name encoding"));
        }
        let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if !alphanumeric(bytes[0]) || !alphanumeric(bytes[bytes.len() - 1]) {
            return Err(HistoryError::InvalidInput("history name ends"));
        }
        if !bytes
            .iter()
            .all(|byte| alphanumeric(*byte) || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(HistoryError::InvalidInput("history name characters"));
        }
        Ok(Self(value.to_owned()))
    }

    /// The name text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! authority_identity {
    ($name:ident, $bytes:expr, $tag:expr) => {
        impl $name {
            /// Builds an identity from an authority-supplied 16-byte body.
            pub fn from_authority(body: [u8; 16]) -> Self {
                let mut bytes = [0u8; $bytes];
                bytes[0] = $tag;
                bytes[1..].copy_from_slice(&body);
                Self(bytes)
            }
        }
    };
}

authority_identity!(BranchId, BRANCH_BYTES, BRANCH_TAG);

impl BranchId {
    /// The identity the SDK gives a Branch it creates by name. Branch names
    /// are unique in their stack and are never reused, so the name decides.
    pub fn named(stack: LayerStackId, name: &HistoryName) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/sdk-branch/v1\0");
        hasher.update(stack.as_slice());
        hasher.update(name.as_str().as_bytes());
        let mut body = [0u8; 16];
        body.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
        Self::from_authority(body)
    }
}
authority_identity!(LayerStackId, LAYER_STACK_BYTES, LAYER_STACK_TAG);

impl CommitId {
    /// Derives the Commit identity of one `(root, parent, base Layer)` triple,
    /// extended by a merge's second parent. A Commit without a second parent
    /// keeps the identity it had before second parents existed.
    ///
    /// The metadata's Branch, kind, message and merge origin are part of the
    /// identity; its author, labels and time are creation-time annotations.
    pub fn derive(
        root: ObjectId,
        parent: Option<Self>,
        base_layer: LayerId,
        second_parent: Option<NodeId>,
        meta: &crate::records::CommitMeta,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/commit/v3\0");
        hasher.update(root.as_bytes());
        update_optional(&mut hasher, parent.as_ref().map(Self::as_slice));
        hasher.update(base_layer.as_slice());
        if let Some(second) = second_parent {
            hasher.update(b"\0second-parent\0");
            hasher.update(second.as_slice());
        }
        hasher.update(b"\0meta\0");
        update_optional(&mut hasher, meta.branch.as_ref().map(BranchId::as_slice));
        hasher.update(&[meta.via.code()]);
        let bytes = meta.message.as_deref().map(str::as_bytes);
        hasher.update(&(bytes.map_or(0, <[u8]>::len) as u64).to_be_bytes());
        update_optional(&mut hasher, bytes);
        update_optional(&mut hasher, meta.merged_from.as_ref().map(NodeId::as_slice));
        update_optional(&mut hasher, meta.displaced.as_ref().map(Self::as_slice));
        Self(tagged_hash(COMMIT_TAG, hasher.finalize()))
    }
}

/// One node of the history graph: a Commit or a published Layer.
///
/// Both identities are 33 bytes with a distinct leading tag, so one stored
/// value names either kind without a separate discriminant column.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NodeId {
    /// An immutable Commit.
    Commit(CommitId),
    /// A published Layer.
    Layer(LayerId),
}

impl NodeId {
    /// The tagged identity bytes of either kind.
    pub fn as_slice(&self) -> &[u8] {
        match self {
            Self::Commit(id) => id.as_slice(),
            Self::Layer(id) => id.as_slice(),
        }
    }

    /// Checks the tag and wraps one tagged identity.
    pub fn from_slice(bytes: &[u8]) -> HistoryResult<Self> {
        match bytes.first() {
            Some(&COMMIT_TAG) => Ok(Self::Commit(CommitId::from_slice(bytes)?)),
            Some(&LAYER_TAG) => Ok(Self::Layer(LayerId::from_slice(bytes)?)),
            _ => Err(HistoryError::InvalidInput("history node identity")),
        }
    }
}

impl LayerId {
    /// Derives the Layer identity of one `(stack, parent, root)` triple.
    ///
    /// Source provenance is intentionally outside this derivation; see the
    /// module documentation and the catalog's immutable-equality check.
    pub fn derive(stack: LayerStackId, parent: Option<Self>, root: ObjectId) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/layer/v2\0");
        hasher.update(stack.as_slice());
        update_optional(&mut hasher, parent.as_ref().map(Self::as_slice));
        hasher.update(root.as_bytes());
        Self(tagged_hash(LAYER_TAG, hasher.finalize()))
    }
}

fn tagged_hash(tag: u8, digest: blake3::Hash) -> [u8; 33] {
    let mut bytes = [0; 33];
    bytes[0] = tag;
    bytes[1..].copy_from_slice(digest.as_bytes());
    bytes
}

fn update_optional(hasher: &mut blake3::Hasher, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            hasher.update(&[1]);
            hasher.update(value);
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn write_hex(bytes: &[u8], formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    use std::fmt::Write as _;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        formatter.write_char(char::from(HEX[usize::from(byte >> 4)]))?;
        formatter.write_char(char::from(HEX[usize::from(byte & 0x0f)]))?;
    }
    Ok(())
}
