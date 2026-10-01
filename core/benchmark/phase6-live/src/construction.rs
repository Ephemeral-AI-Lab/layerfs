use crate::{
    edits::Edits,
    engine::{Engine, Source},
    namespace_stream::{Names, Values},
};
use layerfs_content::{
    filesystem::{
        attributes::{
            build_attribute_tree, value::emit_value, AttributeEntry, AttributeKey, PortableMetadata,
        },
        profile_id, scope_for_seed, FilesystemRootId, FilesystemUpdateCounters,
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
    pub removals: usize,
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
    if old.root().scope() != scope
        || old.root().root_inode().serial() != 1
        || old.root().profile() != profile_id()
    {
        return Err("selected namespace scope/profile/root mismatch".into());
    }
    let mut counters = FilesystemUpdateCounters::default();
    let mut work = ConstructionWork::default();
    let mut after = 0;
    while let Some(n) = engine.next_dirty(after)? {
        if work.dirty_inodes >= 512 {
            return Err("changed inode admission 512".into());
        }
        after = n.id;
        if n.links == 0 {
            if !n.published {
                return Err("unpublished orphan in capture".into());
            }
            engine
                .db
                .execute("INSERT INTO prepared VALUES(?1,0,NULL,NULL,NULL)", [n.id])
                .map_err(|e| e.to_string())?;
            work.removals += 1;
            work.dirty_inodes += 1;
            continue;
        }
        if n.links != 1 || ![1, 2].contains(&n.kind) {
            return Err("single-link regular/directory profile".into());
        }
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
        } else {
            let base_directory = if n.published {
                Some(layerfs_content::filesystem::DirectoryRoot(
                    old.resolve_inode(n.id as u64)
                        .map_err(|e| e.to_string())?
                        .value
                        .content_root,
                ))
            } else {
                None
            };
            let changes_exist: bool = engine
                .db
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM changed_names WHERE parent=?1)",
                    [n.id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let (directory, dwork) = if base_directory.is_none() && !changes_exist {
                (
                    layerfs_content::filesystem::directory::update::empty_directory(
                        &mut FilesystemObjects::new(reader, consumer),
                    )
                    .map_err(|e| e.to_string())?,
                    layerfs_content::filesystem::SortedWork {
                        pages_created: 1,
                        ..Default::default()
                    },
                )
            } else {
                layerfs_content::filesystem::directory::update::apply_bindings(
                    &mut FilesystemObjects::new(reader, consumer),
                    base_directory,
                    Names {
                        engine,
                        parent: n.id,
                        after: Vec::new(),
                        done: false,
                    },
                    FilesystemResources::default().scratch_bytes,
                    &mut |_, _| Ok(()),
                )
                .map_err(|e| e.to_string())?
            };
            counters.directories.pages_read += dwork.pages_read;
            counters.directories.read_waves += dwork.read_waves;
            counters.directories.pages_created += dwork.pages_created;
            counters.directories.pages_reused += dwork.pages_reused;
            counters.directories.change_keys += dwork.change_keys;
            counters.directories.untouched_subtrees += dwork.untouched_subtrees;
            counters.directories.peak_scratch_bytes = counters
                .directories
                .peak_scratch_bytes
                .max(dwork.peak_scratch_bytes);
            counters.directory_updates += 1;
            work.changed_names += dwork.change_keys as usize;
            directory.0
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
    let mut objects = FilesystemObjects::new(reader, consumer);
    let (inode_table, iwork) = layerfs_content::filesystem::inode::update::apply_inode_values(
        &mut objects,
        Some(old.root().inode_table()),
        Values {
            engine,
            after: 0,
            done: false,
        },
        FilesystemResources::default().scratch_bytes,
    )
    .map_err(|e| e.to_string())?;
    counters.inodes = iwork;
    let value = old.root().with_inode_table(inode_table);
    let root = objects
        .emit(
            FinalizedObject::new(
                ObjectRole::FilesystemRoot,
                value.encode().map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?
            .with_references(vec![inode_table]),
        )
        .map_err(|e| e.to_string())?;
    counters.objects = objects.work();
    let result = FilesystemResult {
        root: FilesystemRootId(root),
        value,
        counters,
    };
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
