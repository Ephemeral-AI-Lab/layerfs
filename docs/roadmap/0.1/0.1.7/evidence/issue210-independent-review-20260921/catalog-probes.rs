mod support;
use layerfs_history::*;
use support::*;

fn init(c: &dyn HistoryCatalog, n: u8, text: &str) -> LayerStackRecord {
    c.initialize_layerstack(&StackInitialization { stack: stack(n), name: name(text), scope: root(200), profile: root(201), genesis_root: root(0) }).unwrap()
}
fn fork(c: &dyn HistoryCatalog, s: &LayerStackRecord, n: u8, base: LayerId) -> BranchSnapshot {
    c.fork(&ForkRequest { stack: s.id, branch: branch_id(n), name: name(&format!("b{n}")), source: ForkSource::Layer(base) }).unwrap()
}
fn append(c: &dyn HistoryCatalog, b: BranchId, n: u8) -> CommitRecord {
    let s=c.branch_snapshot(b).unwrap().unwrap();
    let stage=c.stage_changes(&StageRequest { workspace: workspace(n.max(1)), branch:b, expected_head:s.branch.head_commit, expected_base:s.branch.base_layer, expected_root:s.effective_root, construction_base_root:s.effective_root, intended_commit_base:s.branch.base_layer, candidate_root:root(n), profile:s.profile, scope:s.scope, generation:1 }).unwrap();
    match c.commit_staged(&CommitStagedRequest {workspace:stage.workspace,token:stage.token}).unwrap() { CommitStagedOutcome::Committed(c)=>c, _=>panic!("new commit expected") }
}
fn publish(c:&dyn HistoryCatalog,s:&LayerStackRecord,b:BranchId,k:&CommitRecord,head:LayerId)->Result<AddLayerOutcome,HistoryError> {
    c.add_layer(&AddLayerRequest {stack:s.id,branch:b,commit:k.id,expected_stack_head:head,expected_branch_base:k.base_layer})
}

#[test]
fn review_long_names_cannot_be_paginated() {
    let t=Temp::new("review-long-names"); let c=create(&t.join("db"));
    let s=init(&c,1,&"a".repeat(34)); init(&c,2,"z");
    let e=c.layer_stacks(&Page{cursor:None,limit:1}).unwrap_err();
    println!("long stack name: {e:?}"); assert_eq!(e,HistoryError::InvalidInput("page cursor field"));
    for (n,text) in [(1,"a".repeat(34)),(2,"z".into())] {
        c.fork(&ForkRequest{stack:s.id,branch:branch_id(n),name:name(&text),source:ForkSource::Layer(s.head_layer)}).unwrap();
    }
    let e=c.branches(s.id,&Page{cursor:None,limit:1}).unwrap_err();
    println!("long branch name: {e:?}"); assert_eq!(e,HistoryError::InvalidInput("page cursor field"));
}

#[test]
fn review_live_growth_rejects_default_start_cursors() {
    let t=Temp::new("review-growth");let c=create(&t.join("db"));let s=init(&c,1,"main");let b=fork(&c,&s,1,s.head_layer);
    append(&c,b.branch.id,1);let k2=append(&c,b.branch.id,2);
    let p=c.commit_history(&CommitHistoryRequest{branch:b.branch.id,start:None,cursor:None,limit:1}).unwrap();
    append(&c,b.branch.id,3);
    let e=c.commit_history(&CommitHistoryRequest{branch:b.branch.id,start:None,cursor:p.continuation.clone(),limit:1}).unwrap_err();
    println!("live Commit cursor: {e:?}");assert_eq!(e,HistoryError::InvalidInput("page cursor anchor"));
    assert!(c.commit_history(&CommitHistoryRequest{branch:b.branch.id,start:Some(k2.id),cursor:p.continuation,limit:1}).is_ok());
    let k3=c.commit(c.branch(b.branch.id).unwrap().unwrap().head_commit.unwrap()).unwrap().unwrap();
    let l1=match publish(&c,&s,b.branch.id,&k3,s.head_layer).unwrap(){AddLayerOutcome::Added(l)=>l,_=>panic!()};
    let p=c.layer_history(&LayerHistoryRequest{stack:s.id,start:None,cursor:None,limit:1}).unwrap();
    let b2=fork(&c,&s,2,l1.id);let k4=append(&c,b2.branch.id,4);publish(&c,&s,b2.branch.id,&k4,l1.id).unwrap();
    let e=c.layer_history(&LayerHistoryRequest{stack:s.id,start:None,cursor:p.continuation,limit:1}).unwrap_err();
    println!("live Layer cursor: {e:?}");assert_eq!(e,HistoryError::InvalidInput("page cursor anchor"));
}

#[test]
fn review_rehashed_cursor_escapes_branch_ancestry() {
    let t=Temp::new("review-forge");let c=create(&t.join("db"));let s=init(&c,1,"main");let a=fork(&c,&s,1,s.head_layer);let b=fork(&c,&s,2,s.head_layer);
    let a1=append(&c,a.branch.id,1);let a2=append(&c,a.branch.id,2);let b1=append(&c,b.branch.id,3);let b2=append(&c,b.branch.id,4);
    let mut p=c.commit_history(&CommitHistoryRequest{branch:a.branch.id,start:Some(a2.id),cursor:None,limit:1}).unwrap().continuation.unwrap();
    p[110]=33;p[111..144].copy_from_slice(b2.id.as_slice());
    let mut h=blake3::Hasher::new();h.update(b"layerfs/history/cursor/v1\0");h.update(&p[..144]);p[144..160].copy_from_slice(&h.finalize().as_bytes()[..16]);
    let result=c.commit_history(&CommitHistoryRequest{branch:a.branch.id,start:Some(a2.id),cursor:Some(p),limit:1}).unwrap();
    println!("forged cursor returns sibling parent: {}", result.records[0].id==b1.id);
    assert_eq!(result.records[0].id,b1.id);assert_ne!(result.records[0].id,a1.id);
}

#[test]
fn review_terminal_reservation_acknowledges_unrepresentable_end() {
    let t=Temp::new("review-terminal");let path=t.join("db");let c=create(&path);let scope=root(200);
    c.reserve_inodes(&ReserveRequest{scope,count:1}).unwrap();
    let sql=rusqlite::Connection::open(&path).unwrap();
    sql.execute("UPDATE scope_allocator SET highwater=?1",[i64::MAX-1]).unwrap();
    let r=c.reserve_inodes(&ReserveRequest{scope,count:1}).unwrap();
    println!("reserve succeeds: start={} count={} end={:?}",r.start,r.count,r.end());
    assert_eq!(r.start,i64::MAX as u64);assert!(r.end().is_err());
}

#[test]
fn review_open_accepts_missing_constraint_and_bad_binding_row() {
    let t=Temp::new("review-open");let path=t.join("db");let c=create(&path);drop(c);
    let sql=rusqlite::Connection::open(&path).unwrap();sql.execute_batch("DROP INDEX layers_child; UPDATE history_meta SET binding_key=X'78';").unwrap();drop(sql);
    let reopened=sqlite::open_read_only(&path,BINDING);
    println!("open with missing layers_child index and changed binding_key: {}",reopened.is_ok());assert!(reopened.is_ok());
}

#[test]
fn review_refreshed_stack_token_is_not_a_rebase() {
    let t=Temp::new("review-base");let c=create(&t.join("db"));let s=init(&c,1,"main");let b=fork(&c,&s,1,s.head_layer);
    let k1=append(&c,b.branch.id,1);let l1=match publish(&c,&s,b.branch.id,&k1,s.head_layer).unwrap(){AddLayerOutcome::Added(l)=>l,_=>panic!()};
    let k2=append(&c,b.branch.id,2);let e=publish(&c,&s,b.branch.id,&k2,l1.id).unwrap_err();
    println!("stale base with refreshed stack head: {e:?}");assert_eq!(e,HistoryError::Integrity("catalog constraint"));
    let k0=append(&c,b.branch.id,0);let result=publish(&c,&s,b.branch.id,&k0,l1.id).unwrap();
    println!("stale base with base-equal root: {result:?}");assert!(matches!(result,AddLayerOutcome::NoChanges{..}));
}

#[test]
fn review_publication_provenance_conflict_is_integrity() {
    let t=Temp::new("review-provenance");let c=create(&t.join("db"));let s=init(&c,1,"main");let a=fork(&c,&s,1,s.head_layer);let b=fork(&c,&s,2,s.head_layer);
    let ka=append(&c,a.branch.id,1);let kb=append(&c,b.branch.id,1);assert_eq!(ka,kb);
    let l=match publish(&c,&s,a.branch.id,&ka,s.head_layer).unwrap(){AddLayerOutcome::Added(l)=>l,_=>panic!()};
    let e=publish(&c,&s,b.branch.id,&kb,l.id).unwrap_err();println!("same LayerId, different source Branch: {e:?}");assert_eq!(e,HistoryError::Integrity("Layer provenance"));
    assert_eq!(c.layer(l.id).unwrap().unwrap(),l);
}

#[test]
fn review_concurrent_commit_has_one_winner_and_retained_loser() {
    use std::sync::{Arc,Barrier};
    let t=Temp::new("review-cas");let c=Arc::new(create(&t.join("db")));let s=init(c.as_ref(),1,"main");let b=fork(c.as_ref(),&s,1,s.head_layer);
    let mut stages=vec![];
    for n in 1..=2 {stages.push(c.stage_changes(&StageRequest{workspace:workspace(n),branch:b.branch.id,expected_head:None,expected_base:s.head_layer,expected_root:root(0),construction_base_root:root(0),intended_commit_base:s.head_layer,candidate_root:root(n),profile:s.profile,scope:s.scope,generation:1}).unwrap());}
    let barrier=Arc::new(Barrier::new(2));
    let outcomes=std::thread::scope(|threads| {let mut joins=vec![];for stage in &stages {let c=Arc::clone(&c);let barrier=Arc::clone(&barrier);joins.push(threads.spawn(move||{barrier.wait();c.commit_staged(&CommitStagedRequest{workspace:stage.workspace,token:stage.token})}));}joins.into_iter().map(|j|j.join().unwrap()).collect::<Vec<_>>()});
    assert_eq!(outcomes.iter().filter(|r|r.is_ok()).count(),1);
    for (stage,result) in stages.iter().zip(&outcomes) {if result.is_err(){assert_eq!(c.stage(stage.workspace).unwrap().as_ref(),Some(stage));}}
    println!("concurrent Commit results: {outcomes:?}");
}
