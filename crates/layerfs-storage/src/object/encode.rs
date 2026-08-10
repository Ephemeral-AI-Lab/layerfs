//! Canonical `ELSOBJ01` envelope encoding.
//!
//! Payload owners stream their semantic fields, but every object kind uses
//! this single frozen envelope encoder so Create, Update, COW tree building,
//! version construction, admission, and readers cannot drift on magic,
//! schema, kind, profile binding, reserved flags, or payload length.

use crate::format::PhysicalObjectKindV1;
use crate::profile::ProfileSpecV1;

pub const OBJECT_HEADER_BYTES: u64 = 52;
pub const VERSION_RECORD_PAYLOAD_BYTES: u64 = 184;

pub(super) const CANONICAL_OBJECT_MAGIC_V1: &[u8; 8] = b"ELSOBJ01";

pub(crate) fn encode_physical_object_header_v1(
    kind: PhysicalObjectKindV1,
    payload_len: u64,
) -> [u8; OBJECT_HEADER_BYTES as usize] {
    let mut header = [0_u8; OBJECT_HEADER_BYTES as usize];
    header[..8].copy_from_slice(CANONICAL_OBJECT_MAGIC_V1);
    header[8..10].copy_from_slice(&1_u16.to_be_bytes());
    header[10] = match kind {
        PhysicalObjectKindV1::VersionRecord => 0x01,
        PhysicalObjectKindV1::Tree => 0x02,
        PhysicalObjectKindV1::File => 0x03,
        PhysicalObjectKindV1::Symlink => 0x04,
        PhysicalObjectKindV1::Chunk => 0x05,
    };
    header[11] = 0;
    header[12..44].copy_from_slice(ProfileSpecV1::frozen().id().as_bytes());
    header[44..52].copy_from_slice(&payload_len.to_be_bytes());
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_envelope_bytes_are_frozen_for_every_kind() {
        for (kind, tag) in [
            (PhysicalObjectKindV1::VersionRecord, 1),
            (PhysicalObjectKindV1::Tree, 2),
            (PhysicalObjectKindV1::File, 3),
            (PhysicalObjectKindV1::Symlink, 4),
            (PhysicalObjectKindV1::Chunk, 5),
        ] {
            let header = encode_physical_object_header_v1(kind, 0x0102_0304_0506_0708);
            assert_eq!(&header[..8], b"ELSOBJ01");
            assert_eq!(&header[8..10], &1_u16.to_be_bytes());
            assert_eq!(header[10], tag);
            assert_eq!(header[11], 0);
            assert_eq!(&header[12..44], ProfileSpecV1::frozen().id().as_bytes());
            assert_eq!(&header[44..52], &0x0102_0304_0506_0708_u64.to_be_bytes());
        }
    }
}
