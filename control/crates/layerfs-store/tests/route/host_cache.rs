//! Real Store admission, maintenance fences, and process cache limits.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use layerfs_home::disk::{layout, paths};
use layerfs_home::names::StoreName;
use layerfs_home::record::intent::{Op, FIELD};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::admin::open::args::{OpenArgs, PgSelection};
use layerfs_store::admin::open::{cache, close, host};
use layerfs_store::admin::record::store::{self, StoreRecord};
use layerfs_store::admin::{open::service, record::identity};
use layerfs_store::backend::sqlite::create::create;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const WAIT: Duration = Duration::from_secs(5);

struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Self {
        let home =
            std::env::temp_dir().join(format!("layerfs-host-cache-{tag}-{}", std::process::id()));
        layout::create(&home).unwrap();
        Self(home.canonicalize().unwrap())
    }

    fn create(&self, store: &str) {
        create(&self.0, &StoreName::parse(store).unwrap(), None).unwrap();
    }

    fn entry(&self, store: &str) -> PathBuf {
        paths::store_file(&self.0, store)
    }

    fn update(&self, store: &str, edit: impl FnOnce(StoreRecord) -> StoreRecord) {
        let _lock = store::lock(&self.0, store).unwrap();
        edit(StoreRecord::of(&self.0, store).unwrap())
            .write(&self.0)
            .unwrap();
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        if let Ok(entries) = std::fs::read_dir(self.0.join("store")) {
            for entry in entries.flatten() {
                if let Some(store) = entry.file_name().to_str() {
                    let _ = close::close(&self.entry(store));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stats(home: &Path, store: &str) -> Result<SdkValue, SdkRefusal> {
    host::run_host(home, store, "unused", "store.stats", SdkValue::map([]))
}

#[test]
fn a_warm_hit_checks_the_current_fence_and_binding() {
    let home = Home::new("fence");
    home.create("shop");
    stats(&home.0, "shop").unwrap();
    assert!(layerfs_store::admin::open::lease::held_here(
        &home.0, "shop"
    ));
    let lease_path = paths::lease_dir(&home.0, "shop").join(std::process::id().to_string());
    let lease = std::fs::read_to_string(lease_path).unwrap();
    assert_eq!(lease.split_ascii_whitespace().last(), Some("host-worker"));

    let original = StoreRecord::of(&home.0, "shop").unwrap();
    let binding = original.field("binding").unwrap().clone();
    home.update("shop", |r| r.with("binding", SdkValue::Bytes(vec![0; 16])));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Unavailable");
    home.update("shop", |r| r.with("binding", binding));

    let fence = Op::begin("store.compact", "worker-close", SdkValue::Null).unwrap();
    let id = fence.id().to_owned();
    home.update("shop", |r| r.with(FIELD, fence.to_record().clone()));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert!(host::close_host(&home.0, "shop", |_| Err(
        layerfs_home::reply::outcome::refused("Conflict", "wrong operation")
    ))
    .is_err());
    assert_eq!(host::host_store_count(&home.0), 1);
    let report = host::close_host(&home.0, "shop", |record| {
        assert_eq!(record.op().unwrap().id(), id);
        assert!(layerfs_store::admin::open::lease::held_here(
            &home.0, "shop"
        ));
        Ok(())
    })
    .unwrap();
    assert!(report.closed);
    assert_eq!(report.wal_remaining, Some(false));
    assert!(!layerfs_store::admin::open::lease::held_here(
        &home.0, "shop"
    ));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert_eq!(host::host_store_count(&home.0), 0);

    home.update("shop", |r| {
        r.with(FIELD, fence.at("build").to_record().clone())
    });
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert_eq!(host::host_store_count(&home.0), 0);
    home.update("shop", |r| r.without(FIELD));
    stats(&home.0, "shop").unwrap();
    assert!(close::close(&home.entry("shop")).unwrap());
    assert!(!close::close(&home.entry("shop")).unwrap());
}

#[test]
fn management_close_refuses_a_contended_record_lock_before_closing() {
    let home = Home::new("contended-close");
    home.create("shop");
    stats(&home.0, "shop").unwrap();

    let (acquired, wait_for_lock) = mpsc::channel();
    let (release, wait_for_release) = mpsc::channel();
    let root = home.0.clone();
    let holder = thread::spawn(move || {
        let _guard = store::lock(&root, "shop").unwrap();
        acquired.send(()).unwrap();
        let _ = wait_for_release.recv_timeout(WAIT);
    });
    wait_for_lock.recv_timeout(WAIT).unwrap();
    let outcome = host::close_host(&home.0, "shop", |_| Ok(()));
    let _ = release.send(());
    holder.join().unwrap();

    let Err(refusal) = outcome else {
        panic!("the close waited for the Store lock and then closed the cache");
    };
    assert_eq!(refusal.code, "InUse");
    let next = refusal.detail.get("next").unwrap();
    assert_eq!(next.get_text("store"), Some("shop"));
    assert_eq!(next.get_text("state"), Some("record-locked"));
    assert_eq!(host::host_store_count(&home.0), 1);
    assert!(layerfs_store::admin::open::lease::held_here(
        &home.0, "shop"
    ));
}

#[test]
fn a_failed_open_releases_its_slot_and_lease_for_a_later_call() {
    let home = Home::new("failure");
    home.create("shop");
    let file = home.entry("shop");
    let saved = file.with_extension("saved");
    std::fs::rename(&file, &saved).unwrap();
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "NoStore");
    assert_eq!(host::host_store_count(&home.0), 0);
    assert!(!layerfs_store::admin::open::lease::held_here(
        &home.0, "shop"
    ));
    std::fs::rename(saved, file).unwrap();

    let (tx, rx) = mpsc::channel();
    let mut start = Vec::new();
    for _ in 0..3 {
        let (go, waiting) = mpsc::sync_channel(1);
        start.push(go);
        let (home, tx) = (home.0.clone(), tx.clone());
        thread::spawn(move || {
            waiting.recv_timeout(WAIT).unwrap();
            let _ = tx.send(stats(&home, "shop"));
        });
    }
    for go in start {
        go.send(()).unwrap();
    }
    for _ in 0..3 {
        rx.recv_timeout(WAIT).unwrap().unwrap();
    }
    assert!(layerfs_store::admin::open::lease::held_here(
        &home.0, "shop"
    ));
    assert_eq!(host::host_store_count(&home.0), 1);
}

#[test]
fn the_seventeenth_store_evicts_the_least_recent_idle_owner() {
    let home = Home::new("bound");
    for n in 0..=16 {
        let store = format!("s{n}");
        home.create(&store);
        stats(&home.0, &store).unwrap();
        assert_eq!(host::host_store_count(&home.0), (n + 1).min(16));
    }
    assert!(!layerfs_store::admin::open::lease::held_here(&home.0, "s0"));
    assert!(layerfs_store::admin::open::lease::held_here(&home.0, "s1"));
    assert!(layerfs_store::admin::open::lease::held_here(&home.0, "s16"));
}

#[test]
fn a_host_read_leaves_an_independent_writers_wal_to_its_owner() {
    let home = Home::new("foreign-wal");
    home.create("shop");
    let record = StoreRecord::of(&home.0, "shop").unwrap();
    let identity = identity::identity_in(&record).unwrap();
    // Keep the separate writable connection live across the host close.
    let writer = SqliteHandles::open_writable(
        PersistenceConfig::sqlite(home.entry("shop"))
            .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        &identity.binding,
        identity.cursor_key,
    )
    .unwrap();
    let storage = layerfs_storage::Storage::new(writer.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let policy = storage.policy();
    layerfs_telemetry::timer::Timing::disabled("construct", |scope| {
        layerfs_content::construct_bytes(
            policy.construction(),
            &policy.construction().capacities(),
            b"foreign writer keeps these committed frames",
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap();
    save.finish().unwrap();
    let wal = home.entry("shop").with_file_name("store.sqlite-wal");
    let wal_bytes = std::fs::read(&wal).unwrap();
    assert!(!wal_bytes.is_empty());

    stats(&home.0, "shop").unwrap();
    let args = OpenArgs::of_record(&home.0, "shop", &record).unwrap();
    let side_call: Result<(), _> =
        service::with_service(&args, |_| panic!("direct caller borrowed the host service"));
    assert_eq!(side_call.unwrap_err().code, "InUse");
    let report = host::close_host(&home.0, "shop", |_| Ok(()))
        .unwrap_or_else(|refusal| panic!("host read left close work: {refusal:?}"));
    assert!(report.closed);
    assert_eq!(report.checkpoint.unwrap(), None);
    assert_eq!(report.wal_remaining, Some(true));
    assert_eq!(std::fs::read(wal).unwrap(), wal_bytes);
    drop(storage);
    drop(writer);
}

#[test]
fn host_close_reports_lease_unlink_failure_without_a_release_acknowledgment() {
    let home = Home::new("failed-release");
    home.create("shop");
    stats(&home.0, "shop").unwrap();
    let lease = paths::lease_dir(&home.0, "shop").join(std::process::id().to_string());
    std::fs::remove_file(&lease).unwrap();
    std::fs::create_dir(&lease).unwrap();

    match host::close_host(&home.0, "shop", |_| Ok(())) {
        Ok(_) => panic!("host close acknowledged a failed lease release"),
        Err(refusal) => assert_eq!(refusal.code, "Failed", "{refusal:?}"),
    }
    assert!(lease.is_dir());
    assert_eq!(host::host_store_count(&home.0), 0);
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + WAIT;
    loop {
        match listener.accept() {
            Ok((socket, _)) => return socket,
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(5));
            }
            Err(err) => panic!("no controlled connection: {err}"),
        }
    }
}

#[test]
fn a_slow_open_holds_only_its_own_slot_and_refuses_close_as_in_use() {
    let home = Home::new("slow");
    home.create("shop");
    home.create("another");
    stats(&home.0, "shop").unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "postgresql://user@{}/db?sslmode=disable&connect_timeout=10",
        listener.local_addr().unwrap()
    );
    let record = StoreRecord::connected("slow", &url, "user", "testing", "local")
        .with("binding", SdkValue::Bytes(vec![1; 32]))
        .with("cursor", SdkValue::Bytes(vec![2; 32]));
    let open_args = OpenArgs::dialled(
        &home.0,
        "slow",
        &record,
        PgSelection {
            url: Some(url),
            ..PgSelection::default()
        },
    );
    let key = open_args.store_path().unwrap();
    let (slow_tx, slow_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = slow_tx.send(cache::hold(&open_args, None));
    });
    let unanswered = accept(&listener);

    let same_key = key.clone();
    let (tx, rx) = mpsc::channel();
    let work_home = home.0.clone();
    thread::spawn(move || {
        // Warm and cold SQLite paths through both global caches.
        let in_use = close::close(&same_key);
        let warm = stats(&work_home, "shop");
        let cold = stats(&work_home, "another");
        let _ = tx.send((in_use, warm, cold));
    });
    let progress = rx.recv_timeout(Duration::from_secs(2));
    drop(unanswered);
    drop(listener);
    assert!(slow_rx.recv_timeout(WAIT).unwrap().is_err());
    let (in_use, warm, cold) =
        progress.expect("unrelated Store calls stalled during another Store's dial");
    assert_eq!(in_use.unwrap_err().code, "InUse");
    warm.unwrap();
    cold.unwrap();
    assert!(!close::close(&key).unwrap());
}
