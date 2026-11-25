//! Independent #768 check (not the author's tests): ids are never reused
//! under seeded random interleavings of mount, Commit and unmount of two or
//! three Workspaces on one Store service, on SQLite and on PostgreSQL.
//!
//! Each cycle mounts every Workspace in a random order, runs one to three
//! rounds in which a random subset writes a change (usually a few files,
//! sometimes 130 to 230, more values than a handle keeps) and commits,
//! either in a random sequential order or from threads released at the same
//! instant, then unmounts in a random order (so the Store's idle slot is
//! found taken by whichever closes later, older or newer handle). After a
//! random third of the cycles the service is closed and a new one opened.
//! In the shared-Branch variant two Workspaces share a Branch, so threaded
//! Commits race and merge (fused refusal, fallback merge Save).
//!
//! Asserted: every Commit answers `Committed` (a reused ordinal would be
//! refused by the publication's stored-block check), every Commit id is
//! new; after every round no overlapping value block, duplicated locator or
//! id past its counter; at every closed point (no service open) every gap
//! lost at an earlier closed point is still unused (`support/verify768y.rs`);
//! at the end each Branch's chain has exactly the answered Commits and its
//! tree holds every committed file. Lost ids are reported, not asserted
//! (the documented bound for three Workspaces is not zero).
//!
//! The PostgreSQL tests need `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
#[path = "support/adversary.rs"]
#[allow(dead_code)]
mod adversary;
#[path = "support/verify768.rs"]
#[allow(dead_code)]
mod verify768;
#[path = "support/verify768x.rs"]
#[allow(dead_code)]
mod verify768x;
#[path = "support/verify768y.rs"]
#[allow(dead_code)]
mod verify768y;

use adversary::{
    apply, commit, seed_tree, verdict, write, Gate, Node, Op, Service, TestStore, Tree, Verdict,
};
use layerfs_history::{BranchId, CommitId};
use layerfs_store_service::store::BoundWorkspace;
use std::collections::HashSet;
use verify768x::Rng;
use verify768y::Lost;

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];

fn shuffle(items: &mut [usize], rng: &mut Rng) {
    for i in (1..items.len()).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        items.swap(i, j);
    }
}

fn close(service: Service) {
    service.service.close().expect("an orderly close");
    service.owner.stop().expect("the overlay owner stops");
}

type Change = Vec<(String, Vec<u8>)>;

fn make_change(workspace: &BoundWorkspace, dir: &str, files: u64, tag: &str) -> Change {
    apply(workspace, &Op::Mkdir(dir.to_owned())).unwrap();
    let mut change = vec![(dir.to_owned(), Vec::new())];
    for k in 0..files {
        let path = format!("{dir}/f{k}");
        let bytes = format!("{tag} {k}\n").into_bytes();
        write(workspace, &path, &bytes);
        change.push((path, bytes));
    }
    change
}

fn record(tree: &mut Tree, change: &Change) {
    let (dir, _) = &change[0];
    tree.insert(dir.clone(), Node::Dir);
    for (path, bytes) in &change[1..] {
        tree.insert(path.clone(), Node::File(bytes.clone()));
    }
}

/// One seeded run. `shared`: Workspaces 0 and 1 share Branch 0.
fn scenario(store: &TestStore, seed: u64, w: usize, cycles: usize, shared: bool) {
    let kind = store.kind;
    let mut rng = Rng(seed);
    let branch_count = if shared { w - 1 } else { w };
    let branch_of = |i: usize| if shared && i > 0 { i - 1 } else { i };
    let branches: Vec<BranchId> = (0..branch_count)
        .map(|b| store.fork(&format!("s{seed:x}b{b}")))
        .collect();
    let mut expected: Vec<Tree> = (0..branch_count).map(|_| seed_tree(SEED)).collect();
    let mut answered = vec![0usize; branch_count];
    let mut ids: HashSet<CommitId> = HashSet::new();
    let mut lost = Lost::default();
    let (start_ord, start_pack) = lost.closed_point(store, "after Init");
    let mut service: Option<Service> = None;
    let mut commits = 0usize;
    let mut threaded = 0usize;
    let mut closes = 0usize;
    for cycle in 0..cycles {
        let svc = service.get_or_insert_with(|| store.open());
        let mut order: Vec<usize> = (0..w).collect();
        shuffle(&mut order, &mut rng);
        let mut bound: Vec<Option<BoundWorkspace>> = (0..w).map(|_| None).collect();
        for &i in &order {
            bound[i] = Some(svc.bind(branches[branch_of(i)]));
        }
        let rounds = 1 + rng.below(3) as usize;
        for round in 0..rounds {
            let mut changes: Vec<(usize, Change)> = Vec::new();
            for (i, workspace) in bound.iter().enumerate() {
                if rng.below(5) == 0 {
                    continue;
                }
                let files = if rng.below(6) == 0 {
                    130 + rng.below(100)
                } else {
                    1 + rng.below(12)
                };
                let dir = format!("/w{i}c{cycle}r{round}");
                let tag = format!("{seed:x} {i} {cycle} {round}");
                changes.push((
                    i,
                    make_change(workspace.as_ref().unwrap(), &dir, files, &tag),
                ));
            }
            if changes.is_empty() {
                continue;
            }
            let concurrent = changes.len() > 1 && rng.below(2) == 0;
            let label = format!(
                "{kind:?} seed {seed:x} cycle {cycle} round {round} ({})",
                if concurrent { "threads" } else { "sequential" }
            );
            let verdicts: Vec<(usize, Verdict)> = if concurrent {
                threaded += 1;
                let gate = Gate::new(changes.len());
                std::thread::scope(|scope| {
                    let handles: Vec<_> = changes
                        .iter()
                        .map(|(i, _)| {
                            let workspace = bound[*i].as_ref().unwrap();
                            let gate = &gate;
                            let i = *i;
                            scope.spawn(move || {
                                assert!(gate.wait());
                                (i, verdict(&commit(workspace)))
                            })
                        })
                        .collect();
                    handles.into_iter().map(|h| h.join().unwrap()).collect()
                })
            } else {
                let mut picks: Vec<usize> = (0..changes.len()).collect();
                shuffle(&mut picks, &mut rng);
                picks
                    .into_iter()
                    .map(|p| {
                        let i = changes[p].0;
                        (i, verdict(&commit(bound[i].as_ref().unwrap())))
                    })
                    .collect()
            };
            for (i, answer) in verdicts {
                let Verdict::Committed(id) = answer else {
                    panic!("{label}: Workspace {i} answered {answer:?}");
                };
                assert!(ids.insert(id), "{label}: Commit id {id:?} answered twice");
                let change = &changes.iter().find(|(j, _)| *j == i).unwrap().1;
                record(&mut expected[branch_of(i)], change);
                answered[branch_of(i)] += 1;
                commits += 1;
            }
            lost.check(store, &label);
        }
        let mut order: Vec<usize> = (0..w).collect();
        shuffle(&mut order, &mut rng);
        for i in order {
            svc.unmount(bound[i].take().unwrap());
        }
        lost.check(store, &format!("{kind:?} seed {seed:x} cycle {cycle} unmounted"));
        if rng.below(3) == 0 || cycle + 1 == cycles {
            close(service.take().unwrap());
            closes += 1;
            lost.closed_point(store, &format!("{kind:?} seed {seed:x} cycle {cycle} closed"));
        }
    }
    for (b, branch) in branches.iter().enumerate() {
        let chain = store.chain(*branch);
        assert_eq!(
            chain.len(),
            answered[b],
            "{kind:?} seed {seed:x}: Branch {b}'s chain is the answered Commits"
        );
        assert!(chain.iter().all(|r| ids.contains(&r.id)));
        assert_eq!(
            store.tree(*branch),
            expected[b],
            "{kind:?} seed {seed:x}: Branch {b}'s tree reads back"
        );
    }
    let (lost_ord, lost_pack) = lost.closed_point(store, "after the read-back");
    let audit = store.integrity_failures();
    assert!(audit.is_empty(), "{kind:?} seed {seed:x}: audit {audit:#?}");
    println!(
        "Y_INTERLEAVE {{\"kind\":\"{kind:?}\",\"seed\":\"{seed:x}\",\"workspaces\":{w},\"shared\":{shared},\"cycles\":{cycles},\"commits\":{commits},\"threaded_rounds\":{threaded},\"closes\":{closes},\"lost_ordinals\":{},\"lost_pack_ids\":{},\"final\":\"{:?}\"}}",
        lost_ord - start_ord,
        lost_pack - start_pack,
        verify768x::alloc(store)
    );
}

#[test]
fn sqlite_two_workspaces() {
    adversary::tmp();
    let store = TestStore::sqlite("y-int-s2", SEED);
    scenario(&store, 0x768a_0001, 2, 12, false);
}

#[test]
fn sqlite_three_workspaces_shared_branch() {
    adversary::tmp();
    let store = TestStore::sqlite("y-int-s3", SEED);
    scenario(&store, 0x768a_0002, 3, 12, true);
}

#[test]
fn sqlite_three_workspaces() {
    adversary::tmp();
    let store = TestStore::sqlite("y-int-s3b", SEED);
    scenario(&store, 0x768a_0003, 3, 12, false);
}

#[test]
fn postgres_two_workspaces() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-int-p2", SEED) else {
        return;
    };
    scenario(&store, 0x768a_0001, 2, 12, false);
}

#[test]
fn postgres_three_workspaces_shared_branch() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-int-p3", SEED) else {
        return;
    };
    scenario(&store, 0x768a_0002, 3, 12, true);
}

#[test]
fn postgres_three_workspaces() {
    adversary::tmp();
    let Some(store) = TestStore::postgres("y-int-p3b", SEED) else {
        return;
    };
    scenario(&store, 0x768a_0003, 3, 12, false);
}
