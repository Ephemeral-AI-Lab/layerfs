use crate::objects::Reader;
use layerfs_content::{
    filesystem::{FilesystemRootId, InodeScope},
    inode_leaf::InodeKind,
    FilesystemRead, ObjectId,
};
use layerfs_telemetry::timer::Timing;
use std::collections::BTreeSet;
pub fn candidate(
    reader: &Reader,
    root: ObjectId,
    scope: InodeScope,
    profile: ObjectId,
) -> Result<(), String> {
    let mut fs = FilesystemRead::new(reader, FilesystemRootId(root)).map_err(|e| e.to_string())?;
    if fs.root().scope() != scope || fs.root().profile() != profile {
        return Err("candidate scope/profile mismatch".into());
    }
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
                let mut after = None;
                loop {
                    let page = fs
                        .list_inode(id, after.as_ref(), 128, 16384)
                        .map_err(|e| e.to_string())?;
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
                let (result, _) =
                    Timing::disabled("candidate authenticated graph validation", |t| {
                        layerfs_content::read_all_bounded(
                            reader,
                            resolved.value.content_root,
                            i64::MAX as u64,
                            &mut std::io::sink(),
                            t.child("stream"),
                        )
                    });
                result.map_err(|e| e.to_string())?;
            }
            InodeKind::Symlink => {
                return Err("symlink capability unsupported in first profile".into())
            }
        }
    }
    Ok(())
}
