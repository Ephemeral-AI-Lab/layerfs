//! Literal byte expectations through public C1 and real SQLite backing.
//! The bounded in-memory canonical fixture is not a real-provider speed proof.
use layerfs_content::{
    filesystem::{scope_for_seed, FilesystemRootId, LogicalPath},
    *,
};
use layerfs_telemetry::timer::Timing;
use phase6_live_probe::{construction, edits::Edits, engine::Engine};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default)]
struct Objects(Arc<Mutex<BTreeMap<ObjectId, Vec<u8>>>>);
impl AuthenticatedObjects for Objects {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let values = self.0.lock().unwrap();
        ids.iter()
            .map(|id| values.get(id).cloned().ok_or(ContentError::MissingObject))
            .collect()
    }
}
impl FinalizedConsumer for Objects {
    fn accept(&mut self, o: FinalizedObject) -> ContentResult<()> {
        assert_eq!(ObjectId::for_bytes(o.canonical()), o.id());
        self.0
            .lock()
            .unwrap()
            .insert(o.id(), o.canonical().to_vec());
        Ok(())
    }
}
fn fresh() -> (PathBuf, Engine, Objects, FilesystemRootId) {
    let path = std::env::temp_dir().join(format!(
        "p6-construction-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::create_dir(&path).unwrap();
    let mut engine = Engine::create(&path, 2).unwrap();
    let mut objects = Objects::default();
    engine.reader = Some(Arc::new(objects.clone()));
    let root = construction::genesis(&objects.clone(), &mut objects)
        .unwrap()
        .root;
    (path, engine, objects, root)
}
fn build(
    engine: &Engine,
    objects: &mut Objects,
    base: FilesystemRootId,
) -> (FilesystemResult, construction::ConstructionWork) {
    construction::build(
        engine,
        scope_for_seed([6; 32]),
        base,
        &objects.clone(),
        objects,
    )
    .unwrap()
}
fn payload(objects: &Objects, root: FilesystemRootId, path: &str) -> Vec<u8> {
    let mut read = FilesystemRead::new(objects, root).unwrap();
    let file = read
        .resolve(&LogicalPath::new(path).unwrap())
        .unwrap()
        .value
        .content_root;
    let mut out = Vec::new();
    let (result, _) = Timing::disabled("proof", |t| {
        read_all(objects, file, &mut out, t.child("bytes"))
    });
    result.unwrap();
    out
}
#[test]
fn published_overlay_truncate_regrow_and_ordinal_eof() {
    let (path, mut engine, mut objects, base) = fresh();
    let id = engine.create_node(1, b"file", 1, 0o644).unwrap().id;
    engine.write(id, 0, b"abcdefghij").unwrap();
    let (first, work) = build(&engine, &mut objects, base);
    assert_eq!(
        (work.dirty_inodes, work.new_inodes, work.changed_names),
        (2, 1, 1)
    );
    engine.install_prepared().unwrap();
    engine.write(id, 2, b"XY").unwrap();
    engine.truncate(id, 6).unwrap();
    engine.truncate(id, 12).unwrap();
    engine.write(id, 9, b"Z").unwrap();
    let edits = Edits::prepare(&engine, id).unwrap();
    assert_eq!(edits.len(), 2);
    assert_eq!(edits.edit_at(0).unwrap(), Edit::overwrite(2, 4));
    assert_eq!(edits.edit_at(1).unwrap(), Edit::new(6, 10, 6));
    let mut out = [0; 16];
    assert_eq!(edits.read_at(0, 0, &mut out).unwrap(), 2);
    assert_eq!(&out[..2], b"XY");
    assert_eq!(edits.read_at(0, 2, &mut out).unwrap(), 0);
    assert_eq!(edits.read_at(1, 0, &mut out).unwrap(), 6);
    assert_eq!(&out[..6], b"\0\0\0Z\0\0");
    assert!(edits.edit_at(2).is_err());
    let (second, work) = build(&engine, &mut objects, first.root);
    assert_eq!(
        (
            work.dirty_inodes,
            work.new_inodes,
            work.changed_names,
            work.file_edits
        ),
        (1, 0, 0, 2)
    );
    assert_eq!(payload(&objects, second.root, "file"), b"abXYef\0\0\0Z\0\0");
    assert_eq!(payload(&objects, first.root, "file"), b"abcdefghij");
    engine.install_prepared().unwrap();
    let mut out = [0; 16];
    assert_eq!(engine.read(id, 0, &mut out).unwrap(), 12);
    assert_eq!(&out[..12], b"abXYef\0\0\0Z\0\0");
    let (clean, work) = build(&engine, &mut objects, second.root);
    assert_eq!(work.dirty_inodes, 0);
    assert_eq!(clean.root, second.root);
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn changed_names_and_one_dirty_file_preserve_unrelated_values() {
    let (path, mut engine, mut objects, base) = fresh();
    let dir = engine.create_node(1, b"dir", 2, 0o755).unwrap().id;
    let a = engine.create_node(dir, b"a", 1, 0o644).unwrap().id;
    let b = engine.create_node(dir, b"b", 1, 0o600).unwrap().id;
    engine.write(a, 0, b"alpha").unwrap();
    engine.write(b, 0, b"beta").unwrap();
    let (first, _) = build(&engine, &mut objects, base);
    engine.install_prepared().unwrap();
    let original = FilesystemRead::new(&objects, first.root)
        .unwrap()
        .resolve_inode(b as u64)
        .unwrap()
        .value;
    engine.write(a, 0, b"ALPHA").unwrap();
    let (second, work) = build(&engine, &mut objects, first.root);
    assert_eq!(
        (work.dirty_inodes, work.directories, work.changed_names),
        (1, 0, 0)
    );
    assert_eq!(
        FilesystemRead::new(&objects, second.root)
            .unwrap()
            .resolve_inode(b as u64)
            .unwrap()
            .value,
        original
    );
    engine.install_prepared().unwrap();
    engine.unlink(dir, b"a", false).unwrap();
    let (third, work) = build(&engine, &mut objects, second.root);
    assert_eq!(
        (work.dirty_inodes, work.directories, work.changed_names),
        (2, 1, 1)
    );
    assert_eq!(work.removals, 1);
    assert!(FilesystemRead::new(&objects, third.root)
        .unwrap()
        .resolve(&LogicalPath::new("dir/a").unwrap())
        .is_err());
    assert_eq!(payload(&objects, third.root, "dir/b"), b"beta");
    engine.install_prepared().unwrap();
    assert_eq!(
        engine
            .db
            .query_row("SELECT count(*) FROM changed_names", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn large_immediate_file_edit_keeps_tail_and_old_root() {
    let (path, mut engine, mut objects, base) = fresh();
    let id = engine.create_node(1, b"large", 1, 0o644).unwrap().id;
    let mut bytes = vec![0; 1024 * 1024];
    let mut state = 17u64;
    for byte in &mut bytes {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    for (i, block) in bytes.chunks(128 * 1024).enumerate() {
        engine.write(id, (i * 128 * 1024) as i64, block).unwrap();
    }
    let (first, _) = build(&engine, &mut objects, base);
    engine.install_prepared().unwrap();
    engine.write(id, 500_000, b"LOCAL").unwrap();
    let edits = Edits::prepare(&engine, id).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits.edit_at(0).unwrap(), Edit::overwrite(500_000, 500_005));
    let before = engine.reads.get();
    let (second, work) = build(&engine, &mut objects, first.root);
    assert_eq!(work.file_edits, 1);
    let served = engine.reads.get().since(before);
    assert!(
        served.local > 0 && served.local < 4096,
        "one edit must not serve a megabyte as replacement: {served:?}"
    );
    assert_eq!(served.base, 0);
    let mut expected = bytes.clone();
    expected[500_000..500_005].copy_from_slice(b"LOCAL");
    assert_eq!(payload(&objects, second.root, "large"), expected);
    assert_eq!(payload(&objects, first.root, "large"), bytes);
    engine.install_prepared().unwrap();
    engine.truncate(id, 120_000).unwrap();
    let (small, _) = build(&engine, &mut objects, second.root);
    assert_eq!(payload(&objects, small.root, "large"), expected[..120_000]);
    engine.install_prepared().unwrap();
    engine.truncate(id, 150_000).unwrap();
    let (large_again, _) = build(&engine, &mut objects, small.root);
    let mut regrown = expected[..120_000].to_vec();
    regrown.resize(150_000, 0);
    assert_eq!(payload(&objects, large_again.root, "large"), regrown);
    drop(engine);
    std::fs::remove_dir_all(path).unwrap();
}

fn legacy_complete(engine: &Engine, objects: &mut Objects) -> FilesystemRootId {
    use layerfs_content::inode_leaf::{InodeKind, InodeValue};
    let mut ids = Vec::new();
    let mut q = engine
        .db
        .prepare("SELECT id FROM inodes WHERE links>0 ORDER BY id")
        .unwrap();
    for r in q.query_map([], |r| r.get::<_, i64>(0)).unwrap() {
        ids.push(r.unwrap())
    }
    let reader = objects.clone();
    let mut inodes = Vec::new();
    let mut dirs = Vec::new();
    for id in &ids {
        let n = engine.node(*id).unwrap();
        let kind = InodeKind::from_code(n.kind).unwrap();
        let content = if n.kind == 1 {
            let (r, _) = Timing::disabled("reference whole small file", |t| {
                construct_stream(
                    ConstructionPolicy::default(),
                    &ConstructionPolicy::default().capacities(),
                    phase6_live_probe::engine::Source {
                        engine,
                        id: *id,
                        at: 0,
                    },
                    objects,
                    t.child("file"),
                )
            });
            r.unwrap().root
        } else {
            let changes = engine
                .directory_entries(*id)
                .unwrap()
                .into_iter()
                .map(|(i, name, _)| {
                    (
                        PathName::new(std::str::from_utf8(&name).unwrap()).unwrap(),
                        Some(i as u64),
                    )
                })
                .collect();
            dirs.push(DirectoryUpdate {
                parent: *id as u64,
                changes,
            });
            ObjectId::for_bytes(b"reference placeholder replaced by actual C1 directory")
        };
        let meta =
            construction::metadata(&reader, objects, n.mode, n.seconds, n.nanos, kind).unwrap();
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
    let new: Vec<_> = ids.iter().map(|i| *i as u64).collect();
    let input = FilesystemInput {
        base: None,
        scope: scope_for_seed([6; 32]),
        root_serial: 1,
        directories: &dirs,
        inodes: &inodes,
        new_inodes: &new,
        resources: FilesystemResources::default(),
    };
    build_filesystem(&mut FilesystemObjects::new(&reader, objects), &input, None)
        .unwrap()
        .root
}
#[test]
fn sql_native_namespace_matches_legacy_and_consumes270names_once() {
    let (p, mut e, mut objects, base) = fresh();
    let mut parent = 1;
    for _ in 0..270 {
        parent = e.create_node(parent, b"d", 2, 0o755).unwrap().id;
    }
    let file = e.create_node(parent, b"file", 1, 0o644).unwrap().id;
    e.write(file, 0, b"leaf").unwrap();
    let reference = legacy_complete(&e, &mut objects);
    let (first, w) = build(&e, &mut objects, base);
    assert_eq!(first.root, reference);
    assert_eq!(w.changed_names, 271);
    assert_eq!(w.prepared_queries.name_rows, 271);
    assert!(w.prepared_queries.directory_lookups < 600);
    assert_eq!(w.prepared_queries.fresh_lookups, 0);
    e.install_prepared().unwrap();
    e.write(file, 0, b"NEXT").unwrap();
    let (next, w) = build(&e, &mut objects, first.root);
    assert_eq!(w.dirty_inodes, 1);
    assert_eq!(next.root, legacy_complete(&e, &mut objects));
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn tombstone_open_orphan_and_fresh_cancel_have_no_lifetime_scan() {
    let (p, mut e, mut objects, base) = fresh();
    let dir = e.create_node(1, b"dir", 2, 0o755).unwrap().id;
    let f = e.create_node(dir, b"file", 1, 0o644).unwrap().id;
    e.write(f, 0, b"OLD").unwrap();
    let (first, _) = build(&e, &mut objects, base);
    e.install_prepared().unwrap();
    let h = e.open(f, libc::O_RDWR).unwrap();
    e.unlink(dir, b"file", false).unwrap();
    let (second, w) = build(&e, &mut objects, first.root);
    assert_eq!(w.removals, 1);
    assert_eq!(second.root, legacy_complete(&e, &mut objects));
    e.install_prepared().unwrap();
    e.write(f, 0, b"FD!").unwrap();
    let mut b = [0; 3];
    e.read(f, 0, &mut b).unwrap();
    assert_eq!(&b, b"FD!");
    assert!(e.next_dirty(0).unwrap().is_none());
    let temp = e.create_node(dir, b"temp", 1, 0o644).unwrap().id;
    e.write(temp, 0, b"cancelled").unwrap();
    e.unlink(dir, b"temp", false).unwrap();
    let (third, w) = build(&e, &mut objects, second.root);
    assert_eq!(w.removals, 0);
    assert_eq!(third.root, legacy_complete(&e, &mut objects));
    e.install_prepared().unwrap();
    assert!(e.next_dirty(0).unwrap().is_none());
    e.close(h, f).unwrap();
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn moved_directory_replacement_and_new_empty_match_legacy_namespace() {
    let (p, mut e, mut objects, base) = fresh();
    let a = e.create_node(1, b"a", 2, 0o755).unwrap().id;
    let b = e.create_node(1, b"b", 2, 0o755).unwrap().id;
    let dir = e.create_node(a, b"tree", 2, 0o755).unwrap().id;
    let f = e.create_node(dir, b"data", 1, 0o644).unwrap().id;
    e.write(f, 0, b"old").unwrap();
    e.create_node(1, b"empty", 2, 0o755).unwrap();
    let (first, _) = build(&e, &mut objects, base);
    assert_eq!(first.root, legacy_complete(&e, &mut objects));
    e.install_prepared().unwrap();
    e.rename(a, b"tree", b, b"moved", false).unwrap();
    let replacement = e.create_node(dir, b"next", 1, 0o644).unwrap().id;
    e.write(replacement, 0, b"new").unwrap();
    e.rename(dir, b"next", dir, b"data", false).unwrap();
    let (second, w) = build(&e, &mut objects, first.root);
    assert_eq!(w.removals, 1);
    assert_eq!(second.root, legacy_complete(&e, &mut objects));
    assert_eq!(payload(&objects, second.root, "b/moved/data"), b"new");
    assert_eq!(payload(&objects, first.root, "a/tree/data"), b"old");
    e.install_prepared().unwrap();
    assert!(e.next_dirty(0).unwrap().is_none());
    drop(e);
    std::fs::remove_dir_all(p).unwrap();
}
