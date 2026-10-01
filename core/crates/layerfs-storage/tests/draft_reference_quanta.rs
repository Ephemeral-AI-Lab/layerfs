//! Public native draft reference waves and genuine owner work counters.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;
use layerfs_content::file::edit::{DraftCapacity, DraftJob, DraftRecord, DraftState};
use layerfs_content::file::mapping::{ChildDescriptor, ExtentNode, ExtentSlice};
use layerfs_content::ObjectId;
use layerfs_storage::construction_state::{DraftAdapter, ScratchAuthority};
use layerfs_storage::StorageError;
use rusqlite::{Connection, OpenFlags};

fn id(value: u8) -> ObjectId {
    ObjectId::from_bytes(&[value; 32]).unwrap()
}
fn leaf() -> DraftRecord {
    DraftRecord::Node(ExtentNode::Leaf {
        subtree_logical_bytes: 64,
        extents: (0..64)
            .map(|ordinal| ExtentSlice::new(id(90), ordinal * 2, 1).unwrap())
            .collect(),
    })
}
fn parent(references: usize) -> DraftRecord {
    DraftRecord::Node(ExtentNode::Branch {
        level: 1,
        subtree_logical_bytes: references as u64 * 64,
        subtree_extent_count: references as u64 * 64,
        children: (1..=references)
            .map(|ordinal| ChildDescriptor {
                cumulative_logical_end: ordinal as u64 * 64,
                cumulative_extent_end: ordinal as u64 * 64,
                child_object_id: id(1),
            })
            .collect(),
    })
}
fn open(path: &std::path::Path, writable: bool) -> Connection {
    let flags = if writable {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let connection = Connection::open_with_flags(
        std::fs::canonicalize(path).unwrap(),
        flags | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    connection.busy_timeout(std::time::Duration::ZERO).unwrap();
    connection
}
fn reference_rows(connection: &Connection, key: ObjectId) -> Vec<(u16, Vec<u8>, i64)> {
    let mut statement = connection
        .prepare("SELECT ordinal,value,linked FROM draft_references WHERE id=?1 ORDER BY ordinal")
        .unwrap();
    let rows = statement
        .query_map([key.as_bytes().as_slice()], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
    rows.collect::<Result<Vec<_>, _>>().unwrap()
}

#[test]
fn native_maximum_and_partial_reference_waves_keep_rows_and_reduce_preparation() {
    for references in [128usize, 33, 49] {
        let directory = support::TempDir::new("draft-reference-quanta");
        let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
        let mut session = authority
            .begin_drafts([85; 32], DraftCapacity::default())
            .unwrap();
        let path = authority.status().unwrap()[0].path.clone();
        let mut state = DraftAdapter::new(&mut session).unwrap();
        state.hold(id(1), leaf()).unwrap();
        let before = state.stats();
        state.hold(id(2), parent(references)).unwrap();
        let after = state.stats();
        assert_eq!(
            after.reference_insert_rows - before.reference_insert_rows,
            references as u64
        );
        assert_eq!(
            after.reference_insert_prepare_calls - before.reference_insert_prepare_calls,
            references.div_ceil(32) as u64
        );
        let connection = open(&path, false);
        assert_eq!(
            reference_rows(&connection, id(2)),
            (0..references)
                .map(|ordinal| (ordinal as u16, id(1).as_bytes().to_vec(), 1))
                .collect::<Vec<_>>()
        );
        let links: Vec<u8> = connection
            .query_row(
                "SELECT links FROM draft_counts WHERE id=?1",
                [id(1).as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(links, (references as u64).to_be_bytes());
        drop(connection);
        assert_eq!(state.get(id(2)).unwrap(), Some(parent(references)));
        state.select_root(None, id(2)).unwrap();
        while let Some(job) = state.next_job().unwrap() {
            assert!(job.links > 0);
            state.retire_job(job).unwrap();
        }
        state.hold(id(3), leaf()).unwrap();
        state.select_root(Some(id(2)), id(3)).unwrap();
        let before = state.stats();
        loop {
            let job = state.next_job().unwrap().unwrap();
            state.retire_job(job).unwrap();
            if job.id == id(2) {
                assert_eq!(job.links, 0);
                break;
            }
            assert!(
                job.links > 0,
                "only revived preceding jobs before exact parent"
            );
        }
        let after = state.stats();
        assert_eq!(
            after.reference_delete_rows - before.reference_delete_rows,
            references as u64
        );
        assert_eq!(
            after.reference_delete_prepare_calls - before.reference_delete_prepare_calls,
            references.div_ceil(24) as u64
        );
        assert!(state.get(id(2)).unwrap().is_none());
        assert_eq!(state.stats().links_retired, references as u64);
        state.finish().unwrap();
        assert_eq!(state.stats().bytes, 0);
        drop(state);
        let connection = open(&path, false);
        let rows: i64 = connection.query_row("SELECT (SELECT COUNT(*) FROM draft_references)+(SELECT COUNT(*) FROM draft_counts)+(SELECT COUNT(*) FROM draft_jobs)", [], |row| row.get(0)).unwrap();
        assert_eq!(rows, 0);
        drop(connection);
        session.release().unwrap();
        assert_eq!(authority.reserved_bytes().unwrap(), 0);
    }
}

#[test]
fn foreign_supplied_job_count_cannot_consume_real_reference_rows() {
    let directory = support::TempDir::new("draft-reference-foreign-job");
    let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
    let mut session = authority
        .begin_drafts([86; 32], DraftCapacity::default())
        .unwrap();
    let path = authority.status().unwrap()[0].path.clone();
    let mut state = DraftAdapter::new(&mut session).unwrap();
    state.hold(id(1), leaf()).unwrap();
    let job = state.next_job().unwrap().unwrap();
    let before = state.stats();
    assert!(state
        .retire_job(DraftJob {
            links: job.links + 1,
            ..job
        })
        .is_err());
    assert_eq!(state.stats(), before);
    drop(state);
    let connection = open(&path, false);
    assert_eq!(reference_rows(&connection, id(1)).len(), 64);
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM draft_jobs WHERE sequence=?1 AND id=?2",
                rusqlite::params![job.sequence as i64, job.id.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    drop(connection);
    assert!(!session.is_quarantined());
    assert!(matches!(
        session.take_failure(),
        Some(StorageError::Integrity("draft FIRST job expected"))
    ));
    session.release().unwrap();
    assert_eq!(authority.reserved_bytes().unwrap(), 0);
}

#[test]
fn malformed_stored_reference_is_refused_before_any_delete_preparation() {
    let directory = support::TempDir::new("draft-reference-malformed");
    let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
    let mut session = authority
        .begin_drafts([87; 32], DraftCapacity::default())
        .unwrap();
    let path = authority.status().unwrap()[0].path.clone();
    let mut state = DraftAdapter::new(&mut session).unwrap();
    state.hold(id(1), leaf()).unwrap();
    let job = state.next_job().unwrap().unwrap();
    let before = state.stats();
    let connection = open(&path, true);
    connection
        .execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")
        .unwrap();
    assert_eq!(
        connection
            .execute(
                "UPDATE draft_references SET linked=1 WHERE id=?1 AND ordinal=32",
                [id(1).as_bytes().as_slice()]
            )
            .unwrap(),
        1
    );
    drop(connection);
    assert!(state.retire_job(job).is_err());
    assert_eq!(state.stats(), before);
    drop(state);
    let connection = open(&path, false);
    assert_eq!(reference_rows(&connection, id(1)).len(), 64);
    assert_eq!(
        connection
            .query_row(
                "SELECT stage FROM draft_headers WHERE id=?1",
                [id(1).as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    drop(connection);
    assert!(!session.is_quarantined());
    assert!(matches!(
        session.take_failure(),
        Some(StorageError::Integrity("draft body checksum/totals"))
    ));
    session.release().unwrap();
    assert_eq!(authority.reserved_bytes().unwrap(), 0);
}

#[test]
fn real_reference_constraint_rollback_keeps_issued_work_and_removes_partial_run() {
    let directory = support::TempDir::new("draft-reference-constraint");
    let authority = ScratchAuthority::new(directory.path(), 1).unwrap();
    let mut session = authority
        .begin_drafts([88; 32], DraftCapacity::default())
        .unwrap();
    let path = authority.status().unwrap()[0].path.clone();
    let mut state = DraftAdapter::new(&mut session).unwrap();
    state.hold(id(1), leaf()).unwrap();
    let before = state.stats();
    let connection = open(&path, true);
    connection
        .execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA synchronous=OFF;")
        .unwrap();
    // Restrict only future rows: the prerequisite leaf's repeated payloads remain valid.
    connection
        .execute_batch(
            "CREATE UNIQUE INDEX test_reference_run ON draft_references(id,value) WHERE linked=1",
        )
        .unwrap();
    drop(connection);
    assert!(state.hold(id(2), parent(33)).is_err());
    let after = state.stats();
    assert_eq!(
        after.reference_insert_prepare_calls - before.reference_insert_prepare_calls,
        1
    );
    assert_eq!(
        after.reference_insert_rows - before.reference_insert_rows,
        1
    );
    assert_eq!(
        after.reference_delete_prepare_calls,
        before.reference_delete_prepare_calls
    );
    assert_eq!(after.reference_delete_rows, before.reference_delete_rows);
    drop(state);
    let connection = open(&path, false);
    assert!(reference_rows(&connection, id(2)).is_empty());
    assert_eq!(reference_rows(&connection, id(1)).len(), 64);
    let creating: (i64, i64) = connection
        .query_row(
            "SELECT stage,cursor FROM draft_headers WHERE id=?1",
            [id(2).as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        creating,
        (0, 0),
        "CreatingStart committed, failed reference wave rolled back"
    );
    drop(connection);
    assert!(!session.is_quarantined());
    assert!(matches!(
        session.take_failure(),
        Some(StorageError::Engine(rusqlite::Error::SqliteFailure(error, _)))
            if error.extended_code == 2067
    ));
    session.release().unwrap();
    assert_eq!(authority.reserved_bytes().unwrap(), 0);
}
