//! Real kernel mount: `ls -l` of a committed directory of 10,000 files, and
//! the Store work its lookups and getattrs cost (#354 A1). A Workspace reader
//! keeps its storage caches across demands, so the pack bodies and decoded
//! groups one entry's demand read serve its siblings' demands.
//!
//! The Store is opened as the daemon opens it (8 MiB object cache). The
//! listing runs twice in one Workspace: the first pays for every entry, the
//! second shows what the kernel and the kept caches leave. Printed per
//! listing: Store statements, object demands and ids, pack bytes read by
//! the Workspace readers, and the listing's digest. The daemon serves in this
//! process, so VmRSS and VmHWM are its resident memory. The test asserts the
//! listing only; the counts are the measurement.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
use layerfs_store_service::bootstrap::{open_store_observed, OpenedStore};
use mounted::Harness;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::Path,
    process::Command,
};

const FILES: usize = 10_000;
const CACHE_BYTES: usize = 8 * 1024 * 1024;

struct Mark {
    statements: u64,
    object_batches: u64,
    object_ids: u64,
    length_batches: u64,
    pack_bytes: u64,
    pack_selections: u64,
}
fn mark(opened: &OpenedStore) -> Mark {
    let sql = opened.diagnostics().unwrap();
    let work = opened.store.work();
    let (mut pack_bytes, mut pack_selections) = (0, 0);
    for index in 0..256 {
        if let Some(d) = opened.store.reader_storage_diagnostics(index) {
            pack_bytes += d.pack_read_bytes;
            pack_selections += d.read_pack_selections;
        }
    }
    Mark {
        statements: sql.writer.statements + sql.readers.iter().map(|r| r.statements).sum::<u64>(),
        object_batches: work.object_batches,
        object_ids: work.object_ids,
        length_batches: work.length_batches,
        pack_bytes,
        pack_selections,
    }
}
/// VmRSS and VmHWM of this process, in KiB.
fn memory() -> (u64, u64) {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let field = |name: &str| {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|rest| rest.trim().trim_end_matches(" kB").trim().parse().ok())
            .unwrap()
    };
    (field("VmRSS:"), field("VmHWM:"))
}
fn listing(directory: &Path) -> (usize, u64) {
    let output = Command::new("ls")
        .arg("-l")
        .arg(directory)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut hasher = DefaultHasher::new();
    output.stdout.hash(&mut hasher);
    let lines = output
        .stdout
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    (lines, hasher.finish())
}

#[test]
fn ls_l_of_ten_thousand_committed_entries() {
    let fixture = installed::Fixture::built("listing-reads", |source| {
        let directory = source.join("d");
        std::fs::create_dir(&directory).unwrap();
        // Fixed times, so every run commits the same listing.
        let at =
            |n: usize| std::time::UNIX_EPOCH + std::time::Duration::from_secs((1 << 30) + n as u64);
        for n in 0..FILES {
            let path = directory.join(format!("f{n:05}"));
            std::fs::write(&path, format!("file {n}\n")).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(at(n))
                .unwrap();
        }
        std::fs::File::open(&directory)
            .unwrap()
            .set_modified(at(FILES))
            .unwrap();
        FILES + 1
    });
    let opened = open_store_observed(
        fixture.config.clone(),
        installed::BINDING,
        installed::CURSOR,
        64,
        CACHE_BYTES,
        layerfs_storage::ReservationBlocks::default(),
    )
    .unwrap();
    let harness = Harness::new(opened.store.clone(), &fixture.directory, fixture.branch);
    let ready = harness.mount(1);
    let directory = Path::new(&ready.directory).join("d");
    for pass in ["cold", "steady"] {
        let (before, rss) = (mark(&opened), memory());
        let (lines, digest) = listing(&directory);
        let (after, rss_after) = (mark(&opened), memory());
        // "total N" and one line per entry.
        assert_eq!(lines, FILES + 1, "{pass}");
        let statements = after.statements - before.statements;
        let pack_bytes = after.pack_bytes - before.pack_bytes;
        println!(
            "LS_L pass={pass} entries={FILES} digest={digest:016x} statements={statements} \
             statements_per_entry={:.2} object_batches={} object_ids={} length_batches={} \
             pack_selections={} pack_bytes={pack_bytes} pack_bytes_per_entry={:.0} \
             vm_rss_kib={}->{} vm_hwm_kib={}->{}",
            statements as f64 / FILES as f64,
            after.object_batches - before.object_batches,
            after.object_ids - before.object_ids,
            after.length_batches - before.length_batches,
            after.pack_selections - before.pack_selections,
            pack_bytes as f64 / FILES as f64,
            rss.0,
            rss_after.0,
            rss.1,
            rss_after.1,
        );
    }
    harness.unmount(&ready);
    harness.stop();
    drop(opened);
    fixture.cleanup();
}
