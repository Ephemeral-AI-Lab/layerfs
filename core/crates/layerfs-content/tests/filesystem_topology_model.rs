//! The effective-tree proof against an independent parent model.
//!
//! Each case builds a small random directory tree, then states a random batch
//! that creates directories and moves stored ones - under the root, under
//! stored directories, under fresh ones, under their own descendants. Every
//! directory keeps exactly one binding, so the only possible refusal is a
//! cycle. The model knows nothing about placements or territories: it follows
//! each directory's final parent upward and asks whether it reaches the root.
//! Both routes must agree with it, and a refusal must offer nothing. One move
//! in three keeps the directory under its own parent and half of all moves take
//! a new name, so renames in place are mixed with real moves in one batch.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use layerfs_content::filesystem::validate::ValidationWork;
use layerfs_content::filesystem::{FilesystemRead, FilesystemResources};
use layerfs_content::ContentError;
use support::filesystem::name_of;
use support::topology::{update_backed, update_resident, Rowset, Shape, ROOT};

/// A fixed xorshift stream: the cases are the same on every run.
struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

fn name(serial: u64) -> String {
    format!("n{serial}")
}

/// The proof's own counters, which both routes must report alike.
fn proof(work: ValidationWork) -> [u64; 8] {
    [
        work.placements,
        work.ancestry_steps,
        work.territory_directories,
        work.territory_entries,
        work.entries_examined,
        work.in_place_scans,
        work.in_place_rows,
        work.in_place_directories,
    ]
}

#[test]
fn random_directory_moves_agree_with_an_independent_parent_model() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let (mut accepted, mut refused, mut walked) = (0_u32, 0_u32, 0_u32);
    let (mut in_place, mut in_place_unwalked) = (0_u32, 0_u32);
    for case in 0..400 {
        // The base: every directory sits under an earlier one.
        let stored = 2 + random.below(9);
        let mut shape = Shape::new();
        let mut parent: BTreeMap<u64, u64> = BTreeMap::new();
        let mut names: BTreeMap<u64, String> = BTreeMap::new();
        let mut directories = vec![ROOT];
        for _ in 0..stored {
            let under = directories[random.below(directories.len())];
            let expected = ROOT + directories.len() as u64;
            let serial = shape.dir(under, &name(expected));
            assert_eq!(serial, expected);
            parent.insert(serial, under);
            names.insert(serial, name(serial));
            directories.push(serial);
        }
        let tree = shape.build();

        // The batch: fresh directories anywhere, then moves of stored ones.
        let fresh = random.below(4) as u64;
        let mut all = directories.clone();
        all.extend((0..fresh).map(|index| tree.next + index));
        let mut rows = Rowset::new();
        for index in 0..fresh {
            let serial = tree.next + index;
            let under = all[random.below(all.len())];
            rows = rows.mkdir(under, &name(serial), serial);
            parent.insert(serial, under);
            names.insert(serial, name(serial));
        }
        let mut moved = BTreeSet::new();
        for _ in 0..1 + random.below(3) {
            let directory = directories[1 + random.below(stored)];
            if !moved.insert(directory) {
                continue;
            }
            let old = parent[&directory];
            let under = if random.below(3) == 0 {
                old
            } else {
                all[random.below(all.len())]
            };
            let renamed = if random.below(2) == 0 {
                format!("r{directory}")
            } else {
                name(directory)
            };
            rows = rows
                .unbind(old, &name(directory))
                .bind(under, &renamed, directory);
            parent.insert(directory, under);
            names.insert(directory, renamed);
        }

        // The model: every directory's final parents lead to the root.
        let rooted = all.iter().all(|directory| {
            let mut at = *directory;
            for _ in 0..=all.len() {
                if at == ROOT {
                    return true;
                }
                at = parent[&at];
            }
            false
        });

        let run = update_backed(&tree, &rows, FilesystemResources::default());
        let (resident, emitted) = update_resident(&tree, &rows, FilesystemResources::default());
        if rooted {
            let result = run.result.as_ref().unwrap_or_else(|error| {
                panic!("case {case}: a rooted change was refused with {error:?}: {parent:?}")
            });
            let resident = resident.unwrap_or_else(|error| {
                panic!("case {case}: the resident route refused with {error:?}: {parent:?}")
            });
            assert_eq!(resident.root, result.root, "case {case}");
            let store = run.folded(&tree);
            let mut read = FilesystemRead::new(&store, result.root).expect("result");
            for (directory, under) in &parent {
                assert_eq!(
                    read.resolve_child(*under, &name_of(&names[directory]))
                        .unwrap_or_else(|error| panic!("case {case}: {directory}: {error:?}"))
                        .serial,
                    *directory,
                    "case {case}"
                );
            }
            accepted += 1;
            let work = result.counters.validation;
            assert_eq!(
                proof(work),
                proof(resident.counters.validation),
                "case {case}"
            );
            if work.territory_directories > 0 {
                walked += 1;
            }
            if work.in_place_directories > 0 {
                in_place += 1;
                if work.territory_directories == 0 {
                    in_place_unwalked += 1;
                }
            }
        } else {
            assert_eq!(
                run.result.as_ref().err(),
                Some(&ContentError::InvalidRecord("effective tree cycle")),
                "case {case}: an unrooted change was not refused as a cycle: {parent:?}"
            );
            assert!(run.emitted.is_empty(), "case {case}: nothing is offered");
            assert_eq!(
                resident.err(),
                Some(ContentError::InvalidRecord("effective tree cycle")),
                "case {case}: {parent:?}"
            );
            assert!(emitted.is_empty(), "case {case}: nothing is offered");
            refused += 1;
        }
    }
    eprintln!(
        "model: 400 cases, accepted {accepted}, refused {refused}, accepted after a territory \
         walk {walked}, accepted with a rename in place {in_place} ({in_place_unwalked} of them \
         with no walk)"
    );
    // The stream must exercise every outcome, not one of them 400 times.
    assert!(accepted >= 20 && refused >= 20 && walked >= 10);
    assert!(in_place >= 10 && in_place_unwalked >= 5);
}
