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
        (1, 1, 1)
    );
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
