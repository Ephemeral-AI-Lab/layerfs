//! #87: releasing the last acquisition operation empties Init's working
//! tables, so a PostgreSQL Store's size right after Init is the size
//! autovacuum would later leave, and a never-used Store's size.
//!
//! The compaction must never touch a live operation (another session's, or
//! one that begins concurrently), must never change a release's outcome,
//! and must continue the operation identity.
//!
//! Needs a PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for example
//! `host=127.0.0.1 port=55433 user=postgres password=... dbname=postgres
//! sslmode=disable`). Without it each test reports NOT_RUN and passes nothing.
//! Sizes are read with catalog functions on the test's own disposable schema.
#![cfg(feature = "postgresql")]
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{port::acquisition::*, StoragePolicy};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"postgresql-acquisition-compaction";
const CURSOR: [u8; 32] = [87; 32];
const PAGE: i64 = 8192;
const TABLES: [&str; 3] = ["init_entry", "init_host_file", "init_operation"];

/// Drops the test's schema when the test ends, pass or fail.
struct Schema {
    url: String,
    selection: PostgresSelection,
}
impl Schema {
    fn new(url: String, tag: &str) -> Self {
        let name = format!("lfs87_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(&url, &name);
        let schema = Self { url, selection };
        schema.drop_schema();
        schema
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(&self.selection, StoragePolicy::frozen_default(), &catalog())
            .unwrap()
    }
    fn open(&self) -> PostgresHandles {
        PostgresHandles::open_writable(&self.selection, BINDING, CURSOR).unwrap()
    }
    fn client(&self) -> postgres::Client {
        postgres::Client::connect(&self.url, postgres::NoTls).unwrap()
    }
    /// Every table's total size (heap, indexes, TOAST, FSM and VM forks).
    fn sizes(&self) -> Vec<(String, i64)> {
        self.client()
            .query(
                "SELECT c.relname::text, pg_total_relation_size(c.oid) FROM pg_class c \
                 JOIN pg_namespace n ON n.oid = c.relnamespace \
                 WHERE n.nspname = $1 AND c.relkind = 'r' ORDER BY 1",
                &[&self.selection.schema],
            )
            .unwrap()
            .iter()
            .map(|row| (row.get(0), row.get(1)))
            .collect()
    }
    fn acquisition_sizes(&self) -> Vec<(String, i64)> {
        let mut sizes = self.sizes();
        sizes.retain(|(name, _)| TABLES.contains(&name.as_str()));
        assert_eq!(sizes.len(), 3);
        sizes
    }
    /// What autovacuum does to the working tables, run now.
    fn vacuum(&self) {
        let tables: Vec<String> = TABLES
            .iter()
            .map(|t| format!("{}.{t}", self.selection.schema))
            .collect();
        self.client()
            .batch_execute(&format!("VACUUM {}", tables.join(", ")))
            .unwrap();
    }
    fn drop_schema(&self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE",
                self.selection.schema
            ));
        }
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        self.drop_schema();
    }
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

fn meta(mode: u32) -> layerfs_content::filesystem::attributes::PortableMetadata {
    layerfs_content::filesystem::attributes::PortableMetadata {
        mode,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    }
}

fn root() -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: None,
            name: Vec::new(),
        },
        position: Some(0),
        kind: InodeKind::Directory,
        metadata: meta(0o755),
        target_root: None,
        host_path: Some(b"/source".to_vec()),
        host: None,
    }
}

fn file(position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position:06}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata: meta(0o644),
        target_root: None,
        host_path: Some(format!("/source/f{position:06}").into_bytes()),
        host: Some(HostIdentity {
            device: 1,
            inode: position,
            evidence: [7; EVIDENCE_BYTES],
        }),
    }
}

fn root_of(position: u64) -> ObjectId {
    ObjectId::for_bytes(&position.to_be_bytes())
}

/// Scans `files` regular files under one root directory, as Init's walk does.
fn scan(port: &dyn Acquisition, files: u64) -> Owner {
    let owner = port
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: ObjectId::for_bytes(b"scope"),
        })
        .unwrap();
    port.put_entries(owner, &[root()]).unwrap();
    for start in (1..=files).step_by(2_000) {
        let window: Vec<_> = (start..=files.min(start + 1_999)).map(file).collect();
        port.put_entries(owner, &window).unwrap();
    }
    owner
}

/// Every recorded file root, read in windows from the first position.
fn file_roots(port: &dyn Acquisition, owner: Owner) -> Vec<FileRoot> {
    let (mut all, mut after) = (Vec::new(), None);
    loop {
        let window = port.file_roots(owner, after, Limits::MAXIMUM).unwrap();
        let Some(last) = window.last() else {
            return all;
        };
        after = Some(last.position);
        all.extend(window);
    }
}

/// Constructs every file and the root directory, then checks the rows.
fn construct(port: &dyn Acquisition, owner: Owner, files: u64) {
    for start in (1..=files).step_by(2_000) {
        let roots: Vec<_> = (start..=files.min(start + 1_999))
            .map(|n| (n, root_of(n)))
            .collect();
        port.complete_files(owner, &roots).unwrap();
    }
    port.set_directory_roots(owner, &[(0, ObjectId::for_bytes(b"root"))])
        .unwrap();
    let roots = file_roots(port, owner);
    assert_eq!(roots.len() as u64, files);
    assert!(roots
        .iter()
        .all(|row| row.root == Some(root_of(row.position))));
    assert_eq!(port.work(owner).unwrap().held_rows, 1 + 2 * files);
}

/// Removes every working row and the record, as a successful Init does.
fn dispose(port: &dyn Acquisition, owner: Owner) {
    let held = port.work(owner).unwrap();
    let mut removed = (0, 0);
    loop {
        let disposal = port.dispose(owner, WRITE_WINDOW_ROWS).unwrap();
        removed.0 += disposal.discarded.rows;
        removed.1 += disposal.discarded.bytes;
        if disposal.released {
            break;
        }
    }
    assert_eq!(removed, (held.held_rows, held.held_bytes));
    assert_eq!(port.work(owner), Err(AcquisitionError::Stale));
}

fn init(port: &dyn Acquisition, files: u64) -> Owner {
    let owner = scan(port, files);
    construct(port, owner, files);
    dispose(port, owner);
    owner
}

/// Each table within `pages` pages of the other measurement.
fn within(left: &[(String, i64)], right: &[(String, i64)], pages: i64) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(l, r)| l.0 == r.0 && (l.1 - r.1).abs() <= pages * PAGE)
}

#[test]
fn bytes_right_after_init_are_the_bytes_after_autovacuum() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "size");
    let handles = schema.create();
    let fresh = schema.sizes();
    let fresh_acquisition = schema.acquisition_sizes();
    let port = &handles.acquisition;
    let first = scan(port, 6_000);
    construct(port, first, 6_000);
    // The rows really occupy pages while the operation is live.
    let live = schema.acquisition_sizes();
    assert!(live.iter().zip(&fresh_acquisition).all(|(l, f)| l.1 > f.1));
    dispose(port, first);
    // Right after Init the whole Store is a never-used Store's size.
    let after = schema.sizes();
    assert_eq!(after, fresh);
    // Autovacuum (run now) finds nothing to reclaim: ±1 page per table.
    schema.vacuum();
    assert!(within(&schema.acquisition_sizes(), &fresh_acquisition, 1));
    // Back-to-back Inits retain nothing; the identity continues.
    let second = init(port, 3_000);
    assert!(second.operation > first.operation);
    assert_eq!(schema.acquisition_sizes(), fresh_acquisition);
    let third = init(port, 10);
    assert!(third.operation > second.operation);
    assert_eq!(schema.sizes(), fresh);
}

#[test]
fn another_sessions_live_operation_keeps_its_rows() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "live");
    let created = schema.create();
    let fresh = schema.acquisition_sizes();
    let other = schema.open();
    let first = scan(&created.acquisition, 500);
    let live = scan(&other.acquisition, 700);
    construct(&created.acquisition, first, 500);
    dispose(&created.acquisition, first);
    // The other session's operation is untouched and continues.
    assert_eq!(other.acquisition.work(live).unwrap().held_rows, 1 + 2 * 700);
    assert_eq!(file_roots(&other.acquisition, live).len(), 700);
    assert_ne!(schema.acquisition_sizes(), fresh);
    construct(&other.acquisition, live, 700);
    dispose(&other.acquisition, live);
    assert_eq!(schema.acquisition_sizes(), fresh);
}

#[test]
fn a_held_table_keeps_the_release_and_the_rows_until_the_next_init() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "held");
    let handles = schema.create();
    let fresh = schema.acquisition_sizes();
    let port = &handles.acquisition;
    let owner = scan(port, 1_000);
    construct(port, owner, 1_000);
    // A concurrent reader holds `init_entry` in its open transaction.
    let mut reader = schema.client();
    let mut hold = reader.transaction().unwrap();
    hold.query(
        &format!(
            "SELECT count(*) FROM {}.init_entry",
            schema.selection.schema
        ),
        &[],
    )
    .unwrap();
    let started = Instant::now();
    dispose(port, owner);
    // The compaction waited its bound, gave up, and the release committed.
    assert!(started.elapsed() >= Duration::from_millis(1_900));
    hold.commit().unwrap();
    assert_ne!(schema.acquisition_sizes(), fresh);
    // The next Init's release empties the tables.
    init(port, 10);
    assert_eq!(schema.acquisition_sizes(), fresh);
}

#[test]
fn concurrent_inits_never_lose_a_live_operations_rows() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "race");
    let _created = schema.create();
    let fresh = schema.acquisition_sizes();
    let sessions: Vec<_> = (0..3).map(|_| schema.open()).collect();
    std::thread::scope(|scope| {
        for (index, handles) in sessions.iter().enumerate() {
            scope.spawn(move || {
                for cycle in 0..12_u64 {
                    // Different sizes keep the sessions' phases interleaving.
                    init(&handles.acquisition, 50 + 37 * index as u64 + 11 * cycle);
                }
            });
        }
    });
    // The last release found no operation and emptied the tables.
    assert_eq!(schema.acquisition_sizes(), fresh);
}
