use crate::{
    engine::{Engine, Source},
    objects::{Consumer, Reader},
};
use layerfs_content::{
    filesystem::{
        attributes::{
            build_attribute_tree, value::emit_value, AttributeEntry, AttributeKey, PortableMetadata,
        },
        profile_id, scope_for_seed,
    },
    inode_leaf::{InodeKind, InodeValue},
    *,
};
use layerfs_telemetry::timer::Timing;

pub fn metadata(
    reader: &Reader,
    consumer: &mut Consumer,
    mode: u32,
    sec: i64,
    nano: u32,
    kind: InodeKind,
) -> Result<ObjectId, String> {
    let portable = PortableMetadata {
        mode,
        mtime_seconds: sec,
        mtime_nanoseconds: nano,
    };
    let mut objects = FilesystemObjects::new(reader, consumer);
    let mut entries = Vec::new();
    for (key, value) in [
        (
            b"mode".as_slice(),
            portable
                .mode_bytes(kind)
                .map_err(|e| e.to_string())?
                .to_vec(),
        ),
        (
            b"mtime".as_slice(),
            portable.mtime_bytes().map_err(|e| e.to_string())?.to_vec(),
        ),
    ] {
        let root = emit_value(&mut objects, &value).map_err(|e| e.to_string())?;
        entries.push(Ok(AttributeEntry {
            key: AttributeKey::new("portable".into(), key.to_vec()).map_err(|e| e.to_string())?,
            value_root: root,
        }));
    }
    build_attribute_tree(&mut objects, entries.into_iter())
        .map(|r| r.0)
        .map_err(|e| e.to_string())
}
pub fn build(
    engine: &Engine,
    scope: InodeScope,
    reader: &Reader,
    consumer: &mut Consumer,
) -> Result<(FilesystemResult, Vec<(i64, ObjectId)>), String> {
    let ids = engine.live_ids()?;
    if ids.len() > 512 {
        return Err("construction inode admission".into());
    }
    let mut inodes = Vec::new();
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for id in &ids {
        let n = engine.node(*id)?;
        let kind = InodeKind::from_code(n.kind).map_err(|e| e.to_string())?;
        let content = if n.kind == 1 {
            if !n.dirty && n.root.is_some() {
                ObjectId::from_bytes(n.root.as_deref().unwrap()).map_err(|e| e.to_string())?
            } else {
                let (result, _) = Timing::disabled("file.construct", |t| {
                    construct_stream(
                        ConstructionPolicy::default(),
                        &ConstructionPolicy::default().capacities(),
                        Source {
                            engine,
                            id: *id,
                            at: 0,
                        },
                        consumer,
                        t.child("stream"),
                    )
                });
                let root = result.map_err(|e| e.to_string())?.root;
                files.push((*id, root));
                root
            }
        } else {
            let mut q = engine
                .db
                .prepare("SELECT name,ino FROM names WHERE parent=?1 ORDER BY name")
                .map_err(|e| e.to_string())?;
            let rows = q
                .query_map([id], |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)))
                .map_err(|e| e.to_string())?;
            let mut changes = Vec::new();
            for row in rows {
                let (name, ino) = row.map_err(|e| e.to_string())?;
                changes.push((
                    PathName::new(
                        std::str::from_utf8(&name).map_err(|_| "non UTF8 C1 name unsupported")?,
                    )
                    .map_err(|e| e.to_string())?,
                    Some(ino as u64),
                ));
            }
            if changes.len() > 512 {
                return Err("directory construction admission".into());
            }
            directories.push(DirectoryUpdate {
                parent: *id as u64,
                changes,
            });
            ObjectId::for_bytes(b"initial directory content replaced by C1")
        };
        let meta = metadata(reader, consumer, n.mode, n.seconds, n.nanos, kind)?;
        inodes.push(InodeUpdate {
            serial: *id as u64,
            value: InodeValue {
                kind,
                namespace_ref_count: u64::from(*id != 1),
                content_root: content,
                metadata_root: meta,
            },
        });
    }
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &ids.iter().map(|id| *id as u64).collect::<Vec<_>>(),
        resources: FilesystemResources::default(),
    };
    let result = build_filesystem(&mut FilesystemObjects::new(reader, consumer), &input, None)
        .map_err(|e| e.to_string())?;
    Ok((result, files))
}
pub fn genesis(reader: &Reader, consumer: &mut Consumer) -> Result<FilesystemResult, String> {
    let scope = scope_for_seed([6; 32]);
    let meta = metadata(reader, consumer, 0o755, 0, 0, InodeKind::Directory)?;
    let inodes = [InodeUpdate {
        serial: 1,
        value: InodeValue {
            kind: InodeKind::Directory,
            namespace_ref_count: 0,
            content_root: ObjectId::for_bytes(b"initial directory replaced by C1"),
            metadata_root: meta,
        },
    }];
    let directories = [DirectoryUpdate {
        parent: 1,
        changes: vec![],
    }];
    let input = FilesystemInput {
        base: None,
        scope,
        root_serial: 1,
        directories: &directories,
        inodes: &inodes,
        new_inodes: &[1],
        resources: FilesystemResources::default(),
    };
    let _ = profile_id();
    build_filesystem(&mut FilesystemObjects::new(reader, consumer), &input, None)
        .map_err(|e| e.to_string())
}
