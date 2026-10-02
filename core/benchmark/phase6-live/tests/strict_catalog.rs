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
