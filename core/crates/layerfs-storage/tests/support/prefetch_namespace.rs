//! Independent finite v1 namespace bytes, partitions and final-state ledgers.
//!
//! No candidate canonical builder/codec/read provides an expected object or ID.
//! Two inode leaves have the explicit inherited65/66 partition; appends preserve
//! that split while changing the right leaf, as the frozen v1 local update does.

use std::collections::BTreeMap;

use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{ObjectId, ObjectRole};

pub const DIRECTORIES: u64 = 65;
pub const FIRST_FILE: u64 = 67;
pub const LAST_BASE: u64 = 131;
pub const NEW_FILE: u64 = 200;
pub const PAYLOAD: &[u8] = b"independent validation prefetch payload";
const PROFILE: &[u8] = b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5";
const MAPPING: [u8; 32] = [
    0xe9, 0x92, 0x88, 0xf3, 0xbc, 0x4a, 0xde, 0xa6, 0x90, 0x1b, 0xcb, 0xb2, 0xb1, 0x4c, 0x16, 0xf5,
    0xf5, 0x73, 0xc9, 0xcb, 0x43, 0x63, 0x09, 0xa6, 0xcc, 0x73, 0xd5, 0x1d, 0xeb, 0x33, 0x5a, 0x72,
];

pub struct Object {
    pub role: ObjectRole,
    pub canonical: Vec<u8>,
    pub references: Vec<ObjectId>,
}

pub struct Fixture {
    pub objects: BTreeMap<ObjectId, Object>,
    pub order: Vec<ObjectId>,
    pub scope: ObjectId,
    pub root: ObjectId,
    pub inode_root: ObjectId,
    pub leaves: [ObjectId; 2],
    pub file_root: ObjectId,
    pub attributes: ObjectId,
    pub directory_names: BTreeMap<u64, Vec<(Vec<u8>, u64)>>,
    pub values: BTreeMap<u64, InodeValue>,
}

pub fn identity(bytes: &[u8]) -> ObjectId {
    let mut digest = blake3::Hasher::new();
    digest.update(b"layerfs/object/v2\0");
    digest.update(bytes);
    ObjectId::from_bytes(digest.finalize().as_bytes()).unwrap()
}

fn envelope(value: &[u8]) -> Vec<u8> {
    let mut bytes = b"LFSO\x01".to_vec();
    bytes.extend_from_slice(&u32::try_from(value.len() + 4).unwrap().to_be_bytes());
    bytes.extend_from_slice(&u32::try_from(value.len()).unwrap().to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}

fn header(magic: &[u8; 8], role: u8, level: u8, items: usize, count: u64, bytes: u64) -> Vec<u8> {
    let mut value = magic.to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&u16::try_from(items).unwrap().to_be_bytes());
    value.extend_from_slice(&count.to_be_bytes());
    value.extend_from_slice(&bytes.to_be_bytes());
    value
}

impl Fixture {
    fn add(&mut self, role: ObjectRole, canonical: Vec<u8>, references: Vec<ObjectId>) -> ObjectId {
        let id = identity(&canonical);
        if let Some(old) = self.objects.get(&id) {
            assert_eq!(old.canonical, canonical);
            assert_eq!(old.role, role);
        } else {
            self.order.push(id);
            self.objects.insert(
                id,
                Object {
                    role,
                    canonical,
                    references,
                },
            );
        }
        id
    }

    fn directory(&mut self, rows: &[(Vec<u8>, u64)]) -> ObjectId {
        let width: usize = rows.iter().map(|(name, _)| name.len() + 10).sum();
        let mut value = header(
            b"LFS6NSP\0",
            1,
            0,
            rows.len(),
            rows.len() as u64,
            width as u64,
        );
        for (name, serial) in rows {
            value.extend_from_slice(&(name.len() as u16).to_be_bytes());
            value.extend_from_slice(name);
            value.extend_from_slice(&serial.to_be_bytes());
        }
        self.add(ObjectRole::DirectoryLeaf, envelope(&value), Vec::new())
    }

    fn inode_leaf(&mut self, rows: &[(u64, InodeValue)], malformed: bool) -> ObjectId {
        let mut value = header(
            b"LFS6INT\0",
            7,
            0,
            rows.len(),
            rows.len() as u64,
            (rows.len() * 81) as u64,
        );
        let mut references = Vec::new();
        for (serial, inode) in rows {
            value.extend_from_slice(&serial.to_be_bytes());
            value.push(match inode.kind {
                InodeKind::RegularFile => 1,
                InodeKind::Directory => 2,
                InodeKind::Symlink => 3,
            });
            value.extend_from_slice(&inode.namespace_ref_count.to_be_bytes());
            value.extend_from_slice(inode.content_root.as_bytes());
            value.extend_from_slice(inode.metadata_root.as_bytes());
            references.extend([inode.content_root, inode.metadata_root]);
        }
        // The real Store authenticates this deliberately malformed ordinary
        // object. C1 must reject its subtree-byte grammar after real grouped I/O.
        if malformed {
            value[30] ^= 1;
        }
        self.add(
            if malformed {
                ObjectRole::WholeFile
            } else {
                ObjectRole::InodeLeaf
            },
            envelope(&value),
            references,
        )
    }

    fn finish(&mut self, malformed_right: bool) {
        let rows: Vec<_> = self
            .values
            .iter()
            .map(|(serial, value)| (*serial, *value))
            .collect();
        let left = self.inode_leaf(&rows[..65], false);
        let right = self.inode_leaf(&rows[65..], malformed_right);
        self.leaves = [left, right];
        let mut branch = header(
            b"LFS6INT\0",
            8,
            1,
            2,
            rows.len() as u64,
            (rows.len() * 81) as u64,
        );
        for (upper, child) in [(rows[64].0, left), (rows.last().unwrap().0, right)] {
            branch.extend_from_slice(&upper.to_be_bytes());
            branch.extend_from_slice(child.as_bytes());
        }
        self.inode_root = self.add(
            ObjectRole::InodeBranch,
            envelope(&branch),
            vec![left, right],
        );
        let mut root = b"LFS6FSR\0".to_vec();
        root.extend_from_slice(&1u16.to_be_bytes());
        root.extend_from_slice(&[6, 0]);
        root.extend_from_slice(identity(PROFILE).as_bytes());
        root.extend_from_slice(self.scope.as_bytes());
        root.extend_from_slice(&1u64.to_be_bytes());
        root.extend_from_slice(self.inode_root.as_bytes());
        self.root = self.add(
            ObjectRole::FilesystemRoot,
            envelope(&root),
            vec![self.inode_root],
        );
    }

    pub fn new(aliases: usize, new_files: bool, malformed_right: bool) -> Self {
        let zero = ObjectId::from_bytes(&[0; 32]).unwrap();
        let mut scope = b"layerfs/inode-scope/v1\0".to_vec();
        scope.extend(0u8..32);
        let mut fixture = Self {
            objects: BTreeMap::new(),
            order: Vec::new(),
            scope: identity(&scope),
            root: zero,
            inode_root: zero,
            leaves: [zero; 2],
            file_root: zero,
            attributes: zero,
            directory_names: BTreeMap::new(),
            values: BTreeMap::new(),
        };
        let mut chunk = b"LFS4CHK\0".to_vec();
        chunk.extend_from_slice(PAYLOAD);
        let chunk = fixture.add(ObjectRole::Chunk, envelope(&chunk), Vec::new());
        let mut map = b"LFS4MAP\0".to_vec();
        map.extend_from_slice(&3u16.to_be_bytes());
        map.extend_from_slice(&[8, 0, 0]);
        map.extend_from_slice(&1u16.to_be_bytes());
        map.extend_from_slice(&(PAYLOAD.len() as u64).to_be_bytes());
        map.extend_from_slice(&1u64.to_be_bytes());
        map.extend_from_slice(chunk.as_bytes());
        map.extend_from_slice(&0u32.to_be_bytes());
        map.extend_from_slice(&(PAYLOAD.len() as u32).to_be_bytes());
        let map = fixture.add(ObjectRole::ExtentLeaf, envelope(&map), vec![chunk]);
        let mut state = b"LFS4MAP\0".to_vec();
        state.extend_from_slice(&3u16.to_be_bytes());
        state.extend_from_slice(&[10, 0]);
        state.extend_from_slice(&(PAYLOAD.len() as u64).to_be_bytes());
        state.extend_from_slice(&1u64.to_be_bytes());
        state.push(0);
        state.extend_from_slice(&MAPPING);
        state.extend_from_slice(map.as_bytes());
        fixture.file_root = fixture.add(ObjectRole::FileState, envelope(&state), vec![map]);
        fixture.attributes = fixture.add(
            ObjectRole::AttributeLeaf,
            envelope(&header(b"LFS4MET\0", 9, 0, 0, 0, 0)),
            Vec::new(),
        );
        let root_names: Vec<_> = (0..DIRECTORIES)
            .map(|index| (format!("d{index:03}").into_bytes(), index + 2))
            .collect();
        fixture.directory_names.insert(1, root_names);
        for index in 0..DIRECTORIES {
            let parent = index + 2;
            let mut names: Vec<_> = (0..aliases)
                .map(|alias| (format!("f{alias:03}").into_bytes(), FIRST_FILE + index))
                .collect();
            if new_files {
                names.extend((0..8).map(|new| (format!("g{new:03}").into_bytes(), NEW_FILE + new)));
            }
            fixture.directory_names.insert(parent, names);
        }
        let names = fixture.directory_names.clone();
        for (serial, names) in names {
            let content_root = fixture.directory(&names);
            fixture.values.insert(
                serial,
                InodeValue {
                    kind: InodeKind::Directory,
                    namespace_ref_count: u64::from(serial != 1),
                    content_root,
                    metadata_root: fixture.attributes,
                },
            );
        }
        for index in 0..DIRECTORIES {
            fixture.values.insert(
                FIRST_FILE + index,
                InodeValue {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: aliases as u64,
                    content_root: fixture.file_root,
                    metadata_root: fixture.attributes,
                },
            );
        }
        if new_files {
            for serial in NEW_FILE..NEW_FILE + 8 {
                fixture.values.insert(
                    serial,
                    InodeValue {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: DIRECTORIES,
                        content_root: fixture.file_root,
                        metadata_root: fixture.attributes,
                    },
                );
            }
        }
        fixture.finish(malformed_right);
        fixture
    }

    pub fn canonical(&self, id: ObjectId) -> &[u8] {
        &self.objects.get(&id).unwrap().canonical
    }
    pub fn is_inode(&self, id: ObjectId) -> bool {
        id == self.inode_root || self.leaves.contains(&id)
    }
}
