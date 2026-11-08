//! B3 authoritative history and independently damaged derived families.
mod support;
use layerfs_content::{encode_whole_file_payload, ObjectRole};
use layerfs_history::*;
use support::doctor::{clean, failure, Fixture};
fn init(f: &Fixture, n: u8) -> LayerStackRecord {
    f.handles
        .history
        .initialize_layerstack(&StackInitialization {
            stack: support::stack(n),
            name: support::name(&format!("s{n}")),
            scope: support::root(80),
            profile: support::root(81),
            genesis_root: support::root(n),
            logical: 0,
        })
        .unwrap()
}
fn fork(f: &Fixture, s: LayerStackId, n: u8, source: ForkSource) -> BranchId {
    let b = support::branch_id(n);
    f.handles
        .history
        .fork(&ForkRequest {
            stack: s,
            branch: b,
            name: support::name(&format!("b{n}")),
            source,
        })
        .unwrap();
    b
}
fn commit(f: &Fixture, b: BranchId, n: u8, second: Option<NodeId>) -> CommitRecord {
    let s = f.handles.history.branch_snapshot(b).unwrap().unwrap();
    let CommitOutcome::Committed { record, .. } = f
        .handles
        .history
        .commit_branch(&CommitRequest {
            branch: b,
            expected_head: s.branch.head_commit,
            expected_base: s.branch.base_layer,
            expected_root: s.effective_root,
            candidate_root: support::root(n),
            profile: s.profile,
            scope: s.scope,
            second_parent: second,
            force: false,
            meta: CommitMeta::new(CommitVia::Files),
        })
        .unwrap()
    else {
        panic!("commit");
    };
    *record
}
fn publish(f: &Fixture, s: LayerStackId, b: BranchId) -> LayerRecord {
    let branch = f.handles.history.branch(b).unwrap().unwrap();
    let AddLayerOutcome::Added { layer } = f
        .handles
        .history
        .add_layer(&AddLayerRequest {
            stack: s,
            branch: b,
            commit: branch.head_commit.unwrap(),
            expected_stack_head: f
                .handles
                .history
                .layer_stack(s)
                .unwrap()
                .unwrap()
                .head_layer,
            expected_branch_base: branch.base_layer,
        })
        .unwrap()
    else {
        panic!("publish");
    };
    layer
}
fn graph() -> Fixture {
    let f = Fixture::new();
    let s = init(&f, 1);
    let a = fork(&f, s.id, 1, ForkSource::Layer(s.head_layer));
    let c = commit(&f, a, 10, None);
    let b = fork(
        &f,
        s.id,
        2,
        ForkSource::Commit {
            branch: a,
            commit: c.id,
        },
    );
    commit(&f, b, 11, None);
    publish(&f, s.id, b);
    commit(
        &f,
        a,
        12,
        Some(NodeId::Commit(
            f.handles
                .history
                .branch(b)
                .unwrap()
                .unwrap()
                .head_commit
                .unwrap(),
        )),
    );
    publish(&f, s.id, a);
    clean(&f.audit(true));
    f
}
#[test]
fn each_derived_family_is_independently_reconstructed() {
    for sql in [
 "UPDATE layer_lineage SET height=height+20",
 "UPDATE commit_lineage SET depth=depth+20",
 "UPDATE commit_lineage SET newest=newest+20",
 "UPDATE commit_lineage SET run=CAST(X'12'||zeroblob(32) AS BLOB) WHERE commit_id=(SELECT min(commit_id) FROM commit_lineage)",
 "UPDATE commit_lineage SET merged=NULL WHERE merged IS NOT NULL",
 "DELETE FROM absorbed",
 "DELETE FROM reach",
 ]{let f=graph();f.raw().execute_batch(sql).unwrap();let out=f.audit(true);assert!(out.iter().any(|o|o.id.starts_with("store.history.lineage")&&o.outcome==layerfs_persistence::diagnostic::Outcome::Fail),"{sql}: {out:?}");}
}
#[test]
fn missing_cross_stack_heads_bases_and_identity_damage_are_attributable() {
    for sql in [
 "UPDATE layer_stack SET head_layer_id=CAST(X'32'||zeroblob(32) AS BLOB)",
 "UPDATE branch SET base_layer_id=CAST(X'32'||zeroblob(32) AS BLOB)",
 "UPDATE branch SET head_commit_id=CAST(X'12'||zeroblob(32) AS BLOB)",
 "UPDATE layer_stack SET head_layer_id=(SELECT head_layer_id FROM layer_stack WHERE name='s2') WHERE name='s1'",
 "UPDATE branch SET base_layer_id=(SELECT head_layer_id FROM layer_stack WHERE name='s2')",
 "UPDATE \"commit\" SET root_id=zeroblob(32)",
 "UPDATE layer SET root_id=zeroblob(32)",
 ]{let f=graph();init(&f,2);clean(&f.audit(true));f.raw().execute_batch(&format!("PRAGMA foreign_keys=OFF; {sql}")).unwrap();let out=f.audit(true);assert!(out.iter().any(|o|o.id.starts_with("store.history.")&&o.outcome==layerfs_persistence::diagnostic::Outcome::Fail),"{sql}: {out:?}");}
}
#[test]
fn layer_provenance_and_missing_or_cyclic_commit_parents_are_not_silent() {
    for sql in [
 "UPDATE layer SET source_commit_id=(SELECT commit_id FROM \"commit\" ORDER BY commit_id LIMIT 1) WHERE parent_layer_id IS NOT NULL",
 "UPDATE \"commit\" SET parent_commit_id=CAST(X'12'||zeroblob(32) AS BLOB)",
 "UPDATE \"commit\" SET parent_commit_id=commit_id",
 "UPDATE \"commit\" AS c SET parent_commit_id=(SELECT commit_id FROM \"commit\" other WHERE other.commit_id!=c.commit_id ORDER BY commit_id LIMIT 1)",
 "UPDATE \"commit\" SET parent_commit_id=(SELECT c.commit_id FROM \"commit\" c JOIN layer_stack s ON s.layer_stack_id=c.layer_stack_id WHERE s.name='s2' LIMIT 1) WHERE layer_stack_id=(SELECT layer_stack_id FROM layer_stack WHERE name='s1')",
 "UPDATE \"commit\" SET base_layer_id=(SELECT head_layer_id FROM layer_stack WHERE name='s2')",
 ]{let f=graph();let other=init(&f,2);let branch=fork(&f,other.id,3,ForkSource::Layer(other.head_layer));commit(&f,branch,40,None);clean(&f.audit(true));f.raw().execute_batch(&format!("PRAGMA foreign_keys=OFF; {sql}")).unwrap();let out=f.audit(true);assert!(out.iter().any(|o|o.id.starts_with("store.history.")&&o.outcome==layerfs_persistence::diagnostic::Outcome::Fail),"{sql}: {out:?}");if sql.contains("parent_commit_id=commit_id")||sql.contains("AS c SET parent_commit_id"){failure(&out,"store.history.cycles");}
if sql.contains("parent_commit_id=CAST")||sql.contains("base_layer_id=(SELECT"){failure(&out,"store.history.references");}
if sql.contains("source_commit_id"){failure(&out,"store.history.provenance");}}
}
#[test]
fn valid_forced_history_and_removed_project_retained_objects_pass() {
    let f = graph();
    let b = support::branch_id(1);
    let s = f.handles.history.branch_snapshot(b).unwrap().unwrap();
    let stale = s.clone();
    commit(&f, b, 30, None);
    let result = f
        .handles
        .history
        .commit_branch(&CommitRequest {
            branch: b,
            expected_head: stale.branch.head_commit,
            expected_base: stale.branch.base_layer,
            expected_root: stale.effective_root,
            candidate_root: support::root(31),
            profile: stale.profile,
            scope: stale.scope,
            second_parent: None,
            force: true,
            meta: CommitMeta::new(CommitVia::Files),
        })
        .unwrap();
    assert!(matches!(result, CommitOutcome::Committed { .. }));
    clean(&f.audit(true));
    f.put(
        ObjectRole::WholeFile,
        encode_whole_file_payload(b"retained after project removal").unwrap(),
    );
    f.handles.history.remove_layerstack(s.branch.stack).unwrap();
    let out = f.audit(false);
    clean(&out);
    assert!(out
        .iter()
        .any(|o| o.id == "store.objects.summary" && o.message.starts_with("1 winning")));
}
