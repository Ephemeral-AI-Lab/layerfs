//! #533 step 10b: a new empty PostgreSQL Store and Init's acquisition over
//! it. Refusal order, atomic windows, aliases, roots and cleanup match the
//! SQLite provider's (`acquisition_batching.rs`, `acquisition_disposal.rs`).
//!
//! Needs a PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for example
//! `host=127.0.0.1 port=55433 user=postgres password=... dbname=postgres
//! sslmode=disable`). Without it each test reports NOT_RUN and passes nothing.
#![cfg(feature = "postgresql")]
use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{
    port::{acquisition::*, PersistenceError},
    StoragePolicy,
};

const BINDING: &[u8] = b"postgresql-acquisition";
const CURSOR: [u8; 32] = [83; 32];

/// Drops the test's schema when the test ends, pass or fail.
struct Schema {
    url: String,
    selection: PostgresSelection,
}
impl Schema {
    fn new(url: String, tag: &str) -> Self {
        let name = format!("lfs533_{tag}_{}", std::process::id());
        let selection = PostgresSelection::new(&url, &name);
        let schema = Self { url, selection };
        schema.drop_schema();
        schema
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(&self.selection, StoragePolicy::frozen_default(), &catalog())
            .unwrap()
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

fn object(label: &[u8]) -> ObjectId {
    ObjectId::for_bytes(label)
}

fn begin(handles: &PostgresHandles) -> Owner {
    handles
        .acquisition
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: object(b"scope"),
        })
        .unwrap()
}

fn directory(parent: Option<u64>, name: &[u8], position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent,
            name: name.to_vec(),
        },
        position: Some(position),
        kind: InodeKind::Directory,
        metadata: meta(0o755),
        target_root: None,
        host_path: Some(b"/source".to_vec()),
        host: None,
    }
}

fn file(position: u64, identity: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("f{position:04}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::RegularFile,
        metadata: meta(0o644),
        target_root: None,
        host_path: Some(format!("/source/f{position:04}").into_bytes()),
        host: Some(HostIdentity {
            device: 1,
            inode: identity,
            evidence: [7; EVIDENCE_BYTES],
        }),
    }
}

fn symlink(position: u64) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: format!("s{position:04}").into_bytes(),
        },
        position: Some(position),
        kind: InodeKind::Symlink,
        metadata: meta(0o777),
        target_root: Some(object(b"opaque-link-target")),
        host_path: None,
        host: None,
    }
}

#[test]
fn a_new_store_is_empty_refuses_a_second_create_and_reopens() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "create");
    let created = schema.create();
    assert!(created
        .history
        .layer_stack_named(&layerfs_history::HistoryName::new("main").unwrap())
        .unwrap()
        .is_none());
    assert_eq!(created.history.statistics().unwrap().canonical, 0);
    // An existing schema is refused with nothing written, never merged into.
    let again = PostgresHandles::create(
        &schema.selection,
        StoragePolicy::frozen_default(),
        &catalog(),
    );
    assert_eq!(
        again.err(),
        Some(PersistenceError::Refused {
            status: "schema exists".to_owned()
        })
    );
    // Another home reads the binding, then opens it read-only and writable.
    assert_eq!(
        PostgresHandles::binding(&schema.selection).unwrap(),
        BINDING
    );
    assert!(PostgresHandles::open_writable(&schema.selection, b"another", CURSOR).is_err());
    let reader = PostgresHandles::open_read_only(&schema.selection, BINDING, CURSOR).unwrap();
    // A read-only session refuses an acquisition write before any row.
    assert!(reader
        .acquisition
        .begin(&Begin {
            source_device: 1,
            source_inode: 2,
            stack: [3; 16],
            scope: object(b"scope"),
        })
        .is_err());
    let writer = PostgresHandles::open_writable(&schema.selection, BINDING, CURSOR).unwrap();
    let owner = begin(&writer);
    writer
        .acquisition
        .put_entries(owner, &[directory(None, b"", 0)])
        .unwrap();
    // A session that did not begin an operation does not own it.
    assert_eq!(
        created.acquisition.work(owner),
        Err(AcquisitionError::Stale)
    );
    let abandoned = created.acquisition.abandoned(None, 10).unwrap();
    assert_eq!(abandoned.len(), 1);
    assert_eq!((abandoned[0].owner, abandoned[0].held_rows), (owner, 1));
    assert_eq!(
        created
            .acquisition
            .discard_abandoned(owner, 10)
            .unwrap()
            .remaining_rows,
        0
    );
    assert!(created.acquisition.abandoned(None, 10).unwrap().is_empty());
    assert_eq!(writer.acquisition.work(owner), Err(AcquisitionError::Stale));
}

#[test]
fn windows_keep_aliases_charges_and_the_first_refusal_in_order() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "windows");
    let handles = schema.create();
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, b"", 0), file(1, 1)])
        .unwrap();
    let before = port.work(owner).unwrap();
    let prefix: Vec<_> = (2..=40).map(|n| file(n, n)).collect();
    let mut changed = file(41, 1);
    changed.host.as_mut().unwrap().evidence[0] ^= 1;
    let mut invalid = file(42, 42);
    invalid.kind = InodeKind::Directory;
    let duplicate = file(1, 43);
    let malformed = AcquisitionError::Persistence(PersistenceError::Malformed);
    for (tail, expected) in [
        (
            vec![changed.clone(), invalid.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
        (
            vec![invalid.clone(), changed.clone()],
            AcquisitionError::Bounds,
        ),
        (vec![duplicate.clone(), changed.clone()], malformed.clone()),
        (
            vec![changed.clone(), duplicate.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
        (
            vec![file(50, 50), file(50, 51), invalid.clone()],
            malformed.clone(),
        ),
        (
            vec![file(51, 51), file(52, 51), changed.clone()],
            AcquisitionError::Changed { position: 41 },
        ),
        (
            vec![
                directory(Some(0), b"first", 60),
                directory(Some(0), b"second", 60),
                changed.clone(),
            ],
            malformed.clone(),
        ),
    ] {
        let mut entries = prefix.clone();
        entries.extend(tail);
        assert_eq!(port.put_entries(owner, &entries), Err(expected));
        assert_eq!(port.work(owner).unwrap(), before);
        assert_eq!(port.entries(owner, None, Limits::MAXIMUM).unwrap().len(), 2);
        assert_eq!(
            port.file_roots(owner, None, Limits::MAXIMUM).unwrap()[0].aliases,
            0
        );
    }
    // Fresh and existing aliases inside one window keep their first path.
    let mut entries = prefix;
    entries.push(file(41, 2));
    entries.push(file(42, 1));
    entries.extend((43..=60).map(symlink));
    let result = port.put_entries(owner, &entries).unwrap();
    assert_eq!(&result[result.len() - 2..], &[2, 1]);
    assert_eq!(result.len(), 41);
    let roots = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(roots.len(), 40);
    assert_eq!((roots[0].aliases, roots[1].aliases), (1, 1));
    let rows = port.entries(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(rows.len(), 61);
    assert_eq!(rows[0].key.parent, None);
    assert_eq!(rows[0].metadata, meta(0o755));
    let link = rows.iter().find(|row| row.position == 43).unwrap();
    assert_eq!(link.content_root, Some(object(b"opaque-link-target")));
    let alias = rows.iter().find(|row| row.position == 42).unwrap();
    assert_eq!(alias.canonical, Some(1));
    let jobs = port.jobs(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(jobs.len(), 40);
    assert_eq!(port.job(owner, 2).unwrap(), Some(jobs[1].clone()));
    assert_eq!(port.job(owner, 41).unwrap(), None);
    assert_eq!(
        port.directory_path(owner, 0).unwrap(),
        Some(b"/source".to_vec())
    );
    // Narrow windows resume from their last key.
    let mut after = None;
    let mut seen = 0;
    loop {
        let limits = Limits {
            rows: 7,
            bytes: 1 << 20,
        };
        let window = port.bindings(owner, after.as_ref(), limits).unwrap();
        if window.is_empty() {
            break;
        }
        seen += window.len();
        after = window.last().map(|binding| binding.key.clone());
    }
    assert_eq!(seen, 61);
    let held = port.work(owner).unwrap();
    let mut removed = (0, 0);
    loop {
        let disposal = port.dispose(owner, 25).unwrap();
        removed.0 += disposal.discarded.rows;
        removed.1 += disposal.discarded.bytes;
        if disposal.released {
            break;
        }
    }
    assert_eq!(removed, (held.held_rows, held.held_bytes));
    assert_eq!(port.work(owner), Err(AcquisitionError::Stale));
}

#[test]
fn missing_and_repeated_roots_roll_back_the_window_and_report_first_failure() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "roots");
    let handles = schema.create();
    let port = &handles.acquisition;
    let owner = begin(&handles);
    port.put_entries(owner, &[directory(None, b"", 0)]).unwrap();
    let entries: Vec<_> = (1..=100).map(|n| file(n, n)).collect();
    port.put_entries(owner, &entries).unwrap();
    let before = port.work(owner).unwrap();
    let mut roots: Vec<_> = (1_u64..=100)
        .map(|n| (n, object(&n.to_be_bytes())))
        .collect();
    roots[63].0 = 999;
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 999 })
    );
    roots[63].0 = 64;
    roots[77].0 = roots[76].0;
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 77 })
    );
    let roots = [(999, object(b"a")), (1, object(b"b")), (1, object(b"c"))];
    assert_eq!(
        port.complete_files(owner, &roots),
        Err(AcquisitionError::Changed { position: 999 })
    );
    assert_eq!(
        port.complete_files(owner, &[(1, object(b"a")), (u64::MAX, object(b"b"))]),
        Err(AcquisitionError::Bounds)
    );
    assert_eq!(port.work(owner).unwrap(), before);
    assert!(port
        .file_roots(owner, None, Limits::MAXIMUM)
        .unwrap()
        .iter()
        .all(|row| row.root.is_none()));
    let roots: Vec<_> = (1_u64..=100)
        .rev()
        .map(|n| (n, object(&n.to_be_bytes())))
        .collect();
    port.complete_files(owner, &roots).unwrap();
    let recorded = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
    assert!(recorded
        .iter()
        .all(|row| row.root == Some(object(&row.position.to_be_bytes()))));
    // Directory roots go to directory positions only.
    assert_eq!(
        port.set_directory_roots(owner, &[(1, object(b"not a directory"))]),
        Err(AcquisitionError::Changed { position: 1 })
    );
    port.set_directory_roots(owner, &[(0, object(b"root"))])
        .unwrap();
    let rows = port
        .entries(owner, None, Limits { rows: 1, bytes: 1 })
        .unwrap();
    assert_eq!(rows[0].content_root, Some(object(b"root")));
    assert_eq!(
        port.set_directory_roots(owner, &[(0, object(b"again"))]),
        Err(AcquisitionError::Changed { position: 0 })
    );
    port.advance(owner, Phase::Tree).unwrap();
    assert_eq!(
        port.release(owner),
        Err(AcquisitionError::Persistence(PersistenceError::Refused {
            status: "acquisition working rows remain".to_owned()
        }))
    );
}

#[test]
fn unplaced_children_take_their_positions_in_name_order() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "placement");
    let handles = schema.create();
    let port = &handles.acquisition;
    let owner = begin(&handles);
    let mut entries = vec![directory(None, b"", 0)];
    for n in (0..30_u64).rev() {
        let mut child = file(100 + n, 100 + n % 20);
        child.position = None;
        child.key.name = format!("c{n:03}").into_bytes();
        entries.push(child);
    }
    let mut link = symlink(0);
    link.position = None;
    entries.push(link);
    assert!(port.put_entries(owner, &entries).unwrap().is_empty());
    let mut after: Option<Vec<u8>> = None;
    let mut next = 1;
    let mut canonical = Vec::new();
    loop {
        let limits = Limits {
            rows: 8,
            bytes: 1 << 20,
        };
        let window = port
            .unplaced_children(owner, 0, after.as_deref(), limits)
            .unwrap();
        if window.is_empty() {
            break;
        }
        let placed: Vec<_> = window
            .iter()
            .map(|child| {
                let position = next;
                next += 1;
                Placed {
                    name: child.name.clone(),
                    position,
                    host: child.host.map(|host| (host, child.name.clone())),
                }
            })
            .collect();
        canonical.extend(port.place_children(owner, 0, &placed).unwrap());
        after = window.last().map(|child| child.name.clone());
    }
    assert_eq!(next, 32);
    // c000..c019 are first paths; c020..c029 alias c000..c009.
    assert_eq!(&canonical[..3], &[1, 2, 3]);
    assert_eq!(canonical[20], 1);
    assert!(port
        .unplaced_children(owner, 0, None, Limits::MAXIMUM)
        .unwrap()
        .is_empty());
    let roots = port.file_roots(owner, None, Limits::MAXIMUM).unwrap();
    assert_eq!(roots.len(), 20);
    assert_eq!(roots[0].aliases, 1);
    assert_eq!(roots[19].aliases, 0);
    // A child already placed is not placed again.
    let again = Placed {
        name: b"c000".to_vec(),
        position: 99,
        host: None,
    };
    assert_eq!(
        port.place_children(owner, 0, &[again]),
        Err(AcquisitionError::Changed { position: 99 })
    );
}
