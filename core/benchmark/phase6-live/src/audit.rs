use crate::objects::Reader;
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    inode_leaf::InodeKind,
    AuthenticatedObjects, FilesystemRead, ObjectId,
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
        if resolved.value.namespace_ref_count != u64::from(id != 1) {
            return Err("candidate namespace reference count".into());
        }
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
    let (table_count, _, table_pages) =
        table_members(reader, fs.root().inode_table(), &seen, true, None, 0)?;
    if table_count != seen.len() as u64 {
        return Err("candidate inode table membership count".into());
    }
    eprintln!("P6_INODE_MEMBERSHIP rows={table_count} pages={table_pages}");
    eprintln!("P6_NAMESPACE_AUDIT inodes={} directories={directory_count} files={file_count} bindings={binding_count} file_cert={file_work:?}",seen.len());
    Ok(())
}

fn table_members(
    reader: &Reader,
    id: ObjectId,
    seen: &BTreeSet<u64>,
    root: bool,
    expected_level: Option<u8>,
    depth: usize,
) -> Result<(u64, u64, u64), String> {
    use layerfs_content::filesystem::inode::codec::{decode_inode_page, filled_page, InodePage};
    if depth > 31 {
        return Err("candidate inode depth".into());
    }
    let canonical = reader.read_canonical(id).map_err(|e| e.to_string())?;
    let page = decode_inode_page(&canonical).map_err(|e| e.to_string())?;
    if expected_level.is_some_and(|level| page.level() != level) {
        return Err("candidate inode child level".into());
    }
    match page {
        InodePage::Leaf { entries } => {
            if !root && !filled_page(entries.len(), 0) {
                return Err("candidate inode nonroot fill".into());
            }
            for (serial, value) in &entries {
                if !seen.contains(serial) {
                    return Err("candidate unreachable inode record".into());
                }
                value.validate(*serial == 1).map_err(|e| e.to_string())?;
            }
            Ok((entries.len() as u64, entries.last().map_or(0, |r| r.0), 1))
        }
        InodePage::Branch {
            level,
            subtree_count,
            children,
        } => {
            if (root && children.len() < 2) || (!root && !filled_page(children.len(), level)) {
                return Err("candidate inode branch fill".into());
            }
            let mut count = 0;
            let mut maximum = 0;
            let mut pages = 1;
            for (key, child) in children {
                let (n, max, p) =
                    table_members(reader, child, seen, false, Some(level - 1), depth + 1)?;
                if max != key || key <= maximum {
                    return Err("candidate inode child maximum".into());
                }
                count += n;
                maximum = key;
                pages += p;
                if count > 512 {
                    return Err("candidate inode membership admission".into());
                }
            }
            if count != subtree_count {
                return Err("candidate inode subtree count".into());
            }
            Ok((count, maximum, pages))
        }
    }
}
