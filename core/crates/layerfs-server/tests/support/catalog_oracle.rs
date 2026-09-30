//! Independent small v1 namespace encoder for the admission tests.
//!
//! Only the BLAKE3 ObjectId primitive is shared with the product. No C1 builder,
//! page codec, candidate root, Store row or candidate result supplies an expected
//! page. Metadata roots are immutable prerequisite inputs selected before Stage.
use layerfs_bridge::contract::Root;
use layerfs_content::ObjectId;

fn object(value: &[u8]) -> Root {
    let mut bytes = b"LFSO\x01".to_vec();
    bytes.extend_from_slice(&(value.len() as u32 + 4).to_be_bytes());
    bytes.extend_from_slice(&(value.len() as u32).to_be_bytes());
    bytes.extend_from_slice(value);
    ObjectId::for_bytes(&bytes).to_bytes()
}

pub fn file(bytes: &[u8]) -> Root {
    assert!(!bytes.is_empty() && bytes.len() < 131_072);
    let mut value = b"LFS5SML\0".to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(bytes);
    object(&value)
}

fn leaf(magic: &[u8; 8], role: u8, count: usize, rows: &[u8]) -> Root {
    let mut value = magic.to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(&[role, 0, 0]);
    value.extend_from_slice(&(count as u16).to_be_bytes());
    value.extend_from_slice(&(count as u64).to_be_bytes());
    value.extend_from_slice(&(rows.len() as u64).to_be_bytes());
    value.extend_from_slice(rows);
    object(&value)
}

/// Serial and immutable content/metadata inputs of one regular file.
pub struct File<'a> {
    pub name: &'a [u8],
    pub serial: u64,
    pub content: Root,
    pub metadata: Root,
}

pub fn namespace(scope: Root, root_metadata: Root, files: &[File<'_>]) -> Root {
    assert!(files.len() < 50);
    assert!(files.windows(2).all(|pair| pair[0].name < pair[1].name));
    let mut rows = Vec::new();
    for file in files {
        rows.extend_from_slice(&(file.name.len() as u16).to_be_bytes());
        rows.extend_from_slice(file.name);
        rows.extend_from_slice(&file.serial.to_be_bytes());
    }
    let directory = leaf(b"LFS6NSP\0", 1, files.len(), &rows);
    let mut inodes = vec![(1, 2, 0, directory, root_metadata)];
    inodes.extend(
        files
            .iter()
            .map(|file| (file.serial, 1, 1, file.content, file.metadata)),
    );
    inodes.sort_by_key(|inode| inode.0);
    let mut rows = Vec::new();
    for (serial, kind, references, content, metadata) in inodes {
        rows.extend_from_slice(&serial.to_be_bytes());
        rows.push(kind);
        rows.extend_from_slice(&(references as u64).to_be_bytes());
        rows.extend_from_slice(&content);
        rows.extend_from_slice(&metadata);
    }
    let inode_root = leaf(b"LFS6INT\0", 7, files.len() + 1, &rows);
    let profile = ObjectId::for_bytes(b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5");
    let mut root = b"LFS6FSR\0".to_vec();
    root.extend_from_slice(&1u16.to_be_bytes());
    root.extend_from_slice(&[6, 0]);
    root.extend_from_slice(profile.as_bytes());
    root.extend_from_slice(&scope);
    root.extend_from_slice(&1u64.to_be_bytes());
    root.extend_from_slice(&inode_root);
    object(&root)
}
