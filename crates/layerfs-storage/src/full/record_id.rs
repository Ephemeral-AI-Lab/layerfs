//! Persisted product-record identities.

use crate::{EngineError, EngineResult};
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

macro_rules! record_id {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        pub struct $name(pub [u8; 32]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

record_id!(LayerStackId);
record_id!(LayerId);
record_id!(BranchId);
record_id!(OperationId);
record_id!(OperationVersionId);
record_id!(RequestId);
record_id!(LeaseId);

pub fn derive_id(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut bytes = Vec::with_capacity(
        domain.len()
            + parts
                .iter()
                .map(|part| part.len().saturating_add(8))
                .sum::<usize>(),
    );
    bytes.extend_from_slice(domain);
    for part in parts {
        bytes.extend_from_slice(&(part.len() as u64).to_be_bytes());
        bytes.extend_from_slice(part);
    }
    *ObjectId::for_bytes(&bytes).as_bytes()
}

pub(crate) fn transition_identity(parent: ObjectId, child: ObjectId, payload: &[u8]) -> [u8; 32] {
    derive_id(
        b"transition",
        &[parent.as_bytes(), child.as_bytes(), payload],
    )
}

pub(crate) fn full_release_id(
    target_kind: &str,
    owner_id: &[u8; 32],
    version_id: &[u8; 32],
) -> EngineResult<[u8; 32]> {
    if !matches!(target_kind, "layer" | "operation_version") {
        return Err(EngineError::InvalidRecord("release target kind"));
    }
    Ok(derive_id(
        b"layerfs.full.release.v1",
        &[target_kind.as_bytes(), owner_id, version_id],
    ))
}

pub(crate) fn bytes32(bytes: &[u8], field: &'static str) -> EngineResult<[u8; 32]> {
    bytes
        .try_into()
        .map_err(|_| EngineError::InvalidRecord(field))
}

pub(crate) fn object_id(bytes: &[u8]) -> EngineResult<ObjectId> {
    ObjectId::from_bytes(&bytes32(bytes, "ObjectId")?).map_err(EngineError::Core)
}

#[cfg(test)]
mod tests {
    use super::full_release_id;

    #[test]
    fn full_release_identity_is_frozen_and_request_independent() {
        assert_eq!(
            full_release_id("operation_version", &[0x11; 32], &[0x22; 32]).unwrap(),
            [
                177, 188, 222, 62, 233, 209, 89, 214, 147, 34, 19, 230, 232, 109, 95, 45, 157, 148,
                188, 182, 202, 118, 234, 95, 117, 131, 66, 174, 195, 188, 253, 6,
            ]
        );
        assert!(full_release_id("branch", &[0x11; 32], &[0x22; 32]).is_err());
    }
}
