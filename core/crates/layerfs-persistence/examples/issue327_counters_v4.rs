//! Frozen v4 matched component screen for publication accounting. No SDK/FUSE timing claim.
use layerfs_history::*;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
    time::Instant,
};
const COUNT: usize = 128;
const BYTES: usize = 32768;
fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"issue327-counter-screen".to_vec(),
        cursor_key: [71; 32],
        incarnation: 1,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn fixture() -> Vec<Vec<u8>> {
    let mut seed = 327u64;
    (0..COUNT)
        .map(|_| {
            (0..BYTES)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed as u8
                })
                .collect()
        })
        .collect()
}
#[cfg(unix)]
fn main() {
    assert_eq!(
        std::env::var("SCREEN_SCENARIO").as_deref(),
        Ok("issue327-publication-totals-v4")
    );
    let args = std::env::args().collect::<Vec<_>>();
    assert_eq!(args.len(), 4, "run|verify STORE RECEIPT");
    assert_eq!(
        std::env::var("LAYERFS_CONSTRUCTION_WORKERS").as_deref(),
        Ok("1")
    );
    let path = Path::new(&args[2]);
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&args[3])
        .unwrap();
    let total = Instant::now();
    if args[1] == "verify" {
        let before = Instant::now();
        let copy = path.with_extension("verify-copy.db");
        std::fs::copy(path, &copy).unwrap();
        let owner =
            Handles::open_writable(config(&copy), b"issue327-counter-screen", [71; 32]).unwrap();
        let handles =
            Handles::open_read_only(config(&copy), b"issue327-counter-screen", [71; 32]).unwrap();
        let _owner = owner;
        let name = HistoryName::new("screen").unwrap();
        let stack = LayerStackId::from_authority([1; 16]);
        let branch = BranchId::named(stack, &name);
        let mut next = handles.history.branch(branch).unwrap().unwrap().head_commit;
        let mut count = 0;
        let mut logical = 0;
        while let Some(id) = next {
            let record = handles.history.commit(id).unwrap().unwrap();
            assert_eq!(record.meta.message.as_deref(), Some("Counter screen"));
            assert_eq!(record.meta.statistics.logical, BYTES as u64);
            logical += record.meta.statistics.logical;
            count += 1;
            next = record.parent;
        }
        assert_eq!(count, COUNT);
        let stats = handles.history.statistics().unwrap();
        let expected = std::env::var("SCREEN_COUNTERS").unwrap();
        assert_eq!(stats.logical, if expected == "on" { logical } else { 0 });
        if expected == "on" {
            assert!(stats.canonical >= logical && stats.stored > 0);
        }
        let ns = before.elapsed().as_nanos();
        writeln!(output,"{{\"verification_ns\":{ns},\"commits\":{count},\"logical\":{},\"canonical\":{},\"stored\":{},\"file\":{},\"status\":\"PASS\"}}",stats.logical,stats.canonical,stats.stored,stats.file).unwrap();
        return;
    }
    assert_eq!(args[1], "run");
    assert!(!path.exists());
    let input = fixture(); // Fixed warm in-memory fixture, generated outside the operation timer in BOTH arms.
    let started = Instant::now();
    let policy = StoragePolicy::frozen_default();
    let handles = Handles::create(config(path), policy, &authority()).unwrap();
    let root = layerfs_content::ObjectId::from_bytes(&[0; 32]).unwrap();
    let stack = LayerStackId::from_authority([1; 16]);
    let name = HistoryName::new("screen").unwrap();
    let branch = BranchId::named(stack, &name);
    let initial = handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack,
            name: name.clone(),
            scope: root,
            profile: root,
            genesis_root: root,
            logical: 0,
        })
        .unwrap();
    let mut snapshot = handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name,
            source: ForkSource::Layer(initial.head_layer),
        })
        .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    for bytes in &input {
        let save = storage.begin_save().unwrap();
        let construction = policy.construction();
        let capacity = construction.capacities();
        let (built, _) = Timing::disabled("screen.construct", |timer| {
            layerfs_content::file::construct_bytes(
                construction,
                &capacity,
                bytes,
                &mut save.sink(),
                timer.child("construct"),
            )
        });
        let file = built.unwrap();
        let saved = save.finish().unwrap();
        let meta = CommitMeta {
            message: Some("Counter screen".into()),
            statistics: CommitStatistics {
                logical: BYTES as u64,
                new_canonical: saved.canonical_bytes,
                new_stored: saved.stored_bytes,
                new_metadata: saved.metadata_bytes,
            },
            ..CommitMeta::new(CommitVia::Files)
        };
        let result = handles
            .history
            .commit_branch(&CommitRequest {
                branch,
                expected_head: snapshot.branch.head_commit,
                expected_base: snapshot.branch.base_layer,
                expected_root: snapshot.effective_root,
                candidate_root: file.root,
                profile: root,
                scope: root,
                second_parent: None,
                meta,
                force: false,
            })
            .unwrap();
        if let CommitOutcome::Committed { record, .. } = result {
            snapshot.branch.head_commit = Some(record.id);
            snapshot.effective_root = record.root;
        } else {
            panic!("unique root");
        }
    }
    let work = handles.diagnostics().unwrap();
    drop(storage);
    let sealed = handles.seal().unwrap();
    let operation = started.elapsed().as_nanos();
    let sql_version = sealed.sqlite_version.clone();
    let metadata = File::open(path).unwrap().metadata().unwrap();
    #[cfg(unix)]
    let allocated = {
        use std::os::unix::fs::MetadataExt;
        metadata.blocks() * 512
    };
    writeln!(output,"{{\"sql_statement_ns\":{},\"sql_statements\":{},\"sql_vm_steps\":{},\"sqlite\":\"{}\",\"operation_ns\":{operation},\"complete_ns\":{},\"file_bytes\":{},\"allocated_bytes\":{allocated},\"sealed_bytes\":{},\"objects_input\":{COUNT},\"input_bytes\":{},\"cache\":\"created-in-sample Store; fixed warm in-memory fixture\",\"status\":\"RECORDED\"}}",work.statement_ns,work.statements,work.vm_steps,sql_version,total.elapsed().as_nanos(),metadata.len(),sealed.bytes,COUNT*BYTES).unwrap();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("this frozen recorder requires Unix (st_blocks); nothing recorded");
}
