use crate::{CaptureSink, MaterializationError, Result};
use layerfs_content::{CanonicalName, CanonicalPath};
use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

pub fn capture(source: &Path, sink: &mut dyn CaptureSink) -> Result<()> {
    if !source.is_dir() {
        return Err(MaterializationError::Invalid("capture directory"));
    }
    let metadata = std::fs::symlink_metadata(source)?;
    sink.reset(
        metadata.permissions().mode(),
        metadata.mtime(),
        metadata.mtime_nsec() as u32,
    )?;
    walk(source, &CanonicalPath::root(), sink, &mut HashMap::new())
}

fn walk(
    native: &Path,
    logical: &CanonicalPath,
    sink: &mut dyn CaptureSink,
    hard_links: &mut HashMap<(u64, u64), CanonicalPath>,
) -> Result<()> {
    let mut entries = std::fs::read_dir(native)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by(|left, right| {
        left.file_name()
            .as_bytes()
            .cmp(right.file_name().as_bytes())
    });
    for entry in entries {
        let name = CanonicalName::from_bytes(entry.file_name().as_bytes())?;
        let path = child(logical, &name)?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        let identity = (metadata.dev(), metadata.ino());
        if metadata.nlink() > 1 {
            if let Some(source) = hard_links.get(&identity) {
                sink.hard_link(source, &path)?;
                continue;
            }
        }
        if metadata.file_type().is_dir() {
            sink.directory(
                &path,
                metadata.permissions().mode(),
                metadata.mtime(),
                metadata.mtime_nsec() as u32,
            )?;
            walk(&entry.path(), &path, sink, hard_links)?;
        } else if metadata.file_type().is_file() {
            sink.file(
                &path,
                &mut std::fs::File::open(entry.path())?,
                metadata.permissions().mode(),
                metadata.mtime(),
                metadata.mtime_nsec() as u32,
            )?;
        } else if metadata.file_type().is_symlink() {
            sink.symlink(
                &path,
                std::fs::read_link(entry.path())?
                    .as_os_str()
                    .as_bytes()
                    .to_vec(),
                metadata.mtime(),
                metadata.mtime_nsec() as u32,
            )?;
        } else {
            return Err(MaterializationError::Invalid("unsupported capture entry"));
        }
        if metadata.nlink() > 1 {
            hard_links.insert(identity, path);
        }
    }
    Ok(())
}

fn child(parent: &CanonicalPath, name: &CanonicalName) -> Result<CanonicalPath> {
    let mut bytes = parent.as_bytes().to_vec();
    if !bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name.as_bytes());
    Ok(CanonicalPath::from_bytes(&bytes)?)
}
