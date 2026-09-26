//! The prepared namespace as a file-backed row spool: same root, no resident rows.
//!
//! #256's remaining slice carries a prepared update as ordered rows instead of a
//! resident vector per row kind. These cases drive the operation twice from one
//! base root - once with the resident slices a small caller uses, once through a
//! charged file spool - and require the *same canonical root*, because a streamed
//! input that changed the result would have changed canonical identity for an
//! input the service already accepted.
//!
//! The spool is the product's own ([`RowSpool`]): fixed 32-byte slots in key
//! order, payloads behind them, every byte charged, the file removed when the
//! update is done. A pass reads one slot and one row at a time, so the wide case
//! applies 1,025 rows whose resident footprint is one row - the property the
//! resident vector it replaces could not offer, and the reason a frame-sized
//! admission was the only bound left.
//!
//! Three refusals are checked as well: an update that wrote fewer rows than it
//! declared, a row beyond the declared totals or beyond the charged capacity, and
//! a key that does not rise inside its run.

mod support;

use layerfs_content::filesystem::rows::{PreparedUpdate, RowSource, RowSpool, SPOOL_SLOT_BYTES};
use layerfs_content::filesystem::{
    update_filesystem, DirectoryUpdate, FilesystemResources, FilesystemRootId, InodeUpdate,
    LogicalPath, PathName,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{ContentError, ObjectId};
use std::path::{Path, PathBuf};
use support::filesystem::{synthetic, value, with_objects, Session, TreeStore};

/// Names one wide generation binds.
const WIDE: usize = 1_024;

fn name(value: &str) -> PathName {
    PathName::new(value).expect("name")
}

fn file_value(label: &str) -> layerfs_content::object::inode_leaf::InodeValue {
    value(
        InodeKind::RegularFile,
        synthetic(&format!("rows/{label}/content")),
        synthetic(&format!("rows/{label}/metadata")),
    )
}

fn directory_value(label: &str) -> layerfs_content::object::inode_leaf::InodeValue {
    value(
        InodeKind::Directory,
        synthetic(&format!("rows/{label}/content")),
        synthetic(&format!("rows/{label}/metadata")),
    )
}

/// A file path this case owns, removed when it is dropped.
struct SpoolPath(PathBuf);

impl SpoolPath {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let unique = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "layerfs-rows-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for SpoolPath {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A spool sized for these rows, with a capacity generous enough for them.
fn spool(
    label: &str,
    directories: &[DirectoryUpdate],
    inodes: &[InodeUpdate],
    serials: &[u64],
) -> (SpoolPath, RowSpool) {
    let path = SpoolPath::new(label);
    let capacity = 1 << 22;
    let mut spool = RowSpool::create(
        path.path().to_path_buf(),
        directories.len(),
        inodes.len(),
        serials.len(),
        capacity,
    )
    .expect("spool");
    for row in directories {
        spool.push_directory(row).expect("directory row");
    }
    for row in inodes {
        spool.push_inode(row).expect("inode row");
    }
    for serial in serials {
        spool.push_serial(*serial).expect("serial row");
    }
    spool.seal().expect("sealed");
    (path, spool)
}

/// Applies one prepared update and returns the root it published.
fn apply(
    store: &mut TreeStore,
    update: &PreparedUpdate<'_>,
) -> layerfs_content::ContentResult<ObjectId> {
    let result = with_objects(store, |objects| update_filesystem(objects, update, None))?;
    Ok(result.root.0)
}

#[test]
fn a_spooled_update_publishes_the_same_root_as_resident_rows() {
    let mut session = Session::new(1).expect("empty filesystem");
    let directory = session.allocate();
    let first = session.allocate();
    let second = session.allocate();
    let third = session.allocate();
    // Two rows in parent order, each with its names in name order: one directory
    // the update creates, and the names bound inside it.
    let directories = [
        DirectoryUpdate {
            parent: 1,
            changes: vec![(name("d"), Some(directory)), (name("zero"), Some(third))],
        },
        DirectoryUpdate {
            parent: directory,
            changes: vec![(name("one"), Some(first)), (name("three"), Some(second))],
        },
    ];
    let inodes = [
        InodeUpdate {
            serial: directory,
            value: directory_value("d"),
        },
        InodeUpdate {
            serial: first,
            value: file_value("one"),
        },
        InodeUpdate {
            serial: second,
            value: file_value("three"),
        },
        InodeUpdate {
            serial: third,
            value: file_value("zero"),
        },
    ];
    let serials = [directory, first, second, third];

    let mut resident_store = session.store.clone();
    let resident = {
        let input = layerfs_content::filesystem::FilesystemInput {
            base: Some(FilesystemRootId(session.root)),
            scope: session.scope,
            root_serial: session.root_serial,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &serials,
            resources: FilesystemResources::default(),
        };
        let result = with_objects(&mut resident_store, |objects| {
            update_filesystem(objects, &input, None)
        })
        .expect("resident update");
        result.root.0
    };

    let (_path, spool) = spool("identity", &directories, &inodes, &serials);
    let mut spooled_store = session.store.clone();
    let spooled = {
        let update = PreparedUpdate {
            base: Some(FilesystemRootId(session.root)),
            scope: session.scope,
            root_serial: session.root_serial,
            resources: FilesystemResources::default(),
            rows: &spool,
        };
        apply(&mut spooled_store, &update).expect("spooled update")
    };
    assert_eq!(
        resident, spooled,
        "the same rows must publish the same canonical root"
    );

    // The exact rows are reachable through the spool's own lookups, not just
    // through a pass: one row by parent, one value by serial.
    let row = spool
        .directory_for(directory)
        .expect("lookup")
        .expect("row");
    assert_eq!(row.parent, directory);
    assert_eq!(row.changes.len(), 2);
    assert_eq!(
        row.changes[0].0,
        name("one"),
        "a looked-up row keeps its name order"
    );
    let value = spool.value_for(first).expect("lookup").expect("value");
    assert_eq!(value.content_root, file_value("one").content_root);
    assert!(spool.value_for(third + 1).expect("lookup").is_none());

    // And the result reads back through the public API.
    let mut read =
        layerfs_content::filesystem::FilesystemRead::new(&spooled_store, FilesystemRootId(spooled))
            .expect("read");
    let listing = read
        .list(&LogicalPath::root(), None, 8, 4096)
        .expect("listing");
    let listed: Vec<Vec<u8>> = listing
        .entries
        .iter()
        .map(|(key, _)| key.as_bytes().to_vec())
        .collect();
    assert!(listed.contains(&b"d".to_vec()));
    assert!(listed.contains(&b"zero".to_vec()));
}

#[test]
fn a_wide_spooled_generation_applies_every_row_from_the_file() {
    let mut session = Session::new(1).expect("empty filesystem");
    let mut directories = Vec::new();
    let mut inodes = Vec::new();
    let mut serials = Vec::new();
    let mut bindings = Vec::new();
    for index in 0..WIDE {
        let serial = session.allocate();
        serials.push(serial);
        bindings.push((name(&format!("file-{index:04}")), Some(serial)));
        inodes.push(InodeUpdate {
            serial,
            value: file_value(&format!("wide-{index}")),
        });
    }
    bindings.sort_by(|left, right| left.0.cmp(&right.0));
    directories.push(DirectoryUpdate {
        parent: 1,
        changes: bindings,
    });
    serials.sort_unstable();
    inodes.sort_by_key(|row| row.serial);

    let (_path, spool) = spool("wide", &directories, &inodes, &serials);
    // The spool holds one fixed slot per row and the payloads behind them; the
    // resident side of the same update is one row at a time.
    let table = (directories.len() + inodes.len() + serials.len()) as u64 * SPOOL_SLOT_BYTES;
    assert!(spool.held_bytes() >= table, "every row owns one slot");
    assert_eq!(spool.written_directories(), directories.len());
    assert_eq!(spool.written_inodes(), inodes.len());
    assert_eq!(spool.written_new(), serials.len());

    let mut store = session.store.clone();
    let update = PreparedUpdate {
        base: Some(FilesystemRootId(session.root)),
        scope: session.scope,
        root_serial: session.root_serial,
        resources: FilesystemResources::default(),
        rows: &spool,
    };
    let root = apply(&mut store, &update).expect("wide spooled update");

    let mut read = layerfs_content::filesystem::FilesystemRead::new(&store, FilesystemRootId(root))
        .expect("read");
    let mut seen = 0_usize;
    let mut after: Option<PathName> = None;
    loop {
        let page = read
            .list(&LogicalPath::root(), after.as_ref(), 128, 64 * 1024)
            .expect("listing");
        seen += page.entries.len();
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    assert_eq!(seen, WIDE, "every bound name is in the published root");
}

#[test]
fn a_spool_that_does_not_hold_its_declared_rows_is_refused() {
    let path = SpoolPath::new("short");
    let mut spool = RowSpool::create(path.path().to_path_buf(), 2, 1, 1, 1 << 20).expect("spool");
    spool
        .push_directory(&DirectoryUpdate {
            parent: 1,
            changes: vec![(name("a"), Some(2))],
        })
        .expect("first row");
    // Two directory rows were declared and one was written.
    assert!(matches!(
        spool.seal().expect_err("short spool"),
        ContentError::InvalidRecord("prepared row count")
    ));
}

#[test]
fn a_spool_refuses_a_row_beyond_its_declared_totals_or_capacity() {
    let path = SpoolPath::new("over");
    let mut spool = RowSpool::create(path.path().to_path_buf(), 1, 0, 0, 1 << 20).expect("spool");
    spool
        .push_directory(&DirectoryUpdate {
            parent: 1,
            changes: vec![(name("a"), Some(2))],
        })
        .expect("first row");
    assert!(matches!(
        spool.push_directory(&DirectoryUpdate {
            parent: 3,
            changes: Vec::new(),
        }),
        Err(ContentError::InvalidRecord("directory row count"))
    ));

    let path = SpoolPath::new("capacity");
    // The slot table alone is 48 + 32 bytes; one byte less than that cannot hold
    // the spool at all.
    assert!(matches!(
        RowSpool::create(path.path().to_path_buf(), 1, 0, 0, 79),
        Err(ContentError::ResourceUnavailable {
            what: "prepared row spool"
        })
    ));
}

#[test]
fn a_spool_refuses_a_key_that_does_not_rise_inside_its_run() {
    let path = SpoolPath::new("order");
    let mut spool = RowSpool::create(path.path().to_path_buf(), 2, 0, 0, 1 << 20).expect("spool");
    spool
        .push_directory(&DirectoryUpdate {
            parent: 7,
            changes: Vec::new(),
        })
        .expect("first row");
    assert!(matches!(
        spool.push_directory(&DirectoryUpdate {
            parent: 7,
            changes: Vec::new(),
        }),
        Err(ContentError::NonCanonicalOrdering)
    ));
}

#[test]
fn the_spool_file_is_removed_when_the_update_is_done() {
    let path = SpoolPath::new("cleanup");
    let mut spool = RowSpool::create(path.path().to_path_buf(), 1, 0, 0, 1 << 20).expect("spool");
    spool
        .push_directory(&DirectoryUpdate {
            parent: 1,
            changes: Vec::new(),
        })
        .expect("row");
    spool.seal().expect("sealed");
    assert!(path.path().exists());
    spool.cleanup().expect("cleanup");
    assert!(!path.path().exists());
    drop(spool);
}
