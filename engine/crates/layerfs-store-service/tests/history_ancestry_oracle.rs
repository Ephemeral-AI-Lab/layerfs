//! The lineage-based ancestry (#353) answers exactly what a walk over the
//! whole history answers: reachability and merge base on random histories
//! with forks, Commits, merges (main among the sides) and publishes. Among
//! several maximal common ancestors both apply the #353 rank.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::{BranchId, HistoryCatalog, HistoryName, LayerId, LayerStackId, NodeId};
use std::cmp::Reverse;
use std::collections::{HashSet, VecDeque};
use std::fs;
use support::{write, Fixture};

const STACK: [u8; 16] = [23; 16];

/// A small deterministic generator (no test dependency).
struct Lcg(u64);
impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) % n as u64) as usize
    }
}

/// The reference: every edge, the whole history, as the walk was before #352.
fn parents(history: &dyn HistoryCatalog, node: NodeId) -> Vec<NodeId> {
    match node {
        NodeId::Commit(id) => {
            let record = history.commit(id).unwrap().unwrap();
            let mut out = vec![match record.parent {
                Some(parent) => NodeId::Commit(parent),
                None => NodeId::Layer(record.base_layer),
            }];
            out.extend(record.second_parent);
            out
        }
        NodeId::Layer(id) => {
            let record = history.layer(id).unwrap().unwrap();
            record
                .parent
                .map(NodeId::Layer)
                .into_iter()
                .chain(record.source_commit.map(NodeId::Commit))
                .collect()
        }
    }
}

fn walk(history: &dyn HistoryCatalog, start: NodeId, mut visit: impl FnMut(NodeId) -> bool) {
    let mut seen = HashSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some(node) = queue.pop_front() {
        if visit(node) {
            continue;
        }
        for parent in parents(history, node) {
            if seen.insert(parent) {
                queue.push_back(parent);
            }
        }
    }
}

fn reference_is_ancestor(history: &dyn HistoryCatalog, ancestor: NodeId, node: NodeId) -> bool {
    let mut found = false;
    walk(history, node, |n| {
        found |= n == ancestor;
        found
    });
    found
}

/// A Layer's position in its stack chain.
fn height(history: &dyn HistoryCatalog, layer: LayerId) -> usize {
    let mut steps = 0;
    let mut current = history.layer(layer).unwrap().unwrap().parent;
    while let Some(id) = current {
        steps += 1;
        current = history.layer(id).unwrap().unwrap().parent;
    }
    steps
}

/// The #353 order among maximal common ancestors: newest Layer in the
/// node's ancestry, then deepest first parent, then smallest identity.
fn rank(history: &dyn HistoryCatalog, node: NodeId) -> (usize, usize, Reverse<Vec<u8>>) {
    let mut newest = 0;
    walk(history, node, |n| {
        if let NodeId::Layer(layer) = n {
            newest = newest.max(height(history, layer));
        }
        false
    });
    let mut depth = 0;
    let mut current = node;
    while let NodeId::Commit(_) = current {
        depth += 1;
        current = parents(history, current)[0];
    }
    (newest, depth, Reverse(node.as_slice().to_vec()))
}

fn reference_merge_base(
    history: &dyn HistoryCatalog,
    left: NodeId,
    right: NodeId,
) -> (Option<NodeId>, usize) {
    let mut reachable = HashSet::new();
    walk(history, left, |n| {
        reachable.insert(n);
        false
    });
    let mut common = Vec::new();
    walk(history, right, |n| {
        let shared = reachable.contains(&n);
        if shared {
            common.push(n);
        }
        shared
    });
    let maximal: Vec<NodeId> = common
        .iter()
        .enumerate()
        .filter(|(i, candidate)| {
            !common
                .iter()
                .enumerate()
                .any(|(j, other)| *i != j && reference_is_ancestor(history, **candidate, *other))
        })
        .map(|(_, node)| *node)
        .collect();
    let best = maximal
        .iter()
        .copied()
        .max_by_key(|node| rank(history, *node));
    (best, maximal.len())
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

#[test]
fn bounded_ancestry_equals_the_whole_history_walk() {
    for seed in [352u64, 7, 2026] {
        random_history(seed);
    }
}

fn random_history(seed: u64) {
    let fixture = Fixture::new(&format!("history-ancestry-oracle-{seed}"), |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    let stack = LayerStackId::from_authority(STACK);
    let mut rng = Lcg(seed);
    let mut branches: Vec<String> = Vec::new();
    let mut headed: HashSet<String> = HashSet::new();
    let mut files = 0usize;
    let (mut merges, mut publishes) = (0, 0);
    for step in 0..220 {
        let roll = rng.below(100);
        if branches.is_empty() || roll < 18 {
            let name = format!("b{step:03}");
            let from = if !headed.is_empty() && rng.below(3) == 0 {
                let pool: Vec<&String> = branches.iter().filter(|b| headed.contains(*b)).collect();
                Some(pool[rng.below(pool.len())].clone())
            } else {
                None
            };
            let mut fields = vec![("name", text(&name))];
            if let Some(from) = &from {
                fields.push(("from", text(from)));
                headed.insert(name.clone());
            }
            fixture.call("branch.fork", fields);
            branches.push(name);
        } else if roll < 55 {
            let branch = branches[rng.below(branches.len())].clone();
            files += 1;
            fixture.apply(
                &branch,
                vec![write(&format!("/f{files:04}.txt"), format!("{files}\n"))],
            );
            headed.insert(branch);
        } else if roll < 82 {
            let left = branches[rng.below(branches.len())].clone();
            let right = if rng.below(4) == 0 {
                "main".to_owned()
            } else {
                branches[rng.below(branches.len())].clone()
            };
            if left == right {
                continue;
            }
            let name = format!("m{step:03}");
            let merged = fixture.call(
                "branch.merge",
                vec![
                    ("branch", text(&left)),
                    ("with", text(&right)),
                    ("name", text(&name)),
                    ("message", text("merge")),
                ],
            );
            assert_eq!(merged.get_text("kind"), Some("Merged"), "{merged:?}");
            merges += 1;
            if merged
                .get("head")
                .is_some_and(|h| !matches!(h, SdkValue::Null))
            {
                headed.insert(name.clone());
            }
            branches.push(name);
        } else {
            let pool: Vec<&String> = branches.iter().filter(|b| headed.contains(*b)).collect();
            if pool.is_empty() {
                continue;
            }
            let branch = pool[rng.below(pool.len())].clone();
            fixture.call("branch.publish", vec![("branch", text(&branch))]);
            publishes += 1;
        }
    }
    let history = fixture.service.store.history();
    // Nodes: every Branch's head or base, a few of their ancestors, and the Layers.
    let mut nodes = Vec::new();
    for name in &branches {
        let id = BranchId::named(stack, &HistoryName::new(name).unwrap());
        let branch = history.branch(id).unwrap().unwrap();
        let mut node = branch
            .head_commit
            .map_or(NodeId::Layer(branch.base_layer), NodeId::Commit);
        for _ in 0..3 {
            nodes.push(node);
            let up = parents(history, node);
            if up.is_empty() {
                break;
            }
            node = up[rng.below(up.len())];
        }
    }
    let mut layer = Some(history.layer_stack(stack).unwrap().unwrap().head_layer);
    while let Some(id) = layer {
        nodes.push(NodeId::Layer(id));
        layer = history.layer(id).unwrap().unwrap().parent;
    }
    let distinct: HashSet<NodeId> = nodes.iter().copied().collect();
    let mut nodes: Vec<NodeId> = distinct.into_iter().collect();
    nodes.sort_by_key(|n| format!("{n:?}"));
    let (mut checked, mut related, mut commit_bases, mut criss_cross) = (0, 0, 0, 0);
    for _ in 0..1500 {
        let (a, b) = (
            &nodes[rng.below(nodes.len())],
            &nodes[rng.below(nodes.len())],
        );
        let want = reference_is_ancestor(history, *a, *b);
        assert_eq!(
            history.is_ancestor(*a, *b).unwrap(),
            want,
            "is_ancestor({a:?}, {b:?})"
        );
        related += usize::from(want && a != b);
        let base = history.merge_base(*a, *b).unwrap();
        let (want, maximal) = reference_merge_base(history, *a, *b);
        assert_eq!(base, want, "merge_base({a:?}, {b:?})");
        criss_cross += usize::from(maximal > 1);
        commit_bases += usize::from(matches!(base, Some(NodeId::Commit(_))));
        checked += 1;
    }
    println!("ANCESTRY_ORACLE seed={seed} branches={} nodes={} pairs={checked} related={related} commit_bases={commit_bases} criss_cross={criss_cross} merges={merges} publishes={publishes}", branches.len(), nodes.len());
    assert!(publishes > 5 && merges > 20 && related > 100 && commit_bases > 50);
    fixture.cleanup();
}
