//! The effective-tree proof against an independent parent model.
//!
//! Each case builds a random directory tree, then states a random batch over
//! it: directories created anywhere, stored directories moved - under the root,
//! under stored directories, under fresh ones, under their own descendants -
//! renamed inside their own parent, removed, swapped, and replaced at their own
//! name by another directory. Most cases are small; every eighth is large, up
//! to 149 stored directories, 20 fresh ones and 70 operations, and every
//! sixteenth restricts each placement to a parent with a smaller serial, so
//! that large batches are accepted as well as refused.
//!
//! Every directory ends with exactly one binding or is removed with nothing
//! left in it, so the only possible refusal is a cycle. The model knows nothing
//! about placements, territories or renames in place: it keeps each
//! directory's final parent and name, follows the parents upward and asks
//! whether every directory reaches the root. Both routes must agree with it, a
//! refusal must offer nothing, and an accepted result is read back binding by
//! binding.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use layerfs_content::filesystem::validate::ValidationWork;
use layerfs_content::filesystem::{FilesystemRead, FilesystemResources};
use layerfs_content::ContentError;
use support::filesystem::name_of;
use support::topology::{inode, listing, update_backed, update_resident, Rowset, Shape, ROOT};

/// A fixed xorshift stream: the cases are the same on every run.
struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
    fn pick(&mut self, from: &[u64]) -> u64 {
        from[self.below(from.len())]
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

/// Where a directory is bound: its parent and its name there.
type Binding = Option<(u64, String)>;

/// What one batch did, for the printed coverage table.
#[derive(Default)]
struct Shapes {
    /// A stored directory was removed.
    removal: bool,
    /// A removed directory had its children moved out in the same batch.
    emptied: bool,
    /// A name a stored directory left was bound to another directory.
    replaced: bool,
    /// A directory was renamed inside its parent and its old name bound again.
    rebound: bool,
    /// Two directories of one parent exchanged names.
    swap: bool,
}

/// The final state one batch describes, beside the rows that state it.
struct Batch {
    rows: Rowset,
    /// The final binding of every directory but the root; `None` once removed.
    at: BTreeMap<u64, Binding>,
    /// The directory that finally holds each name.
    slot: BTreeMap<(u64, String), u64>,
    /// Stored directories this batch already changed.
    touched: BTreeSet<u64>,
    /// Directories whose own listing this batch changes.
    changed: BTreeSet<u64>,
    /// The first serial the base never used.
    fresh_from: u64,
    /// Every placement names a parent with a smaller serial.
    ordered: bool,
}

impl Batch {
    fn parent(&self, directory: u64) -> u64 {
        self.at[&directory].as_ref().expect("a bound directory").0
    }

    /// Stored directories still bound where the base bound them.
    fn untouched(&self) -> Vec<u64> {
        self.at
            .iter()
            .filter(|(directory, at)| {
                **directory < self.fresh_from && at.is_some() && !self.touched.contains(directory)
            })
            .map(|(directory, _)| *directory)
            .collect()
    }

    /// Directories finally bound in `directory`.
    fn children(&self, directory: u64) -> Vec<u64> {
        self.at
            .iter()
            .filter(|(_, at)| matches!(at, Some((parent, _)) if *parent == directory))
            .map(|(child, _)| *child)
            .collect()
    }

    /// Directories a placement of `directory` may name, never `not`.
    fn targets(&self, directory: u64, not: u64) -> Vec<u64> {
        std::iter::once(ROOT)
            .chain(
                self.at
                    .iter()
                    .filter(|(_, at)| at.is_some())
                    .map(|(candidate, _)| *candidate),
            )
            .filter(|candidate| *candidate != not && (!self.ordered || *candidate < directory))
            .collect()
    }

    /// Takes a directory out of the name it holds.
    fn vacate(&mut self, directory: u64) {
        let (parent, name) = self
            .at
            .insert(directory, None)
            .flatten()
            .expect("a bound directory");
        assert_eq!(self.slot.remove(&(parent, name.clone())), Some(directory));
        self.rows = std::mem::take(&mut self.rows).unbind(parent, &name);
        self.changed.insert(parent);
        self.touched.insert(directory);
    }

    /// Binds a directory that holds no name at a name nothing holds.
    fn bind(&mut self, directory: u64, parent: u64, name: String) {
        assert!(self.at[&directory].is_none());
        assert!(self
            .slot
            .insert((parent, name.clone()), directory)
            .is_none());
        self.rows = std::mem::take(&mut self.rows).bind(parent, &name, directory);
        self.changed.insert(parent);
        self.at.insert(directory, Some((parent, name)));
    }

    /// Creates a fresh directory under any directory that exists.
    fn create(&mut self, random: &mut Random, serial: u64) {
        let under = random.pick(&self.targets(serial, 0));
        self.rows = std::mem::take(&mut self.rows).mkdir(under, &name(serial), serial);
        self.at.insert(serial, Some((under, name(serial))));
        self.slot.insert((under, name(serial)), serial);
        self.changed.insert(under);
    }

    /// Moves a stored directory: one time in three it stays under its own
    /// parent, and half the time it takes a new name.
    fn wander(&mut self, random: &mut Random, directory: u64, not: u64) {
        let old = self.parent(directory);
        let under = if random.below(3) == 0 && old != not {
            old
        } else {
            random.pick(&self.targets(directory, not))
        };
        let renamed = if random.below(2) == 0 {
            format!("r{directory}")
        } else {
            name(directory)
        };
        self.vacate(directory);
        self.bind(directory, under, renamed);
    }

    /// Removes a stored directory once nothing is left in it: its stored
    /// children are moved elsewhere first. `keep` is a child that is about to
    /// leave by itself. False when a child cannot be moved out.
    fn remove(
        &mut self,
        random: &mut Random,
        directory: u64,
        keep: Option<u64>,
        shapes: &mut Shapes,
    ) -> bool {
        let children: Vec<u64> = self
            .children(directory)
            .into_iter()
            .filter(|child| Some(*child) != keep)
            .collect();
        if children
            .iter()
            .any(|child| *child >= self.fresh_from || self.touched.contains(child))
        {
            return false;
        }
        for child in &children {
            self.wander(random, *child, directory);
        }
        self.vacate(directory);
        shapes.removal = true;
        shapes.emptied |= !children.is_empty();
        true
    }

    /// Binds `directory` at the name another stored directory holds; that one
    /// is renamed inside its parent, moved elsewhere or removed.
    fn replace(&mut self, random: &mut Random, directory: u64, shapes: &mut Shapes) -> bool {
        let candidates: Vec<u64> = self
            .untouched()
            .into_iter()
            .filter(|other| {
                *other != directory && (!self.ordered || self.parent(*other) < directory)
            })
            .collect();
        if candidates.is_empty() {
            return false;
        }
        let other = random.pick(&candidates);
        let parent = self.parent(other);
        let fate = random.below(3);
        if fate == 2 && self.remove(random, other, Some(directory), shapes) {
            // `rm -r d; mv x d`.
        } else if fate == 1 {
            let under = random.pick(&self.targets(other, 0));
            self.vacate(other);
            self.bind(other, under, format!("r{other}"));
            shapes.rebound |= under == parent;
        } else {
            // `mv d d.old; mv x d`.
            self.vacate(other);
            self.bind(other, parent, format!("r{other}"));
            shapes.rebound = true;
        }
        self.vacate(directory);
        self.bind(directory, parent, name(other));
        shapes.replaced = true;
        true
    }

    /// Exchanges the names of two stored directories of one parent.
    fn swap(&mut self, random: &mut Random, directory: u64, shapes: &mut Shapes) -> bool {
        let parent = self.parent(directory);
        let siblings: Vec<u64> = self
            .untouched()
            .into_iter()
            .filter(|other| *other != directory && self.parent(*other) == parent)
            .collect();
        if siblings.is_empty() {
            return false;
        }
        let other = random.pick(&siblings);
        self.vacate(directory);
        self.vacate(other);
        self.bind(directory, parent, name(other));
        self.bind(other, parent, name(directory));
        shapes.swap = true;
        shapes.rebound = true;
        true
    }
}

/// Accepted and refused batch counts of every shape.
#[derive(Default)]
struct Coverage(BTreeMap<&'static str, [u32; 2]>);

impl Coverage {
    fn note(&mut self, shape: &'static str, present: bool, accepted: bool) {
        let counts = self.0.entry(shape).or_default();
        if present {
            counts[usize::from(!accepted)] += 1;
        }
    }
    fn accepted(&self, shape: &str) -> u32 {
        self.0[shape][0]
    }
    fn refused(&self, shape: &str) -> u32 {
        self.0[shape][1]
    }
}

#[test]
fn random_directory_batches_agree_with_an_independent_parent_model() {
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    let mut coverage = Coverage::default();
    let (mut most_accepted, mut most_refused) = (0_usize, 0_usize);
    for case in 0..400 {
        let large = case % 8 == 7;
        let ordered = case % 16 == 15;
        let largest = case % 64 == 63;

        // The base: every directory sits under an earlier one. Half the
        // directories of a large base go near the newest, so it is deep too.
        let stored = match (largest, large) {
            (true, _) => 149,
            (false, true) => 60 + random.below(90),
            (false, false) => 2 + random.below(9),
        };
        let mut shape = Shape::new();
        let mut batch = Batch {
            rows: Rowset::new(),
            at: BTreeMap::new(),
            slot: BTreeMap::new(),
            touched: BTreeSet::new(),
            changed: BTreeSet::new(),
            fresh_from: 0,
            ordered,
        };
        let mut directories = vec![ROOT];
        for _ in 0..stored {
            let under = if large && random.below(2) == 0 {
                let newest = directories.len().min(4);
                directories[directories.len() - 1 - random.below(newest)]
            } else {
                directories[random.below(directories.len())]
            };
            let expected = ROOT + directories.len() as u64;
            let serial = shape.dir(under, &name(expected));
            assert_eq!(serial, expected);
            batch.at.insert(serial, Some((under, name(serial))));
            batch.slot.insert((under, name(serial)), serial);
            directories.push(serial);
        }
        let tree = shape.build();
        let base = batch.at.clone();
        batch.fresh_from = tree.next;

        // The batch: fresh directories anywhere, then operations on stored ones.
        let fresh = match (largest, large) {
            (true, _) => 20,
            (false, true) => random.below(21),
            (false, false) => random.below(4),
        } as u64;
        for index in 0..fresh {
            batch.create(&mut random, tree.next + index);
        }
        let operations = match (largest, large) {
            (true, _) => 70,
            (false, true) => 20 + random.below(51),
            (false, false) => 1 + random.below(3),
        };
        let mut shapes = Shapes::default();
        for _ in 0..operations {
            let untouched = batch.untouched();
            if untouched.is_empty() {
                break;
            }
            let directory = random.pick(&untouched);
            let done = match random.below(20) {
                0..=11 => false,
                12..=14 => batch.replace(&mut random, directory, &mut shapes),
                15..=16 => batch.swap(&mut random, directory, &mut shapes),
                _ => batch.remove(&mut random, directory, None, &mut shapes),
            };
            if !done {
                batch.wander(&mut random, directory, 0);
            }
        }
        let Batch {
            rows,
            at,
            slot,
            changed,
            ..
        } = batch;

        // The model: every directory that is still bound sits in one that is,
        // and its final parents lead to the root.
        let live: Vec<u64> = at
            .iter()
            .filter(|(_, at)| at.is_some())
            .map(|(directory, _)| *directory)
            .collect();
        let above = |directory: u64| at[&directory].as_ref().expect("a bound directory").0;
        for directory in &live {
            let parent = above(*directory);
            assert!(
                parent == ROOT || at[&parent].is_some(),
                "case {case}: the generator left {directory} in a removed directory"
            );
        }
        let rooted = live.iter().all(|directory| {
            let mut current = *directory;
            for _ in 0..=live.len() {
                if current == ROOT {
                    return true;
                }
                current = above(current);
            }
            false
        });
        assert!(
            rooted || !ordered,
            "case {case}: an ordered batch is rooted"
        );
        // Bindings the base does not have, and renames inside the base parent.
        let stated = live
            .iter()
            .filter(|directory| base.get(*directory) != Some(&at[*directory]))
            .count();
        let renamed = live
            .iter()
            .filter(|directory| {
                base.get(*directory).is_some_and(|before| {
                    before != &at[*directory]
                        && before.as_ref().map(|(parent, _)| *parent) == Some(above(**directory))
                })
            })
            .count() as u64;

        let run = update_backed(&tree, &rows, FilesystemResources::default());
        let (resident, emitted) = update_resident(&tree, &rows, FilesystemResources::default());
        if rooted {
            let result = run.result.as_ref().unwrap_or_else(|error| {
                panic!("case {case}: a rooted change was refused with {error:?}: {at:?}")
            });
            let resident = resident.unwrap_or_else(|error| {
                panic!("case {case}: the resident route refused with {error:?}: {at:?}")
            });
            assert_eq!(resident.root, result.root, "case {case}");
            let store = run.folded(&tree);
            let mut read = FilesystemRead::new(&store, result.root).expect("result");
            for (directory, binding) in &at {
                match binding {
                    Some((under, bound)) => assert_eq!(
                        read.resolve_child(*under, &name_of(bound))
                            .unwrap_or_else(|error| panic!("case {case}: {directory}: {error:?}"))
                            .serial,
                        *directory,
                        "case {case}"
                    ),
                    None => assert_eq!(
                        inode(&store, result.root, *directory),
                        None,
                        "case {case}: removed directory {directory} is released"
                    ),
                }
            }
            // Every listing the batch changed holds exactly the final names.
            for parent in &changed {
                if *parent != ROOT && at[parent].is_none() {
                    continue;
                }
                let mut expected: Vec<(String, u64)> = slot
                    .range((*parent, String::new())..(*parent + 1, String::new()))
                    .map(|((_, bound), directory)| (bound.clone(), *directory))
                    .collect();
                let mut listed = listing(&store, result.root, *parent);
                expected.sort();
                listed.sort();
                assert_eq!(listed, expected, "case {case}: listing of {parent}");
            }
            let work = result.counters.validation;
            assert_eq!(
                proof(work),
                proof(resident.counters.validation),
                "case {case}"
            );
            // One placement for every directory binding the base does not have.
            assert_eq!(work.placements, stated as u64, "case {case}");
            // When any parent was scanned, every rename in place was found and
            // nothing else was; when none was, none is claimed.
            if work.in_place_scans > 0 {
                assert_eq!(work.in_place_directories, renamed, "case {case}");
            } else {
                assert_eq!(work.in_place_directories, 0, "case {case}");
            }
            most_accepted = most_accepted.max(stated);
            coverage.note("walks a territory", work.territory_directories > 0, true);
            coverage.note(
                "finds a rename in place",
                work.in_place_directories > 0,
                true,
            );
            coverage.note(
                "finds a rename in place and walks nothing",
                work.in_place_directories > 0 && work.territory_directories == 0,
                true,
            );
        } else {
            assert_eq!(
                run.result.as_ref().err(),
                Some(&ContentError::InvalidRecord("effective tree cycle")),
                "case {case}: an unrooted change was not refused as a cycle: {at:?}"
            );
            assert!(run.emitted.is_empty(), "case {case}: nothing is offered");
            assert_eq!(
                resident.err(),
                Some(ContentError::InvalidRecord("effective tree cycle")),
                "case {case}: {at:?}"
            );
            assert!(emitted.is_empty(), "case {case}: nothing is offered");
            most_refused = most_refused.max(stated);
        }
        for (shape, present) in [
            ("every batch", true),
            ("small", !large),
            ("large, any parent", large && !ordered),
            ("large, smaller-serial parents", ordered),
            ("states more than 64 new bindings", stated > 64),
            ("removes a directory", shapes.removal),
            (
                "removes a directory after moving its children out",
                shapes.emptied,
            ),
            ("binds a vacated name to another directory", shapes.replaced),
            (
                "renames in place and binds the old name again",
                shapes.rebound,
            ),
            ("swaps two names in one parent", shapes.swap),
        ] {
            coverage.note(shape, present, rooted);
        }
    }
    eprintln!("model: 400 batches, (accepted, refused) by shape:");
    for (shape, [accepted, refused]) in &coverage.0 {
        eprintln!("  {shape}: ({accepted}, {refused})");
    }
    eprintln!(
        "model: most new bindings in one accepted batch {most_accepted}, in one refused batch \
         {most_refused}"
    );
    // The stream must exercise every outcome and shape, not one of them 400 times.
    assert!(coverage.accepted("every batch") >= 20 && coverage.refused("every batch") >= 20);
    assert!(coverage.accepted("walks a territory") >= 10);
    assert!(coverage.accepted("finds a rename in place") >= 10);
    assert!(coverage.accepted("finds a rename in place and walks nothing") >= 5);
    assert!(coverage.accepted("large, smaller-serial parents") >= 5);
    assert!(coverage.refused("large, any parent") >= 5);
    assert!(coverage.accepted("states more than 64 new bindings") >= 1);
    assert!(coverage.accepted("removes a directory") >= 10);
    assert!(coverage.accepted("removes a directory after moving its children out") >= 3);
    assert!(coverage.accepted("binds a vacated name to another directory") >= 10);
    assert!(coverage.accepted("renames in place and binds the old name again") >= 5);
    assert!(coverage.accepted("swaps two names in one parent") >= 3);
}
