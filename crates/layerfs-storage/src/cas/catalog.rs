//! Canonical filesystem-CAS catalog marker codec.

use super::fs::FsCasErrorV1;
use crate::identity::PackIdV1;
use crate::pack::SealedPackV1;

const CATALOG_MAGIC: &[u8; 8] = b"LFSCAT01";
pub const CATALOG_MARKER_BYTES: usize = 64;

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
    decode_catalog_marker_with_consumed_v1(bytes).0
}

/// Decode one fixed catalog record while reporting the exact highest input
/// boundary inspected before success or failure. The caller records this
/// value only after the codec has returned, so a malformed prefix is never
/// reported as a successfully decoded complete marker.
pub(super) fn decode_catalog_marker_with_consumed_v1(
    bytes: [u8; CATALOG_MARKER_BYTES],
) -> (Result<SealedPackV1, FsCasErrorV1>, u64) {
    let mut consumed = 8_u64;
    let result = (|| {
        if &bytes[..8] != CATALOG_MAGIC {
            return Err(FsCasErrorV1::Integrity);
        }
        consumed = 56;
        if bytes[52..56] != [0_u8; 4] {
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
        consumed = CATALOG_MARKER_BYTES as u64;
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
    })();
    (result, consumed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_decoder_reports_exact_consumed_fault_boundary() {
        let sealed = SealedPackV1::from_validated_parts(
            PackIdV1::from_digest([0x11; 32]),
            0x0102_0304_0506_0708,
            0x090a_0b0c,
            0x1112_1314_1516_1718,
        );
        let encoded = encode_catalog_marker(sealed);
        assert_eq!(
            decode_catalog_marker_with_consumed_v1(encoded),
            (Ok(sealed), CATALOG_MARKER_BYTES as u64)
        );

        let mut bad_magic = encoded;
        bad_magic[0] ^= 0xff;
        assert_eq!(decode_catalog_marker_with_consumed_v1(bad_magic).1, 8);

        let mut bad_reserved = encoded;
        bad_reserved[52] = 1;
        let (result, consumed) = decode_catalog_marker_with_consumed_v1(bad_reserved);
        assert_eq!(result, Err(FsCasErrorV1::Integrity));
        assert_eq!(consumed, 56);
    }
}
