//! #818 H4: `first_parent_window` answers what its default loop reads, one
//! Commit and one serial at a time, on random histories with forks from
//! the middle of a run, merges, publications (Layer boundaries) and removed
//! Branches. SQLite runs the default itself; PostgreSQL answers in one
//! statement. The PostgreSQL half reports NOT_RUN without
//! `LAYERFS_TEST_POSTGRES_URL`.
#![cfg(any(target_os = "macos", target_os = "linux"))]

#[path = "../support/mod.rs"]
mod support;

use layerfs_content::ObjectId;
use layerfs_history::catalog::first_parent_window_serial;
use layerfs_history::records::{CommitOutcome, ForkRequest, ForkSource};
use layerfs_history::{BranchId, CommitId, HistoryCatalog, HistoryCatalogConfig, NodeId};

const BINDING: &[u8] = b"first-parent-window";
const CURSOR: [u8; 32] = [53; 32];

/// The window sizes compared, the store service's among them.
const COUNTS: [usize; 6] = [0, 1, 3, 16, 32, 400];

/// A small deterministic generator.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// What one random history did, so the test knows it covered each shape.
#[derive(Default, Debug)]
struct Shapes {
    commits: Vec<CommitId>,
    mid_forks: usize,
    merges: usize,
    publishes: usize,
    removals: usize,
}

fn branch(n: u16) -> BranchId {
    let mut body = [0x40; 16];
    body[..2].copy_from_slice(&n.to_be_bytes());
    BranchId::from_authority(body)
}

/// A history of `steps` random operations on project 1.
fn history_of(history: &dyn HistoryCatalog, seed: u64, steps: usize) -> Shapes {
    let mut random = Random(seed);
    let (stack, work, _) = support::pg::project(history, 1);
    let mut active = vec![work];
    let mut shapes = Shapes::default();
    let mut roots = 0u64;
    let mut names = 0u16;
    for _ in 0..steps {
        let at = random.below(active.len());
        let on = active[at];
        let record = history.branch(on).unwrap().expect("an active Branch");
        match random.below(10) {
            // A fork from the head or an older Commit of the Branch.
            0 | 1 => {
                let mut source = record.head_commit;
                let back = random.below(4);
                for _ in 0..back {
                    let older = source.and_then(|id| history.commit(id).unwrap().unwrap().parent);
                    if older.is_none() {
                        break;
                    }
                    shapes.mid_forks += 1;
                    source = older;
                }
                names += 1;
                let new = branch(names);
                history
                    .fork(&ForkRequest {
                        stack,
                        branch: new,
                        name: support::name(&format!("b{names}")),
                        source: match source {
                            Some(commit) => ForkSource::Commit { branch: on, commit },
                            None => ForkSource::Layer(record.base_layer),
                        },
                    })
                    .unwrap();
                active.push(new);
            }
            // A publication, where the Branch is on the stack's head.
            2 => {
                let head = history.layer_stack(stack).unwrap().unwrap().head_layer;
                if record.head_commit.is_some() && record.base_layer == head {
                    support::pg::publish(history, stack, on);
                    shapes.publishes += 1;
                }
            }
            // A removed Branch (never the last one).
            3 if active.len() > 1 => {
                history.mark_branch_removed(on, 1_000).unwrap();
                active.remove(at);
                shapes.removals += 1;
            }
            // A Commit, a merge one time in four.
            _ => {
                roots += 1;
                let candidate =
                    ObjectId::for_bytes(&[b"fpw/".as_slice(), &roots.to_be_bytes()].concat());
                let mut request = support::pg::commit_request(history, on, candidate);
                if !shapes.commits.is_empty() && random.below(4) == 0 {
                    let other = shapes.commits[random.below(shapes.commits.len())];
                    if Some(other) != record.head_commit {
                        request.second_parent = Some(NodeId::Commit(other));
                        shapes.merges += 1;
                    }
                }
                match history.commit_branch(&request).unwrap() {
                    CommitOutcome::Committed { record, .. } => shapes.commits.push(record.id),
                    other => panic!("commit: {other:?}"),
                }
            }
        }
    }
    shapes
}

/// Every Commit's windows equal the default's, and a missing Commit's
/// window is empty.
fn check(history: &dyn HistoryCatalog, shapes: &Shapes) {
    assert!(
        shapes.mid_forks > 0 && shapes.merges > 0 && shapes.publishes > 0 && shapes.removals > 0,
        "every shape is covered: {shapes:?}"
    );
    let mut longest = 0;
    for &id in &shapes.commits {
        for count in COUNTS {
            let window = history.first_parent_window(id, count).expect("window");
            let serial = first_parent_window_serial(history, id, count).expect("default");
            assert_eq!(window, serial, "{id} count {count}");
            assert!(window.len() <= count);
            longest = longest.max(window.len());
        }
    }
    assert!(longest > 16, "a window crosses runs: {longest}");
    let mut absent = [0x5a; 33];
    absent[0] = 0x12;
    let absent = CommitId::from_bytes(absent).expect("a Commit id");
    assert!(history.first_parent_window(absent, 16).unwrap().is_empty());
}

#[test]
fn sqlite_window_equals_the_default() {
    let temp = support::Temp::new("fpw");
    let history = support::sqlite::create(
        &temp.join("store.sqlite"),
        &HistoryCatalogConfig {
            cursor_key: CURSOR,
            binding_key: BINDING.to_vec(),
            incarnation: 3,
        },
    )
    .expect("catalog");
    let shapes = history_of(&history, 0x9e37_79b9_7f4a_7c15, 160);
    check(&history, &shapes);
}

#[test]
fn postgresql_window_equals_the_default() {
    let Some(url) = support::pg::url() else {
        return;
    };
    for (n, seed) in [0x9e37_79b9_7f4a_7c15u64, 0x2545_f491_4f6c_dd1d]
        .into_iter()
        .enumerate()
    {
        let schema = support::pg::Schema::new(url.clone(), &format!("fpw{n}"));
        let handles = schema.create(BINDING, CURSOR);
        let shapes = history_of(&handles.history, seed, 160);
        check(&handles.history, &shapes);
    }
}
