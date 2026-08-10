//! Persistent FsCas object-locator record and incumbent policy.
//!
//! This module owns the frozen `LFSOBJ01` codec and the semantic binding from
//! one typed physical-object identifier to one sealed carrier index entry.
//! It has no filesystem or publication authority: `cas::fs` performs the
//! fallible open and atomic no-replace installation, while
//! `cas::locator_index` is only an operation-private rollover lookup.

use crate::identity::{ObjectChecksumV1, PackIdV1};
use crate::object::TypedPhysicalObjectIdV1;
use crate::pack::{PackIndexEntryV1, SealedPackV1};

pub(crate) const PERSISTENT_LOCATOR_BYTES_V1: usize = 160;

const PERSISTENT_LOCATOR_MAGIC_V1: &[u8; 8] = b"LFSOBJ01";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PersistentObjectLocatorV1 {
    sealed: SealedPackV1,
    entry: PackIndexEntryV1,
    transaction: u64,
}

impl PersistentObjectLocatorV1 {
    pub(super) const fn new(
        sealed: SealedPackV1,
        entry: PackIndexEntryV1,
        transaction: u64,
    ) -> Self {
        Self {
            sealed,
            entry,
            transaction,
        }
    }

    pub(super) const fn sealed(self) -> SealedPackV1 {
        self.sealed
    }

    pub(super) const fn entry(self) -> PackIndexEntryV1 {
        self.entry
    }

    pub(super) const fn transaction(self) -> u64 {
        self.transaction
    }

    /// An already authenticated locator for the same candidate carrier and
    /// canonical pack entry is the same persistent binding. Any mismatch is
    /// a malformed/colliding binding and must be handled fail closed by the
    /// filesystem owner.
    pub(super) fn matches_binding(self, sealed: SealedPackV1, entry: PackIndexEntryV1) -> bool {
        self.sealed == sealed && self.entry == entry
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PersistentLocatorCodecErrorV1 {
    Malformed,
}

pub(super) fn encode_persistent_locator_v1(
    locator: PersistentObjectLocatorV1,
) -> [u8; PERSISTENT_LOCATOR_BYTES_V1] {
    let mut bytes = [0_u8; PERSISTENT_LOCATOR_BYTES_V1];
    bytes[..8].copy_from_slice(PERSISTENT_LOCATOR_MAGIC_V1);
    bytes[8] = typed_kind_byte(locator.entry.id());
    bytes[16..48].copy_from_slice(locator.entry.id().as_bytes());
    bytes[48..80].copy_from_slice(locator.sealed.id().as_bytes());
    bytes[80..88].copy_from_slice(&locator.sealed.pack_len().to_be_bytes());
    bytes[88..92].copy_from_slice(&locator.sealed.record_count().to_be_bytes());
    bytes[96..104].copy_from_slice(&locator.sealed.index_offset().to_be_bytes());
    bytes[104..112].copy_from_slice(&locator.entry.absolute_offset().to_be_bytes());
    bytes[112..116].copy_from_slice(&locator.entry.object_len().to_be_bytes());
    bytes[120..152].copy_from_slice(locator.entry.object_checksum().as_bytes());
    bytes[152..160].copy_from_slice(&locator.transaction.to_be_bytes());
    bytes
}

pub(super) fn decode_persistent_locator_v1(
    bytes: [u8; PERSISTENT_LOCATOR_BYTES_V1],
    expected: TypedPhysicalObjectIdV1,
) -> Result<PersistentObjectLocatorV1, PersistentLocatorCodecErrorV1> {
    if &bytes[..8] != PERSISTENT_LOCATOR_MAGIC_V1
        || bytes[8] != typed_kind_byte(expected)
        || bytes[9..16] != [0_u8; 7]
        || bytes[16..48] != *expected.as_bytes()
        || bytes[92..96] != [0_u8; 4]
        || bytes[116..120] != [0_u8; 4]
    {
        return Err(PersistentLocatorCodecErrorV1::Malformed);
    }
    let pack_id = <[u8; 32]>::try_from(&bytes[48..80])
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    let pack_len = u64::from_be_bytes(
        bytes[80..88]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let record_count = u32::from_be_bytes(
        bytes[88..92]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let index_offset = u64::from_be_bytes(
        bytes[96..104]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let absolute_offset = u64::from_be_bytes(
        bytes[104..112]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let object_len = u32::from_be_bytes(
        bytes[112..116]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    let checksum = <[u8; 32]>::try_from(&bytes[120..152])
        .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?;
    let transaction = u64::from_be_bytes(
        bytes[152..160]
            .try_into()
            .map_err(|_| PersistentLocatorCodecErrorV1::Malformed)?,
    );
    Ok(PersistentObjectLocatorV1::new(
        SealedPackV1::from_validated_parts(
            PackIdV1::from_digest(pack_id),
            pack_len,
            record_count,
            index_offset,
        ),
        PackIndexEntryV1::from_validated_parts(
            expected,
            absolute_offset,
            object_len,
            ObjectChecksumV1::from_digest(checksum),
        ),
        transaction,
    ))
}

const fn typed_kind_byte(id: TypedPhysicalObjectIdV1) -> u8 {
    match id {
        TypedPhysicalObjectIdV1::VersionRecord(_) => 1,
        TypedPhysicalObjectIdV1::Tree(_) => 2,
        TypedPhysicalObjectIdV1::File(_) => 3,
        TypedPhysicalObjectIdV1::Symlink(_) => 4,
        TypedPhysicalObjectIdV1::Chunk(_) => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{ObjectChecksumV1, PackIdV1, PhysicalFileIdV1};

    fn fixture() -> (PersistentObjectLocatorV1, TypedPhysicalObjectIdV1) {
        let id = TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest([0x11; 32]));
        let sealed = SealedPackV1::from_validated_parts(
            PackIdV1::from_digest([0x22; 32]),
            0x0102_0304_0506_0708,
            0x090a_0b0c,
            0x1112_1314_1516_1718,
        );
        let entry = PackIndexEntryV1::from_validated_parts(
            id,
            0x2122_2324_2526_2728,
            0x3132_3334,
            ObjectChecksumV1::from_digest([0x44; 32]),
        );
        (
            PersistentObjectLocatorV1::new(sealed, entry, 0x5152_5354_5556_5758),
            id,
        )
    }

    #[test]
    fn persistent_locator_frozen_bytes_round_trip_exactly() {
        let (locator, id) = fixture();
        let encoded = encode_persistent_locator_v1(locator);
        assert_eq!(&encoded[..8], b"LFSOBJ01");
        assert_eq!(encoded[8], 3);
        assert_eq!(&encoded[9..16], &[0; 7]);
        assert_eq!(&encoded[16..48], &[0x11; 32]);
        assert_eq!(&encoded[48..80], &[0x22; 32]);
        assert_eq!(&encoded[80..88], &0x0102_0304_0506_0708_u64.to_be_bytes());
        assert_eq!(&encoded[88..92], &0x090a_0b0c_u32.to_be_bytes());
        assert_eq!(&encoded[92..96], &[0; 4]);
        assert_eq!(&encoded[96..104], &0x1112_1314_1516_1718_u64.to_be_bytes());
        assert_eq!(&encoded[104..112], &0x2122_2324_2526_2728_u64.to_be_bytes());
        assert_eq!(&encoded[112..116], &0x3132_3334_u32.to_be_bytes());
        assert_eq!(&encoded[116..120], &[0; 4]);
        assert_eq!(&encoded[120..152], &[0x44; 32]);
        assert_eq!(&encoded[152..160], &0x5152_5354_5556_5758_u64.to_be_bytes());
        assert_eq!(decode_persistent_locator_v1(encoded, id), Ok(locator));
        assert!(locator.matches_binding(locator.sealed(), locator.entry()));
    }

    #[test]
    fn persistent_locator_rejects_wrong_type_or_reserved_bytes() {
        let (locator, id) = fixture();
        let mut encoded = encode_persistent_locator_v1(locator);
        encoded[9] = 1;
        assert_eq!(
            decode_persistent_locator_v1(encoded, id),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );

        let encoded = encode_persistent_locator_v1(locator);
        let wrong = TypedPhysicalObjectIdV1::Chunk(
            crate::identity::PhysicalChunkIdV1::from_digest([0x11; 32]),
        );
        assert_eq!(
            decode_persistent_locator_v1(encoded, wrong),
            Err(PersistentLocatorCodecErrorV1::Malformed)
        );
    }
}
