use layerfs_branch_store::BranchStore;
use layerfs_core::inode::{InodeId, InodeKind};
use layerfs_core::logical::{self, LogicalCounters};
use layerfs_core::{CanonicalName, CanonicalPath};
use layerfs_storage_core::{BranchId, CoreReader, Result, StorageError};
use std::collections::HashMap;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

pub fn materialize(store: &BranchStore, branch_id: BranchId, destination: &Path) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    if std::fs::read_dir(destination)?.next().is_some() {
        return Err(StorageError::InvalidInput("materialization destination"));
    }
    let reader = CoreReader(store);
    let mut inodes = HashMap::new();
    walk(
        &reader,
        store.root(branch_id)?,
        &CanonicalPath::root(),
        destination,
        &mut inodes,
    )
}

fn walk(
    store: &CoreReader<'_>,
    root: layerfs_core::ObjectId,
    logical_path: &CanonicalPath,
    destination: &Path,
    inodes: &mut HashMap<InodeId, PathBuf>,
) -> Result<()> {
    let mut after: Option<CanonicalName> = None;
    loop {
        let (page, _) = logical::list(store, root, logical_path, after.as_ref(), 128, 256 * 1024)?;
        for (name, inode) in page.entries {
            let logical_path = child(logical_path, &name)?;
            let path = destination.join(std::ffi::OsStr::from_bytes(name.as_bytes()));
            let resolved =
                logical::resolve(store, root, &logical_path, &mut LogicalCounters::default())?;
            match resolved.record.kind {
                InodeKind::Directory => {
                    std::fs::create_dir(&path)?;
                    walk(store, root, &logical_path, &path, inodes)?;
                }
                InodeKind::RegularFile => {
                    if let Some(source) = inodes.get(&inode) {
                        std::fs::hard_link(source, &path)?;
                    } else {
                        let mut file = std::fs::File::create(&path)?;
                        logical::stream(store, root, &logical_path, &mut file)?;
                        inodes.insert(inode, path);
                    }
                }
                InodeKind::Symlink => {
                    if let Some(source) = inodes.get(&inode) {
                        std::fs::hard_link(source, &path)?;
                    } else {
                        let target = logical::readlink(store, root, &logical_path)?.0;
                        symlink(std::ffi::OsStr::from_bytes(&target), &path)?;
                        inodes.insert(inode, path);
                    }
                }
            }
        }
        let Some(next) = page.continuation else {
            return Ok(());
        };
        after = Some(next);
    }
}

fn child(parent: &CanonicalPath, name: &CanonicalName) -> Result<CanonicalPath> {
    let mut bytes = parent.as_bytes().to_vec();
    if !bytes.is_empty() {
        bytes.push(b'/');
    }
    bytes.extend_from_slice(name.as_bytes());
    Ok(CanonicalPath::from_bytes(&bytes)?)
}
