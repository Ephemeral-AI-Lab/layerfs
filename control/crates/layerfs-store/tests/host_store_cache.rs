//! Real Store admission, maintenance fences, and process cache limits.
#![cfg(any(target_os = "macos", target_os = "linux"))]
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{layout, paths};
use layerfs_home::names::StoreName;
use layerfs_home::op::{Op, FIELD};
use layerfs_store::open;
use layerfs_store::open_args::{OpenArgs, PgSelection};
use layerfs_store::record::{self, StoreRecord};
use layerfs_store::sqlite::create::create;
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
        let _lock = record::lock(&self.0, store).unwrap();
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
                    let _ = open::close(&self.entry(store));
                }
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stats(home: &Path, store: &str) -> Result<SdkValue, SdkRefusal> {
    open::run_host(home, store, "unused", "store.stats", SdkValue::map([]))
}

#[test]
fn a_warm_hit_checks_the_current_fence_and_binding() {
    let home = Home::new("fence");
    home.create("shop");
    stats(&home.0, "shop").unwrap();
    assert!(layerfs_store::lease::held_here(&home.0, "shop"));

    let original = StoreRecord::of(&home.0, "shop").unwrap();
    let binding = original.field("binding").unwrap().clone();
    home.update("shop", |r| r.with("binding", SdkValue::Bytes(vec![0; 16])));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Unavailable");
    home.update("shop", |r| r.with("binding", binding));

    let fence = Op::begin("store.compact", "worker-close", SdkValue::Null).unwrap();
    let id = fence.id().to_owned();
    home.update("shop", |r| r.with(FIELD, fence.to_record().clone()));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert!(
        open::close_host(&home.0, "shop", |_| Err(layerfs_home::outcome::refused(
            "Conflict",
            "wrong operation"
        )))
        .is_err()
    );
    assert_eq!(open::host_store_count(&home.0), 1);
    let report = open::close_host(&home.0, "shop", |record| {
        assert_eq!(record.op().unwrap().id(), id);
        assert!(layerfs_store::lease::held_here(&home.0, "shop"));
        Ok(())
    })
    .unwrap();
    assert!(report.closed);
    assert_eq!(report.wal_remaining, Some(false));
    assert!(!layerfs_store::lease::held_here(&home.0, "shop"));
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert_eq!(open::host_store_count(&home.0), 0);

    home.update("shop", |r| {
        r.with(FIELD, fence.at("build").to_record().clone())
    });
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "Busy");
    assert_eq!(open::host_store_count(&home.0), 0);
    home.update("shop", |r| r.without(FIELD));
    stats(&home.0, "shop").unwrap();
    assert!(open::close(&home.entry("shop")).unwrap());
    assert!(!open::close(&home.entry("shop")).unwrap());
}

#[test]
fn a_failed_open_releases_its_slot_and_lease_for_a_later_call() {
    let home = Home::new("failure");
    home.create("shop");
    let file = home.entry("shop");
    let saved = file.with_extension("saved");
    std::fs::rename(&file, &saved).unwrap();
    assert_eq!(stats(&home.0, "shop").unwrap_err().code, "NoStore");
    assert_eq!(open::host_store_count(&home.0), 0);
    assert!(!layerfs_store::lease::held_here(&home.0, "shop"));
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
    assert!(layerfs_store::lease::held_here(&home.0, "shop"));
    assert_eq!(open::host_store_count(&home.0), 1);
}

#[test]
fn the_seventeenth_store_evicts_the_least_recent_idle_owner() {
    let home = Home::new("bound");
    for n in 0..=16 {
        let store = format!("s{n}");
        home.create(&store);
        stats(&home.0, &store).unwrap();
        assert_eq!(open::host_store_count(&home.0), (n + 1).min(16));
    }
    assert!(!layerfs_store::lease::held_here(&home.0, "s0"));
    assert!(layerfs_store::lease::held_here(&home.0, "s1"));
    assert!(layerfs_store::lease::held_here(&home.0, "s16"));
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
        let _ = slow_tx.send(open::hold(&open_args, None));
    });
    let unanswered = accept(&listener);

    let same_key = key.clone();
    let (tx, rx) = mpsc::channel();
    let work_home = home.0.clone();
    thread::spawn(move || {
        // Warm and cold SQLite paths through both global caches.
        let in_use = open::close(&same_key);
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
    assert!(!open::close(&key).unwrap());
}
