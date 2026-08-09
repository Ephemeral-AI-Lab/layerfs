//! Canonical filesystem-CAS catalog marker codec.

use super::fs::FsCasErrorV1;
use crate::identity::PackIdV1;
use crate::pack::SealedPackV1;

const CATALOG_MAGIC: &[u8; 8] = b"LFSCAT01";
pub(super) const CATALOG_MARKER_BYTES: usize = 64;

pub(super) fn encode_catalog_marker(sealed: SealedPackV1) -> [u8; CATALOG_MARKER_BYTES] {
    let mut bytes = [0_u8; CATALOG_MARKER_BYTES];
    bytes[..8].copy_from_slice(CATALOG_MAGIC);
    bytes[8..40].copy_from_slice(sealed.id().as_bytes());
    bytes[40..48].copy_from_slice(&sealed.pack_len().to_be_bytes());
    bytes[48..52].copy_from_slice(&sealed.record_count().to_be_bytes());
    bytes[56..64].copy_from_slice(&sealed.index_offset().to_be_bytes());
    bytes
}

pub(super) fn decode_catalog_marker(
    bytes: [u8; CATALOG_MARKER_BYTES],
) -> Result<SealedPackV1, FsCasErrorV1> {
    if &bytes[..8] != CATALOG_MAGIC || bytes[52..56] != [0_u8; 4] {
        return Err(FsCasErrorV1::Integrity);
    }
    let id = <[u8; 32]>::try_from(&bytes[8..40]).map_err(|_| FsCasErrorV1::Integrity)?;
    let pack_len = u64::from_be_bytes(
        bytes[40..48]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let record_count = u32::from_be_bytes(
        bytes[48..52]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    let index_offset = u64::from_be_bytes(
        bytes[56..64]
            .try_into()
            .map_err(|_| FsCasErrorV1::Integrity)?,
    );
    Ok(SealedPackV1::from_validated_parts(
        PackIdV1::from_digest(id),
        pack_len,
        record_count,
        index_offset,
    ))
}
