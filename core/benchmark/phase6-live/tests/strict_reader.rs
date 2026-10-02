//! Real-provider strict-v2 reader witnesses. Missing prerequisites remain ignored.
mod strict_witness;
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{StorageCapacities, StoragePolicy};
use phase6_live_probe::{
    strict_catalog::{LogicalUse, PlacementDomain, StrictCatalog},
    strict_read::{Access, ReadOwner},
};
use std::collections::BTreeMap;
use strict_witness::{disposable, evidence, id, old_fixture, old_objects, provider};
fn capacities() -> StorageCapacities {
    StorageCapacities::from_policy(StoragePolicy::frozen_default()).unwrap()
}
#[test]
#[ignore = "real MinIO identity/resources and fresh SP1_WITNESS_OUT required"]
fn real_provider_reader_payload_chains_and_sql_pools() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let path = disposable("reader-chains");
    let catalog = StrictCatalog::create(&path).unwrap();
    let objects = old_objects();
    let bases = [
        ("whole1", "whole0"),
        ("chunk1", "chunk0"),
        ("chunk2", "chunk1"),
        ("pool1", "pool0"),
        ("pool2", "pool1"),
        ("pool3", "pool2"),
    ]
    .into_iter()
    .map(|(child, base)| (objects[child], objects[base]))
    .collect();
    old_fixture(
        &catalog,
        &provider,
        &evidence().join("old-producer-v1/producer.sqlite"),
        None,
        &bases,
    );
    let reopened = StrictCatalog::open_read_only(&path).unwrap();
    let scope = reopened.capture(None).unwrap();
    let mut owner = ReadOwner::new().unwrap();
    let capacity = capacities();
    let access = Access {
        catalog: &reopened,
        provider: &provider,
        scope,
        logical_use: LogicalUse::RegularFileGraph,
    };
    let names = [
        "chunk2",
        "chunk0",
        "whole1",
        "whole0",
        "chunk1",
        "unrelated",
        "chunk2",
    ];
    let requests = names.iter().map(|name| objects[*name]).collect::<Vec<_>>();
    let result = owner.read(&access, &capacity, &requests).unwrap();
    assert_eq!(result.len(), names.len());
    for (name, bytes) in names.iter().zip(result) {
        assert_eq!(
            bytes,
            std::fs::read(evidence().join(format!("old-producer-v1/{name}.canonical"))).unwrap()
        );
        assert_eq!(ObjectId::for_bytes(&bytes), objects[*name]);
    }
    assert!(owner.counters.edges >= 2);
    assert!(provider.statistics().unwrap().get_calls > 0);
    let gets = provider.statistics().unwrap().get_calls;
    let access = Access {
        logical_use: LogicalUse::MetadataGraph,
        ..access
    };
    for name in ["pool3", "pool2", "pool1", "pool0"] {
        let bytes = owner
            .read(&access, &capacity, &[objects[name]])
            .unwrap()
            .remove(0);
        assert_eq!(ObjectId::for_bytes(&bytes), objects[name]);
        assert_eq!(
            bytes,
            std::fs::read(evidence().join(format!("old-producer-v1/{name}.canonical"))).unwrap()
        );
    }
    assert_eq!(
        provider.statistics().unwrap().get_calls - gets,
        0,
        "SQL metadata/pool window must make zero MinIO GETs"
    );
    println!(
        "sp1-reader-strict-v2 payload/pool component PASS: {:?}; {}",
        owner.counters,
        provider.statistics().unwrap().json()
    );
}
#[test]
#[ignore = "real MinIO identity/resources and fresh SP1_WITNESS_OUT required"]
fn real_provider_actual_cdc_dual_use_read_orders_and_missing_domain_refusal() {
    let provider = provider();
    provider.create_bucket().unwrap();
    let source = evidence().join("strict-oracle-v2/dual-use/dual-chunk.sqlite");
    let chunk = id("a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399");
    let expected=std::fs::read(evidence().join("strict-oracle-v2/vectors/a0210f01dd9a468b515cc918df5c77c5960bd47bfad53c4353e634c7e6bbd399.canonical")).unwrap();
    for (ordinal, first) in [LogicalUse::MetadataGraph, LogicalUse::RegularFileGraph]
        .into_iter()
        .enumerate()
    {
        let path = disposable(&format!("dual-order-{ordinal}"));
        let catalog = StrictCatalog::create(&path).unwrap();
        for (domain, usage) in [
            (PlacementDomain::Metadata, LogicalUse::MetadataGraph),
            (PlacementDomain::FilePayload, LogicalUse::RegularFileGraph),
        ] {
            old_fixture(
                &catalog,
                &provider,
                &source,
                Some((domain, usage)),
                &BTreeMap::new(),
            );
        }
        let catalog = StrictCatalog::open_read_only(&path).unwrap();
        let scope = catalog.capture(None).unwrap();
        let mut owner = ReadOwner::new().unwrap();
        let capacity = capacities();
        let second = if first == LogicalUse::MetadataGraph {
            LogicalUse::RegularFileGraph
        } else {
            LogicalUse::MetadataGraph
        };
        for logical_use in [first, second, first] {
            let before = provider.statistics().unwrap().get_calls;
            let access = Access {
                catalog: &catalog,
                provider: &provider,
                scope,
                logical_use,
            };
            assert_eq!(
                owner.read(&access, &capacity, &[chunk]).unwrap(),
                vec![expected.clone()]
            );
            if logical_use == LogicalUse::MetadataGraph {
                assert_eq!(provider.statistics().unwrap().get_calls, before);
            }
        }
        assert_eq!(
            catalog.descriptor(chunk).unwrap(),
            Some((ObjectRole::Chunk, expected.len()))
        );
    }
    for (ordinal, present) in [LogicalUse::MetadataGraph, LogicalUse::RegularFileGraph]
        .into_iter()
        .enumerate()
    {
        let path = disposable(&format!("dual-missing-{ordinal}"));
        let catalog = StrictCatalog::create(&path).unwrap();
        let domain = if present == LogicalUse::MetadataGraph {
            PlacementDomain::Metadata
        } else {
            PlacementDomain::FilePayload
        };
        old_fixture(
            &catalog,
            &provider,
            &source,
            Some((domain, present)),
            &BTreeMap::new(),
        );
        let scope = catalog.capture(None).unwrap();
        let access = Access {
            catalog: &catalog,
            provider: &provider,
            scope,
            logical_use: present,
        };
        let mut owner = ReadOwner::new().unwrap();
        assert_eq!(
            owner.read(&access, &capacities(), &[chunk]).unwrap(),
            vec![expected.clone()]
        );
        let missing = if present == LogicalUse::MetadataGraph {
            LogicalUse::RegularFileGraph
        } else {
            LogicalUse::MetadataGraph
        };
        let access = Access {
            logical_use: missing,
            ..access
        };
        let error = owner.read(&access, &capacities(), &[chunk]).unwrap_err();
        assert!(
            error.contains("required domain placement missing"),
            "{error}"
        );
    }
    println!(
        "sp1-reader-strict-v2 real CDC dual-domain read orders and placement cache refusal PASS"
    );
}

#[test]
#[ignore = "real MinIO identity/resources and fresh SP1_WITNESS_OUT required"]
fn real_provider_sql_metadata_graph_retains_all_independent_canonicals() {
    let provider = provider();
    let path = disposable("reader-metadata-graph");
    let catalog = StrictCatalog::create(&path).unwrap();
    old_fixture(
        &catalog,
        &provider,
        &evidence().join("strict-oracle-v2/domain-packs/metadata.sqlite"),
        Some((PlacementDomain::Metadata, LogicalUse::MetadataGraph)),
        &BTreeMap::new(),
    );
    let catalog = StrictCatalog::open_read_only(&path).unwrap();
    let scope = catalog.capture(None).unwrap();
    let access = Access {
        catalog: &catalog,
        provider: &provider,
        scope,
        logical_use: LogicalUse::MetadataGraph,
    };
    let mut owner = ReadOwner::new().unwrap();
    let inventory =
        std::fs::read_to_string(evidence().join("strict-oracle-v2/domain-packs/metadata.tsv"))
            .unwrap();
    let before = provider.statistics().unwrap();
    let mut requests = 0;
    let mapping_ids = strict_witness::regular_mapping_ids();
    for row in inventory.lines().skip(1) {
        let cells = row.split('\t').collect::<Vec<_>>();
        let object = id(cells[1]);
        let access = Access {
            logical_use: if mapping_ids.contains(&object) {
                LogicalUse::RegularFileGraph
            } else {
                LogicalUse::MetadataGraph
            },
            ..access
        };
        let canonical = owner
            .read(&access, &capacities(), &[object])
            .unwrap()
            .remove(0);
        assert_eq!(ObjectId::for_bytes(&canonical), object);
        assert_eq!(
            canonical,
            std::fs::read(
                evidence().join(format!("strict-oracle-v2/vectors/{}.canonical", cells[1]))
            )
            .unwrap()
        );
        requests += 1;
    }
    let after = provider.statistics().unwrap();
    assert_eq!(after.get_calls - before.get_calls, 0);
    assert_eq!(after.put_calls - before.put_calls, 0);
    println!("sp1-reader-strict-v2 SQL canonical metadata graph PASS: {requests} independently authenticated objects, MinIO GET/PUT delta0; full logical FUSE history remains separate");
}
