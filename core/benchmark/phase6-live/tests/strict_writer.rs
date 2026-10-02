//! Real-provider writer parity against independently sealed C1/C2 objects.
mod strict_witness;
use layerfs_content::{
    AdvisoryPredecessors, AuthenticatedObjects, ConstructionPolicy, FinalizedObject, ObjectId,
    ObjectRole, PredecessorProvenance,
};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use phase6_live_probe::{
    strict_catalog::{LogicalUse, PlacementDomain, StrictCatalog},
    strict_read::{Access, ReadOwner},
    strict_writer::{ConstructionReader, Consumer, Writer},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use strict_witness::{disposable, evidence, id, provider};
fn capacities() -> StorageCapacities {
    StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap()
}
fn owner(label: &str) -> (Rc<RefCell<Writer>>, std::path::PathBuf, Arc<StrictCatalog>) {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable(label);
    let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
    let writer = Writer::new(catalog.clone(), provider, capacities()).unwrap();
    (Rc::new(RefCell::new(writer)), path, catalog)
}
fn sealed(name: &str, base: Option<ObjectId>) -> FinalizedObject {
    let row = std::fs::read_to_string(evidence().join("old-producer-v1/objects.tsv")).unwrap();
    let row = row
        .lines()
        .find(|row| row.starts_with(&format!("{name}\t")))
        .unwrap();
    let cells = row.split('\t').collect::<Vec<_>>();
    let object = FinalizedObject::new(
        ObjectRole::from_code(cells[2].parse().unwrap()).unwrap(),
        std::fs::read(evidence().join(format!("old-producer-v1/{name}.canonical"))).unwrap(),
    )
    .unwrap();
    assert_eq!(object.id(), id(cells[1]));
    let mut hints = AdvisoryPredecessors::new();
    if let Some(base) = base {
        hints
            .push(base, PredecessorProvenance::OriginalBase)
            .unwrap();
    }
    object.with_predecessors(hints)
}
fn stream(owner: &Rc<RefCell<Writer>>, raw: &[u8]) -> ObjectId {
    let policy = ConstructionPolicy::frozen_default();
    let mut consumer = Consumer {
        owner: owner.clone(),
        logical_use: LogicalUse::RegularFileGraph,
    };
    layerfs_telemetry::timer::Timing::disabled("sp1.fixture", |scope| {
        layerfs_content::construct_stream(
            policy,
            &policy.capacities(),
            std::io::Cursor::new(raw),
            &mut consumer,
            scope.child("construct"),
        )
    })
    .0
    .unwrap()
    .root
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_same_save_selection_grouping_and_private_eligibility() {
    let (owner, path, catalog) = owner("writer-same-save");
    let old_scope = catalog.capture(None).unwrap();
    let initial_puts = owner.borrow().provider.statistics().unwrap().put_calls;
    let expected =
        std::fs::read_to_string(evidence().join("strict-oracle-v2/same-save/writer-emission.tsv"))
            .unwrap();
    let mut ids = Vec::new();
    let mut canonical = Vec::new();
    for row in expected.lines().skip(1) {
        let cells = row.split('\t').collect::<Vec<_>>();
        let bytes = std::fs::read(
            evidence().join(format!("strict-oracle-v2/same-save/{}.canonical", cells[1])),
        )
        .unwrap();
        let object = FinalizedObject::new(
            ObjectRole::from_code(cells[2].parse().unwrap()).unwrap(),
            bytes.clone(),
        )
        .unwrap();
        ids.push(object.id());
        canonical.push(bytes);
        owner
            .borrow_mut()
            .offer(LogicalUse::RegularFileGraph, object)
            .unwrap();
    }
    assert_eq!(owner.borrow().counts.exact_reuses, 1);
    assert_eq!(
        (
            owner.borrow().delta.prepared_full,
            owner.borrow().delta.trials,
            owner.borrow().delta.prefix_selected,
            owner.borrow().delta.no_candidate
        ),
        (5, 1, 1, 4)
    );
    let reader = ConstructionReader {
        owner: owner.clone(),
        logical_use: LogicalUse::RegularFileGraph,
    };
    assert_eq!(reader.read_canonical_batch(&ids).unwrap(), canonical);
    assert!(owner.borrow().counts.private_group_seals > 0);
    assert_eq!(
        owner.borrow().provider.statistics().unwrap().put_calls - initial_puts,
        0,
        "same-save placed groups precede external ACK"
    );
    let provider = owner.borrow().provider.clone();
    let access = Access {
        catalog: catalog.as_ref(),
        provider: &provider,
        scope: old_scope,
        logical_use: LogicalUse::RegularFileGraph,
    };
    assert!(ReadOwner::new()
        .unwrap()
        .read(&access, &capacities(), &[ids[0]])
        .unwrap_err()
        .contains("required domain placement missing"));
    owner.borrow_mut().finish_storage().unwrap();
    let save = owner.borrow().save;
    assert!(catalog
        .location(
            catalog.capture(None).unwrap(),
            PlacementDomain::FilePayload,
            ids[0]
        )
        .unwrap()
        .is_none());
    catalog.publish(save).unwrap();
    let catalog = StrictCatalog::open_read_only(&path).unwrap();
    let scope = catalog.capture(None).unwrap();
    let access = Access {
        catalog: &catalog,
        provider: &provider,
        scope,
        logical_use: LogicalUse::RegularFileGraph,
    };
    assert_eq!(
        ReadOwner::new()
            .unwrap()
            .read(&access, &capacities(), &ids)
            .unwrap(),
        canonical
    );
    let db =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let grouped:i64=db.query_row("SELECT count(*) FROM (SELECT body_order,group_number,count(*) AS n FROM locators GROUP BY body_order,group_number HAVING n>1)",[],|r|r.get(0)).unwrap();
    assert!(grouped > 0, "native group must contain multiple records");
    assert!(
        owner.borrow().counts.payload_packs < 5,
        "no immutable PUT per selected payload"
    );
    println!(
        "sp1-writer-strict-v2 same-save {:?} delta{:?} stats{}",
        owner.borrow().counts,
        owner.borrow().delta,
        provider.statistics().unwrap().json()
    );
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_cross_pack_payload_and_sql_four_leaf_pool_parity() {
    let (owner, path, catalog) = owner("writer-cross-pack");
    let provider = owner.borrow().provider.clone();
    catalog.abandon(owner.borrow().save).unwrap();
    drop(owner);
    let mut expected = Vec::new();
    // One writer per save preserves the independently sealed cross-pack fixture.
    let mut previous_whole = None;
    let mut previous_chunk = None;
    let mut previous_pool = None;
    for name in [
        "whole0",
        "whole1",
        "chunk0",
        "chunk1",
        "chunk2",
        "unrelated",
        "pool0",
        "pool1",
        "pool2",
        "pool3",
    ] {
        let base = if name.starts_with("whole") {
            previous_whole
        } else if name == "unrelated" {
            Some(sealed("chunk0", None).id())
        } else if name.starts_with("chunk") {
            previous_chunk
        } else {
            previous_pool
        };
        let object = sealed(name, base);
        let object_id = object.id();
        let bytes = object.canonical().to_vec();
        let mut writer = Writer::new(catalog.clone(), provider.clone(), capacities()).unwrap();
        let usage = if name.starts_with("pool") {
            LogicalUse::MetadataGraph
        } else {
            LogicalUse::RegularFileGraph
        };
        let before = provider.statistics().unwrap();
        writer.offer(usage, object).unwrap();
        writer.finish_storage().unwrap();
        catalog.publish(writer.save).unwrap();
        if name.starts_with("pool") {
            assert_eq!(provider.statistics().unwrap().put_calls, before.put_calls);
            assert_eq!(
                (
                    writer.pool_counts.full_leaves,
                    writer.pool_counts.delta_leaves
                ),
                if name == "pool0" { (1, 0) } else { (0, 1) }
            );
            assert_eq!(
                (
                    writer.pool_counts.new_values,
                    writer.pool_counts.reused_values
                ),
                if name == "pool0" { (40, 0) } else { (4, 36) }
            );
            previous_pool = Some(object_id);
        } else {
            assert_eq!(
                writer.delta.trials,
                u64::from(!matches!(name, "whole0" | "chunk0"))
            );
            assert_eq!(
                writer.delta.prefix_selected,
                u64::from(matches!(name, "whole1" | "chunk1" | "chunk2"))
            );
            if name.starts_with("whole") {
                previous_whole = Some(object_id)
            } else if name.starts_with("chunk") {
                previous_chunk = Some(object_id)
            }
        }
        expected.push((usage, object_id, bytes));
        println!(
            "sealed {name}: {:?} {:?} {:?}",
            writer.counts, writer.delta, writer.pool_counts
        );
    }
    let catalog = StrictCatalog::open_read_only(&path).unwrap();
    let scope = catalog.capture(None).unwrap();
    let mut reader = ReadOwner::new().unwrap();
    for (usage, object, canonical) in expected.into_iter().rev() {
        let before = provider.statistics().unwrap().get_calls;
        let access = Access {
            catalog: &catalog,
            provider: &provider,
            scope,
            logical_use: usage,
        };
        assert_eq!(
            reader.read(&access, &capacities(), &[object]).unwrap(),
            vec![canonical]
        );
        if usage == LogicalUse::MetadataGraph {
            assert_eq!(provider.statistics().unwrap().get_calls, before);
        }
    }
    println!(
        "sp1-writer-strict-v2 cross-pack/pool component PASS; {}",
        provider.statistics().unwrap().json()
    );
}
#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_actual_cdc_attribute_dual_use_and_threshold_roots() {
    let (owner, path, catalog) = owner("writer-real-cdc");
    let policy = ConstructionPolicy::frozen_default();
    let source = evidence().join("strict-oracle-v2/vectors");
    let raw = std::fs::read(source.join("state0-chunked.raw")).unwrap();
    let root = stream(&owner, &raw);
    assert_eq!(
        root,
        id("305406a42d5a606fe0824693d19d8f860284026320a02b9b97f84bea559f65a2")
    );
    let first = id("a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399");
    let canonical = std::fs::read(source.join(format!(
        "{}.canonical",
        "a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399"
    )))
    .unwrap();
    let chunk = std::fs::read(source.join("first-cdc.raw")).unwrap();
    let reader = ConstructionReader {
        owner: owner.clone(),
        logical_use: LogicalUse::MetadataGraph,
    };
    let mut consumer = Consumer {
        owner: owner.clone(),
        logical_use: LogicalUse::MetadataGraph,
    };
    let mut objects = layerfs_content::filesystem::FilesystemObjects::new(&reader, &mut consumer);
    let attribute =
        layerfs_content::filesystem::attributes::value::emit_value(&mut objects, &chunk).unwrap();
    assert_eq!(
        attribute,
        id(std::fs::read_to_string(source.join("opaque-root.txt"))
            .unwrap()
            .trim())
    );
    use layerfs_content::filesystem::attributes::{
        build::build_attribute_tree, codec::AttributeEntry, keys::AttributeKey,
        portable::PortableMetadata, value::emit_value,
    };
    let portable = PortableMetadata {
        mode: 0o640,
        mtime_seconds: 1_700_000_000,
        mtime_nanoseconds: 0,
    };
    let mut entries = vec![Ok(AttributeEntry {
        key: AttributeKey::new("sp1".to_owned(), b"opaque".to_vec()).unwrap(),
        value_root: attribute,
    })];
    for (key, bytes) in [
        (
            b"mode".as_slice(),
            portable
                .mode_bytes(layerfs_content::inode_leaf::InodeKind::RegularFile)
                .unwrap()
                .to_vec(),
        ),
        (
            b"mtime".as_slice(),
            portable.mtime_bytes().unwrap().to_vec(),
        ),
    ] {
        let value_root = emit_value(&mut objects, &bytes).unwrap();
        entries.push(Ok(AttributeEntry {
            key: AttributeKey::new("portable".to_owned(), key.to_vec()).unwrap(),
            value_root,
        }));
    }
    entries.sort_by(|a, b| a.as_ref().unwrap().key.cmp(&b.as_ref().unwrap().key));
    let attribute_root = build_attribute_tree(&mut objects, entries.into_iter())
        .unwrap()
        .0;
    assert_eq!(
        attribute_root,
        id("6ef294c5525112b43a7edd91bfcd3cd2c85a1e85ef59e1bac2eb9e43ebeec2e7")
    );
    let mut roots = Vec::new();
    for n in [131071, 131072, 200000] {
        let bytes = if n == 131072 {
            let mut b = std::fs::read(source.join("state0-boundary.raw")).unwrap();
            b.push(0x7a);
            b
        } else if n == 131071 {
            std::fs::read(source.join("state0-boundary.raw")).unwrap()
        } else {
            raw.clone()
        };
        let got = stream(&owner, &bytes);
        let table = std::fs::read_to_string(source.join("content-roots.tsv")).unwrap();
        let state = if n == 131072 { "1" } else { "0" };
        let path = if n == 200000 { "chunked" } else { "boundary" };
        let row = table
            .lines()
            .skip(1)
            .find(|row| {
                let c = row.split('\t').collect::<Vec<_>>();
                c[0] == state && c[1] == path
            })
            .unwrap();
        let expected = id(row.split('\t').nth(3).unwrap());
        assert_eq!(got, expected);
        roots.push((got, bytes));
    }
    assert_eq!(
        stream(
            &owner,
            &std::fs::read(source.join("state0-boundary.raw")).unwrap()
        ),
        roots[0].0
    );
    owner.borrow_mut().finish_storage().unwrap();
    catalog.publish(owner.borrow().save).unwrap();
    let provider = owner.borrow().provider.clone();
    let reopened = StrictCatalog::open_read_only(&path).unwrap();
    let scope = reopened.capture(None).unwrap();
    for (domain, usage) in [
        (PlacementDomain::Metadata, LogicalUse::MetadataGraph),
        (PlacementDomain::FilePayload, LogicalUse::RegularFileGraph),
    ] {
        assert!(reopened.location(scope, domain, first).unwrap().is_some());
        let access = Access {
            catalog: &reopened,
            provider: &provider,
            scope,
            logical_use: usage,
        };
        assert_eq!(
            ReadOwner::new()
                .unwrap()
                .read(&access, &capacities(), &[first])
                .unwrap(),
            vec![canonical.clone()]
        );
    }
    let catalog = Arc::new(StrictCatalog::open_read_only(&path).unwrap());
    let reader = phase6_live_probe::strict_read::Reader {
        scope: catalog.capture(None).unwrap(),
        catalog,
        provider: provider.clone(),
        logical_use: LogicalUse::RegularFileGraph,
        owner: Rc::new(RefCell::new(ReadOwner::new().unwrap())),
        capacities: capacities(),
    };
    for (root, expected) in roots {
        let mut bytes = Vec::new();
        layerfs_telemetry::timer::Timing::disabled("strict.logical-proof", |scope| {
            layerfs_content::read_all(&reader, root, &mut bytes, scope.child("read"))
        })
        .0
        .unwrap();
        assert_eq!(bytes, expected);
    }
    println!("sp1-writer-strict-v2 ordinary stream CDC dual placement and threshold pinned roots PASS; policy{:?},counts{:?}",policy,owner.borrow().counts);
}

#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_sql_pool_same_save_ordinals_and_captured_scope_refusal() {
    let (owner, path, catalog) = owner("writer-pool-same-save");
    let provider = owner.borrow().provider.clone();
    let foreign = catalog.capture(None).unwrap();
    let before = provider.statistics().unwrap();
    let source = evidence().join("strict-oracle-v2/same-save");
    let inventory = std::fs::read_to_string(source.join("pool-same-save.tsv")).unwrap();
    let mut expected = Vec::new();
    let mut previous = None;
    for row in inventory.lines().skip(1) {
        let cells = row.split('\t').collect::<Vec<_>>();
        let bytes = std::fs::read(source.join(format!("{}.canonical", cells[1]))).unwrap();
        let mut hints = AdvisoryPredecessors::new();
        if let Some(base) = previous {
            hints
                .push(base, PredecessorProvenance::OriginalBase)
                .unwrap();
        }
        let object = FinalizedObject::new(ObjectRole::InodeLeaf, bytes.clone())
            .unwrap()
            .with_predecessors(hints);
        previous = Some(object.id());
        owner
            .borrow_mut()
            .offer(LogicalUse::MetadataGraph, object)
            .unwrap();
        expected.push((previous.unwrap(), bytes));
    }
    assert_eq!(
        (
            owner.borrow().pool_counts.leaves,
            owner.borrow().pool_counts.reused_values,
            owner.borrow().pool_counts.new_values
        ),
        (2, 8, 10)
    );
    assert_eq!(
        (
            owner.borrow().pool_counts.full_leaves,
            owner.borrow().pool_counts.delta_leaves
        ),
        (2, 0)
    );
    assert_eq!(
        owner.borrow().pool_counts.trials,
        0,
        "old pending pooled leaf is ineligible before its group is sealed"
    );
    owner.borrow_mut().finish_storage().unwrap();
    let private = catalog.capture(Some(owner.borrow().save)).unwrap();
    let mut reads = ReadOwner::new().unwrap();
    let access = Access {
        catalog: catalog.as_ref(),
        provider: &provider,
        scope: private,
        logical_use: LogicalUse::MetadataGraph,
    };
    for (object, canonical) in &expected {
        assert_eq!(
            reads.read(&access, &capacities(), &[*object]).unwrap(),
            vec![canonical.clone()]
        );
        assert_eq!(ObjectId::for_bytes(canonical), *object);
    }
    let access = Access {
        scope: foreign,
        ..access
    };
    assert!(reads
        .read(&access, &capacities(), &[expected[1].0])
        .unwrap_err()
        .contains("required domain placement missing"));
    assert!(catalog.group_for(private, 1).unwrap().is_some());
    assert!(catalog.group_for(foreign, 1).unwrap().is_none());
    let after = provider.statistics().unwrap();
    assert_eq!(after.put_calls - before.put_calls, 0);
    assert_eq!(after.get_calls - before.get_calls, 0);
    catalog.publish(owner.borrow().save).unwrap();
    let catalog = StrictCatalog::open_read_only(&path).unwrap();
    let access = Access {
        catalog: &catalog,
        provider: &provider,
        scope: catalog.capture(None).unwrap(),
        logical_use: LogicalUse::MetadataGraph,
    };
    let mut reads = ReadOwner::new().unwrap();
    for (object, canonical) in expected {
        assert_eq!(
            reads.read(&access, &capacities(), &[object]).unwrap(),
            vec![canonical]
        );
    }
    println!("sp1-writer-strict-v2 SQL same-save ordinals/private captured cache refusal PASS {:?}; MinIO GET/PUTdelta0",owner.borrow().pool_counts);
}

#[test]
#[ignore = "real provider identity and fresh SP1_WITNESS_OUT required"]
fn real_provider_shared_mapping_body_keeps_per_logical_use_child_routes() {
    // Adapter dual-graph fixture: the SAME sealed ordinary C1 file graph is
    // submitted for both uses. Ordinary emit_value's shared Chunk proof remains
    // the separate actual attribute test; it cannot emit this multiple-extent map.
    let provider = provider();
    provider.create_bucket().unwrap();
    let source = evidence().join("strict-oracle-v2/vectors");
    let expected = std::fs::read(source.join("state0-chunked.raw")).unwrap();
    let root = id("305406a42d5a606fe0824693d19d8f860284026320a02b9b97f84bea559f65a2");
    let first = id("a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399");
    for (iteration, missing) in [LogicalUse::RegularFileGraph, LogicalUse::MetadataGraph]
        .into_iter()
        .enumerate()
    {
        let path = disposable(&format!("writer-shared-map-{iteration}"));
        let catalog = Arc::new(StrictCatalog::create(&path).unwrap());
        let writer = Rc::new(RefCell::new(
            Writer::new(catalog.clone(), provider.clone(), capacities()).unwrap(),
        ));
        assert_eq!(stream(&writer, &expected), root);
        let inventory = std::fs::read_to_string(source.join("regular-N200000.tsv")).unwrap();
        for row in inventory.lines().skip(1) {
            let cells = row.split('\t').collect::<Vec<_>>();
            let bytes = std::fs::read(source.join(format!("{}.canonical", cells[1]))).unwrap();
            let references = if cells[4].is_empty() {
                vec![]
            } else {
                cells[4].split(',').map(id).collect()
            };
            let object = FinalizedObject::new(
                ObjectRole::from_code(cells[2].parse().unwrap()).unwrap(),
                bytes,
            )
            .unwrap()
            .with_references(references);
            writer
                .borrow_mut()
                .offer(LogicalUse::MetadataGraph, object)
                .unwrap();
        }
        writer.borrow_mut().finish_storage().unwrap();
        catalog.publish(writer.borrow().save).unwrap();
        drop(writer);
        let catalog = Arc::new(StrictCatalog::open_read_only(&path).unwrap());
        let scope = catalog.capture(None).unwrap();
        let owner = Rc::new(RefCell::new(ReadOwner::new().unwrap()));
        let present = if missing == LogicalUse::MetadataGraph {
            LogicalUse::RegularFileGraph
        } else {
            LogicalUse::MetadataGraph
        };
        let reader = phase6_live_probe::strict_read::Reader {
            catalog: catalog.clone(),
            provider: provider.clone(),
            scope,
            logical_use: present,
            owner: owner.clone(),
            capacities: capacities(),
        };
        let mut bytes = Vec::new();
        let before = provider.statistics().unwrap().get_calls;
        layerfs_telemetry::timer::Timing::disabled("shared-map.present", |timing| {
            layerfs_content::read_all(&reader, root, &mut bytes, timing.child("read"))
        })
        .0
        .unwrap();
        assert_eq!(bytes, expected);
        if present == LogicalUse::MetadataGraph {
            assert_eq!(provider.statistics().unwrap().get_calls, before);
        }
        let reader = phase6_live_probe::strict_read::Reader {
            logical_use: missing,
            ..reader
        };
        let mut bytes = Vec::new();
        let before = provider.statistics().unwrap().get_calls;
        layerfs_telemetry::timer::Timing::disabled("shared-map.other", |timing| {
            layerfs_content::read_all(&reader, root, &mut bytes, timing.child("read"))
        })
        .0
        .unwrap();
        assert_eq!(bytes, expected);
        if missing == LogicalUse::MetadataGraph {
            assert_eq!(provider.statistics().unwrap().get_calls, before);
        }
        let domain = if missing == LogicalUse::MetadataGraph {
            PlacementDomain::Metadata
        } else {
            PlacementDomain::FilePayload
        };
        let db = rusqlite::Connection::open(&path).unwrap();
        // Deliberately malformed disposable fixture: preserve referring use facts
        // while removing one required placement. Product FK policy is unchanged.
        db.pragma_update(None, "foreign_keys", false).unwrap();
        let selected = catalog.location(scope, domain, first).unwrap().unwrap();
        let count = db
            .execute(
                "DELETE FROM locators WHERE id=?1 AND domain=?2 AND body_order=?3",
                rusqlite::params![
                    first.as_bytes().as_slice(),
                    domain as i64,
                    selected.location.pack_id
                ],
            )
            .unwrap();
        assert_eq!(count, 1);
        drop(db);
        let mut bytes = Vec::new();
        let result = layerfs_telemetry::timer::Timing::disabled("shared-map.missing", |timing| {
            layerfs_content::read_all(&reader, root, &mut bytes, timing.child("read"))
        })
        .0;
        assert!(
            result.is_err(),
            "a warmed mapping/canonical cache cannot answer a missing child domain"
        );
        println!("sp1-writer-strict-v2 shared SQL mapping body {:?} child-domain missing refuses with owner caches warm; adapter dual-graph fixture",missing);
    }
}
