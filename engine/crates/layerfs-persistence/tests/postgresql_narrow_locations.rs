//! #90: PostgreSQL object locations are narrow rows found through a B-tree
//! on the id's first eight bytes; the full id is compared, and uniqueness is
//! kept under the lane by inserting only absent ids. Ids that share their
//! first eight bytes are distinct objects; an id already stored is lost to
//! the first writer, never stored twice; a signature naming no location is
//! refused with everything else of its publication; the tiny tables have no
//! TOAST relation.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{
    location::{ObjectLocation, PackDomain, PackInfo, SignatureRow},
    pack::{assemble, build_group, PackLane},
    port::*,
    StoragePolicy,
};
use std::sync::Arc;

struct Schema {
    url: String,
    selection: PostgresSelection,
}
impl Schema {
    fn new(url: String, tag: &str) -> (Self, PostgresHandles) {
        let name = format!("lfs90_{tag}_{}", std::process::id());
        let schema = Self {
            selection: PostgresSelection::new(&url, &name),
            url,
        };
        schema.sql(&format!("DROP SCHEMA IF EXISTS {name} CASCADE"));
        let handles = PostgresHandles::create(
            &schema.selection,
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: b"postgresql-narrow-locations".to_vec(),
                cursor_key: [90; 32],
                incarnation: 5,
            },
        )
        .unwrap();
        (schema, handles)
    }
    fn sql(&self, sql: &str) {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client.batch_execute(sql).unwrap();
    }
    fn count(&self, sql: &str) -> i64 {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client.query_one(sql, &[]).unwrap().get(0)
    }
}
impl Drop for Schema {
    fn drop(&mut self) {
        let _ = postgres::Client::connect(&self.url, postgres::NoTls).and_then(|mut c| {
            c.batch_execute(&format!(
                "DROP SCHEMA IF EXISTS {} CASCADE",
                self.selection.schema
            ))
        });
    }
}
fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

/// An id whose first eight bytes are `head` and whose rest is `tail`.
fn id(head: u8, tail: u8) -> ObjectId {
    let mut bytes = [tail; 32];
    bytes[..8].copy_from_slice(&[head; 8]);
    ObjectId::from_bytes(&bytes).unwrap()
}
fn location(object_id: ObjectId, pack_id: i64, record_number: usize) -> ObjectLocation {
    ObjectLocation {
        object_id,
        role: ObjectRole::WholeFile,
        canonical_length: 64,
        pack_id,
        group_number: 0,
        record_number,
    }
}
/// A publication of one new pack holding `objects`.
fn unit(handles: &PostgresHandles, objects: Vec<(ObjectId, usize)>) -> Publication {
    let pack_id = handles
        .storage
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap()
        .first_pack_id;
    let body = assemble(
        PackLane::WholeFile,
        &[build_group(PackLane::WholeFile, &[vec![0x10; 64]], None).unwrap()],
    )
    .unwrap();
    Publication {
        packs: vec![PublishedPack {
            info: PackInfo {
                pack_id,
                domain: PackDomain::Payload,
                key: ObjectKey::for_bytes(&body),
                length: body.len(),
            },
            body: Arc::new(body),
        }],
        objects: objects
            .into_iter()
            .map(|(object, record)| location(object, pack_id, record))
            .collect(),
        ..Publication::default()
    }
}
fn located(handles: &PostgresHandles, ids: &[ObjectId]) -> Vec<(ObjectId, i64, usize)> {
    let mut out = Vec::new();
    handles.storage.locate(ids, &mut out).unwrap();
    let mut found: Vec<_> = out
        .into_iter()
        .map(|o| {
            (
                o.location.object_id,
                o.location.pack_id,
                o.location.record_number,
            )
        })
        .collect();
    found.sort();
    found
}

#[test]
fn ids_that_share_a_prefix_are_distinct_and_a_repeat_is_lost_to_the_first() {
    let Some(url) = url() else { return };
    let (schema, handles) = Schema::new(url, "prefix");
    let s = &schema.selection.schema;
    // Three ids with one prefix, one with another.
    let (a, b, c, d) = (id(1, 1), id(1, 2), id(1, 3), id(2, 1));
    // A batch that repeats an id is refused before anything is sent.
    let repeated = unit(&handles, vec![(a, 0), (a, 1)]);
    assert_eq!(
        handles.storage.publish(&repeated).unwrap_err(),
        PersistenceError::Malformed
    );
    let first = unit(&handles, vec![(a, 0), (b, 1), (d, 2)]);
    let one = first.packs[0].info.pack_id;
    assert_eq!(handles.storage.publish(&first).unwrap().lost, vec![]);
    assert_eq!(
        located(&handles, &[a, b, c, d]),
        vec![(a, one, 0), (b, one, 1), (d, one, 2)]
    );
    // A second writer publishes a stored id and a new one with the same
    // prefix: the stored one is lost and keeps its location.
    let second = unit(&handles, vec![(b, 4), (c, 5)]);
    let two = second.packs[0].info.pack_id;
    assert_eq!(handles.storage.publish(&second).unwrap().lost, vec![b]);
    assert_eq!(
        located(&handles, &[a, b, c, d]),
        vec![(a, one, 0), (b, one, 1), (c, two, 5), (d, one, 2)]
    );
    assert_eq!(
        schema.count(&format!("SELECT count(*) FROM {s}.object_location")),
        4
    );
    // The lookup goes through the prefix index, not a scan.
    let mut client = postgres::Client::connect(&schema.url, postgres::NoTls).unwrap();
    let plan: Vec<String> = client
        .query(
            &format!(
                "EXPLAIN SELECT 1 FROM {s}.object_location o WHERE \
                 ('x'||encode(substring(o.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=$1 AND o.object_id=$2"
            ),
            &[&0x0101_0101_0101_0101_i64, &a.as_bytes().as_slice()],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert!(
        plan.iter()
            .any(|line| line.contains("object_location_prefix")),
        "{plan:?}"
    );
}

#[test]
fn a_signature_that_names_no_location_refuses_its_publication() {
    let Some(url) = url() else { return };
    let (schema, handles) = Schema::new(url, "signature");
    let s = &schema.selection.schema;
    let stored = id(3, 1);
    handles
        .storage
        .publish(&unit(&handles, vec![(stored, 0)]))
        .unwrap();
    let signature = |slot: usize, object_id| SignatureRow {
        slot,
        stamp: slot as u64 + 1,
        object_id,
        signature: [slot as u8; 32],
    };
    // A signature may name a stored location or one of its publication.
    let fresh = id(3, 2);
    let mut batch = unit(&handles, vec![(fresh, 0)]);
    batch.signatures = vec![signature(1, stored), signature(2, fresh)];
    handles.storage.publish(&batch).unwrap();
    // One that names neither refuses the whole publication.
    let other = id(3, 3);
    let mut refused = unit(&handles, vec![(other, 0)]);
    refused.signatures = vec![signature(3, id(3, 4))];
    assert!(handles.storage.publish(&refused).is_err());
    assert_eq!(located(&handles, &[other]), vec![]);
    let mut rows = Vec::new();
    handles.storage.signatures(&mut rows).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        schema.count(&format!("SELECT count(*) FROM {s}.object_location")),
        2
    );
}

#[test]
fn only_tables_with_unbounded_values_have_a_toast_relation() {
    let Some(url) = url() else { return };
    let (schema, _handles) = Schema::new(url, "toast");
    let mut client = postgres::Client::connect(&schema.url, postgres::NoTls).unwrap();
    let toasted: Vec<String> = client
        .query(
            "SELECT c.relname::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace \
             WHERE n.nspname=$1 AND c.relkind='r' AND c.reltoastrelid<>0 ORDER BY 1",
            &[&schema.selection.schema],
        )
        .unwrap()
        .iter()
        .map(|row| row.get(0))
        .collect();
    assert_eq!(
        toasted,
        ["commit", "history_meta", "layer", "pack", "reach"]
    );
}
