//! #818 I2: wide-directory placement in set windows answers and stores
//! exactly what the per-row loop does, on SQLite and on PostgreSQL.
//!
//! Each case sets up two equal Stores. One places the unit's children in one
//! `place_children` call (set windows, dependency checks, per-row
//! boundaries). The reference places them one child per call: a one-child
//! unit runs the unchanged per-row bind and place, so calling it child by
//! child in order replays the loop's statements in the loop's order, each
//! seeing the rows the children before it stored. The two must give the
//! same first refusal; on success every working row and the held charge are
//! equal, and a refused unit leaves the rows as they were before it. (The
//! peak charges are not compared: the reference charges each child apart.)
//!
//! Shapes: the corrections of `init.md` I2 (a repeated name, a repeated
//! position, a position that does not convert to signed, a dependency on a
//! row stored by an earlier unit or an earlier window of the same unit, an
//! all-hardlink directory), each with and without an earlier `Changed` in
//! the same window, plus kind mismatches, missing names and a large mixed
//! directory. The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and
//! reports NOT_RUN without it.
mod support;

use layerfs_content::{inode_leaf::InodeKind, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, SqliteAcquisitionSchema, SqliteHandles,
};
use layerfs_storage::{port::acquisition::*, StoragePolicy};
use support::{pg, Temp};

const BINDING: &[u8] = b"acquisition-placement";
const CURSOR: [u8; 32] = [91; 32];

#[derive(Clone, Copy, Debug)]
enum Backend {
    Sqlite,
    Postgresql,
}

/// One new Store with one begun acquisition.
struct Rig {
    sqlite: Option<(SqliteHandles, Temp)>,
    postgresql: Option<(PostgresHandles, pg::Schema)>,
    owner: Owner,
}

impl Rig {
    fn new(backend: Backend, tag: &str) -> Option<Self> {
        let catalog = HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 1,
        };
        let (sqlite, postgresql) = match backend {
            Backend::Sqlite => {
                let temp = Temp::new(&format!("placement-{tag}"));
                let handles = SqliteHandles::create(
                    PersistenceConfig::sqlite(temp.join("store.sqlite"))
                        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
                    StoragePolicy::frozen_default(),
                    &catalog,
                )
                .unwrap();
                (Some((handles, temp)), None)
            }
            Backend::Postgresql => {
                let schema = pg::Schema::new(pg::url()?, &format!("i2_{tag}"));
                let handles = PostgresHandles::create(
                    &schema.selection(),
                    StoragePolicy::frozen_default(),
                    &catalog,
                )
                .unwrap();
                (None, Some((handles, schema)))
            }
        };
        let mut rig = Self {
            sqlite,
            postgresql,
            owner: Owner {
                operation: 0,
                epoch: 0,
            },
        };
        rig.owner = rig
            .port()
            .begin(&Begin {
                source_device: 1,
                source_inode: 2,
                stack: [3; 16],
                scope: ObjectId::for_bytes(b"scope"),
            })
            .unwrap();
        Some(rig)
    }

    fn port(&self) -> &dyn Acquisition {
        match (&self.sqlite, &self.postgresql) {
            (Some((handles, _)), _) => &handles.acquisition,
            (_, Some((handles, _))) => &handles.acquisition,
            _ => unreachable!(),
        }
    }

    /// Every working row and the held charge, as text.
    fn dump(&self) -> Vec<String> {
        const ENTRIES: [&str; 2] = [
            "SELECT parent_position||'|'||hex(name)||'|'||coalesce(position,-9)||'|'||kind||'|'||coalesce(canonical_position,-9)||'|'||coalesce(hex(host_identity),'-')||'|'||coalesce(hex(host_path),'-')||'|'||coalesce(hex(content_root),'-') FROM init_entry ORDER BY parent_position,name",
            "SELECT concat_ws('|',parent_position,encode(name,'hex'),coalesce(position,-9),kind,coalesce(canonical_position,-9),coalesce(encode(host_identity,'hex'),'-'),coalesce(encode(host_path,'hex'),'-'),coalesce(encode(content_root,'hex'),'-')) FROM init_entry ORDER BY parent_position,name",
        ];
        const HOSTS: [&str; 2] = [
            "SELECT canonical_position||'|'||hex(device)||'|'||hex(inode)||'|'||hex(evidence)||'|'||hex(host_path)||'|'||aliases FROM init_host_file ORDER BY canonical_position",
            "SELECT concat_ws('|',canonical_position,encode(device,'hex'),encode(inode,'hex'),encode(evidence,'hex'),encode(host_path,'hex'),aliases) FROM init_host_file ORDER BY canonical_position",
        ];
        const HELD: [&str; 2] = [
            "SELECT 'held|'||held_rows||'|'||held_bytes FROM init_operation",
            "SELECT concat_ws('|','held',held_rows,held_bytes) FROM init_operation",
        ];
        let mut rows = Vec::new();
        if let Some((_, temp)) = &self.sqlite {
            let connection = rusqlite::Connection::open(temp.join("store.sqlite")).unwrap();
            for sql in [ENTRIES[0], HOSTS[0], HELD[0]] {
                let mut statement = connection.prepare(sql).unwrap();
                let found = statement
                    .query_map([], |row| row.get::<_, String>(0))
                    .unwrap();
                rows.extend(found.map(Result::unwrap));
            }
        }
        if let Some((_, schema)) = &self.postgresql {
            let mut admin = schema.admin();
            for sql in [ENTRIES[1], HOSTS[1], HELD[1]] {
                let found = admin.query(sql, &[]).unwrap();
                // PostgreSQL's hex is lower case, SQLite's upper case.
                rows.extend(found.iter().map(|row| row.get::<_, String>(0).to_uppercase()));
            }
        }
        rows
    }
}

fn meta(mode: u32) -> layerfs_content::filesystem::attributes::PortableMetadata {
    layerfs_content::filesystem::attributes::PortableMetadata {
        mode,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
    }
}

fn identity(inode: u64, evidence: u8) -> HostIdentity {
    HostIdentity {
        device: 1,
        inode,
        evidence: [evidence; EVIDENCE_BYTES],
    }
}

fn path(name: &str) -> Vec<u8> {
    format!("/source/{name}").into_bytes()
}

/// One child of the source root (position 0), unpositioned unless placed.
fn entry(name: &str, kind: InodeKind, position: Option<u64>, host: Option<HostIdentity>) -> NewEntry {
    NewEntry {
        key: EntryKey {
            parent: Some(0),
            name: name.as_bytes().to_vec(),
        },
        position,
        kind,
        metadata: meta(if kind == InodeKind::Symlink { 0o777 } else { 0o644 }),
        target_root: (kind == InodeKind::Symlink).then(|| ObjectId::for_bytes(b"target")),
        host_path: (kind != InodeKind::Symlink).then(|| path(name)),
        host,
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

/// An unplaced regular file, directory or symlink.
fn file(name: &str, inode: u64) -> NewEntry {
    entry(name, InodeKind::RegularFile, None, Some(identity(inode, 7)))
}
fn dir(name: &str) -> NewEntry {
    entry(name, InodeKind::Directory, None, None)
}
fn link(name: &str) -> NewEntry {
    entry(name, InodeKind::Symlink, None, None)
}

/// A placement: `host` is the identity the scan observed for a regular file.
fn at(name: &str, position: u64, host: Option<HostIdentity>) -> Placed {
    Placed {
        name: name.as_bytes().to_vec(),
        position,
        host: host.map(|host| (host, path(name))),
    }
}
fn file_at(name: &str, position: u64, inode: u64) -> Placed {
    at(name, position, Some(identity(inode, 7)))
}

fn setup(rig: &Rig, rows: &[NewEntry]) {
    let mut all = vec![root()];
    all.extend(rows.iter().cloned());
    for chunk in all.chunks(WRITE_WINDOW_ROWS) {
        rig.port().put_entries(rig.owner, chunk).unwrap();
    }
}

/// Places `placed` in one unit and child by child on equal Stores, and
/// compares. Returns the unit's answer.
fn case(
    backend: Backend,
    tag: &str,
    rows: &[NewEntry],
    placed: &[Placed],
) -> Option<AcquisitionResult<Vec<u64>>> {
    let set = Rig::new(backend, &format!("{tag}_s"))?;
    setup(&set, rows);
    let before = set.dump();
    let answer = set.port().place_children(set.owner, 0, placed);
    let after = set.dump();

    let reference = Rig::new(backend, &format!("{tag}_r"))?;
    setup(&reference, rows);
    assert_eq!(reference.dump(), before, "{backend:?} {tag}: equal setups");
    let mut expected = Ok(Vec::new());
    for child in placed {
        match reference
            .port()
            .place_children(reference.owner, 0, std::slice::from_ref(child))
        {
            Ok(canonical) => expected.as_mut().unwrap().extend(canonical),
            Err(error) => {
                expected = Err(error);
                break;
            }
        }
    }
    assert_eq!(answer, expected, "{backend:?} {tag}: first refusal");
    if answer.is_ok() {
        assert_eq!(after, reference.dump(), "{backend:?} {tag}: rows");
        assert_ne!(after, before, "{backend:?} {tag}: placed");
    } else {
        assert_eq!(after, before, "{backend:?} {tag}: a refused unit rolls back");
    }
    Some(answer)
}

fn changed(position: u64) -> AcquisitionResult<Vec<u64>> {
    Err(AcquisitionError::Changed { position })
}

fn constraint() -> AcquisitionResult<Vec<u64>> {
    Err(AcquisitionError::Persistence(
        layerfs_storage::port::PersistenceError::Malformed,
    ))
}

fn shapes(backend: Backend) {
    // A stored identity (inode 900, evidence 7) at position 30, and a stored
    // directory at position 20, both placed by the setup unit.
    let stored = || {
        vec![
            entry("stored-file", InodeKind::RegularFile, Some(30), Some(identity(900, 7))),
            entry("stored-dir", InodeKind::Directory, Some(20), None),
        ]
    };
    // A child whose identity is the stored one with other evidence: its bind
    // answers no row, so the loop refuses it `Changed`.
    let mismatch = || at("m", 5, Some(identity(900, 8)));
    let with = |mut rows: Vec<NewEntry>, more: &[NewEntry]| {
        rows.extend(more.iter().cloned());
        rows
    };

    // Repeated name: the second placement finds the row positioned.
    let rows = [file("a", 1), link("b")];
    let answer = case(
        backend,
        "name",
        &rows,
        &[file_at("a", 1, 1), at("b", 2, None), at("a", 3, None)],
    );
    let Some(answer) = answer else { return };
    assert_eq!(answer, changed(3));
    let rows = with(stored(), &[file("m", 99), dir("d1"), link("b")]);
    assert_eq!(
        case(
            backend,
            "name_after_changed",
            &rows,
            &[mismatch(), at("b", 6, None), at("b", 7, None)],
        ),
        Some(changed(5))
    );

    // Repeated position: two directories collide on the directory index;
    // two symlinks may share it; an earlier Changed wins.
    let rows = with(stored(), &[file("m", 99), dir("d1"), dir("d2"), link("s1"), link("s2")]);
    assert_eq!(
        case(backend, "position_dirs", &rows, &[at("d1", 6, None), at("d2", 6, None)]),
        Some(constraint())
    );
    assert_eq!(
        case(
            backend,
            "position_after_changed",
            &rows,
            &[mismatch(), at("d1", 6, None), at("d2", 6, None)],
        ),
        Some(changed(5))
    );
    assert!(matches!(
        case(
            backend,
            "position_links",
            &rows,
            &[at("s1", 7, None), at("s2", 7, None), at("d1", 8, None)],
        ),
        Some(Ok(_))
    ));

    // A position that does not convert to signed ends the window.
    let rows = [link("s1"), link("s2"), link("s3"), file("m", 99)];
    assert_eq!(
        case(
            backend,
            "unsigned",
            &rows,
            &[at("s1", 9, None), at("s2", 10, None), at("s3", u64::MAX, None)],
        ),
        Some(Err(AcquisitionError::Bounds))
    );
    let rows = with(stored(), &[link("s1"), link("s3"), file("m", 99)]);
    assert_eq!(
        case(
            backend,
            "unsigned_after_changed",
            &rows,
            &[at("s1", 9, None), mismatch(), at("s3", u64::MAX, None)],
        ),
        Some(changed(5))
    );

    // Dependencies on rows stored by an earlier unit: the stored directory's
    // position and the stored identity's canonical position.
    let rows = with(stored(), &[file("m", 99), link("s"), dir("d"), file("f", 901)]);
    assert_eq!(
        case(backend, "stored_dir", &rows, &[at("s", 4, None), at("d", 20, None)]),
        Some(constraint())
    );
    assert_eq!(
        case(
            backend,
            "stored_dir_after_changed",
            &rows,
            &[at("s", 4, None), mismatch(), at("d", 20, None)],
        ),
        Some(changed(5))
    );
    assert_eq!(
        case(backend, "stored_host", &rows, &[at("s", 4, None), file_at("f", 30, 901)]),
        Some(constraint())
    );
    assert_eq!(
        case(
            backend,
            "stored_host_after_changed",
            &rows,
            &[mismatch(), at("s", 4, None), file_at("f", 30, 901)],
        ),
        Some(changed(5))
    );
    // An identity stored by an earlier unit answers its canonical position.
    assert!(matches!(
        case(
            backend,
            "stored_alias",
            &rows,
            &[at("s", 4, None), file_at("f", 6, 900), at("d", 7, None)],
        ),
        Some(Ok(canonical)) if canonical == vec![30]
    ));

    // A dependency on a row an earlier window of the same unit stored: the
    // repeated identity of `b` cuts the window, and `c` then reuses `a`'s
    // canonical position with another identity.
    let rows = [file("a", 1), file("b", 1), file("c", 2), link("e")];
    assert_eq!(
        case(
            backend,
            "earlier_window",
            &rows,
            &[file_at("a", 12, 1), file_at("b", 13, 1), file_at("c", 12, 2)],
        ),
        Some(constraint())
    );
    assert_eq!(
        case(
            backend,
            "earlier_window_after_changed",
            &rows,
            &[
                file_at("a", 12, 1),
                file_at("b", 13, 1),
                at("missing", 14, None),
                file_at("c", 12, 2),
            ],
        ),
        Some(changed(14))
    );

    // Kind mismatches and a missing name.
    let rows = [file("f", 1), link("s"), dir("d")];
    assert_eq!(
        case(
            backend,
            "kind",
            &rows,
            &[at("d", 1, None), file_at("s", 2, 2), file_at("f", 3, 1)],
        ),
        Some(changed(2))
    );
    assert_eq!(
        case(backend, "kind_unbound", &rows, &[at("d", 1, None), at("f", 2, None)]),
        Some(changed(2))
    );

    // An all-hardlink directory: every child after the first is a window
    // boundary.
    let rows: Vec<_> = (0..40).map(|n| file(&format!("h{n:02}"), 7)).collect();
    let placed: Vec<_> = (0..40)
        .map(|n| file_at(&format!("h{n:02}"), 1 + n, 7))
        .collect();
    assert_eq!(
        case(backend, "all_hardlinks", &rows, &placed),
        Some(Ok(vec![1; 40]))
    );
    // The same, with one changed path in the middle.
    let mut placed = placed;
    placed[25].host.as_mut().unwrap().0.evidence[0] ^= 1;
    assert_eq!(
        case(backend, "all_hardlinks_changed", &rows, &placed),
        Some(changed(26))
    );

    // A large mixed directory: files, hard links near and far, directories
    // and symlinks, over several set windows.
    let mut rows = Vec::new();
    let mut placed = Vec::new();
    for n in 0..1000u64 {
        let name = format!("x{n:04}");
        let position = 1 + n;
        if n % 11 == 3 {
            rows.push(dir(&name));
            placed.push(at(&name, position, None));
        } else if n % 13 == 5 {
            rows.push(link(&name));
            placed.push(at(&name, position, None));
        } else {
            let inode = if n % 7 == 0 { n / 70 } else { 1000 + n };
            rows.push(file(&name, inode));
            placed.push(file_at(&name, position, inode));
        }
    }
    assert!(matches!(
        case(backend, "large", &rows, &placed),
        Some(Ok(_))
    ));
}

#[test]
fn sqlite_placement_equals_the_per_row_loop() {
    shapes(Backend::Sqlite);
}

#[test]
fn postgresql_placement_equals_the_per_row_loop() {
    shapes(Backend::Postgresql);
}
