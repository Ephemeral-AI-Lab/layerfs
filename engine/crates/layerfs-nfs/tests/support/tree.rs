//! A host tree as an oracle, and a protocol walk that checks an export
//! against it: every path, kind, mode, byte, link target, listing, `..`,
//! directory link count and the identity behind hard links.
use super::client::Client;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// What the host tree holds, read before the export runs.
#[derive(Debug)]
pub enum Node {
    File { mode: u32, bytes: Vec<u8>, ino: u64 },
    Directory { mode: u32, names: BTreeSet<Vec<u8>> },
    Link { target: Vec<u8> },
}
pub fn oracle(root: &Path) -> BTreeMap<PathBuf, Node> {
    use std::os::unix::ffi::OsStrExt;
    let mut nodes = BTreeMap::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let path = root.join(&relative);
        let meta = fs::symlink_metadata(&path).unwrap();
        let node = if meta.file_type().is_symlink() {
            Node::Link {
                target: fs::read_link(&path)
                    .unwrap()
                    .as_os_str()
                    .as_bytes()
                    .to_vec(),
            }
        } else if meta.is_dir() {
            let mut names = BTreeSet::new();
            for entry in fs::read_dir(&path).unwrap() {
                let name = entry.unwrap().file_name();
                pending.push(relative.join(&name));
                names.insert(name.as_bytes().to_vec());
            }
            Node::Directory {
                mode: meta.mode() & 0o7777,
                names,
            }
        } else {
            Node::File {
                mode: meta.mode() & 0o7777,
                bytes: fs::read(&path).unwrap(),
                ino: meta.ino(),
            }
        };
        nodes.insert(relative, node);
    }
    nodes
}

/// Walks the export from `handle` and checks every node against the oracle.
/// Returns (paths, regular bytes read) and the fileid of every host inode.
pub fn walk(
    client: &mut Client,
    oracle: &BTreeMap<PathBuf, Node>,
    budget: u32,
    owner: (u32, u32),
) -> (usize, usize, BTreeMap<u64, BTreeSet<u64>>) {
    use std::os::unix::ffi::OsStrExt;
    let root = client.mnt("/");
    let root_attr = client.getattr(&root).unwrap();
    let mut identities: BTreeMap<u64, BTreeSet<u64>> = BTreeMap::new();
    let (mut paths, mut bytes) = (0, 0);
    let mut pending = vec![(PathBuf::new(), root, root_attr.clone(), root_attr.fileid)];
    while let Some((relative, handle, attr, parent)) = pending.pop() {
        paths += 1;
        assert_eq!((attr.uid, attr.gid), owner, "{relative:?}");
        match &oracle[&relative] {
            Node::File {
                mode,
                bytes: expected,
                ino,
            } => {
                assert_eq!(attr.kind, 1, "{relative:?}");
                assert_eq!(attr.mode, *mode, "{relative:?} mode");
                assert_eq!(attr.size, expected.len() as u64, "{relative:?} size");
                let actual = client.read_all(&handle, 128 * 1024);
                assert_eq!(&actual, expected, "{relative:?} bytes");
                bytes += actual.len();
                identities.entry(*ino).or_default().insert(attr.fileid);
            }
            Node::Link { target } => {
                assert_eq!(attr.kind, 5, "{relative:?}");
                assert_eq!(&client.readlink(&handle).unwrap(), target, "{relative:?}");
            }
            Node::Directory { mode, names } => {
                assert_eq!(attr.kind, 2, "{relative:?}");
                assert_eq!(attr.mode, *mode, "{relative:?} mode");
                let (entries, _) = client.list(&handle, budget, false);
                let mut listed = BTreeSet::new();
                for entry in &entries {
                    assert!(listed.insert(entry.name.clone()), "{relative:?} duplicate");
                }
                let dot = entries.iter().find(|entry| entry.name == b".").unwrap();
                let dotdot = entries.iter().find(|entry| entry.name == b"..");
                assert_eq!(dot.fileid, attr.fileid, "{relative:?} .");
                if let Some(dotdot) = dotdot {
                    assert_eq!(dotdot.fileid, parent, "{relative:?} ..");
                }
                listed.remove(&b"."[..]);
                listed.remove(&b".."[..]);
                assert_eq!(&listed, names, "{relative:?} complete listing");
                let (up, up_attr) = client.lookup(&handle, b"..").unwrap();
                assert_eq!(up_attr.fileid, parent, "{relative:?} LOOKUP ..");
                drop(up);
                // Subdirectory link counts: two plus child directories.
                let subdirectories = names
                    .iter()
                    .filter(|name| {
                        matches!(
                            oracle.get(&relative.join(std::ffi::OsStr::from_bytes(name))),
                            Some(Node::Directory { .. })
                        )
                    })
                    .count();
                assert_eq!(
                    attr.nlink as usize,
                    subdirectories + 2,
                    "{relative:?} nlink"
                );
                for name in names {
                    let (child, child_attr) = client.lookup(&handle, name).unwrap();
                    let child_path = relative.join(std::ffi::OsStr::from_bytes(name));
                    pending.push((child_path, child, child_attr, attr.fileid));
                }
            }
        }
    }
    (paths, bytes, identities)
}
