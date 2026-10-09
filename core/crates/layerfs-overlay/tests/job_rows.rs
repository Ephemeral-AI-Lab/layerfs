//! The inode rows of one visit job. A row the job read is read once and is
//! written over what was read: the stored row is the one the indexed path
//! stores, for every kind of change. Past the rows a job keeps, its reads and
//! writes are the indexed statements, and nothing is refused. A serial
//! reserved for a job is inserted without a read, so a row that already has
//! it fails the whole job.
use layerfs_overlay::{
    Binding, Changes, DirectoryEntryChange, Inode, InodeKind, NativeEffect, NativeMount, Overlay,
    OverlayError, ProfileConfig, Route, StatementKind, COMPOUND_INODES,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct World {
    db: Overlay,
    route: Route,
    mount: NativeMount,
    path: PathBuf,
}
impl Drop for World {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).unwrap();
    }
}
fn world() -> World {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "layerfs-job-rows-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
    let route = db.open_workspace([41; 32], [42; 32]).unwrap();
    let mount = db.create_native_mount(route, 1).unwrap();
    World {
        db,
        route,
        mount,
        path,
    }
}
/// A file created in the Workspace's first generation.
fn file(serial: u64) -> Inode {
    Inode {
        serial,
        kind: InodeKind::File,
        mode: 0o644,
        mtime_seconds: 7,
        mtime_nanoseconds: 9,
        nlink: 1,
        size: 9000,
        inherited_cutoff: 0,
        born: 1,
        entries: 0,
        subdirs: 0,
    }
}
fn directory(entries: u64, subdirs: u64) -> Inode {
    Inode {
        kind: InodeKind::Directory,
        mode: 0o755,
        size: 0,
        born: 0,
        entries,
        subdirs,
        ..file(1)
    }
}
/// The file `serial` created under the root, whose row counts `entries`.
fn created(serial: u64, entries: u64) -> Changes {
    Changes {
        created: Some(serial),
        inodes: vec![directory(entries, 0), file(serial)],
        directory_entries: vec![DirectoryEntryChange {
            parent: 1,
            name: format!("f{serial}").into_bytes(),
            binding: Binding::Bound {
                serial,
                inherited: false,
            },
        }],
        ..Changes::default()
    }
}
impl World {
    fn create(&self, serial: u64, entries: u64) {
        let applied = self
            .db
            .mutate_native_visit(self.mount, serial, 1, None, |rows, _| {
                assert_eq!(rows.active().number(), 1);
                Ok(Some((created(serial, entries), NativeEffect::None)))
            })
            .unwrap()
            .expect("published");
        self.db.reply_attempted(applied.publication).unwrap();
    }
    /// One visit that reads `reads` in order and publishes `finals`; the
    /// Inode-family statements it attempted.
    fn visit(&self, reads: &[u64], finals: Vec<Inode>) -> u64 {
        let family = || self.db.diagnostics().statements[StatementKind::Inode as usize].attempts;
        let before = family();
        let applied = self
            .db
            .mutate_native_visit(self.mount, 900, 1, None, |rows, _| {
                for serial in reads {
                    rows.inode(*serial)?;
                }
                Ok(Some((
                    Changes {
                        inodes: finals,
                        ..Changes::default()
                    },
                    NativeEffect::None,
                )))
            })
            .unwrap()
            .expect("published");
        let attempts = family() - before;
        self.db.reply_attempted(applied.publication).unwrap();
        attempts
    }
    fn row(&self, serial: u64) -> Inode {
        self.db.inode(self.route, serial).unwrap().unwrap()
    }
}

#[test]
fn a_row_the_job_read_is_written_as_the_indexed_path_writes_it() {
    let w = world();
    w.create(50, 1);
    w.create(51, 2);
    // Every kind of change, on a row the job read and on one it did not:
    // more bytes, a time alone, a mode, a link, fewer bytes, bytes again.
    let changes: [fn(Inode) -> Inode; 6] = [
        |old| Inode {
            size: 20_000,
            mtime_seconds: 8,
            ..old
        },
        |old| Inode {
            mtime_seconds: 9,
            mtime_nanoseconds: 1,
            ..old
        },
        |old| Inode { mode: 0o600, ..old },
        |old| Inode { nlink: 2, ..old },
        |old| Inode {
            size: 100,
            mtime_seconds: 10,
            ..old
        },
        |old| Inode {
            size: 5000,
            mtime_seconds: 11,
            ..old
        },
    ];
    for (step, change) in changes.iter().enumerate() {
        let (read, unread) = (change(w.row(50)), change(w.row(51)));
        // One read of the row and its one update; without the read, the
        // row's active layer probe and its update.
        assert_eq!(w.visit(&[50, 50, 50], vec![read.clone()]), 2, "step {step}");
        assert_eq!(w.visit(&[], vec![unread.clone()]), 2, "step {step}");
        let (kept, indexed) = (w.row(50), w.row(51));
        assert_eq!(
            kept,
            Inode {
                serial: 50,
                ..indexed.clone()
            },
            "step {step}"
        );
        // The engine keeps the cutoff; every other value is the caller's.
        assert_eq!(
            kept,
            Inode {
                inherited_cutoff: kept.inherited_cutoff,
                ..read
            },
            "step {step}"
        );
    }
    // The shrink lowered the cutoff of both rows, and the regrow kept it.
    assert_eq!(w.row(50).inherited_cutoff, 0);

    // A directory's counts and time, then its mode, over the row it read.
    assert_eq!(w.visit(&[1], vec![directory(3, 1)]), 2);
    assert_eq!(w.row(1), directory(3, 1));
    let renamed = Inode {
        mode: 0o700,
        mtime_seconds: 12,
        ..directory(3, 1)
    };
    assert_eq!(w.visit(&[1], vec![renamed.clone()]), 2);
    assert_eq!(w.row(1), renamed);
}

#[test]
fn a_job_keeps_the_rows_it_can_publish_and_reads_the_rest_by_index() {
    let w = world();
    let serials: Vec<u64> = (50..56).collect();
    assert!(serials.len() > COMPOUND_INODES);
    for (index, serial) in serials.iter().enumerate() {
        w.create(*serial, index as u64 + 1);
    }
    let grown = |serial| Inode {
        size: 12_345,
        mtime_seconds: 8,
        ..file(serial)
    };
    // Two passes over six rows: the first reads all six and keeps as many
    // as one job can publish; the second reads the two that were not kept.
    // The kept row is updated with no probe, the other after its probe.
    let twice: Vec<u64> = serials.iter().chain(&serials).copied().collect();
    let kept = COMPOUND_INODES as u64;
    let unkept = serials.len() as u64 - kept;
    assert_eq!(
        w.visit(&twice, vec![grown(50), grown(55)]),
        kept + 2 * unkept + 1 + 2
    );
    for serial in [50, 55] {
        assert_eq!(w.row(serial), grown(serial));
    }
    for serial in 51..55 {
        assert_eq!(w.row(serial), file(serial));
    }
}

#[test]
fn a_reserved_serial_that_already_has_a_row_fails_the_whole_job() {
    let w = world();
    w.create(50, 1);
    let source = w.db.acquire_base_source(w.route, 1).unwrap();
    let state = w.db.state(w.route).unwrap();
    let counts = w.db.resources(Some(w.route)).unwrap().counts;
    let again = w.db.apply(source, &created(50, 2));
    assert!(matches!(again, Err(OverlayError::Sql(_))), "{again:?}");
    assert_eq!(w.db.state(w.route).unwrap(), state);
    assert_eq!(w.db.resources(Some(w.route)).unwrap().counts, counts);
    assert_eq!(w.row(1), directory(1, 0));
    assert_eq!(w.row(50), file(50));
}
