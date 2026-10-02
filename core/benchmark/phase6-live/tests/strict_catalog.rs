use layerfs_content::{encode_whole_file_payload, FinalizedObject, ObjectRole};
use layerfs_storage::{encoding::CompressionWorkspace, sqlite::ObjectLocation};
use phase6_live_probe::{
    packing,
    strict_catalog::{CatalogScope, LogicalUse, PlacementDomain, Registration, StrictCatalog},
};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Database {
    path: PathBuf,
    catalog: StrictCatalog,
}
impl Database {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "layerfs-strict-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let catalog = StrictCatalog::create(&path).unwrap();
        Self { path, catalog }
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).unwrap();
    }
}
fn pack() -> phase6_live_probe::packing::Pack {
    let object = FinalizedObject::new(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"catalog exact dual use").unwrap(),
    )
    .unwrap();
    packing::build(vec![object], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap()
}
fn row(
    pack: &phase6_live_probe::packing::Pack,
    order: i64,
    domain: PlacementDomain,
    logical_use: LogicalUse,
) -> Registration {
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
        domain,
        logical_use,
        references: Vec::new(),
        base: None,
    }
}
fn insert(
    db: &StrictCatalog,
    save: i64,
    p: &phase6_live_probe::packing::Pack,
    domain: PlacementDomain,
    logical_use: LogicalUse,
) -> (CatalogScope, i64) {
    let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
    let order = db
        .register_body(
            domain,
            save,
            digest,
            if domain == PlacementDomain::Metadata {
                Some(&p.bytes)
            } else {
                None
            },
        )
        .unwrap();
    let scope = db.capture(Some(save)).unwrap();
    db.register(scope, save, &[row(p, order, domain, logical_use)])
        .unwrap();
    (scope, order)
}
#[test]
fn equal_identity_has_two_domain_locations_and_no_payload_sql_bytes() {
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let (scope, metadata) = insert(
        &db.catalog,
        save,
        &p,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    assert!(db
        .catalog
        .has_use(scope, p.rows[0].id, LogicalUse::MetadataGraph)
        .unwrap());
    assert!(!db
        .catalog
        .has_use(scope, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
    assert!(db
        .catalog
        .location(scope, PlacementDomain::FilePayload, p.rows[0].id)
        .unwrap()
        .is_none());
    let (scope, payload) = insert(
        &db.catalog,
        save,
        &p,
        PlacementDomain::FilePayload,
        LogicalUse::RegularFileGraph,
    );
    assert!(payload > metadata);
    assert_eq!(
        db.catalog
            .body(scope, PlacementDomain::Metadata, metadata)
            .unwrap()
            .1
            .unwrap(),
        p.bytes
    );
    assert!(db
        .catalog
        .body(scope, PlacementDomain::FilePayload, payload)
        .unwrap()
        .1
        .is_none());
    assert!(db
        .catalog
        .body(scope, PlacementDomain::Metadata, payload)
        .is_err());
    db.catalog.finish_storage(save).unwrap();
    db.catalog.publish(save).unwrap();
    let scope = db.catalog.capture(None).unwrap();
    assert!(db
        .catalog
        .has_use(scope, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
    let reader = StrictCatalog::open_read_only(&db.path).unwrap();
    assert_eq!(
        reader.descriptor(p.rows[0].id).unwrap(),
        Some((ObjectRole::WholeFile, p.rows[0].length))
    );
    assert!(reader.begin_save().is_err());
    let sql = rusqlite::Connection::open(&db.path).unwrap();
    let payload_bytes: i64 = sql
        .query_row(
            "SELECT COUNT(*) FROM bodies WHERE domain=0 AND metadata IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(payload_bytes, 0);
}
#[test]
fn captured_scope_excludes_foreign_private_and_later_publication() {
    let db = Database::new();
    let p = pack();
    let before = db.catalog.capture(None).unwrap();
    let save = db.catalog.begin_save().unwrap();
    let (own, _) = insert(
        &db.catalog,
        save,
        &p,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    assert!(db
        .catalog
        .location(own, PlacementDomain::Metadata, p.rows[0].id)
        .unwrap()
        .is_some());
    assert!(db
        .catalog
        .location(before, PlacementDomain::Metadata, p.rows[0].id)
        .unwrap()
        .is_none());
    let foreign = db.catalog.begin_save().unwrap();
    let other = db.catalog.capture(Some(foreign)).unwrap();
    assert!(db
        .catalog
        .location(other, PlacementDomain::Metadata, p.rows[0].id)
        .unwrap()
        .is_none());
    db.catalog.finish_storage(save).unwrap();
    db.catalog.publish(save).unwrap();
    assert!(db
        .catalog
        .location(other, PlacementDomain::Metadata, p.rows[0].id)
        .unwrap()
        .is_none());
    assert!(db
        .catalog
        .location(
            db.catalog.capture(None).unwrap(),
            PlacementDomain::Metadata,
            p.rows[0].id
        )
        .unwrap()
        .is_some());
    db.catalog.abandon(foreign).unwrap();
}
#[test]
fn pending_payload_blocks_publication_and_quarantine_hides_private_scope() {
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
    let order = db
        .catalog
        .reserve_body(PlacementDomain::FilePayload, save, digest)
        .unwrap();
    let scope = db.catalog.capture(Some(save)).unwrap();
    db.catalog
        .register(
            scope,
            save,
            &[row(
                &p,
                order,
                PlacementDomain::FilePayload,
                LogicalUse::RegularFileGraph,
            )],
        )
        .unwrap();
    assert!(db.catalog.finish_storage(save).is_err());
    assert!(db.catalog.publish(save).is_err());
    db.catalog.acknowledge_body(save, order, None).unwrap();
    assert!(db.catalog.acknowledge_body(save, order, None).is_err());
    db.catalog.finish_storage(save).unwrap();
    db.catalog.quarantine(save).unwrap();
    assert!(db
        .catalog
        .location(scope, PlacementDomain::FilePayload, p.rows[0].id)
        .unwrap()
        .is_none());
    assert!(db.catalog.publish(save).is_err());
}
#[test]
fn domain_bytes_and_old_schema_are_refused() {
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
    assert!(db
        .catalog
        .register_body(PlacementDomain::FilePayload, save, digest, Some(&p.bytes))
        .is_err());
    assert!(db
        .catalog
        .register_body(PlacementDomain::Metadata, save, digest, None)
        .is_err());
    assert!(StrictCatalog::create(&db.path).is_err());
    let sql = rusqlite::Connection::open(&db.path).unwrap();
    sql.pragma_update(None, "application_id", u32::from_be_bytes(*b"P6L2"))
        .unwrap();
    assert!(StrictCatalog::open_read_only(&db.path).is_err());
}
#[test]
fn ordinal_reservations_are_global_and_not_recycled_on_abandon() {
    let db = Database::new();
    let first = db.catalog.begin_save().unwrap();
    let second = db.catalog.begin_save().unwrap();
    assert_eq!(db.catalog.reserve_ordinals(first, 165).unwrap(), 1);
    assert_eq!(db.catalog.reserve_ordinals(second, 2).unwrap(), 166);
    db.catalog.abandon(first).unwrap();
    assert_eq!(db.catalog.reserve_ordinals(second, 1).unwrap(), 168);
    assert!(db.catalog.reserve_ordinals(second, 0).is_err());
}
#[test]
fn ordered_metadata_blob_pages_finish_once_without_payload_shadow() {
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
    let order = db
        .catalog
        .begin_metadata_body(save, digest, p.bytes.len())
        .unwrap();
    assert!(db.catalog.finish_metadata_body(save, order).is_err());
    assert!(db
        .catalog
        .append_metadata_body(save, order, 1, &p.bytes[..1])
        .is_err());
    for (n, page) in p.bytes.chunks(8192).enumerate() {
        db.catalog
            .append_metadata_body(save, order, n * 8192, page)
            .unwrap();
    }
    db.catalog.finish_metadata_body(save, order).unwrap();
    assert!(db.catalog.finish_metadata_body(save, order).is_err());
    let scope = db.catalog.capture(Some(save)).unwrap();
    let mut actual = Vec::new();
    for offset in (0..p.bytes.len()).step_by(8192) {
        let (got, total, page) = db
            .catalog
            .body_page(scope, PlacementDomain::Metadata, order, offset, 8192)
            .unwrap();
        assert_eq!(got, digest);
        assert_eq!(total, Some(p.bytes.len()));
        actual.extend(page);
    }
    assert_eq!(actual, p.bytes);
    let abandoned = db
        .catalog
        .reserve_body(PlacementDomain::FilePayload, save, [0; 32])
        .unwrap();
    db.catalog.discard_reserved_body(save, abandoned).unwrap();
    let replacement = db
        .catalog
        .reserve_body(PlacementDomain::FilePayload, save, [0; 32])
        .unwrap();
    assert!(replacement > abandoned);
    db.catalog
        .complete_reserved_body(save, replacement, digest, None)
        .unwrap();
    db.catalog.finish_storage(save).unwrap();
}
#[test]
fn unused_ordinal_tail_only_reclaims_its_owned_latest_reservation() {
    let db = Database::new();
    let save = db.catalog.begin_save().unwrap();
    let first = db.catalog.reserve_ordinals(save, 2000).unwrap();
    assert_eq!(first, 1);
    db.catalog.release_ordinals(save, 1001, 2001).unwrap();
    assert_eq!(db.catalog.reserve_ordinals(save, 1).unwrap(), 1001);
    let other = db.catalog.begin_save().unwrap();
    let first = db.catalog.reserve_ordinals(save, 1000).unwrap();
    let end = first as u64 + 1000;
    let next = db.catalog.reserve_ordinals(other, 1).unwrap();
    db.catalog
        .release_ordinals(save, first as u64 + 10, end)
        .unwrap();
    assert_eq!(db.catalog.reserve_ordinals(other, 1).unwrap(), next + 1);
}
#[test]
fn new_use_of_committed_metadata_stays_private_until_publication() {
    use layerfs_content::filesystem::{profile_id, scope_for_seed, FilesystemRoot};
    let db = Database::new();
    let save = db.catalog.begin_save().unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FilesystemRoot,
        FilesystemRoot::new(
            profile_id(),
            scope_for_seed([7; 32]),
            1,
            layerfs_content::ObjectId::for_bytes(b"inode root"),
        )
        .unwrap()
        .encode()
        .unwrap(),
    )
    .unwrap();
    let p = packing::build(vec![object], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let (_, order) = insert(
        &db.catalog,
        save,
        &p,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    db.catalog.finish_storage(save).unwrap();
    db.catalog.publish(save).unwrap();
    let before = db.catalog.capture(None).unwrap();
    let next = db.catalog.begin_save().unwrap();
    let private = db.catalog.capture(Some(next)).unwrap();
    db.catalog
        .register_use(
            private,
            next,
            PlacementDomain::Metadata,
            p.rows[0].id,
            LogicalUse::RegularFileGraph,
            &[],
        )
        .unwrap();
    assert!(!db
        .catalog
        .has_use(before, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
    assert!(!db
        .catalog
        .has_use(
            db.catalog.capture(None).unwrap(),
            p.rows[0].id,
            LogicalUse::RegularFileGraph
        )
        .unwrap());
    assert!(db
        .catalog
        .has_use(private, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
    db.catalog.finish_storage(next).unwrap();
    db.catalog.publish(next).unwrap();
    let current = db.catalog.capture(None).unwrap();
    assert!(db
        .catalog
        .has_use(current, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
    assert_eq!(
        db.catalog
            .location(current, PlacementDomain::Metadata, p.rows[0].id)
            .unwrap()
            .unwrap()
            .location
            .pack_id,
        order
    );
    assert!(!db
        .catalog
        .has_use(before, p.rows[0].id, LogicalUse::RegularFileGraph)
        .unwrap());
}
#[test]
fn canonical_cross_lane_reference_accepts_later_body_order_after_child_ready() {
    use layerfs_content::file::mapping::{
        encode_chunk_object, encode_node, ExtentNode, ExtentSlice,
    };
    use phase6_live_probe::strict_catalog::Reference;
    let db = Database::new();
    let save = db.catalog.begin_save().unwrap();
    let child = FinalizedObject::new(
        ObjectRole::Chunk,
        encode_chunk_object(b"metadata chunk").unwrap(),
    )
    .unwrap();
    let id = child.id();
    let parent = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        encode_node(
            &ExtentNode::Leaf {
                subtree_logical_bytes: 14,
                extents: vec![ExtentSlice::new(id, 0, 14).unwrap()],
            },
            true,
        )
        .unwrap(),
    )
    .unwrap();
    let child = packing::build(vec![child], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let parent = packing::build(vec![parent], &mut CompressionWorkspace::new().unwrap())
        .unwrap()
        .pop()
        .unwrap();
    let digest: [u8; 32] = Sha256::digest(&parent.bytes).into();
    let parent_order = db
        .catalog
        .reserve_body(PlacementDomain::Metadata, save, digest)
        .unwrap();
    let (_, child_order) = insert(
        &db.catalog,
        save,
        &child,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    assert!(child_order > parent_order);
    db.catalog
        .complete_reserved_body(save, parent_order, digest, Some(&parent.bytes))
        .unwrap();
    let scope = db.catalog.capture(Some(save)).unwrap();
    let mut registration = row(
        &parent,
        parent_order,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    registration.references.push(Reference {
        id,
        domain: PlacementDomain::Metadata,
        logical_use: LogicalUse::MetadataGraph,
    });
    db.catalog.register(scope, save, &[registration]).unwrap();
    db.catalog.finish_storage(save).unwrap();
}
#[test]
fn candidate_ring_pages_are_domain_and_publication_scoped() {
    use phase6_live_probe::strict_catalog::CandidateRow;
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let (scope, _) = insert(
        &db.catalog,
        save,
        &p,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    let candidate = CandidateRow {
        slot: 0,
        stamp: 1,
        id: p.rows[0].id,
        signature: [7; 32],
    };
    db.catalog
        .stage_candidates(scope, save, PlacementDomain::Metadata, &[candidate])
        .unwrap();
    assert!(db
        .catalog
        .candidate_page(scope, PlacementDomain::FilePayload, 0, 128)
        .unwrap()
        .is_empty());
    assert_eq!(
        db.catalog
            .candidate_page(scope, PlacementDomain::Metadata, 0, 128)
            .unwrap()
            .len(),
        1
    );
    db.catalog.finish_storage(save).unwrap();
    db.catalog.publish(save).unwrap();
    let public = db.catalog.capture(None).unwrap();
    let next = db.catalog.begin_save().unwrap();
    let private = db.catalog.capture(Some(next)).unwrap();
    let candidate = CandidateRow {
        stamp: 8193,
        ..candidate
    };
    db.catalog
        .stage_candidates(private, next, PlacementDomain::Metadata, &[candidate])
        .unwrap();
    assert_eq!(
        db.catalog
            .candidate_page(public, PlacementDomain::Metadata, 0, 128)
            .unwrap()[0]
            .stamp,
        1
    );
    assert_eq!(
        db.catalog
            .candidate_page(private, PlacementDomain::Metadata, 0, 128)
            .unwrap()[0]
            .stamp,
        8193
    );
    db.catalog.quarantine(next).unwrap();
    assert_eq!(
        db.catalog
            .candidate_page(private, PlacementDomain::Metadata, 0, 128)
            .unwrap()[0]
            .stamp,
        1
    );
    let ids = vec![p.rows[0].id; 128];
    assert_eq!(
        db.catalog
            .locations(public, PlacementDomain::Metadata, &ids)
            .unwrap()
            .len(),
        128
    );
    let ids = vec![p.rows[0].id; 129];
    assert!(db
        .catalog
        .locations(public, PlacementDomain::Metadata, &ids)
        .is_err());
}
#[test]
fn actual_locator_trigger_refusal_acknowledges_single_abort_and_keeps_reads_available() {
    let db = Database::new();
    let p = pack();
    let save = db.catalog.begin_save().unwrap();
    let digest: [u8; 32] = Sha256::digest(&p.bytes).into();
    let order = db
        .catalog
        .register_body(PlacementDomain::Metadata, save, digest, Some(&p.bytes))
        .unwrap();
    let external = rusqlite::Connection::open(&db.path).unwrap();
    external.execute_batch("CREATE TRIGGER deny_locator BEFORE INSERT ON locators BEGIN SELECT RAISE(ABORT,'external locator refusal'); END;").unwrap();
    let scope = db.catalog.capture(Some(save)).unwrap();
    let error = db
        .catalog
        .register(
            scope,
            save,
            &[row(
                &p,
                order,
                PlacementDomain::Metadata,
                LogicalUse::MetadataGraph,
            )],
        )
        .unwrap_err();
    assert!(phase6_live_probe::strict_catalog::is_definite_catalog_error(&error));
    let identities: i64 = external
        .query_row("SELECT COUNT(*) FROM identities", [], |r| r.get(0))
        .unwrap();
    let locators: i64 = external
        .query_row("SELECT COUNT(*) FROM locators", [], |r| r.get(0))
        .unwrap();
    assert_eq!((identities, locators), (0, 0));
    assert!(db.catalog.capture(Some(save)).is_ok());
    assert!(db
        .catalog
        .location(scope, PlacementDomain::Metadata, p.rows[0].id)
        .unwrap()
        .is_none());
    assert_eq!(
        db.catalog
            .body(scope, PlacementDomain::Metadata, order)
            .unwrap()
            .1
            .unwrap(),
        p.bytes
    );
}
#[test]
fn current_markers_do_not_authorize_an_incompatible_schema() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-strict-shape-{}-{}.db",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute_batch("CREATE TABLE old_marker(id INTEGER PRIMARY KEY)")
        .unwrap();
    raw.pragma_update(
        None,
        "application_id",
        phase6_live_probe::strict_catalog::APPLICATION_ID,
    )
    .unwrap();
    raw.pragma_update(
        None,
        "user_version",
        phase6_live_probe::strict_catalog::USER_VERSION,
    )
    .unwrap();
    drop(raw);
    let before = std::fs::read(&path).unwrap();
    assert!(StrictCatalog::open_read_only(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn current_profile_refuses_missing_transfer_table_changed_columns_and_indexes() {
    for alteration in [
        "DROP TABLE body_transfers",
        "ALTER TABLE bodies ADD COLUMN old_received INTEGER",
        "DROP INDEX locators_domain_id",
        "DROP TRIGGER abandon_owned_capture",
    ] {
        let db = Database::new();
        let raw = rusqlite::Connection::open(&db.path).unwrap();
        raw.execute_batch(alteration).unwrap();
        drop(raw);
        let before = std::fs::read(&db.path).unwrap();
        assert!(
            StrictCatalog::open_read_only(&db.path).is_err(),
            "accepted {alteration}"
        );
        assert_eq!(std::fs::read(&db.path).unwrap(), before);
    }
}
#[test]
fn readonly_profile_authentication_reads_no_product_population() {
    let db = Database::new();
    let save = db.catalog.begin_save().unwrap();
    db.catalog.finish_storage(save).unwrap();
    db.catalog.publish(save).unwrap();
    let reader = StrictCatalog::open_read_only(&db.path).unwrap();
    assert_eq!(reader.capture(None).unwrap().publication, 1);
    assert_eq!(
        phase6_live_probe::strict_catalog::SCHEMA_FINGERPRINT.len(),
        32
    );
}
#[test]
fn implicit_statement_uncertainty_quarantines_without_guessing_absence() {
    let db = Database::new();
    let external = rusqlite::Connection::open(&db.path).unwrap();
    external.execute_batch("CREATE TRIGGER fail_after_save AFTER INSERT ON saves BEGIN SELECT RAISE(FAIL,'external retained-effects refusal'); END").unwrap();
    let error = db.catalog.begin_save().unwrap_err();
    assert!(!phase6_live_probe::strict_catalog::is_definite_catalog_error(&error));
    // RAISE(FAIL) can retain the statement's row; the adapter does not guess absent.
    let count: i64 = external
        .query_row("SELECT COUNT(*) FROM saves", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    assert!(db
        .catalog
        .capture(None)
        .unwrap_err()
        .contains("engine quarantined"));
    assert!(db
        .catalog
        .begin_save()
        .unwrap_err()
        .contains("engine quarantined"));
}
#[cfg(unix)]
#[test]
#[ignore = "real OS write refusal; requires fresh SP1_SQL_FAULT_OUT; never a speed/durability gate"]
fn actual_os_write_limit_commit_quarantines_engine() {
    const NAME: &str = "actual_os_write_limit_commit_quarantines_engine";
    let out = PathBuf::from(
        std::env::var_os("SP1_SQL_FAULT_OUT").expect("fresh diagnostic output required"),
    );
    if std::env::var_os("SP1_SQL_FAULT_CHILD").is_none() {
        std::fs::create_dir(&out).unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                NAME,
                "--nocapture",
                "--test-threads=1",
            ])
            .env("SP1_SQL_FAULT_CHILD", "1")
            .output()
            .unwrap();
        std::fs::write(out.join("child.stdout"), &result.stdout).unwrap();
        std::fs::write(out.join("child.stderr"), &result.stderr).unwrap();
        assert!(
            result.status.success(),
            "OS diagnostic failed; preserve {}",
            out.display()
        );
        return;
    }
    let path = out.join("fault.sqlite");
    let catalog = StrictCatalog::create(&path).unwrap();
    let p = pack();
    let old = catalog.begin_save().unwrap();
    let (_, body) = insert(
        &catalog,
        old,
        &p,
        PlacementDomain::Metadata,
        LogicalUse::MetadataGraph,
    );
    catalog.finish_storage(old).unwrap();
    let publication = catalog.publish(old).unwrap();
    let source = catalog
        .body(
            catalog.capture(None).unwrap(),
            PlacementDomain::Metadata,
            body,
        )
        .unwrap()
        .1
        .unwrap();
    assert_eq!(source, p.bytes);
    let original = std::fs::read(&path).unwrap();
    std::fs::copy(&path, out.join("before-fault.sqlite")).unwrap();
    std::fs::write(out.join("previous.json"),format!("{{\"save\":{old},\"publication\":{publication},\"body_order\":{body},\"canonical_id\":\"{}\",\"db_bytes\":{},\"body_sha256\":\"{}\",\"db_sha256\":\"{}\"}}\n",phase6_live_probe::minio::hex(p.rows[0].id.as_bytes()),original.len(),phase6_live_probe::minio::hex(&Sha256::digest(&source)),phase6_live_probe::minio::hex(&Sha256::digest(&original)))).unwrap();
    let save = catalog.begin_save().unwrap();
    let limit = libc::rlimit {
        rlim_cur: std::fs::metadata(&path).unwrap().len() as libc::rlim_t,
        rlim_max: std::fs::metadata(&path).unwrap().len() as libc::rlim_t,
    };
    // Process isolation keeps the owning runner and every other owner unaffected.
    unsafe {
        assert_ne!(libc::signal(libc::SIGXFSZ, libc::SIG_IGN), libc::SIG_ERR);
        assert_eq!(libc::setrlimit(libc::RLIMIT_FSIZE, &limit), 0);
    }
    let result =
        catalog.begin_metadata_body(save, [9; 32], phase6_live_probe::strict_catalog::BODY_LIMIT);
    let error = result
        .expect_err("the growing metadata allocation must encounter the actual OS write limit");
    std::fs::write(out.join("failure.txt"), &error).unwrap();
    assert!(
        error.contains("SQL COMMIT:"),
        "earlier failure is not a COMMIT witness: {error}"
    );
    let read = catalog.capture(None).unwrap_err();
    let write = catalog.begin_save().unwrap_err();
    assert!(read.contains("engine quarantined"));
    assert!(write.contains("engine quarantined"));
    assert!(path.exists());
    let forensic = match StrictCatalog::open_read_only(&path) {
        Ok(db) => match db.capture(None) {
            Ok(scope) => format!("readable publication {}", scope.publication),
            Err(e) => format!("unavailable: {e}"),
        },
        Err(e) => format!("unavailable: {e}"),
    };
    std::fs::write(out.join("outcome.txt"),format!("DIAGNOSTIC: actual OS write refusal at explicit COMMIT\nread refusal: {read}\nwrite refusal: {write}\nforensic previous/current DB observation: {forensic}\nNo durability, recovery, cleanup or speed PASS. Failed-abort physical proof remains NOT_RUN.\n")).unwrap();
}
