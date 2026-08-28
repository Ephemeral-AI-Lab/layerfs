use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Eq, PartialEq)]
struct TreeEntry {
    kind: u8,
    mode: u32,
    modified: (i64, i64),
    body: Vec<u8>,
    hard_link_group: Option<usize>,
}

pub(super) fn read_range(
    path: &Path,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let mut file = fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; length];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn deterministic_byte(index: usize) -> u8 {
    (index as u64).wrapping_mul(0x9e37_79b9) as u8
}

pub(super) fn assert_tree_equal(
    left: &Path,
    right: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let left_tree = snapshot_tree(left)?;
    let right_tree = snapshot_tree(right)?;
    if left_tree != right_tree {
        return Err("exact tree metadata/topology oracle mismatch".into());
    }
    for (relative, entry) in left_tree {
        if entry.kind == 2 && !files_equal(&left.join(&relative), &right.join(relative))? {
            return Err("exact tree file-byte oracle mismatch".into());
        }
    }
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, std::io::Error> {
    let mut left = fs::File::open(left)?;
    let mut right = fs::File::open(right)?;
    let mut left_buffer = vec![0; 1024 * 1024];
    let mut right_buffer = vec![0; 1024 * 1024];
    loop {
        let left_count = left.read(&mut left_buffer)?;
        let right_count = right.read(&mut right_buffer)?;
        if left_count != right_count || left_buffer[..left_count] != right_buffer[..right_count] {
            return Ok(false);
        }
        if left_count == 0 {
            return Ok(true);
        }
    }
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<PathBuf, TreeEntry>, std::io::Error> {
    fn walk(
        root: &Path,
        relative: &Path,
        links: &mut BTreeMap<(u64, u64), usize>,
        output: &mut BTreeMap<PathBuf, TreeEntry>,
    ) -> Result<(), std::io::Error> {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        let (kind, body, hard_link_group) = if file_type.is_dir() {
            (0, Vec::new(), None)
        } else if file_type.is_symlink() {
            (
                1,
                fs::read_link(&path)?.as_os_str().as_bytes().to_vec(),
                None,
            )
        } else {
            let key = (metadata.dev(), metadata.ino());
            let next = links.len();
            let group = *links.entry(key).or_insert(next);
            (2, Vec::new(), Some(group))
        };
        output.insert(
            relative.to_path_buf(),
            TreeEntry {
                kind,
                mode: metadata.mode(),
                modified: (metadata.mtime(), metadata.mtime_nsec()),
                body,
                hard_link_group,
            },
        );
        if file_type.is_dir() {
            let mut children = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
            children.sort_by_key(|entry| entry.file_name());
            for child in children {
                walk(root, &relative.join(child.file_name()), links, output)?;
            }
        }
        Ok(())
    }

    let mut output = BTreeMap::new();
    walk(root, Path::new(""), &mut BTreeMap::new(), &mut output)?;
    Ok(output)
}
