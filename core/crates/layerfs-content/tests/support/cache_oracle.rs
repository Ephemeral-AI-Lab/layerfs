//! Independent finite v1 fixture for retained-cache and navigation-owner proofs.
//!
//! Encoding below follows the frozen envelope/mapping specification directly.
//! It calls neither C1 canonical encoders/builders nor a candidate reader to
//! determine expected bytes, IDs, profile, counts or partitions.

#![allow(dead_code)]

use std::collections::BTreeMap;

use layerfs_content::file::mapping::FileState;
use layerfs_content::{ObjectId, ObjectRole};

pub const LEAVES: usize = 65;
pub const LEAF_EXTENTS: usize = 128;
pub const EXTENT_BYTES: usize = 32;
pub const LEAF_LOGICAL_BYTES: usize = LEAF_EXTENTS * EXTENT_BYTES;
pub const FIRST_WAVE_BYTES: usize = 32 * LEAF_LOGICAL_BYTES;
pub const LEAF_CANONICAL_BYTES: usize = 44 + LEAF_EXTENTS * 40;
pub const ROOT_CANONICAL_BYTES: usize = 44 + LEAVES * 48;

/// Frozen v1 mapping profile, independently pinned before the cache change.
const MAPPING_PROFILE: [u8; 32] = [
    0xe9, 0x92, 0x88, 0xf3, 0xbc, 0x4a, 0xde, 0xa6, 0x90, 0x1b, 0xcb, 0xb2, 0xb1, 0x4c, 0x16, 0xf5,
    0xf5, 0x73, 0xc9, 0xcb, 0x43, 0x63, 0x09, 0xa6, 0xcc, 0x73, 0xd5, 0x1d, 0xeb, 0x33, 0x5a, 0x72,
];

pub struct Fixture {
    pub objects: BTreeMap<ObjectId, Vec<u8>>,
    pub state: FileState,
    pub file_root: ObjectId,
    pub payload_id: ObjectId,
    pub leaves: Vec<ObjectId>,
    pub dummy: Vec<ObjectId>,
    pub payload: Vec<u8>,
    pub expected: Vec<u8>,
}

impl Fixture {
    pub fn canonical(&self, id: ObjectId) -> &[u8] {
        self.objects.get(&id).expect("independent fixture ID")
    }

    pub fn role(&self, id: ObjectId) -> ObjectRole {
        let canonical = self.canonical(id);
        if &canonical[13..21] == b"LFS4CHK\0" {
            ObjectRole::Chunk
        } else {
            match canonical[23] {
                8 => ObjectRole::ExtentLeaf,
                9 => ObjectRole::ExtentBranch,
                10 => ObjectRole::FileState,
                _ => panic!("independent fixture role"),
            }
        }
    }

    /// Exact known cache keys; payload/FileState are not navigation pages.
    pub fn mapping_keys(&self) -> impl Iterator<Item = (ObjectId, bool)> + '_ {
        std::iter::once((self.state.mapping_root, true))
            .chain(self.leaves.iter().copied().map(|id| (id, false)))
            .chain(self.dummy.iter().copied().map(|id| (id, true)))
    }

    /// root + first-wave leaf0 +32 later leaves +30 independent dummy leaves.
    pub fn mixed_prefill(&self) -> impl Iterator<Item = (ObjectId, bool)> + '_ {
        std::iter::once((self.state.mapping_root, true))
            .chain(std::iter::once((self.leaves[0], false)))
            .chain(self.leaves[33..65].iter().copied().map(|id| (id, false)))
            .chain(self.dummy.iter().copied().map(|id| (id, true)))
    }
}

fn identity(canonical: &[u8]) -> ObjectId {
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/object/v2\0");
    hash.update(canonical);
    ObjectId::from_bytes(hash.finalize().as_bytes()).expect("fixed digest width")
}

fn envelope(value: &[u8]) -> Vec<u8> {
    let mut canonical = Vec::with_capacity(value.len() + 13);
    canonical.extend_from_slice(b"LFSO\x01");
    canonical.extend_from_slice(&u32::try_from(value.len() + 4).unwrap().to_be_bytes());
    canonical.extend_from_slice(&u32::try_from(value.len()).unwrap().to_be_bytes());
    canonical.extend_from_slice(value);
    canonical
}

fn header(role: u8, level: u8, entries: usize, logical: usize, extents: usize) -> Vec<u8> {
    let mut value = Vec::with_capacity(31);
    value.extend_from_slice(b"LFS4MAP\0");
    value.extend_from_slice(&3u16.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&u16::try_from(entries).unwrap().to_be_bytes());
    value.extend_from_slice(&u64::try_from(logical).unwrap().to_be_bytes());
    value.extend_from_slice(&u64::try_from(extents).unwrap().to_be_bytes());
    value
}

fn leaf(payload: ObjectId, shift: usize) -> Vec<u8> {
    let mut value = header(8, 0, LEAF_EXTENTS, LEAF_LOGICAL_BYTES, LEAF_EXTENTS);
    for index in 0..LEAF_EXTENTS {
        let offset = index * 63 + shift;
        assert!(offset + EXTENT_BYTES <= 8192);
        value.extend_from_slice(payload.as_bytes());
        value.extend_from_slice(&u32::try_from(offset).unwrap().to_be_bytes());
        value.extend_from_slice(&u32::try_from(EXTENT_BYTES).unwrap().to_be_bytes());
    }
    envelope(&value)
}

fn add(objects: &mut BTreeMap<ObjectId, Vec<u8>>, canonical: Vec<u8>) -> ObjectId {
    let id = identity(&canonical);
    assert!(
        objects.insert(id, canonical).is_none(),
        "distinct fixture objects"
    );
    id
}

/// 65 distinct full non-root leaves, one valid root branch and30 dummy pages.
///
/// Extents are32 bytes with source starts63 bytes apart. Maximum source end is
/// 8,097 for the real leaves and8,158 for the dummy leaves; none can coalesce.
/// Logical length266,240 keeps the ordinary edit route above the frozen cutoff.
pub fn wide() -> Fixture {
    let payload: Vec<u8> = (0..8192)
        .map(|i| ((i * 73 + (i >> 3) * 19 + (i >> 8)) % 251) as u8)
        .collect();
    let mut value = Vec::with_capacity(8 + payload.len());
    value.extend_from_slice(b"LFS4CHK\0");
    value.extend_from_slice(&payload);
    let mut objects = BTreeMap::new();
    let payload_id = add(&mut objects, envelope(&value));
    let mut leaves = Vec::with_capacity(LEAVES);
    let mut expected = Vec::with_capacity(LEAVES * LEAF_LOGICAL_BYTES);
    for shift in 0..LEAVES {
        leaves.push(add(&mut objects, leaf(payload_id, shift)));
        for index in 0..LEAF_EXTENTS {
            let offset = index * 63 + shift;
            expected.extend_from_slice(&payload[offset..offset + EXTENT_BYTES]);
        }
    }
    let dummy: Vec<ObjectId> = (96..126)
        .map(|shift| add(&mut objects, leaf(payload_id, shift)))
        .collect();
    let logical_len = LEAVES * LEAF_LOGICAL_BYTES;
    let extent_count = LEAVES * LEAF_EXTENTS;
    let mut branch = header(9, 1, LEAVES, logical_len, extent_count);
    for (index, id) in leaves.iter().enumerate() {
        branch.extend_from_slice(
            &u64::try_from((index + 1) * LEAF_LOGICAL_BYTES)
                .unwrap()
                .to_be_bytes(),
        );
        branch.extend_from_slice(
            &u64::try_from((index + 1) * LEAF_EXTENTS)
                .unwrap()
                .to_be_bytes(),
        );
        branch.extend_from_slice(id.as_bytes());
    }
    let mapping_root = add(&mut objects, envelope(&branch));
    let profile_id = ObjectId::from_bytes(&MAPPING_PROFILE).unwrap();
    let state = FileState {
        logical_len: u64::try_from(logical_len).unwrap(),
        extent_count: u64::try_from(extent_count).unwrap(),
        tree_level: 1,
        profile_id,
        mapping_root,
    };
    let mut state_value = Vec::with_capacity(93);
    state_value.extend_from_slice(b"LFS4MAP\0");
    state_value.extend_from_slice(&3u16.to_be_bytes());
    state_value.extend_from_slice(&[10, 0]);
    state_value.extend_from_slice(&state.logical_len.to_be_bytes());
    state_value.extend_from_slice(&state.extent_count.to_be_bytes());
    state_value.push(1);
    state_value.extend_from_slice(&MAPPING_PROFILE);
    state_value.extend_from_slice(mapping_root.as_bytes());
    let file_root = add(&mut objects, envelope(&state_value));
    Fixture {
        objects,
        state,
        file_root,
        payload_id,
        leaves,
        dummy,
        payload,
        expected,
    }
}
