use crate::objects::Reader;
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    inode_leaf::InodeKind,
    FilesystemRead, ObjectId,
};
use std::collections::BTreeSet;
pub fn candidate(
    reader: &Reader,
    root: ObjectId,
    scope: InodeScope,
    profile: ObjectId,
    locators: &crate::metadata_catalog::LocatorDb,
) -> Result<(), String> {
    let mut fs = FilesystemRead::new(reader, FilesystemRootId(root)).map_err(|e| e.to_string())?;
    if fs.root().scope() != scope || fs.root().profile() != profile {
        return Err("candidate scope/profile mismatch".into());
    }
    let mut file_work = crate::file_facts::Work::default();
    let mut file_count = 0u64;
    let mut directory_count = 0u64;
    let mut binding_count = 0u64;
    let mut pending = vec![1u64];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) || seen.len() > 512 {
            return Err("candidate graph/admission".into());
        }
        let resolved = fs.resolve_inode(id).map_err(|e| e.to_string())?;
        fs.read_portable_inode(id).map_err(|e| e.to_string())?;
        match resolved.value.kind {
            InodeKind::Directory => {
                directory_count += 1;
                let mut after = None;
                loop {
                    let page = fs
                        .list_inode(id, after.as_ref(), 128, 16384)
                        .map_err(|e| e.to_string())?;
                    binding_count += page.entries.len() as u64;
                    for (_, child) in page.entries {
                        pending.push(child);
                        if pending.len() + seen.len() > 512 {
                            return Err("candidate node admission".into());
                        }
                    }
                    after = page.continuation;
                    if after.is_none() {
                        break;
                    }
                }
            }
            InodeKind::RegularFile => {
                let work = locators.certify_file(resolved.value.content_root)?;
                file_count += 1;
                file_work.visited += work.visited;
                file_work.reused += work.reused;
                file_work.edges += work.edges;
                file_work.newly_certified += work.newly_certified;
            }
            InodeKind::Symlink => {
                return Err("symlink capability unsupported in first profile".into())
            }
        }
    }
    eprintln!("P6_NAMESPACE_AUDIT inodes={} directories={directory_count} files={file_count} bindings={binding_count} file_cert={file_work:?}",seen.len());
    Ok(())
}
