//! Exact overwrite reconstruction includes late publication and retained history.
mod support;
use layerfs_history::*;
use layerfs_persistence::{
    diagnostic::{Observation, Options, Outcome},
    PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;

struct Graph<'a> {
    h: &'a dyn HistoryCatalog,
    stack: LayerStackId,
    tag: u32,
}
impl Graph<'_> {
    fn fork(&self, n: u8, source: ForkSource) -> BranchId {
        let branch = support::branch_id(n);
        self.h
            .fork(&ForkRequest {
                stack: self.stack,
                branch,
                name: support::name(&format!("b{n}")),
                source,
            })
            .unwrap();
        branch
    }
    fn commit(&mut self, branch: BranchId, second: Option<NodeId>) -> CommitId {
        let s = self.h.branch_snapshot(branch).unwrap().unwrap();
        self.tag += 1;
        let result = self
            .h
            .commit_branch(&CommitRequest {
                branch,
                expected_head: s.branch.head_commit,
                expected_base: s.branch.base_layer,
                expected_root: s.effective_root,
                candidate_root: layerfs_content::ObjectId::for_bytes(&self.tag.to_le_bytes()),
                profile: s.profile,
                scope: s.scope,
                second_parent: second,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
            })
            .unwrap();
        match result {
            CommitOutcome::Committed { record, .. } => record.id,
            other => panic!("{other:?}"),
        }
    }
    fn publish(&self, branch: BranchId) -> LayerRecord {
        let b = self.h.branch(branch).unwrap().unwrap();
        let result = self
            .h
            .add_layer(&AddLayerRequest {
                stack: self.stack,
                branch,
                commit: b.head_commit.unwrap(),
                expected_stack_head: self.h.layer_stack(self.stack).unwrap().unwrap().head_layer,
                expected_branch_base: b.base_layer,
            })
            .unwrap();
        match result {
            AddLayerOutcome::Added { layer } => layer,
            other => panic!("{other:?}"),
        }
    }
}
fn diagnose(handles: &SqliteHandles) -> Vec<Observation> {
    let mut out = Vec::new();
    handles
        .diagnose(
            Options {
                quick: true,
                ..Options::default()
            },
            &mut |o| out.push(o),
        )
        .unwrap();
    out
}
fn initialize(h: &dyn HistoryCatalog, n: u8) -> LayerStackRecord {
    h.initialize_layerstack(&StackInitialization {
        logical: 0,
        stack: support::stack(n),
        name: support::name(&format!("s{n}")),
        scope: support::root(200),
        profile: support::root(201),
        genesis_root: support::root(0),
    })
    .unwrap()
}
#[test]
fn local_merges_late_publication_and_separate_stacks_have_exact_counts() {
    let temp = support::Temp::new("doctor-overwrite");
    let path = temp.join("db");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: support::BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    let h = &handles.history;
    let s = initialize(h, 1);
    let mut g = Graph {
        h,
        stack: s.id,
        tag: 0,
    };
    let x = g.fork(1, ForkSource::Layer(s.head_layer));
    let x1 = g.commit(x, None);
    let z = g.fork(
        2,
        ForkSource::Commit {
            branch: x,
            commit: x1,
        },
    );
    g.commit(z, None);
    let z_layer = g.publish(z);
    assert_eq!(z_layer.overwrote_count, 0);
    let w = g.fork(3, ForkSource::Layer(z_layer.id));
    let w1 = g.commit(w, None);
    // x1 was absorbed by z, but x is published only after w captured z.
    let late = g.publish(x);
    assert_eq!(late.overwrote_count, 1);
    let w_layer = g.publish(w);
    assert_eq!(w_layer.overwrote_count, 0);
    let y = g.fork(4, ForkSource::Layer(s.head_layer));
    g.commit(y, None);
    let skipped = g.publish(y);
    assert_eq!(skipped.overwrote_count, 3);
    g.commit(y, Some(NodeId::Commit(w1)));
    let merged = g.publish(y);
    assert_eq!(merged.overwrote_count, 0);
    // A retained first-parent fork starts another run, and merges a Layer.
    let retained = g.fork(
        5,
        ForkSource::Commit {
            branch: x,
            commit: x1,
        },
    );
    g.commit(retained, Some(NodeId::Layer(merged.id)));
    let retained_layer = g.publish(retained);
    assert_eq!(retained_layer.overwrote_count, 0);
    // Same heights on a separate stack must not contribute publication reach.
    let second = initialize(h, 2);
    let mut g2 = Graph {
        h,
        stack: second.id,
        tag: 100,
    };
    let b = g2.fork(6, ForkSource::Layer(second.head_layer));
    g2.commit(b, None);
    let separate = g2.publish(b);
    assert_eq!(separate.overwrote_count, 0);
    let clean = diagnose(&handles);
    assert!(
        !clean.iter().any(|o| o.outcome == Outcome::Fail),
        "{clean:?}"
    );
    assert!(
        !clean
            .iter()
            .any(|o| o.id == "store.history.overwrite.coverage" && o.outcome == Outcome::Unknown),
        "{clean:?}"
    );
    assert!(
        clean
            .iter()
            .any(|o| o.id == "store.history.lineage.summary" && o.outcome == Outcome::Pass),
        "{clean:?}"
    );
    let raw = rusqlite::Connection::open(&path).unwrap();
    let populated: i64 = raw
        .query_row(
            "SELECT count(*) FROM layer WHERE length(overwrote)!=0",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        populated, 0,
        "modern publications deliberately omit legacy lists"
    );
    // Corrupt only the derived count, keeping it within its accepted range.
    // Each Layer is checked even when old publications have been overwritten.
    for layer in [
        z_layer,
        late,
        w_layer,
        skipped,
        merged,
        retained_layer,
        separate,
    ] {
        let wrong = if layer.overwrote_count == 0 { 1 } else { 0 };
        raw.execute(
            "UPDATE layer_lineage SET overwrote_count=?1 WHERE layer_id=?2",
            rusqlite::params![wrong, layer.id.as_slice()],
        )
        .unwrap();
        let damaged = diagnose(&handles);
        assert!(
            damaged
                .iter()
                .any(|o| o.id == "store.history.overwrite" && o.outcome == Outcome::Fail),
            "{damaged:?}"
        );
        raw.execute(
            "UPDATE layer_lineage SET overwrote_count=?1 WHERE layer_id=?2",
            rusqlite::params![layer.overwrote_count as i64, layer.id.as_slice()],
        )
        .unwrap();
    }
    assert!(!diagnose(&handles)
        .iter()
        .any(|o| o.outcome == Outcome::Fail));
}

#[test]
fn cancellation_interrupts_large_scratch_publication_analysis_and_joins() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};
    let temp = support::Temp::new("doctor-overwrite-cancel");
    let path = temp.join("db");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: support::BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    let s = initialize(&handles.history, 1);
    let mut g = Graph {
        h: &handles.history,
        stack: s.id,
        tag: 0,
    };
    let branch = g.fork(1, ForkSource::Layer(s.head_layer));
    for _ in 0..4096 {
        g.commit(branch, None);
    }
    g.publish(branch);
    let cancelled = AtomicBool::new(false);
    std::thread::scope(|scope| {
        let mut started = None;
        let mut trigger = None;
        let mut output = Vec::new();
        let result = handles.diagnose(
            Options {
                quick: true,
                deadline: None,
                cancelled: Some(&cancelled),
            },
            &mut |o| {
                if o.id == "store.history.lineage.graph" {
                    started = Some(Instant::now());
                    trigger = Some(scope.spawn(|| {
                        std::thread::sleep(Duration::from_millis(1));
                        cancelled.store(true, Ordering::Relaxed);
                    }));
                }
                output.push(o);
            },
        );
        trigger
            .expect("entered independent scratch analysis")
            .join()
            .unwrap();
        assert!(result.is_err(), "{output:?}");
        assert!(started.unwrap().elapsed() < Duration::from_secs(2));
        assert!(!output
            .iter()
            .any(|o| o.id == "store.history.lineage.summary" && o.outcome == Outcome::Pass));
        assert!(
            !output.iter().any(|o| o.outcome == Outcome::Fail),
            "interruption is not corruption: {output:?}"
        );
    });
    let began = Instant::now();
    let mut output = Vec::new();
    let result = handles.diagnose(
        Options {
            quick: true,
            deadline: Some(began + Duration::from_secs(2)),
            cancelled: None,
        },
        &mut |o| output.push(o),
    );
    assert!(result.is_err(), "{output:?}");
    assert!(began.elapsed() < Duration::from_secs(3));
    assert!(
        !output.iter().any(|o| o.outcome == Outcome::Fail),
        "deadline is not corruption: {output:?}"
    );
    assert!(!output
        .iter()
        .any(|o| o.id == "store.history.lineage.summary" && o.outcome == Outcome::Pass));
}
