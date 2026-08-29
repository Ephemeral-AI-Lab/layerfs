use crate::{Attr, Kind, MaterializationError, MaterializationSource, NodeId, Result};
use std::collections::HashMap;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

pub fn materialize(source: &dyn MaterializationSource, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    if std::fs::read_dir(destination)?.next().is_some() {
        return Err(MaterializationError::Invalid("materialization destination"));
    }
    let root = source.root();
    let mut inodes = HashMap::new();
    walk(source, root.node, destination, &mut inodes)?;
    set_metadata(destination, root)
}

pub fn matches(source: &dyn MaterializationSource, destination: &Path) -> Result<bool> {
    if !destination.is_dir() || !same_metadata(source.root(), &std::fs::metadata(destination)?) {
        return Ok(false);
    }
    compare_directory(
        source,
        source.root().node,
        destination,
        &mut HashMap::new(),
        &mut HashMap::new(),
    )
}

fn walk(
    source: &dyn MaterializationSource,
    node: NodeId,
    destination: &Path,
    inodes: &mut HashMap<NodeId, PathBuf>,
) -> Result<()> {
    for entry in source.entries(node)? {
        let path = destination.join(std::ffi::OsStr::from_bytes(&entry.name));
        if let Some(first) = inodes.get(&entry.attr.node) {
            std::fs::hard_link(first, &path)?;
            continue;
        }
        match entry.attr.kind {
            Kind::Directory => {
                std::fs::create_dir(&path)?;
                walk(source, entry.attr.node, &path, inodes)?;
                set_metadata(&path, entry.attr)?;
            }
            Kind::File => {
                let mut file = std::fs::File::create(&path)?;
                source.read(entry.attr.node, &mut file)?;
                set_metadata(&path, entry.attr)?;
            }
            Kind::Symlink => {
                let target = source.readlink(entry.attr.node)?;
                symlink(std::ffi::OsStr::from_bytes(&target), &path)?;
            }
        }
        inodes.insert(entry.attr.node, path);
    }
    Ok(())
}

fn compare_directory(
    source: &dyn MaterializationSource,
    node: NodeId,
    destination: &Path,
    source_links: &mut HashMap<NodeId, (u64, u64)>,
    native_links: &mut HashMap<(u64, u64), NodeId>,
) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let entries = source.entries(node)?;
    let mut native = std::fs::read_dir(destination)?.collect::<std::io::Result<Vec<_>>>()?;
    native.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    if entries.len() != native.len() {
        return Ok(false);
    }
    for (entry, native) in entries.into_iter().zip(native) {
        if entry.name != native.file_name().as_bytes() {
            return Ok(false);
        }
        let path = native.path();
        let metadata = std::fs::symlink_metadata(&path)?;
        let native_id = (metadata.dev(), metadata.ino());
        if source_links
            .get(&entry.attr.node)
            .is_some_and(|expected| *expected != native_id)
            || native_links
                .get(&native_id)
                .is_some_and(|expected| *expected != entry.attr.node)
        {
            return Ok(false);
        }
        source_links.insert(entry.attr.node, native_id);
        native_links.insert(native_id, entry.attr.node);
        let kind_matches = match entry.attr.kind {
            Kind::Directory => metadata.file_type().is_dir(),
            Kind::File => metadata.file_type().is_file(),
            Kind::Symlink => metadata.file_type().is_symlink(),
        };
        if !kind_matches || !same_metadata(entry.attr, &metadata) {
            return Ok(false);
        }
        match entry.attr.kind {
            Kind::Directory => {
                if !compare_directory(source, entry.attr.node, &path, source_links, native_links)? {
                    return Ok(false);
                }
            }
            Kind::File => {
                let mut comparison = CompareWriter::new(std::fs::File::open(path)?);
                source.read(entry.attr.node, &mut comparison)?;
                if !comparison.finish()? {
                    return Ok(false);
                }
            }
            Kind::Symlink => {
                if std::fs::read_link(path)?.as_os_str().as_bytes()
                    != source.readlink(entry.attr.node)?
                {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}

fn same_metadata(attr: Attr, metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let mode = metadata.permissions().mode()
        & if attr.kind == Kind::Directory {
            0o1777
        } else {
            0o777
        };
    mode == attr.mode
        && metadata.mtime() == attr.mtime_seconds
        && metadata.mtime_nsec() as u32 == attr.mtime_nanoseconds
}

struct CompareWriter {
    native: std::fs::File,
    equal: bool,
}

impl CompareWriter {
    fn new(native: std::fs::File) -> Self {
        Self {
            native,
            equal: true,
        }
    }

    fn finish(mut self) -> std::io::Result<bool> {
        let mut byte = [0];
        Ok(self.equal && self.native.read(&mut byte)? == 0)
    }
}

impl std::io::Write for CompareWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let mut native = vec![0; bytes.len()];
        let mut read = 0;
        while read < native.len() {
            let next = self.native.read(&mut native[read..])?;
            if next == 0 {
                break;
            }
            read += next;
        }
        self.equal &= read == bytes.len() && native[..read] == *bytes;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn set_metadata(path: &Path, attr: Attr) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(attr.mode))?;
    let modified = if attr.mtime_seconds >= 0 {
        std::time::UNIX_EPOCH
            + std::time::Duration::new(attr.mtime_seconds as u64, attr.mtime_nanoseconds)
    } else {
        std::time::UNIX_EPOCH
    };
    std::fs::File::open(path)?.set_times(std::fs::FileTimes::new().set_modified(modified))?;
    Ok(())
}
