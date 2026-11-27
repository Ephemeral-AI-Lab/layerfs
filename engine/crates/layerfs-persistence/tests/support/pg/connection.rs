#![allow(dead_code)]
//! Shared fixture for the PostgreSQL runtime-evidence tests (T1.3b, T6, T6.5,
//! T7.1, T7.2). One disposable schema per test, driven only through the public
//! `PostgresHandles`/`HistoryCatalog` API except where a test proves the
//! physical layout (a trigger, a dropped table, an epoch row).
use layerfs_content::ObjectId;
use layerfs_history::records::{
    AddLayerOutcome, AddLayerRequest, BranchMergeRequest, CommitMeta, CommitOutcome, CommitRequest,
    CommitVia, ForkRequest, ForkSource, MergeCommit, MergeHead, MergeSide, StackInitialization,
};
use layerfs_history::{
    BranchId, CommitId, CommitRecord, HistoryCatalog, HistoryCatalogConfig, LayerId, LayerRecord,
    LayerStackId, NodeId,
};
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::port::PersistenceError;
use layerfs_storage::StoragePolicy;

use super::super::{name, root, stack};

/// The URL a PostgreSQL-backed test runs against, or `None` with the usual
/// NOT_RUN note.
pub fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

/// One `PostgresSelection` under a disposable schema named for the test.
pub struct Schema {
    pub url: String,
    pub name: String,
}

impl Schema {
    pub fn new(url: String, tag: &str) -> Self {
        let name = format!("lfsg6a_{tag}_{}", std::process::id());
        let schema = Self { url, name };
        schema.drop_schema();
        schema
    }

    pub fn selection(&self) -> PostgresSelection {
        PostgresSelection::new(&self.url, &self.name)
    }

    pub fn create(&self, binding: &[u8], cursor: [u8; 32]) -> PostgresHandles {
        PostgresHandles::create(
            &self.selection(),
            StoragePolicy::frozen_default(),
            &catalog(binding, cursor, 3),
        )
        .expect("PostgreSQL Store creation")
    }

    pub fn open(
        &self,
        binding: &[u8],
        cursor: [u8; 32],
    ) -> Result<PostgresHandles, PersistenceError> {
        PostgresHandles::open_writable(&self.selection(), binding, cursor)
    }

    /// An admin connection with `search_path` pointing at the test schema.
    pub fn admin(&self) -> postgres::Client {
        let mut client = postgres::Client::connect(&self.url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("SET search_path TO {}", self.name))
            .unwrap();
        client
    }

    /// The live column definition the Store records, exactly as
    /// `history_meta.schema_definition` holds it.
    pub fn definition(&self) -> String {
        self.admin().query_one(DEFINITION, &[]).unwrap().get(0)
    }

    pub fn drop_schema(&self) {
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

/// The live column definition, as the Store records it.
pub const DEFINITION: &str = "SELECT string_agg(table_name::text||' '||column_name::text||' '||data_type::text||' '||is_nullable::text, chr(10) ORDER BY table_name, ordinal_position)||chr(10) FROM information_schema.columns WHERE table_schema=current_schema()";

pub fn catalog(binding: &[u8], cursor: [u8; 32], incarnation: u64) -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: binding.to_vec(),
        cursor_key: cursor,
        incarnation,
    }
}

/// Project `tag` with one Branch `work`, at its genesis Layer.
pub fn project(history: &dyn HistoryCatalog, tag: u8) -> (LayerStackId, BranchId, LayerId) {
    project_with_genesis(history, tag, root(1))
}

/// Project `tag` with one Branch `work`, rooted at an already-published
/// genesis object (a mark needs a locator for every retained root).
pub fn project_with_genesis(
    history: &dyn HistoryCatalog,
    tag: u8,
    genesis_root: ObjectId,
) -> (LayerStackId, BranchId, LayerId) {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(tag),
            name: name(&format!("project-{tag}")),
            scope: root(200),
            profile: root(201),
            genesis_root,
        })
        .unwrap();
    let branch = branch_id(tag);
    history
        .fork(&ForkRequest {
            stack: stack(tag),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    (stack(tag), branch, record.head_layer)
}

pub fn branch_id(tag: u8) -> BranchId {
    super::super::branch_id(tag)
}

pub fn commit_request(
    history: &dyn HistoryCatalog,
    branch: BranchId,
    candidate_root: ObjectId,
) -> CommitRequest {
    let record = history.branch(branch).unwrap().unwrap();
    let expected_root = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    CommitRequest {
        branch,
        expected_head: record.head_commit,
        expected_base: record.base_layer,
        expected_root,
        candidate_root,
        profile: root(201),
        scope: root(200),
        second_parent: None,
        meta: CommitMeta::new(CommitVia::Files),
        force: false,
        required_head: None,
    }
}

pub fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRecord {
    commit_with(history, branch, root(tag))
}

pub fn commit_with(
    history: &dyn HistoryCatalog,
    branch: BranchId,
    candidate_root: ObjectId,
) -> CommitRecord {
    match history
        .commit_branch(&commit_request(history, branch, candidate_root))
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
}

pub fn layer_request(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    branch: BranchId,
) -> AddLayerRequest {
    let record = history.branch(branch).unwrap().unwrap();
    let head = history.layer_stack(stack).unwrap().unwrap();
    AddLayerRequest {
        stack,
        branch,
        commit: record.head_commit.expect("a committed Branch"),
        expected_stack_head: head.head_layer,
        expected_branch_base: record.base_layer,
    }
}

pub fn publish(history: &dyn HistoryCatalog, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    match history
        .add_layer(&layer_request(history, stack, branch))
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

/// A merge of `work`'s Commit with a second Branch still at the genesis Layer:
/// one new Commit, second parent the genesis node, root `tag`.
pub fn merge_request(
    history: &dyn HistoryCatalog,
    stack: LayerStackId,
    branch: BranchId,
    commit: CommitId,
    genesis: LayerId,
    tag: u8,
) -> BranchMergeRequest {
    let side = branch_id(2);
    history
        .fork(&ForkRequest {
            stack,
            branch: side,
            name: name("side"),
            source: ForkSource::Layer(genesis),
        })
        .unwrap();
    BranchMergeRequest {
        stack,
        branch: branch_id(9),
        name: name("merged"),
        sides: [
            (MergeSide::Branch(branch), NodeId::Commit(commit)),
            (MergeSide::Branch(side), NodeId::Layer(genesis)),
        ],
        head: MergeHead::Commit(Box::new(MergeCommit {
            base: genesis,
            parent: Some(commit),
            second_parent: NodeId::Layer(genesis),
            root: root(tag),
            meta: CommitMeta::new(CommitVia::Merge),
        })),
    }
}

/// `(commit rows, layer rows)` to prove a refusal wrote nothing.
pub fn counts(admin: &mut postgres::Client) -> (i64, i64) {
    let count = |admin: &mut postgres::Client, table: &str| -> i64 {
        admin
            .query_one(&format!("SELECT count(*) FROM \"{table}\""), &[])
            .unwrap()
            .get(0)
    };
    (count(admin, "commit"), count(admin, "layer"))
}

pub fn epoch(admin: &mut postgres::Client, stack: LayerStackId) -> i64 {
    admin
        .query_one(
            "SELECT epoch FROM project_epoch WHERE layer_stack_id=$1",
            &[&stack.as_slice()],
        )
        .unwrap()
        .get(0)
}

/// Name of every base table in the test schema, in name order.
pub fn tables(admin: &mut postgres::Client) -> Vec<String> {
    admin
        .query(
            "SELECT table_name::text FROM information_schema.tables \
             WHERE table_schema=current_schema() AND table_type='BASE TABLE' ORDER BY 1",
            &[],
        )
        .unwrap()
        .into_iter()
        .map(|row| row.get(0))
        .collect()
}
