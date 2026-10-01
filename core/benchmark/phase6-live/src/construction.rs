use crate::{
    edits::Edits,
    engine::{Engine, Source},
    prepared::Prepared,
};
use layerfs_content::{
    filesystem::{
        attributes::{
            build_attribute_tree, value::emit_value, AttributeEntry, AttributeKey, PortableMetadata,
        },
        profile_id, scope_for_seed, FilesystemRootId,
    },
    inode_leaf::{InodeKind, InodeValue},
    *,
};
use layerfs_telemetry::timer::Timing;

pub fn metadata(
    reader: &dyn AuthenticatedObjects,
    consumer: &mut dyn FinalizedConsumer,
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
#[derive(Clone, Copy, Default, Debug)]
pub struct ConstructionWork {
    pub dirty_inodes: usize,
    pub directories: usize,
    pub new_inodes: usize,
    pub file_edits: usize,
    pub changed_names: usize,
    pub prepared_queries: crate::prepared::QueryCounts,
}
pub fn build(
    engine: &Engine,
    scope: InodeScope,
    base: FilesystemRootId,
    reader: &dyn AuthenticatedObjects,
    consumer: &mut dyn FinalizedConsumer,
) -> Result<(FilesystemResult, ConstructionWork), String> {
    let query_start = engine.row_queries.get();
    engine
        .db
        .execute("DELETE FROM prepared", [])
        .map_err(|e| e.to_string())?;
    let mut old = FilesystemRead::new(reader, base).map_err(|e| e.to_string())?;
    let mut work = ConstructionWork::default();
    let mut after = 0;
    while let Some(n) = engine.next_dirty(after)? {
        if work.dirty_inodes >= 512 {
            return Err("changed inode admission 512".into());
        }
        after = n.id;
        let kind = InodeKind::from_code(n.kind).map_err(|e| e.to_string())?;
        let content = if n.kind == 1 {
            let policy = ConstructionPolicy::default();
            let (result, _) = Timing::disabled("file.construct", |t| {
                if let Some(root) = n.root.as_deref() {
                    let root = ObjectId::from_bytes(root)?;
                    let edits = Edits::prepare(engine, n.id).map_err(|_| ContentError::Io)?;
                    work.file_edits += edits.len();
                    apply_edits(
                        policy,
                        &policy.capacities(),
                        reader,
                        EditRequest {
                            root,
                            edits: &edits,
                            source: &edits,
                        },
                        consumer,
                        t.child("edit"),
                    )
                } else {
                    construct_stream(
                        policy,
                        &policy.capacities(),
                        Source {
                            engine,
                            id: n.id,
                            at: 0,
                        },
                        consumer,
                        t.child("stream"),
                    )
                }
            });
            result.map_err(|e| e.to_string())?.root
        } else if n.is_new {
            // A real empty directory root is the typed initial content; C1 merges
            // this operation's final names rather than trusting a placeholder.
            layerfs_content::filesystem::directory::update::empty_directory(
                &mut FilesystemObjects::new(reader, consumer),
            )
            .map_err(|e| e.to_string())?
            .0
        } else {
            old.resolve_inode(n.id as u64)
                .map_err(|e| e.to_string())?
                .value
                .content_root
        };
        let meta = metadata(reader, consumer, n.mode, n.seconds, n.nanos, kind)?;
        let pos = if n.is_new {
            Some(work.new_inodes as i64)
        } else {
            None
        };
        engine
            .db
            .execute(
                "INSERT INTO prepared VALUES(?1,?2,?3,?4,?5)",
                rusqlite::params![
                    n.id,
                    n.kind,
                    content.as_bytes().as_slice(),
                    meta.as_bytes().as_slice(),
                    pos
                ],
            )
            .map_err(|e| e.to_string())?;
        work.dirty_inodes += 1;
        work.directories += usize::from(kind == InodeKind::Directory);
        work.new_inodes += usize::from(n.is_new);
    }
    let prepared = Prepared {
        engine,
        root: base,
        inode_scope: scope,
        counts: (work.directories, work.dirty_inodes, work.new_inodes),
    };
    use layerfs_content::filesystem::rows::RowSource;
    let mut dirs = prepared.directories().map_err(|e| e.to_string())?;
    while let Some(dir) = dirs.next_row().map_err(|e| e.to_string())? {
        work.changed_names += dir.changes.len();
    }
    let result = update_filesystem(
        &mut FilesystemObjects::new(reader, consumer),
        &prepared,
        None,
    )
    .map_err(|e| e.to_string())?;
    work.prepared_queries = engine.row_queries.get().since(query_start);
    Ok((result, work))
}
pub fn genesis(
    reader: &dyn AuthenticatedObjects,
    consumer: &mut dyn FinalizedConsumer,
) -> Result<FilesystemResult, String> {
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
