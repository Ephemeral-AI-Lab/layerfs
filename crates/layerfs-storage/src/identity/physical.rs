//! Canonical physical object, pack, and checksum identities.

use super::framing::derive_framed_bytes;
use super::*;
use crate::format::validate_physical_object_len;
use crate::{CoreError, CoreResult};

pub fn derive_physical_chunk_id_v1(object: &[u8]) -> CoreResult<PhysicalChunkIdV1> {
    derive_physical_object(TAG_PHYSICAL_CHUNK, object).map(PhysicalChunkIdV1)
}

pub fn derive_physical_version_record_id_v1(
    object: &[u8],
) -> CoreResult<PhysicalVersionRecordIdV1> {
    derive_physical_object(TAG_PHYSICAL_VERSION_RECORD, object).map(PhysicalVersionRecordIdV1)
}

pub fn derive_physical_tree_id_v1(object: &[u8]) -> CoreResult<PhysicalTreeIdV1> {
    derive_physical_object(TAG_PHYSICAL_TREE, object).map(PhysicalTreeIdV1)
}

pub fn derive_physical_file_id_v1(object: &[u8]) -> CoreResult<PhysicalFileIdV1> {
    derive_physical_object(TAG_PHYSICAL_FILE, object).map(PhysicalFileIdV1)
}

pub fn derive_physical_symlink_id_v1(object: &[u8]) -> CoreResult<PhysicalSymlinkIdV1> {
    derive_physical_object(TAG_PHYSICAL_SYMLINK, object).map(PhysicalSymlinkIdV1)
}

#[allow(dead_code)]
pub(crate) fn derive_pack_id_v1(bytes_before_id: &[u8]) -> CoreResult<PackIdV1> {
    derive_framed_bytes(TAG_PACK, bytes_before_id).map(PackIdV1)
}

#[allow(dead_code)]
pub(crate) fn derive_object_checksum_v1(object: &[u8]) -> CoreResult<ObjectChecksumV1> {
    derive_framed_bytes(TAG_OBJECT_CHECKSUM, object).map(ObjectChecksumV1)
}

fn derive_physical_object(tag: u8, object: &[u8]) -> CoreResult<[u8; DIGEST_BYTES]> {
    let object_len = u64::try_from(object.len()).map_err(|_| CoreError::IntegerOverflow)?;
    validate_physical_object_len(object_len)?;
    derive_framed_bytes(tag, object)
}
