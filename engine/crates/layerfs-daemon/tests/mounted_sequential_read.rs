//! Real kernel mount, layerfs-project #97: `cat` of a committed 64 MiB
//! non-repeating file (fs-bench-quick C17) reaches the Store in read-ahead
//! windows, not one demand per kernel read. Each kernel request gets its own
//! operation client, so the stream a sequential read continues must outlive
//! that client. Prints `MOUNTED_READ <json>` with the read's Store demands
//! and SQL statements; SQLite always, PostgreSQL when
//! `LAYERFS_TEST_POSTGRES_URL` is set and the `postgresql` feature is on.
//! The line also carries the resident set when the read starts and the
//! process's resident peak during it (the daemon serves in this process).
//! Disposable profile; the daemon's 64 MiB object cache.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
use layerfs_persistence::SqlWork;
use layerfs_store_service::bootstrap::{open_store_observed, OpenedStore};
use mounted::Harness;
use std::io::Read;
use std::path::Path;

const BIG: usize = 64 * 1024 * 1024;
/// The daemon's Store object cache (`install_types.rs`).
const CACHE: usize = 64 * 1024 * 1024;
/// `cat`'s buffer.
const BUFFER: usize = 128 * 1024;
/// Store demands per 64 MiB above which the read was not read ahead: one
/// kernel read is 128 KiB, so without read-ahead it is about 512.
const MOST_DEMANDS: u64 = 64;

/// `length` non-repeating bytes from a fixed xorshift64* stream.
fn random(length: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut out = Vec::with_capacity(length + 8);
    while out.len() < length {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        out.extend_from_slice(&state.wrapping_mul(0x2545_F491_4F6C_DD1D).to_le_bytes());
    }
    out.truncate(length);
    out
}

/// One `/proc/self/status` field in KiB.
fn status_kb(field: &str) -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix(field))
                .and_then(|rest| rest.split_whitespace().next().map(str::to_owned))
        })
        .unwrap_or_else(|| "null".to_owned())
}

/// Resets the resident high-water mark to the current resident set (Linux
/// `clear_refs` 5), so the next `VmHWM` is the read's own peak; returns
/// `VmRSS` at the reset. The daemon serves in this process.
fn resident_reset() -> String {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
    status_kb("VmRSS:")
}

fn readers(opened: &OpenedStore) -> SqlWork {
    let mut total = SqlWork::default();
    for reader in opened.diagnostics().unwrap().readers {
        total.accumulate(&reader);
    }
    total
}

/// Mounts the Branch, reads `rand` as `cat` does and prints the counts.
fn read_mounted(
    backend: &str,
    opened: &OpenedStore,
    directory: &Path,
    branch: layerfs_history::BranchId,
    big: &[u8],
) -> u64 {
    let harness = Harness::new(opened.store.clone(), directory, branch);
    let ready = harness.mount(1);
    let path = Path::new(&ready.directory).join("rand");
    let rss = resident_reset();
    let (work, sql) = (opened.store.work(), readers(opened));
    let mut file = std::fs::File::open(&path).unwrap();
    let mut buffer = vec![0; BUFFER];
    let mut offset = 0;
    loop {
        let read = file.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        assert_eq!(buffer[..read], big[offset..offset + read], "at {offset}");
        offset += read;
    }
    assert_eq!(offset, BIG);
    drop(file);
    let (after, after_sql) = (opened.store.work(), readers(opened));
    let demands = after.object_batches - work.object_batches;
    println!(
        "MOUNTED_READ {{\"backend\":\"{backend}\",\"object_batches\":{demands},\"object_ids\":{},\"statements\":{},\"returned_rows\":{},\"rss_before_kb\":{rss},\"vm_hwm_kb\":{}}}",
        after.object_ids - work.object_ids,
        after_sql.statements - sql.statements,
        after_sql.returned_rows - sql.returned_rows,
        status_kb("VmHWM:"),
    );
    harness.unmount(&ready);
    harness.stop();
    demands
}

#[test]
fn a_mounted_sequential_read_reads_ahead_on_sqlite() {
    let big = random(BIG);
    let fixture = installed::Fixture::built("sequential-read", |source| {
        std::fs::write(source.join("rand"), &big).unwrap();
        1
    });
    let opened = open_store_observed(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        64,
        CACHE,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let demands = read_mounted("sqlite", &opened, &fixture.directory, fixture.branch, &big);
    assert!(
        demands <= MOST_DEMANDS,
        "{demands} Store demands for 64 MiB"
    );
    drop(opened);
    fixture.cleanup();
}

#[cfg(feature = "postgresql")]
#[test]
fn a_mounted_sequential_read_reads_ahead_on_postgresql() {
    use layerfs_history::{
        BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
        LayerStackId,
    };
    use layerfs_persistence::{PostgresHandles, PostgresSelection, StoreSelection};
    use layerfs_project::{init, InitRequest};
    use layerfs_storage::{Storage, StoragePolicy};
    use layerfs_telemetry::timer::Timing;
    use std::time::{Duration, Instant};
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let big = random(BIG);
    let directory =
        std::env::temp_dir().join(format!("layerfs-mounted-read-pg-{}", std::process::id()));
    let source = directory.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("rand"), &big).unwrap();
    let schema = format!("lfs97_mr_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let catalog = HistoryCatalogConfig {
        binding_key: installed::BINDING.to_vec(),
        cursor_key: installed::CURSOR,
        incarnation: 1,
    };
    let handles =
        PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog).unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let initialized = Timing::disabled("mounted.init", |timing| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: &source,
                acquisition: &handles.acquisition,
                stack: LayerStackId::from_authority([23; 16]),
                name: HistoryName::new("installed").unwrap(),
                scope_seed: [31; 32],
                deadline: Instant::now() + Duration::from_secs(30),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = BranchId::from_authority([24; 16]);
    handles
        .history
        .fork(&ForkRequest {
            stack: initialized.stack.id,
            branch,
            name: HistoryName::new("main").unwrap(),
            source: ForkSource::Layer(initialized.stack.head_layer),
        })
        .unwrap();
    drop(storage);
    drop(handles);
    std::fs::remove_dir_all(&source).unwrap();
    let opened = open_store_observed(
        StoreSelection::Postgres(selection),
        installed::BINDING,
        installed::CURSOR,
        64,
        CACHE,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let demands = read_mounted("postgresql", &opened, &directory, branch, &big);
    drop(opened);
    // The schema goes with the test database, which the runner drops.
    std::fs::remove_dir_all(&directory).unwrap();
    assert!(
        demands <= MOST_DEMANDS,
        "{demands} Store demands for 64 MiB"
    );
}
