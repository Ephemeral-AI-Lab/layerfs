//! Actual linked-provider child participation; unsupported startup is explicitly unrun.
use layerfs_content::{AuthenticatedObjects, FinalizedObject, ObjectId, ObjectRole};
use layerfs_storage::{
    engine::{bootstrap_exclusive, ConnectionClass},
    sqlite::connection,
    Store, StoreProvider,
};
use layerfs_telemetry::timer::Timing;
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn supporting_provider_guarded_store_save_read_and_exact_connection_shapes() {
    run("supported");
}
#[test]
fn supporting_provider_foreign_limit_refuses_new_factory_before_file_effects() {
    run("foreign");
}
#[test]
fn supporting_provider_boundaries_preserve_known_prebegin_and_open_transaction_unknown() {
    run("boundary-pre");
    run("boundary-open");
}
fn run(case: &str) {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine_participation_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("LAYERFS_ENGINE_PARTICIPATION_CASE", case)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut b = Vec::new();
        stdout.take(8193).read_to_end(&mut b).unwrap();
        b
    });
    let err = std::thread::spawn(move || {
        let mut b = Vec::new();
        stderr.take(8193).read_to_end(&mut b).unwrap();
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(5) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("engine child exceeded fixed bound");
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    child.wait().unwrap();
    let out = out.join().unwrap();
    let err = err.join().unwrap();
    assert!(out.len() <= 8192 && err.len() <= 8192);
    assert!(
        status.success(),
        "{} {}",
        String::from_utf8_lossy(&out),
        String::from_utf8_lossy(&err)
    );
    let text = String::from_utf8_lossy(&out);
    assert!(
        text.contains("engine participation SUPPORTED")
            || text.contains("engine participation NOT_RUN unsupported provider")
    );
    eprintln!("{} {}", text, String::from_utf8_lossy(&err));
}
#[test]
fn engine_participation_child() {
    let Ok(case) = std::env::var("LAYERFS_ENGINE_PARTICIPATION_CASE") else {
        return;
    };
    // SAFETY: selected fresh child before any SQL, other test or native worker.
    let guard = match unsafe { bootstrap_exclusive() } {
        Ok(guard) => guard,
        Err(error) => {
            assert!(
                matches!(
                    error.cause(),
                    layerfs_storage::StorageError::UnsupportedPolicy { .. }
                ),
                "unexpected bootstrap failure: {error}"
            );
            println!("engine participation NOT_RUN unsupported provider: stage={:?} cause={} custody={:?}",error.stage(),error.cause(),error.custody());
            return;
        }
    };
    let path = std::env::temp_dir().join(format!(
        "layerfs-engine-participation-{}",
        std::process::id()
    ));
    std::fs::create_dir(&path).unwrap();
    let db = path.join("store.sqlite");
    if case == "foreign" {
        // External proof changes the actual native limit once; no restore/retry exists.
        unsafe { rusqlite::ffi::sqlite3_hard_heap_limit64(40 * 1024 * 1024) };
        let result = Timing::disabled("refuse", |s| {
            Store::create_guarded(&db, Store::default_policy(), guard, s.child("create"))
        })
        .0;
        assert!(result.is_err());
        assert!(guard.is_quarantined());
        assert!(!db.exists());
    } else {
        let store = Timing::disabled("create", |s| {
            Store::create_guarded(&db, Store::default_policy(), guard, s.child("create"))
        })
        .0
        .unwrap();
        assert!(std::ptr::eq(store.engine_guard().unwrap(), guard));
        if case.starts_with("boundary-") {
            let conn =
                connection::open_guarded(&db, false, guard, ConnectionClass::ContentWrite).unwrap();
            let open = case == "boundary-open";
            if open {
                conn.execute_batch("BEGIN IMMEDIATE;").unwrap();
            }
            assert_eq!(conn.is_autocommit(), !open);
            // External proof changes this actual linked provider once, never a fake guard.
            unsafe { rusqlite::ffi::sqlite3_hard_heap_limit64(40 * 1024 * 1024) };
            let error = layerfs_storage::sqlite::ownership::validate_engine_boundary(&conn, guard)
                .unwrap_err();
            assert_eq!(error.is_unknown_outcome(), open);
            assert_eq!(conn.is_autocommit(), !open, "no guessed COMMIT or rollback");
            assert!(guard.is_quarantined());
            if open {
                println!(
                    "retained actual open connection until child exit: {}",
                    path.display()
                );
                std::mem::forget(conn);
                std::mem::forget(store);
            } else {
                drop(conn);
                drop(store);
                std::fs::remove_dir_all(&path).unwrap();
            }
            println!("engine participation SUPPORTED {case}: actual boundary custody; combined fit UNQUALIFIED");
            return;
        }

        let setter = Timing::disabled("configuration", |s| {
            store.set_max_concurrent_writes(1, s.child("set"))
        })
        .0;
        assert!(matches!(
            setter,
            Err(layerfs_storage::StorageError::UnsupportedPolicy {
                field: "guarded writer-budget transaction owner"
            })
        ));
        assert_eq!(store.max_concurrent_writes().unwrap(), 2);
        let mut save = Timing::disabled("save", |s| store.begin_save(s.child("acquire")))
            .0
            .unwrap();
        let profile = save.connection_profile().unwrap();
        assert_eq!((profile.cache_size, profile.mmap_size), (-2048, 0));
        let mut value = b"LFS5SML\0".to_vec();
        value.extend_from_slice(&1u16.to_be_bytes());
        value.extend_from_slice(b"actual guarded canonical bytes");
        let bytes = layerfs_content::object::encode_bytes_object(&value).unwrap();
        let id = ObjectId::for_bytes(&bytes);
        save.accept(FinalizedObject::new(ObjectRole::WholeFile, bytes.clone()).unwrap())
            .unwrap();
        Timing::disabled("finish", |s| save.finish(s.child("finish")))
            .0
            .unwrap();
        let provider = StoreProvider::new(&store);
        assert_eq!(provider.read_canonical(id).unwrap(), bytes);
        let conn =
            connection::open_guarded(&db, false, guard, ConnectionClass::ContentRead).unwrap();
        assert_eq!(
            connection::pragma_i64(&conn, connection::Pragma::CacheSize).unwrap(),
            -1024
        );
        assert_eq!(
            conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_LENGTH)
                .unwrap(),
            16 * 1024 * 1024 + 8192
        );
        assert_eq!(
            conn.limit(rusqlite::limits::Limit::SQLITE_LIMIT_SQL_LENGTH)
                .unwrap(),
            65536
        );
        conn.execute_batch("PRAGMA cache_size=-2048;").unwrap();
        assert!(connection::verify_guarded(&conn, guard, ConnectionClass::ContentRead).is_err());
        drop(conn);
        drop(provider);
        drop(store);
        assert_eq!(
            guard.validate().unwrap().hard_heap_limit_bytes,
            32 * 1024 * 1024
        );
    }
    std::fs::remove_dir_all(path).unwrap();
    println!("engine participation SUPPORTED {case}: actual profiles/custody; combined-shape fit UNQUALIFIED");
    std::io::stdout().flush().unwrap();
}
