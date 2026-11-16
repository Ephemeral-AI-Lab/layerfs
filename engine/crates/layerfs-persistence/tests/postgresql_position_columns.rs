//! T1.2c: a PostgreSQL Store written before the position columns gains
//! `commit.history_serial`, `layer.history_serial` and
//! `store_policy.trace_salt` at its first writable open, with T1.4's serial
//! backfill in the same transaction. A Store that already has the columns is
//! left alone, the salt is never replaced, a recorded serial is never lowered
//! and a recorded source no release wrote is refused.
//! Needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    LayerRecord, LayerStackId, LayerStackRecord, StackInitialization,
};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
use support::{branch_id, name, root, stack};

const BINDING: &[u8] = b"postgresql-position-columns";
const CURSOR: [u8; 32] = [12; 32];
/// The `commit` and `layer` position column as `schema.sql` writes it; it
/// appears once in each of the two tables.
const HISTORY_SERIAL: &str = " history_serial bigint NOT NULL DEFAULT 0,\n";
/// The `store_policy` position salt column and the comma the column before it
/// gains, as `schema.sql` writes them.
const TRACE_SALT: &str = ",\n trace_salt bytea NOT NULL DEFAULT decode(repeat('00',32),'hex') CHECK (length(trace_salt)=32)";
/// The live column definition, as the Store records it.
const DEFINITION: &str = "SELECT string_agg(table_name::text||' '||column_name::text||' '||data_type::text||' '||is_nullable::text, chr(10) ORDER BY table_name, ordinal_position)||chr(10) FROM information_schema.columns WHERE table_schema=current_schema()";

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 4,
    }
}

/// One schema, dropped when the test ends, pass or fail.
struct Schema {
    url: String,
    name: String,
}

impl Schema {
    fn new(url: String, tag: &str) -> Self {
        let name = format!("lfs12c_{tag}_{}", std::process::id());
        let schema = Self { url, name };
        schema.drop_schema();
        schema
    }
    fn selection(&self) -> PostgresSelection {
        PostgresSelection::new(&self.url, &self.name)
    }
    fn create(&self) -> PostgresHandles {
        PostgresHandles::create(
            &self.selection(),
            StoragePolicy::frozen_default(),
            &catalog(),
        )
        .unwrap()
    }
    fn open(&self) -> Result<PostgresHandles, PersistenceError> {
        PostgresHandles::open_writable(&self.selection(), BINDING, CURSOR)
    }
    fn admin(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("SET search_path TO {}", self.name))
            .unwrap();
        client
    }
    fn drop_schema(&self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.name));
        }
    }
}

impl Drop for Schema {
    fn drop(&mut self) {
        self.drop_schema();
    }
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

/// Rewrites a fresh Store into the format written before T1.2c: the three
/// position columns are gone, and the recorded schema is what T1.4 wrote.
/// A T1.4 Store never allocated a serial, so the counter is back at 1.
/// Returns the fresh source it replaced.
fn make_old(schema: &Schema) -> String {
    let fresh: String = schema
        .admin()
        .query_one("SELECT schema_source FROM history_meta", &[])
        .unwrap()
        .get(0);
    assert_eq!(
        fresh.matches(HISTORY_SERIAL).count(),
        2,
        "a fresh Store holds the position columns"
    );
    assert_eq!(fresh.matches(TRACE_SALT).count(), 1);
    schema
        .admin()
        .batch_execute(
            "ALTER TABLE \"commit\" DROP COLUMN history_serial; \
             ALTER TABLE layer DROP COLUMN history_serial; \
             ALTER TABLE store_policy DROP COLUMN trace_salt; \
             UPDATE history_meta SET next_history_serial=1;",
        )
        .unwrap();
    let definition: String = schema.admin().query_one(DEFINITION, &[]).unwrap().get(0);
    let old = fresh
        .replacen(HISTORY_SERIAL, "", 2)
        .replacen(TRACE_SALT, "", 1);
    schema
        .admin()
        .execute(
            "UPDATE history_meta SET schema_source=$1,schema_definition=$2",
            &[&old, &definition],
        )
        .unwrap();
    fresh
}

fn initialize(handles: &PostgresHandles, tag: u8) -> LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(tag),
            name: name(&format!("project-{tag}")),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap()
}

fn fork(
    handles: &PostgresHandles,
    stack: LayerStackId,
    branch: BranchId,
    tag: u8,
    source: ForkSource,
) {
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: name(&format!("branch-{tag}")),
            source,
        })
        .unwrap();
}

fn commit(handles: &PostgresHandles, branch: BranchId, tag: u8) -> CommitRecord {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => handles
            .history
            .layer(snapshot.base_layer)
            .unwrap()
            .unwrap()
            .root,
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
}

fn publish(handles: &PostgresHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

fn commits(admin: &mut postgres::Client) -> Vec<(Vec<u8>, i64, i64)> {
    admin
        .query(
            "SELECT commit_id,created_ms,history_serial FROM \"commit\"",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect()
}

type LayerRow = (Vec<u8>, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, i64);

fn layers(admin: &mut postgres::Client) -> Vec<LayerRow> {
    admin
        .query(
            "SELECT layer_id,layer_stack_id,parent_layer_id,source_commit_id,history_serial FROM layer",
            &[],
        )
        .unwrap()
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3), row.get(4)))
        .collect()
}

fn salt(admin: &mut postgres::Client) -> Vec<u8> {
    admin
        .query_one("SELECT trace_salt FROM store_policy WHERE id=1", &[])
        .unwrap()
        .get(0)
}

fn next_serial(admin: &mut postgres::Client) -> i64 {
    admin
        .query_one("SELECT next_history_serial FROM history_meta", &[])
        .unwrap()
        .get(0)
}

fn definition(admin: &mut postgres::Client) -> String {
    admin.query_one(DEFINITION, &[]).unwrap().get(0)
}

fn recorded(admin: &mut postgres::Client) -> (String, String) {
    let row = admin
        .query_one(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            &[],
        )
        .unwrap();
    (row.get(0), row.get(1))
}

#[test]
fn an_empty_store_gains_the_columns_their_defaults_and_a_random_salt() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "empty");
    drop(schema.create());
    {
        let mut admin = schema.admin();
        let created = salt(&mut admin);
        assert_eq!(created.len(), 32);
        assert_ne!(created, vec![0; 32], "initialize seeds one random salt");
    }
    let fresh = make_old(&schema);
    schema.open().expect("a pre-T1.2c Store opens writable");
    drop(schema.open().expect("an upgraded Store reopens"));
    let mut admin = schema.admin();
    assert_eq!(
        next_serial(&mut admin),
        1,
        "no history keeps the next serial at 1"
    );
    let epochs: i64 = admin
        .query_one("SELECT count(*) FROM project_epoch", &[])
        .unwrap()
        .get(0);
    assert_eq!(epochs, 0, "a Store with no project has no epoch");
    let upgraded = salt(&mut admin);
    assert_eq!(upgraded.len(), 32);
    assert_ne!(upgraded, vec![0; 32], "the Store keeps one random salt");
    for (table, column, data_type) in [
        ("commit", "history_serial", "bigint"),
        ("layer", "history_serial", "bigint"),
        ("store_policy", "trace_salt", "bytea"),
    ] {
        let row = admin
            .query_one(
                "SELECT is_nullable::text,data_type::text,column_default::text \
                 FROM information_schema.columns \
                 WHERE table_schema=current_schema() AND table_name=$1 AND column_name=$2",
                &[&table, &column],
            )
            .unwrap();
        assert_eq!(row.get::<_, String>(0), "NO", "{table}.{column} is NOT NULL");
        assert_eq!(row.get::<_, String>(1), data_type, "{table}.{column} type");
        let default: Option<String> = row.get(2);
        assert!(default.is_some(), "{table}.{column} has a default");
    }
    let (source, recorded_definition) = recorded(&mut admin);
    assert_eq!(source, fresh, "the Store records the current schema");
    assert_eq!(recorded_definition, definition(&mut admin));
}

#[test]
fn commits_layers_and_the_next_serial_are_backfilled_once() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "graph");
    let (stack_one, stack_two);
    {
        let handles = schema.create();
        let one = initialize(&handles, 1);
        let two = initialize(&handles, 2);
        let a = branch_id(1);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        commit(&handles, a, 10);
        publish(&handles, one.id, a);
        commit(&handles, a, 11);
        publish(&handles, one.id, a);
        let b = branch_id(2);
        fork(&handles, two.id, b, 2, ForkSource::Layer(two.head_layer));
        commit(&handles, b, 20);
        publish(&handles, two.id, b);
        stack_one = one.id;
        stack_two = two.id;
    }
    let fresh = make_old(&schema);
    schema.open().expect("a pre-T1.2c Store opens writable");
    let mut admin = schema.admin();
    let assigned_salt = salt(&mut admin);
    let assigned_next = next_serial(&mut admin);

    // Commits take consecutive Store serials in (created_ms, commit_id) order.
    let mut ordered = commits(&mut admin);
    let count = ordered.len() as i64;
    ordered.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    let serials: Vec<i64> = ordered.iter().map(|row| row.2).collect();
    assert_eq!(
        serials,
        (1..=count).collect::<Vec<_>>(),
        "serials are dense and follow the deterministic order"
    );
    let per_commit: std::collections::HashMap<Vec<u8>, i64> =
        ordered.iter().map(|row| (row.0.clone(), row.2)).collect();

    // Per project the genesis Layer takes the next serial, in stack-id order.
    let trees = layers(&mut admin);
    let mut genesis: Vec<(Vec<u8>, i64)> = trees
        .iter()
        .filter(|row| row.2.is_none())
        .map(|row| (row.1.clone(), row.4))
        .collect();
    genesis.sort_by(|a, b| a.0.cmp(&b.0));
    let stacks: Vec<Vec<u8>> = genesis.iter().map(|row| row.0.clone()).collect();
    let mut expected_stacks = vec![stack_one.as_slice().to_vec(), stack_two.as_slice().to_vec()];
    expected_stacks.sort();
    assert_eq!(stacks, expected_stacks);
    let assigned: Vec<i64> = genesis.iter().map(|row| row.1).collect();
    let expected_genesis: Vec<i64> = (count + 1..=count + genesis.len() as i64).collect();
    assert_eq!(
        assigned, expected_genesis,
        "genesis Layers follow the commits"
    );

    // Every republished tree carries the serial of the Commit it publishes.
    for row in trees.iter().filter(|row| row.2.is_some()) {
        let source = row.3.as_deref().expect("a published Layer holds its Commit");
        assert_eq!(
            row.4,
            per_commit[source],
            "a Layer carries its source Commit serial"
        );
    }
    assert_eq!(
        next_serial(&mut admin),
        count + genesis.len() as i64 + 1,
        "the next serial follows the highest assigned one"
    );

    // Reopening changes nothing: the columns exist, so the step is skipped.
    let (source, recorded_definition) = recorded(&mut admin);
    assert_eq!(source, fresh, "the Store records the current schema");
    assert_eq!(recorded_definition, definition(&mut admin));
    drop(admin);
    drop(schema.open().expect("an upgraded Store reopens"));
    let mut admin = schema.admin();
    assert_eq!(salt(&mut admin), assigned_salt, "the salt is never replaced");
    assert_eq!(next_serial(&mut admin), assigned_next);
}

#[test]
fn the_backfill_starts_at_the_recorded_next_serial() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "next");
    {
        let handles = schema.create();
        let one = initialize(&handles, 1);
        let a = branch_id(1);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        commit(&handles, a, 10);
        publish(&handles, one.id, a);
    }
    make_old(&schema);
    schema
        .admin()
        .execute("UPDATE history_meta SET next_history_serial=500", &[])
        .unwrap();
    schema.open().expect("a part-way Store opens writable");
    let mut admin = schema.admin();
    let mut serials: Vec<i64> = commits(&mut admin).iter().map(|row| row.2).collect();
    serials.sort();
    assert_eq!(
        serials,
        vec![500],
        "history starts at the recorded next serial"
    );
    let trees = layers(&mut admin);
    let genesis = trees.iter().find(|row| row.2.is_none()).unwrap().4;
    let published = trees.iter().find(|row| row.2.is_some()).unwrap().4;
    assert_eq!(genesis, 501, "a genesis Layer follows the commits");
    assert_eq!(published, 500, "a Layer carries its source Commit serial");
    assert_eq!(next_serial(&mut admin), 502);
}

#[test]
fn a_recorded_source_no_release_wrote_is_refused() {
    let Some(url) = url() else { return };
    let schema = Schema::new(url, "refused");
    drop(schema.create());
    make_old(&schema);
    schema
        .admin()
        .execute("UPDATE history_meta SET schema_source=schema_source||'x'", &[])
        .unwrap();
    let refused = match schema.open() {
        Ok(_) => panic!("a tampered source is refused"),
        Err(error) => error,
    };
    assert!(
        matches!(refused, PersistenceError::Malformed),
        "{refused:?}"
    );
}
