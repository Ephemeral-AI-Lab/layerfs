//! The write path's cost on a replay Store (#364 item 2 (b)): `fs.write`
//! latency over small files, and one large file.
//!
//! Set `LAYERFS_364_RELINK_STORE`, `LAYERFS_364_RELINK_BINDING` and
//! `LAYERFS_364_RELINK_FROM` as for `relink_commit`, then run with
//! `--ignored`. The Store is copied first. Prints one `WRITE_SMALL` line
//! (200 writes of 1-8 KiB, each its own Commit) and one `WRITE_BIG` line
//! (one 64 MiB write), each followed by a `_WORK` line: per call, the Store's
//! and the overlay's statements, VM steps and time inside SQLite, the overlay
//! hold time, and the time BLAKE3 takes over the same bytes in this process.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;

use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_store_service::{bootstrap::open_store_observed, Owner, OwnerConfig, StoreService};

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn call(service: &StoreService, method: &str, fields: Vec<(&str, SdkValue)>) -> SdkValue {
    let mut fields: Vec<(String, SdkValue)> = fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect();
    fields.push(("project".into(), SdkValue::text("dsh")));
    let call = SdkCall {
        method: method.into(),
        args: SdkValue::Map(fields),
    };
    layerfs_api::call(service, &call).unwrap()
}

fn bytes(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

/// Work so far: Store (statements, VM steps, statement ns), overlay
/// foreground (executions, VM steps, elapsed ns) and overlay hold ns.
fn work(service: &StoreService, owner: &Owner) -> [u64; 7] {
    let store = service.opened.diagnostics().unwrap();
    let mut out = [0u64; 7];
    for sql in std::iter::once(&store.writer).chain(store.readers.iter()) {
        out[0] += sql.statements;
        out[1] += sql.vm_steps;
        out[2] += sql.statement_ns;
    }
    let overlay = owner.client().diagnostics().unwrap();
    for statement in overlay.sql_foreground.statements {
        out[3] += statement.executions;
        out[4] += statement.vm_steps;
        out[5] += statement.elapsed_ns;
    }
    out[6] = overlay.hold_ns.iter().sum();
    out
}

fn report(name: &str, calls: u64, before: [u64; 7], after: [u64; 7], hash_ns: u64) {
    let per = |at: usize| (after[at] - before[at]) as f64 / calls as f64;
    println!(
        "{name}_WORK calls={calls} store_statements={:.1} store_vm_steps={:.0} store_sql_ms={:.3} overlay_statements={:.1} overlay_vm_steps={:.0} overlay_sql_ms={:.3} overlay_hold_ms={:.3} blake3_ms={:.4}",
        per(0),
        per(1),
        per(2) / 1e6,
        per(3),
        per(4),
        per(5) / 1e6,
        per(6) / 1e6,
        hash_ns as f64 / calls as f64 / 1e6,
    );
}

fn hash_ns(content: &[u8]) -> u64 {
    let started = std::time::Instant::now();
    std::hint::black_box(blake3::hash(std::hint::black_box(content)));
    started.elapsed().as_nanos() as u64
}

fn write(service: &StoreService, path: &str, content: Vec<u8>) -> f64 {
    let started = std::time::Instant::now();
    let reply = call(
        service,
        "fs.write",
        vec![
            ("branch", SdkValue::text("write")),
            ("path", SdkValue::text(path)),
            ("bytes", SdkValue::Bytes(content)),
        ],
    );
    let ms = started.elapsed().as_secs_f64() * 1e3;
    assert_eq!(reply.get_text("kind"), Some("Committed"), "{reply:?}");
    ms
}

#[test]
#[ignore = "needs a replay Store: LAYERFS_364_RELINK_STORE, _BINDING and _FROM"]
fn write_cost() {
    let source = std::env::var("LAYERFS_364_RELINK_STORE").expect("replay Store");
    let binding = unhex(&std::env::var("LAYERFS_364_RELINK_BINDING").expect("binding key"));
    let from = std::env::var("LAYERFS_364_RELINK_FROM").expect("Commit to start from");
    let directory = std::env::temp_dir().join(format!("layerfs-364-write-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("store.sqlite");
    std::fs::copy(&source, &path).unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&path, permissions).unwrap();
    let opened = open_store_observed(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &binding,
        installed::CURSOR,
        OwnerConfig::default().namespaces,
        64 * 1024,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    call(
        &service,
        "branch.fork",
        vec![
            ("name", SdkValue::text("write")),
            ("from", SdkValue::text(&from)),
        ],
    );
    let mut hashed = 0u64;
    let before = work(&service, &owner);
    let mut small: Vec<f64> = (0..200u64)
        .map(|at| {
            let length = 1024 + (at as usize * 7_919) % (7 * 1024);
            let content = bytes(at, length);
            hashed += hash_ns(&content);
            write(&service, &format!("/w364-small-{at:03}.bin"), content)
        })
        .collect();
    report("WRITE_SMALL", 200, before, work(&service, &owner), hashed);
    small.sort_by(f64::total_cmp);
    println!(
        "WRITE_SMALL n=200 p50_ms={:.2} p90_ms={:.2} max_ms={:.2}",
        small[100], small[180], small[199]
    );
    let content = bytes(999, 64 << 20);
    let hashed = hash_ns(&content);
    let before = work(&service, &owner);
    let big = write(&service, "/w364-big.bin", content);
    println!("WRITE_BIG bytes={} ms={big:.1}", 64u64 << 20);
    report("WRITE_BIG", 1, before, work(&service, &owner), hashed);
    drop(service);
    owner.stop().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
