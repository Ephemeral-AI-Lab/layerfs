//! Independent finite demand protocol over the fixture's two-leaf serial ranges.

use std::collections::BTreeSet;

use layerfs_content::filesystem::validate::ValidationPrefetchWork;
use layerfs_content::ObjectId;

use super::namespace::{Fixture, LAST_BASE};

pub struct Model {
    pub prefetch: ValidationPrefetchWork,
    pub batches: Vec<Vec<ObjectId>>,
    pub prefetch_pages: u64,
    pub allocation_pages: u64,
    pub binding_pages: u64,
    pub cycle_pages: u64,
    pub read_waves: u64,
    pub logical_demands: u64,
}

fn pages(fixture: &Fixture, serials: &[u64], batches: &mut Vec<Vec<ObjectId>>) -> (u64, u64) {
    batches.push(vec![fixture.inode_root]);
    let mut leaves = Vec::new();
    if serials.iter().any(|serial| *serial <= 65) {
        leaves.push(fixture.leaves[0]);
    }
    if serials
        .iter()
        .any(|serial| (66..=LAST_BASE).contains(serial))
    {
        leaves.push(fixture.leaves[1]);
    }
    let count = 1 + leaves.len() as u64;
    let waves = 1 + u64::from(!leaves.is_empty());
    if !leaves.is_empty() {
        batches.push(leaves);
    }
    (count, waves)
}

fn remember(memo: &mut BTreeSet<u64>, limit: usize, serial: u64) {
    if memo.len() >= limit {
        memo.clear();
    }
    memo.insert(serial);
}

impl Model {
    pub fn new(fixture: &Fixture, rows: &[(u64, Vec<u64>)], new: &[u64], limit: usize) -> Self {
        let mut model = Self {
            prefetch: ValidationPrefetchWork::default(),
            batches: vec![vec![fixture.root]],
            prefetch_pages: 0,
            allocation_pages: 0,
            binding_pages: 0,
            cycle_pages: 0,
            read_waves: 0,
            logical_demands: 0,
        };
        if !new.is_empty() {
            for wave in new.chunks(64) {
                let (count, calls) = pages(fixture, wave, &mut model.batches);
                model.allocation_pages += count;
                model.read_waves += calls;
            }
        }
        let raw: Vec<_> = rows
            .iter()
            .flat_map(|(parent, children)| std::iter::once(*parent).chain(children.iter().copied()))
            .collect();
        let mut memo = BTreeSet::new();
        for wave in raw.chunks(64) {
            model.prefetch.examined_occurrences += wave.len() as u64;
            model.prefetch.max_pending = model.prefetch.max_pending.max(wave.len() as u64);
            let missing_occurrences: Vec<_> = wave
                .iter()
                .copied()
                .filter(|serial| {
                    if memo.contains(serial) {
                        model.prefetch.memo_hits += 1;
                        false
                    } else {
                        true
                    }
                })
                .collect();
            let missing: BTreeSet<_> = missing_occurrences.iter().copied().collect();
            model.prefetch.local_duplicates += (missing_occurrences.len() - missing.len()) as u64;
            if missing.is_empty() {
                continue;
            }
            let demand: Vec<_> = missing.into_iter().collect();
            model.prefetch.submitted_serials += demand.len() as u64;
            model.prefetch.lookup_calls += 1;
            model.prefetch.max_missing = model.prefetch.max_missing.max(demand.len() as u64);
            model.prefetch.max_answers = model.prefetch.max_answers.max(demand.len() as u64);
            let (count, calls) = pages(fixture, &demand, &mut model.batches);
            model.prefetch_pages += count;
            model.read_waves += calls;
            for serial in demand {
                remember(&mut memo, limit, serial);
            }
        }
        for (parent, children) in rows {
            for serial in std::iter::once(parent).chain(children) {
                let (count, calls) = model.lookup(fixture, &mut memo, limit, *serial);
                model.binding_pages += count;
                model.read_waves += calls;
            }
        }
        for (_, children) in rows {
            for serial in children {
                let (count, calls) = model.lookup(fixture, &mut memo, limit, *serial);
                model.cycle_pages += count;
                model.read_waves += calls;
            }
        }
        model
    }

    fn lookup(
        &mut self,
        fixture: &Fixture,
        memo: &mut BTreeSet<u64>,
        limit: usize,
        serial: u64,
    ) -> (u64, u64) {
        // The existing known-absence path charges one logical demand, whereas
        // an out-of-range branch descent reaching no leaf charges none. This
        // explicit model preserves that disclosed quirk instead of smoothing it.
        if memo.contains(&serial) {
            self.logical_demands += 1;
            return (0, 0);
        }
        if serial <= LAST_BASE {
            self.logical_demands += 1;
        }
        let work = pages(fixture, &[serial], &mut self.batches);
        remember(memo, limit, serial);
        work
    }
}
