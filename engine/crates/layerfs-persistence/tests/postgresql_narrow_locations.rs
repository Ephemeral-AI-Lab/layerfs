//! #90: PostgreSQL object locations are narrow rows found through a B-tree
//! on the id's first eight bytes; the full id is compared, and uniqueness is
//! kept under the lane by inserting only absent ids. Ids that share their
//! first eight bytes are distinct objects; an id already stored is lost to
//! the first writer, never stored twice; a signature naming no location is
//! refused with everything else of its publication; the tiny tables have no
//! TOAST relation.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use layerfs_content::filesystem::attributes::portable::PortableMetadata;
use layerfs_content::{
    inode_leaf::{encode_inode_value, InodeKind, InodeMetadata, InodeValue},
    ObjectId, ObjectRole,
};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{
    location::{InodeValueRow, ObjectLocation, PackDomain, SignatureRow},
    pack::{assemble, build_group, PackLane},
    port::*,
    StoragePolicy,
};
use std::sync::Arc;

const BINDING: &[u8] = b"postgresql-narrow-locations";
const CURSOR: [u8; 32] = [90; 32];

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
                binding_key: BINDING.to_vec(),
                cursor_key: CURSOR,
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
        base: None,
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
        packs: vec![PublishedPack::seal(
            pack_id,
            PackDomain::Payload,
            Arc::new(body),
        )],
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
    let one = first.packs[0].info().pack_id;
    assert_eq!(handles.storage.publish(&first).unwrap().lost, vec![]);
    assert_eq!(
        located(&handles, &[a, b, c, d]),
        vec![(a, one, 0), (b, one, 1), (d, one, 2)]
    );
    // A second writer publishes a stored id and a new one with the same
    // prefix: the stored one is lost and keeps its location.
    let second = unit(&handles, vec![(b, 4), (c, 5)]);
    let two = second.packs[0].info().pack_id;
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

fn captured_batch(
    handles: &PostgresHandles,
    objects: Vec<(ObjectId, usize)>,
    slot: usize,
    signed: ObjectId,
) -> Publication {
    let mut batch = unit(handles, objects);
    let ordinal = handles
        .storage
        .reserve(Reserve {
            packs: 0,
            ordinals: 1,
        })
        .unwrap()
        .first_ordinal;
    batch.inode_values.push(InodeValueRow {
        ordinal,
        value: encode_inode_value(InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: signed,
            metadata_root: InodeMetadata::Inline(PortableMetadata {
                mode: 0o644,
                mtime_seconds: 1,
                mtime_nanoseconds: 0,
            }),
        }),
    });
    batch.signatures.push(SignatureRow {
        slot,
        stamp: slot as u64 + 1,
        object_id: signed,
        signature: [slot as u8; 32],
    });
    batch
}

fn publication_state(schema: &Schema) -> Vec<String> {
    let mut admin = postgres::Client::connect(&schema.url, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("SET search_path TO {}", schema.selection.schema))
        .unwrap();
    [
        ("pack", "pack_id"),
        ("object_location", "object_id"),
        ("inode_value_block", "first_ordinal"),
        ("content_signature", "slot"),
        ("store_totals", "id"),
    ]
    .into_iter()
    .map(|(table, key)| {
        admin
            .query_one(
                &format!("SELECT coalesce(json_agg(t ORDER BY {key})::text,'[]') FROM {table} t"),
                &[],
            )
            .unwrap()
            .get(0)
    })
    .collect()
}

#[test]
fn captured_publication_keeps_the_winner_and_rolls_back_invalid_rows() {
    let Some(url) = url() else { return };
    let exercise = |tag, captured| {
        let (schema, first_writer) = Schema::new(url.clone(), tag);
        let writer = PostgresHandles::open_writable(&schema.selection, BINDING, CURSOR).unwrap();
        let publish = |batch: &Publication| {
            if captured {
                writer.storage.publish_captured(batch)
            } else {
                writer.storage.publish(batch)
            }
        };
        let (winner, warm, fresh, refused, absent, next) =
            (id(4, 1), id(4, 2), id(4, 3), id(4, 4), id(4, 5), id(4, 6));
        let seed = unit(&first_writer, vec![(winner, 0)]);
        let winner_pack = seed.packs[0].info().pack_id;
        assert!(first_writer.storage.publish(&seed).unwrap().lost.is_empty());
        let initial = captured_batch(&writer, vec![(warm, 0)], 1, warm);
        assert!(publish(&initial).unwrap().lost.is_empty());

        let mixed = captured_batch(&writer, vec![(winner, 0), (fresh, 1)], 2, fresh);
        let start = writer.diagnostics().unwrap();
        assert_eq!(publish(&mixed).unwrap().lost, vec![winner]);
        let work = writer.diagnostics().unwrap().since(&start);
        if captured {
            assert_eq!(work.round_trips, 1, "{work:?}");
            assert_eq!(work.write_commits, 1, "{work:?}");
        }
        assert_eq!(
            located(&first_writer, &[winner, fresh]),
            vec![
                (winner, winner_pack, 0),
                (fresh, mixed.packs[0].info().pack_id, 1)
            ]
        );

        // This new pack, object and inode value would be publishable on
        // their own. The signature names an absent object, which the row
        // statement discovers only when the server runs the transaction.
        let invalid = captured_batch(&writer, vec![(refused, 0)], 3, absent);
        let before = publication_state(&schema);
        let failure = publish(&invalid).unwrap_err();
        assert_eq!(publication_state(&schema), before);
        assert!(located(&first_writer, &[refused, absent]).is_empty());

        // An independent valid call on that session succeeds after the
        // refusal. The prior request never publishes its pack or values.
        let valid = captured_batch(&writer, vec![(next, 0)], 4, next);
        assert!(publish(&valid).unwrap().lost.is_empty());
        assert_eq!(
            located(&first_writer, &[next]),
            vec![(next, valid.packs[0].info().pack_id, 0)]
        );
        (publication_state(&schema), failure)
    };
    assert_eq!(exercise("capture", true), exercise("ordinary", false));
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
    // The additive T1.2c/T1.3b/T7.1 tables keep PostgreSQL's default column
    // storage, so they gain a TOAST relation even where every column is
    // bounded (bytea ids, bigint counters, short text); the base schema marks
    // exactly its bounded columns STORAGE PLAIN.
    assert_eq!(
        toasted,
        [
            "branch_retired",
            "commit",
            "gc_dead",
            "gc_intent",
            "gc_op",
            "history_meta",
            "kept_node",
            "layer",
            "layer_stack_retired",
            "pack",
            "path_removed",
            "project_epoch",
            "reach",
            "store_policy",
        ]
    );
}
