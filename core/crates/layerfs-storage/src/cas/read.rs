//! Batched object reads and the retained-pack visibility ceiling.
//!
//! One read captures its permitted pack ceiling once and applies it to every
//! acquired location, including every dependency read; the ceiling never moves
//! while the read is in progress. Pack bodies are read once per pack per wave,
//! dependencies are reconstructed iteratively under fixed work budgets, and every
//! reconstructed object - base or requested - is authenticated against the
//! identity it was stored under.

use std::collections::BTreeMap;

use rusqlite::Connection;

use layerfs_content::ObjectId;

use crate::encoding::codec::DecompressionWorkspace;
use crate::encoding::delta::read::{ChainCounters, Resolver};
use crate::error::{StorageError, StorageResult};
use crate::policy::StorageCapacities;
use crate::sqlite::lookup;

/// Work performed by one bounded read wave.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadCounters {
    /// Objects returned.
    pub objects: u64,
    /// Pack bodies read.
    pub packs_read: u64,
    /// Membership/locator pages issued.
    pub pages: u64,
    /// Ceiling applied to every acquired location.
    pub ceiling: i64,
    /// Dependency edges followed.
    pub edges: u64,
    /// Longest dependency chain reconstructed.
    pub max_depth: u64,
    /// Canonical bytes reconstructed, including dependencies.
    pub canonical_bytes: u64,
}

/// Reads every requested object in demand order under one ceiling.
pub fn read_objects(
    connection: &Connection,
    ids: &[ObjectId],
    ceiling: i64,
    capacities: &StorageCapacities,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<(Vec<Vec<u8>>, ReadCounters)> {
    // Locators are collected above the ceiling on purpose: a record that exists
    // but is not yet published must be reported as a visibility refusal, never
    // mistaken for a missing object. Dependency reads inside the resolver do use
    // the captured ceiling.
    let locations = lookup::locations(connection, ids, i64::MAX)?;
    let mut counters = ReadCounters {
        ceiling,
        pages: ids.len().div_ceil(crate::policy::LOOKUP_PAGE_IDS) as u64,
        ..ReadCounters::default()
    };
    let mut by_id: BTreeMap<ObjectId, lookup::ObjectLocation> = BTreeMap::new();
    for location in locations {
        if location.pack_id > ceiling {
            return Err(StorageError::VisibilityCeiling {
                pack_id: location.pack_id,
                ceiling,
            });
        }
        by_id.insert(location.object_id, location);
    }
    let mut packs: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    let mut chain = ChainCounters::default();
    let mut totals = ChainCounters::default();
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        let location = by_id
            .get(id)
            .copied()
            .ok_or(StorageError::ObjectMissing(*id))?;
        let (canonical, packs_fetched) = {
            let mut resolver = Resolver::new(
                connection, ceiling, capacities, &mut packs, workspace, &mut chain,
            );
            let canonical = resolver.resolve_at(location)?;
            (canonical, resolver.packs_read())
        };
        totals.objects = totals.objects.saturating_add(chain.objects);
        totals.edges = totals.edges.saturating_add(chain.edges);
        totals.encoded_bytes = totals.encoded_bytes.saturating_add(chain.encoded_bytes);
        totals.canonical_bytes = totals.canonical_bytes.saturating_add(chain.canonical_bytes);
        totals.max_depth = totals.max_depth.max(chain.max_depth);
        counters.packs_read += packs_fetched;
        if ObjectId::for_bytes(&canonical) != *id {
            return Err(StorageError::Integrity("read identity"));
        }
        counters.objects += 1;
        values.push(canonical);
    }
    counters.edges = totals.edges;
    counters.max_depth = totals.max_depth;
    counters.canonical_bytes = totals.canonical_bytes;
    Ok((values, counters))
}
