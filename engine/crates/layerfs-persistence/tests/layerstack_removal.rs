//! #506: removing one project (LayerStack) from a Store that holds two leaves
//! the other whole, frees the name, and takes the removed project's logical
//! bytes out of the totals. Runs on SQLite; with `LAYERFS_TEST_POSTGRES_URL`
//! set it also runs on a PostgreSQL Store imported from the same SQLite Store.
mod support;

use layerfs_history::error::Missing;
use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRecord, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig, HistoryError, NodeId, Page};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"layerstack-removal";
const CURSOR: [u8; 32] = [31; 32];

fn sqlite(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

/// Project `n`: stack `n` with `logical` genesis bytes and Branch `n`.
fn project(history: &dyn HistoryCatalog, n: u8, text: &str, logical: u64) -> BranchId {
    let record = history
        .initialize_layerstack(&StackInitialization {
            logical,
            stack: stack(n),
            name: name(text),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap();
    let branch = branch_id(n);
    history
        .fork(&ForkRequest {
            stack: stack(n),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    branch
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, tag: u8) -> CommitRecord {
    let record = history.branch(branch).unwrap().unwrap();
    let current = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    let outcome = history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: current,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{outcome:?}");
    };
    *record
}

/// The two projects in a new SQLite Store, `api` first so the totals of a
/// Store holding only `api` are known. Returns the Store, those totals, and
/// the Commits of `api` and `shop`.
fn two_projects(temp: &Temp) -> (Handles, u64, CommitRecord, CommitRecord) {
    let created = Handles::create(
        sqlite(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
    let api = project(&created.history, 2, "api", 20);
    let api_commit = commit(&created.history, api, 5);
    let api_only = created.history.statistics().unwrap().logical;
    let shop = project(&created.history, 1, "shop", 1000);
    let first = commit(&created.history, shop, 6);
    commit(&created.history, shop, 7);
    assert_eq!(first.stack, stack(1));
    (created, api_only, api_commit, first)
}

/// Removes `shop` and checks what stays and what goes.
fn removes_one_project(
    history: &dyn HistoryCatalog,
    api_only: u64,
    api_commit: &CommitRecord,
    shop_commit: &CommitRecord,
) {
    assert!(history.statistics().unwrap().logical > api_only);
    history.remove_layerstack(stack(1)).unwrap();

    assert_eq!(history.layer_stack(stack(1)).unwrap(), None);
    assert_eq!(history.branch(branch_id(1)).unwrap(), None);
    assert_eq!(history.commit(shop_commit.id).unwrap(), None);
    assert_eq!(history.layer(shop_commit.base_layer).unwrap(), None);
    assert_eq!(history.statistics().unwrap().logical, api_only);
    assert_eq!(
        history.remove_layerstack(stack(1)),
        Err(HistoryError::Missing(Missing::LayerStack))
    );
    let names: Vec<_> = history
        .layer_stacks(&Page {
            cursor: None,
            limit: 10,
        })
        .unwrap()
        .records
        .into_iter()
        .map(|s| s.name.as_str().to_owned())
        .collect();
    assert_eq!(names, ["api"]);

    // The other project is whole and keeps committing.
    assert_eq!(
        history.commit(api_commit.id).unwrap().as_ref(),
        Some(api_commit)
    );
    let next = commit(history, branch_id(2), 8);
    assert_eq!(next.parent, Some(api_commit.id));
    assert!(history
        .is_ancestor(NodeId::Commit(api_commit.id), NodeId::Commit(next.id))
        .unwrap());

    // The name is free again, for a new stack with its own lineage.
    let again = project(history, 3, "shop", 0);
    let fresh = commit(history, again, 6);
    assert_eq!(fresh.stack, stack(3));
    assert_eq!(fresh.parent, None);
}

#[test]
fn removing_one_project_leaves_the_other_whole_on_sqlite() {
    let temp = Temp::new("layerstack-removal");
    let (created, api_only, api_commit, shop_commit) = two_projects(&temp);
    removes_one_project(&created.history, api_only, &api_commit, &shop_commit);
}

#[cfg(feature = "postgresql")]
#[test]
fn removing_one_project_leaves_the_other_whole_on_postgresql() {
    use layerfs_persistence::{PostgresHandles, PostgresSelection};
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    struct Schema(String, String);
    impl Drop for Schema {
        fn drop(&mut self) {
            if let Ok(mut client) = postgres::Client::connect(&self.0, postgres::NoTls) {
                let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.1));
            }
        }
    }
    let temp = Temp::new("layerstack-removal-pg");
    let (created, api_only, api_commit, shop_commit) = two_projects(&temp);
    let copy = created.snapshot(&temp.join("copy")).unwrap();
    let schema = Schema(
        url.clone(),
        format!("lfs506_removal_{}", std::process::id()),
    );
    let selection = PostgresSelection::new(&url, &schema.1);
    PostgresHandles::import(sqlite(copy.path), &selection, BINDING, CURSOR).unwrap();
    // A home that did not import the schema reads its binding to open it.
    assert_eq!(PostgresHandles::binding(&selection).unwrap(), BINDING);
    let pg = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    removes_one_project(&pg.history, api_only, &api_commit, &shop_commit);
    // A schema that is not a LayerFS Store has no binding.
    let other = PostgresSelection::new(&url, "public");
    assert!(PostgresHandles::binding(&other).is_err());
    drop(pg);
    drop(schema);
}
