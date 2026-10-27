//! A Store written before the lineage tables (#353) opens writable, gains them
//! and is backfilled at open (REBUILD.md kind 2): ancestry, merge bases and
//! overwrite counts are the same as before, and publication continues.
mod support;
use layerfs_history::records::{
    AddLayerOutcome, AddLayerRequest, CommitMeta, CommitOutcome, CommitRequest, CommitVia,
    ForkRequest, ForkSource, StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, LayerId, NodeId};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use support::*;

const LINEAGE: &str = include_str!("../sql/sqlite/lineage.sql");
const SCRATCH: &str = include_str!("../sql/sqlite/merge_scratch.sql");
const DIGESTS: &str = include_str!("../sql/sqlite/file_digest.sql");

fn definition(sql: &rusqlite::Connection) -> String {
    let mut statement = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap();
    rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect()
}

/// Rewrites a Store into the format written before #353: each Layer lists
/// the Layers it overwrote, newest first, and the lineage tables are absent.
fn make_old(path: &std::path::Path) {
    let sql = rusqlite::Connection::open(path).unwrap();
    let layers: Vec<(Vec<u8>, Vec<u8>, i64, i64)> = sql
        .prepare("SELECT layer_id, layer_stack_id, height, overwrote_count FROM layer_lineage")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for (id, stack, height, count) in layers {
        let mut list = Vec::new();
        for below in (height - count..height).rev() {
            let id: Vec<u8> = sql
                .query_row(
                    "SELECT layer_id FROM layer_lineage WHERE layer_stack_id=?1 AND height=?2",
                    rusqlite::params![stack, below],
                    |r| r.get(0),
                )
                .unwrap();
            list.extend(id);
        }
        sql.execute(
            "UPDATE layer SET overwrote=?1 WHERE layer_id=?2",
            rusqlite::params![list, id],
        )
        .unwrap();
    }
    sql.execute_batch(
        "DROP TABLE layer_lineage; DROP TABLE commit_lineage; DROP TABLE absorbed; DROP TABLE reach; \
         DROP TABLE merge_operation; DROP TABLE merge_record; DROP TABLE file_digest;",
    )
    .unwrap();
    let source: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    let old = source
        .replacen(LINEAGE, "", 1)
        .replacen(SCRATCH, "", 1)
        .replacen(DIGESTS, "", 1);
    assert_ne!(old, source, "the schema holds the lineage tables");
    sql.execute(
        "UPDATE history_meta SET schema_source=?1, schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
}

fn writable(path: &std::path::Path) -> Handles {
    Handles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        BINDING,
        [71; 32],
    )
    .expect("a pre-lineage Store opens writable")
}

struct Builder<'a> {
    history: &'a dyn HistoryCatalog,
    tag: u8,
}

impl Builder<'_> {
    fn fork(&self, n: u8, source: ForkSource) -> BranchId {
        let id = branch_id(n);
        self.history
            .fork(&ForkRequest {
                stack: stack(1),
                branch: id,
                name: name(&format!("b{n}")),
                source,
            })
            .unwrap();
        id
    }

    fn commit(&mut self, branch: BranchId, second: Option<NodeId>) -> NodeId {
        let record = self.history.branch(branch).unwrap().unwrap();
        let current = match record.head_commit {
            Some(head) => self.history.commit(head).unwrap().unwrap().root,
            None => self.history.layer(record.base_layer).unwrap().unwrap().root,
        };
        self.tag += 1;
        let outcome = self
            .history
            .commit_branch(&CommitRequest {
                branch,
                expected_head: record.head_commit,
                expected_base: record.base_layer,
                expected_root: current,
                candidate_root: root(self.tag),
                profile: root(201),
                scope: root(200),
                second_parent: second,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
            })
            .unwrap();
        let CommitOutcome::Committed { record, .. } = outcome else {
            panic!("{outcome:?}");
        };
        NodeId::Commit(record.id)
    }

    fn publish(&self, branch: BranchId) -> LayerId {
        let record = self.history.branch(branch).unwrap().unwrap();
        let outcome = self
            .history
            .add_layer(&AddLayerRequest {
                stack: stack(1),
                branch,
                commit: record.head_commit.unwrap(),
                expected_stack_head: self
                    .history
                    .layer_stack(stack(1))
                    .unwrap()
                    .unwrap()
                    .head_layer,
                expected_branch_base: record.base_layer,
            })
            .unwrap();
        let AddLayerOutcome::Added { layer, .. } = outcome else {
            panic!("{outcome:?}");
        };
        layer.id
    }
}

type Answers = Vec<(bool, Option<NodeId>)>;

fn answers(history: &dyn HistoryCatalog, nodes: &[NodeId]) -> Answers {
    let mut out = Vec::new();
    for a in nodes {
        for b in nodes {
            out.push((
                history.is_ancestor(*a, *b).unwrap(),
                history.merge_base(*a, *b).unwrap(),
            ));
        }
    }
    out
}

#[test]
fn a_pre_lineage_store_is_backfilled_at_open_and_answers_the_same() {
    let t = Temp::new("lineage-backfill");
    let path = t.join("db");
    let (nodes, before, layers) = {
        let catalog = create(&path);
        let history: &dyn HistoryCatalog = &catalog;
        let stack_record = history
            .initialize_layerstack(&StackInitialization {
                logical: 0,
                stack: stack(1),
                name: name("main"),
                scope: root(200),
                profile: root(201),
                genesis_root: root(0),
            })
            .unwrap();
        let mut b = Builder { history, tag: 0 };
        let genesis = stack_record.head_layer;
        let mut nodes = vec![NodeId::Layer(genesis)];
        let x = b.fork(1, ForkSource::Layer(genesis));
        let y = b.fork(2, ForkSource::Layer(genesis));
        nodes.push(b.commit(x, None));
        let x1 = b.commit(x, None);
        nodes.push(x1);
        nodes.push(b.commit(y, None));
        let mut layers = vec![b.publish(y)];
        let NodeId::Commit(x1c) = x1 else {
            unreachable!()
        };
        let z = b.fork(
            3,
            ForkSource::Commit {
                branch: x,
                commit: x1c,
            },
        );
        nodes.push(b.commit(z, None));
        nodes.push(b.commit(x, Some(NodeId::Layer(layers[0]))));
        nodes.push(b.commit(z, Some(x1)));
        layers.push(b.publish(x));
        let w = b.fork(4, ForkSource::Layer(layers[1]));
        nodes.push(b.commit(w, None));
        layers.push(b.publish(z));
        nodes.push(b.commit(y, None));
        layers.push(b.publish(y));
        nodes.extend(layers.iter().map(|l| NodeId::Layer(*l)));
        let before = answers(history, &nodes);
        let counts: Vec<u64> = layers
            .iter()
            .map(|l| history.layer(*l).unwrap().unwrap().overwrote_count)
            .collect();
        (nodes, before, counts)
    };
    make_old(&path);
    let handles = writable(&path);
    let history: &dyn HistoryCatalog = &handles.history;
    assert_eq!(answers(history, &nodes), before, "backfilled answers");
    let NodeId::Layer(top) = nodes[nodes.len() - 1] else {
        unreachable!()
    };
    let mut counts = Vec::new();
    for node in &nodes[nodes.len() - layers.len()..] {
        let NodeId::Layer(layer) = node else {
            unreachable!()
        };
        counts.push(history.layer(*layer).unwrap().unwrap().overwrote_count);
    }
    assert_eq!(counts, layers);
    // Publication continues on the backfilled lineage.
    let mut b = Builder { history, tag: 100 };
    let v = b.fork(5, ForkSource::Layer(top));
    let head = b.commit(v, None);
    let published = b.publish(v);
    assert!(history.is_ancestor(head, NodeId::Layer(published)).unwrap());
    drop(handles);
    reopen(&path);
}
