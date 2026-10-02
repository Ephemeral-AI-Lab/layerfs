//! Count-driven SQLite mechanism diagnostics; no latency/speed/admission claims.
use rusqlite::Connection;
use std::{
    io::{Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
static DIAGNOSTIC_LOCK: Mutex<()> = Mutex::new(());
static NEXT: AtomicU64 = AtomicU64::new(0);
struct File(PathBuf);
impl File {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "strict-window-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for File {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}
fn cache_writes(db: &Connection, reset: bool) -> i32 {
    let (mut current, mut peak) = (0, 0);
    // External diagnostic only: this owns the live connection for the entire FFI call.
    let rc = unsafe {
        rusqlite::ffi::sqlite3_db_status(
            db.handle(),
            rusqlite::ffi::SQLITE_DBSTATUS_CACHE_WRITE,
            &mut current,
            &mut peak,
            i32::from(reset),
        )
    };
    assert_eq!(rc, 0);
    current
}
fn largest_sqlite_allocation(reset: bool) -> i64 {
    let (mut current, mut high) = (0, 0);
    let rc = unsafe {
        rusqlite::ffi::sqlite3_status64(
            rusqlite::ffi::SQLITE_STATUS_MALLOC_SIZE,
            &mut current,
            &mut high,
            i32::from(reset),
        )
    };
    assert_eq!(rc, 0);
    high
}
fn progress_diagnostic(separate: bool) -> (i32, i64) {
    let file = File::new();
    let mut db = Connection::open(&file.0).unwrap();
    db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;CREATE TABLE bodies(body_order INTEGER PRIMARY KEY,domain INTEGER,save_id INTEGER,digest BLOB,metadata BLOB,ready INTEGER,received INTEGER);CREATE TABLE progress(body_order INTEGER PRIMARY KEY,received INTEGER,total INTEGER);").unwrap();
    let total = 1024 * 1024;
    db.execute(
        "INSERT INTO bodies VALUES(1,1,1,zeroblob(32),zeroblob(?1),0,0)",
        [total],
    )
    .unwrap();
    db.execute("INSERT INTO progress VALUES(1,0,?1)", [total])
        .unwrap();
    cache_writes(&db, true);
    largest_sqlite_allocation(true);
    for offset in (0..total).step_by(8192) {
        let tx = db.transaction().unwrap();
        {
            let mut blob = tx
                .blob_open("main", "bodies", "metadata", 1, false)
                .unwrap();
            blob.seek(SeekFrom::Start(offset as u64)).unwrap();
            blob.write_all(&[0x5a; 8192]).unwrap();
            blob.close().unwrap();
        }
        let sql = if separate {
            "UPDATE progress SET received=?1 WHERE body_order=1"
        } else {
            "UPDATE bodies SET received=?1 WHERE body_order=1"
        };
        tx.execute(sql, [offset + 8192]).unwrap();
        tx.commit().unwrap();
    }
    let writes = cache_writes(&db, false);
    let largest = largest_sqlite_allocation(false);
    let bytes: Vec<u8> = db
        .query_row("SELECT metadata FROM bodies", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bytes, vec![0x5a; total as usize]);
    (writes, largest)
}
#[test]
fn separate_transfer_cursor_avoids_full_blob_record_materialization() {
    let _guard = DIAGNOSTIC_LOCK.lock().unwrap();
    let (same, same_alloc) = progress_diagnostic(false);
    let (separate, separate_alloc) = progress_diagnostic(true);
    eprintln!("DIAGNOSTIC body_bytes=1048576 page_bytes=8192 append_calls=128 old_cursor_cache_writes={same} scalar_cursor_cache_writes={separate} old_largest_sqlite_malloc={same_alloc} scalar_largest_sqlite_malloc={separate_alloc}");
    assert!(same > separate, "same-row progress dirties more pages");
    assert!(
        same_alloc >= 1048576,
        "old UPDATE materializes the BLOB-sized record"
    );
    assert!(
        separate_alloc < 1048576,
        "scalar progress does not allocate a whole-BLOB record"
    );
}

use layerfs_content::{file::mapping::encode_chunk_object, FinalizedObject, ObjectRole};
use layerfs_storage::{access::ObjectLocation, encoding::CompressionWorkspace};
use phase6_live_probe::{
    packing,
    strict_catalog::{CandidateRow, LogicalUse, PlacementDomain, Registration, StrictCatalog},
};
use sha2::{Digest, Sha256};
fn metadata_pack() -> packing::Pack {
    let mut seed = 1234567u64;
    let raw: Vec<u8> = (0..32768)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect();
    let object =
        FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(&raw).unwrap()).unwrap();
    packing::build(vec![object], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap()
}
fn registration(pack: &packing::Pack, order: i64) -> Registration {
    let r = &pack.rows[0];
    Registration {
        location: ObjectLocation {
            object_id: r.id,
            role: r.role,
            canonical_length: r.length,
            pack_id: order,
            group_number: r.group as usize,
            record_number: r.record as usize,
        },
        domain: PlacementDomain::Metadata,
        logical_use: LogicalUse::MetadataGraph,
        references: vec![],
        base: None,
    }
}
#[test]
fn production_transfer_cursor_is_scalar_and_released_after_complete_body() {
    let _guard = DIAGNOSTIC_LOCK.lock().unwrap();
    let file = File::new();
    let catalog = StrictCatalog::create(&file.0).unwrap();
    let save = catalog.begin_save().unwrap();
    let pack = metadata_pack();
    let digest = Sha256::digest(&pack.bytes).into();
    let order = catalog
        .begin_metadata_body(save, digest, pack.bytes.len())
        .unwrap();
    let observer = Connection::open(&file.0).unwrap();
    let old_column: bool = observer
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('bodies') WHERE name='received')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!old_column);
    assert!(catalog
        .append_metadata_body(save, order, 1, &pack.bytes[..8192])
        .is_err());
    for (page, bytes) in pack.bytes.chunks(8192).enumerate() {
        catalog
            .append_metadata_body(save, order, page * 8192, bytes)
            .unwrap();
        let (received,total,ready):(i64,i64,bool)=observer.query_row("SELECT t.received,t.total,b.ready FROM body_transfers t JOIN bodies b USING(body_order) WHERE body_order=?1",[order],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(
            usize::try_from(received).unwrap(),
            page * 8192 + bytes.len()
        );
        assert_eq!(usize::try_from(total).unwrap(), pack.bytes.len());
        assert!(!ready);
    }
    catalog.finish_metadata_body(save, order).unwrap();
    let progress: i64 = observer
        .query_row("SELECT COUNT(*) FROM body_transfers", [], |r| r.get(0))
        .unwrap();
    assert_eq!(progress, 0);
    let scope = catalog.capture(Some(save)).unwrap();
    let (_, total, page) = catalog
        .body_page(scope, PlacementDomain::Metadata, order, 0, 8192)
        .unwrap();
    assert_eq!(total, Some(pack.bytes.len()));
    assert_eq!(page, pack.bytes[..8192]);
    let reserved = catalog
        .reserve_body(PlacementDomain::Metadata, save, [5; 32])
        .unwrap();
    catalog
        .begin_reserved_metadata_body(save, reserved, [5; 32], 32)
        .unwrap();
    catalog.discard_reserved_body(save, reserved).unwrap();
    assert_eq!(
        observer
            .query_row("SELECT COUNT(*) FROM body_transfers", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn candidate_snapshot_builds_once_and_pages_use_stamp_index() {
    let _guard = DIAGNOSTIC_LOCK.lock().unwrap();
    let file = File::new();
    let catalog = StrictCatalog::create(&file.0).unwrap();
    let save = catalog.begin_save().unwrap();
    let pack = metadata_pack();
    let order = catalog
        .register_body(
            PlacementDomain::Metadata,
            save,
            Sha256::digest(&pack.bytes).into(),
            Some(&pack.bytes),
        )
        .unwrap();
    let scope = catalog.capture(Some(save)).unwrap();
    catalog
        .register(scope, save, &[registration(&pack, order)])
        .unwrap();
    for start in (0..8192).step_by(128) {
        let page: Vec<_> = (start..start + 128)
            .map(|slot| CandidateRow {
                slot: slot as u16,
                stamp: slot as u64 + 1,
                id: pack.rows[0].id,
                signature: [slot as u8; 32],
            })
            .collect();
        catalog
            .stage_candidates(scope, save, PlacementDomain::Metadata, &page)
            .unwrap();
    }
    let before = catalog.candidate_snapshot_work().unwrap();
    let first = catalog
        .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
        .unwrap();
    assert_eq!(first.len(), 128);
    assert!(catalog
        .candidate_page(scope, PlacementDomain::FilePayload, 128, 128)
        .is_err());
    assert!(catalog
        .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
        .is_err());
    assert!(catalog
        .stage_candidates(scope, save, PlacementDomain::Metadata, &first)
        .is_err());
    let mut after = 128;
    let mut count = 128;
    loop {
        let page = catalog
            .candidate_page(scope, PlacementDomain::Metadata, after, 128)
            .unwrap();
        if page.is_empty() {
            break;
        }
        for row in &page {
            assert_eq!(row.stamp, row.slot as u64 + 1);
            assert_eq!(row.id, pack.rows[0].id);
            assert_eq!(row.signature, [row.slot as u8; 32]);
        }
        count += page.len();
        after = page.last().unwrap().stamp;
    }
    assert_eq!(count, 8192);
    let work = catalog.candidate_snapshot_work().unwrap();
    assert_eq!(work.builds - before.builds, 1);
    assert_eq!(work.slot_lookups - before.slot_lookups, 8192);
    assert_eq!(work.pages - before.pages, 65);
    assert_eq!(work.page_fullscan_steps - before.page_fullscan_steps, 0);
    assert_eq!(work.page_sorts - before.page_sorts, 0);
    eprintln!("DIAGNOSTIC candidate_slots=8192 builds={} slot_lookups={} pages={} vm_steps={} page_vm_steps={} page_fullscan_steps={} page_sorts={} temp_pages={} temp_page_bytes={} raw_rows_bytes=655360",work.builds-before.builds,work.slot_lookups-before.slot_lookups,work.pages-before.pages,work.vm_steps-before.vm_steps,work.page_vm_steps-before.page_vm_steps,work.page_fullscan_steps-before.page_fullscan_steps,work.page_sorts-before.page_sorts,work.temp_pages,work.temp_page_bytes);
    let reader = StrictCatalog::open_read_only(&file.0).unwrap();
    assert!(reader
        .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
        .unwrap_err()
        .contains("readonly"));
    // A captured own-save snapshot must refuse after ownership eligibility changes.
    catalog
        .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
        .unwrap();
    catalog.quarantine(save).unwrap();
    assert!(catalog
        .candidate_page(scope, PlacementDomain::Metadata, 128, 128)
        .unwrap_err()
        .contains("eligibility changed"));
    assert!(catalog
        .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
        .unwrap()
        .is_empty());
}
