//! Disposable serverless Store proofs through the real public provider.
mod support;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, HistoryError, ReserveRequest};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
use layerfs_storage::{
    port::{PackPersistence, PersistenceError, Reserve},
    Storage, StorageError, StoragePolicy,
};
use std::{
    io::{Read, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"pre-s8-store".to_vec(),
        cursor_key: [71; 32],
        incarnation: 7,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn create(path: &Path) -> Handles {
    Handles::create(config(path), StoragePolicy::frozen_default(), &authority()).unwrap()
}
fn open(path: &Path) -> Handles {
    Handles::open_writable(
        config(path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap()
}
fn object() -> FinalizedObject {
    FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(b"one complete immutable value")
            .unwrap(),
    )
    .unwrap()
}
fn put(storage: &Storage) {
    let save = storage.begin_save().unwrap();
    save.accept(object()).unwrap();
    assert_eq!(save.finish().unwrap().inserted, 1);
}
fn check(storage: &Storage) {
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object().id()])
            .unwrap(),
        vec![object().canonical().to_vec()]
    );
}

/// The test harness invokes this body in a real second process. Its socket
/// timeout ends the original transaction even if its parent disappears.
#[test]
fn lock_child() {
    let Some(path) = std::env::var_os("LAYERFS_STORE_LOCK_CHILD") else {
        return;
    };
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.busy_timeout(Duration::ZERO).unwrap();
    let mut socket =
        UnixStream::connect(std::env::var_os("LAYERFS_STORE_LOCK_SOCKET").unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    sql.execute_batch(
        "BEGIN IMMEDIATE; UPDATE store_policy SET next_pack_id=next_pack_id+1000000 WHERE id=1;",
    )
    .unwrap();
    socket.write_all(b"L").unwrap();
    let mut release = [0];
    let released = socket.read_exact(&mut release);
    sql.execute_batch("ROLLBACK").unwrap();
    released.unwrap();
}

struct Held {
    child: Child,
    socket: UnixStream,
}
impl Held {
    fn start(path: &Path, socket_path: &Path) -> Self {
        let listener = UnixListener::bind(socket_path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "lock_child", "--nocapture"])
            .env("LAYERFS_STORE_LOCK_CHILD", path)
            .env("LAYERFS_STORE_LOCK_SOCKET", socket_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("child readiness: {error}");
                }
            }
        };
        let mut held = Self { child, socket };
        held.socket.set_nonblocking(false).unwrap();
        held.socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        held.socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut ready = [0];
        held.socket.read_exact(&mut ready).unwrap();
        assert_eq!(ready, *b"L");
        held
    }
    fn release(mut self) {
        self.socket.write_all(b"R").unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < deadline, "child exit deadline");
            std::thread::yield_now();
        }
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn disposable_create_publish_seal_and_reopen_all_layouts() {
    for layout in [
        SqlitePackLayout::Monolithic,
        SqlitePackLayout::GroupRows,
        SqlitePackLayout::GroupRowsIndexed,
    ] {
        let temp = support::Temp::new("serverless-layout");
        let path = temp.join("store.sqlite");
        let handles = Handles::create(
            config(&path).with_sqlite_pack_layout(layout),
            StoragePolicy::frozen_default(),
            &authority(),
        )
        .unwrap();
        let profile = handles.profile();
        assert_eq!(profile.identity, "sqlite-wal-off-v2");
        assert_eq!(
            (
                &*profile.journal_mode,
                profile.synchronous,
                profile.busy_timeout
            ),
            ("wal", 0, 0)
        );
        assert_eq!(
            (
                profile.page_size,
                profile.foreign_keys,
                profile.wal_autocheckpoint,
                profile.journal_size_limit
            ),
            (4096, 1, 1000, 4194304)
        );
        assert_eq!(
            (profile.cache_size, profile.mmap_size, profile.temp_store),
            (-2048, 0, 2)
        );
        println!("STORE_PROFILE {profile:?}");
        let storage = Storage::new(handles.storage.clone()).unwrap();
        put(&storage);
        check(&storage);
        drop(storage);
        let sealed = handles.seal().unwrap();
        assert_eq!(sealed.path, path);
        assert_eq!(sealed.bytes, std::fs::metadata(&path).unwrap().len());
        assert!(sealed.bytes > 0);
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
        let reopened = open(&path);
        assert_eq!(reopened.profile().pack_layout, layout);
        let storage = Storage::new(reopened.storage.clone()).unwrap();
        check(&storage);
        drop(storage);
        reopened.seal().unwrap();
    }
}

#[test]
fn second_process_busy_has_no_effect_and_readers_keep_progressing() {
    let temp = support::Temp::new("serverless-busy");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    put(&storage);
    let request = Reserve {
        packs: 1,
        ordinals: 1,
    };
    let first = handles.storage.reserve(request).unwrap();
    let inodes = ReserveRequest {
        scope: ObjectId::for_bytes(b"pre-s8-scope"),
        count: 1,
    };
    let first_inode = handles.history.reserve_inodes(&inodes).unwrap();
    let held = Held::start(&path, &temp.join("lock.sock"));
    let before = handles.diagnostics().unwrap();
    assert_eq!(
        handles.storage.reserve(request),
        Err(PersistenceError::Busy)
    );
    let after = handles.diagnostics().unwrap();
    assert_eq!(after.statements - before.statements, 1);
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.rollbacks, before.rollbacks);
    assert!(matches!(
        StorageError::from(PersistenceError::Busy),
        StorageError::Busy
    ));
    assert_eq!(
        handles.history.reserve_inodes(&inodes),
        Err(HistoryError::Busy)
    );
    // A new independent read connection opens and completes while the other
    // process still holds an uncommitted mutation, before release is sent.
    let reader = Handles::open_read_only(
        config(&path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap();
    let read_storage = Storage::new(reader.storage.clone()).unwrap();
    check(&read_storage);
    check(&storage);
    drop(read_storage);
    drop(reader);
    held.release();
    // These are later explicit calls, not a failed-operation replay in product.
    let next = handles.storage.reserve(request).unwrap();
    assert_eq!(next.first_pack_id, first.first_pack_id + 1);
    assert_eq!(next.first_ordinal, first.first_ordinal + 1);
    assert_eq!(
        handles.history.reserve_inodes(&inodes).unwrap().start,
        first_inode.start + 1
    );
    drop(storage);
    handles.seal().unwrap();
}

#[test]
fn seal_refuses_retained_provider_without_effect() {
    let temp = support::Temp::new("seal-custody");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let provider = handles.storage.clone();
    let before = std::fs::metadata(temp.join("store.sqlite-wal"))
        .unwrap()
        .len();
    assert!(matches!(handles.seal(), Err(PersistenceError::Busy)));
    assert_eq!(
        std::fs::metadata(temp.join("store.sqlite-wal"))
            .unwrap()
            .len(),
        before
    );
    provider
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    drop(provider);
    open(&path).seal().unwrap();
}

#[test]
fn non_wal_store_is_refused_without_conversion() {
    let temp = support::Temp::new("non-wal");
    let path = temp.join("store.sqlite");
    create(&path).seal().unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.pragma_update(None, "journal_mode", "DELETE").unwrap();
    drop(sql);
    assert!(matches!(
        Handles::open_writable(
            config(&path),
            &authority().binding_key,
            authority().cursor_key
        ),
        Err(PersistenceError::Malformed)
    ));
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.pragma_query_value(None, "journal_mode", |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
}
