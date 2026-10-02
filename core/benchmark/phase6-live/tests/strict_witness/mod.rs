//! Sealed independent old-producer fixture import, outside the product.
#![allow(dead_code)]
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    access::{ObjectLocation, ValueGroupRow},
    pack::layout::{parse_header, PackLane},
};
use phase6_live_probe::{
    minio::{digest, Minio},
    strict_catalog::{LogicalUse, PhysicalBase, PlacementDomain, Registration, StrictCatalog},
};
use rusqlite::Connection;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
pub fn evidence() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/architecture/proposal/phase6-sqlite-minio/implementation/sp1/evidence")
}
pub fn provider() -> Minio {
    Minio {
        authority: std::env::var("SP1_MINIO_AUTHORITY").expect("real MinIO authority prerequisite"),
        bucket: std::env::var("SP1_MINIO_BUCKET").expect("isolated real MinIO bucket prerequisite"),
        access: std::env::var("SP1_MINIO_ACCESS").expect("real MinIO access prerequisite"),
        secret: std::env::var("SP1_MINIO_SECRET").expect("real MinIO secret prerequisite"),
        stats: Arc::new(Mutex::new(Default::default())),
    }
}
pub fn id(text: &str) -> ObjectId {
    let bytes = (0..32)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).unwrap())
        .collect::<Vec<_>>();
    ObjectId::from_bytes(&bytes).unwrap()
}
pub fn disposable(label: &str) -> PathBuf {
    let root = PathBuf::from(
        std::env::var("SP1_WITNESS_OUT").expect("fresh owned witness output prerequisite"),
    );
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!("{label}.sqlite"));
    assert!(!path.exists(), "witness evidence is append-only");
    path
}
/// Imports existing framed bytes, preserving source order, ordinals and locators.
/// Source provenance is fixed to the original named fixture; no runtime classifier.
pub fn regular_mapping_ids() -> std::collections::BTreeSet<ObjectId> {
    let mut ids = std::collections::BTreeSet::new();
    for state in 0..3 {
        for name in ["whole", "chunked", "boundary", "duplicate"] {
            let path = evidence().join(format!("strict-oracle-v2/vectors/state{state}-{name}.tsv"));
            if !path.exists() {
                continue;
            }
            for row in std::fs::read_to_string(path).unwrap().lines().skip(1) {
                let cells = row.split('\t').collect::<Vec<_>>();
                let role = ObjectRole::from_code(cells[2].parse().unwrap()).unwrap();
                if !matches!(role, ObjectRole::Chunk | ObjectRole::WholeFile) {
                    ids.insert(id(cells[1]));
                }
            }
        }
    }
    ids
}
pub fn old_fixture(
    catalog: &StrictCatalog,
    provider: &Minio,
    source: &Path,
    forced: Option<(PlacementDomain, LogicalUse)>,
    bases: &BTreeMap<ObjectId, ObjectId>,
) {
    let mapping_ids = if source
        .file_name()
        .is_some_and(|name| name == "metadata.sqlite")
    {
        regular_mapping_ids()
    } else {
        Default::default()
    };
    let db =
        Connection::open_with_flags(source, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    let save = catalog.begin_save().unwrap();
    let mut bodies = BTreeMap::new();
    let mut stmt = db
        .prepare("SELECT pack_id,data FROM object_packs ORDER BY pack_id")
        .unwrap();
    let packs = stmt
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
        })
        .unwrap();
    for row in packs {
        let (order, bytes) = row.unwrap();
        let lane = parse_header(&bytes).unwrap().lane;
        let domain = forced.map(|v| v.0).unwrap_or_else(|| {
            if lane == PackLane::WholeFile || lane == PackLane::Native {
                PlacementDomain::FilePayload
            } else {
                PlacementDomain::Metadata
            }
        });
        let digest = digest(&bytes);
        if domain == PlacementDomain::FilePayload {
            provider.put(&digest, &bytes).unwrap();
        }
        let placed = catalog
            .register_body(
                domain,
                save,
                digest,
                if domain == PlacementDomain::Metadata {
                    Some(&bytes)
                } else {
                    None
                },
            )
            .unwrap();
        bodies.insert(order, (placed, domain));
    }
    let mut stmt=db.prepare("SELECT first_ordinal,count,pack_id,group_number,digest FROM metadata_value_groups ORDER BY first_ordinal").unwrap();
    let groups = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, u32>(0)?,
                row.get::<_, u32>(1)? as usize,
                row.get::<_, i64>(2)?,
                row.get::<_, u32>(3)? as usize,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .unwrap();
    for row in groups {
        let (first, count, pack, group, digest) = row.unwrap();
        assert_eq!(catalog.reserve_ordinals(save, count).unwrap(), first);
        catalog
            .insert_groups(
                catalog.capture(Some(save)).unwrap(),
                save,
                &[ValueGroupRow {
                    first_ordinal: first,
                    count,
                    pack_id: bodies[&pack].0,
                    group_number: group,
                    digest: ObjectId::from_bytes(&digest).unwrap(),
                }],
            )
            .unwrap();
    }
    let mut stmt=db.prepare("SELECT object_id,object_role,canonical_length,pack_id,group_number,record_number FROM objects ORDER BY pack_id,group_number,record_number").unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, u8>(1)?,
                row.get::<_, u32>(2)? as usize,
                row.get::<_, i64>(3)?,
                row.get::<_, u32>(4)? as usize,
                row.get::<_, u32>(5)? as usize,
            ))
        })
        .unwrap();
    for row in rows {
        let (object, role, length, pack, group, record) = row.unwrap();
        let object = ObjectId::from_bytes(&object).unwrap();
        let (placed, domain) = bodies[&pack];
        let usage = forced
            .map(|v| v.1)
            .unwrap_or(if domain == PlacementDomain::FilePayload {
                LogicalUse::RegularFileGraph
            } else {
                LogicalUse::MetadataGraph
            });
        let usage = if mapping_ids.contains(&object) {
            LogicalUse::RegularFileGraph
        } else {
            usage
        };
        let scope = catalog.capture(Some(save)).unwrap();
        let base = bases.get(&object).map(|id| PhysicalBase {
            id: *id,
            domain,
            body: catalog
                .location(scope, domain, *id)
                .unwrap()
                .unwrap()
                .location
                .pack_id,
        });
        catalog
            .register(
                scope,
                save,
                &[Registration {
                    location: ObjectLocation {
                        object_id: object,
                        role: ObjectRole::from_code(role).unwrap(),
                        canonical_length: length,
                        pack_id: placed,
                        group_number: group,
                        record_number: record,
                    },
                    domain,
                    logical_use: usage,
                    references: vec![],
                    base,
                }],
            )
            .unwrap();
    }
    catalog.finish_storage(save).unwrap();
    catalog.publish(save).unwrap();
}
pub fn old_objects() -> BTreeMap<String, ObjectId> {
    std::fs::read_to_string(evidence().join("old-producer-v1/objects.tsv"))
        .unwrap()
        .lines()
        .skip(1)
        .map(|row| {
            let cells = row.split('\t').collect::<Vec<_>>();
            (cells[0].to_owned(), id(cells[1]))
        })
        .collect()
}
