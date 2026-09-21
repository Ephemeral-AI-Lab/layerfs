//! The `namespace-10000` byte plan: the same *work* as the v0.1.6 fixture.
//!
//! `c1.fs.build-scale`'s recipe is a pure function of `(profile, entries,
//! directories, seed)` and materializes no file content at all — every file's
//! `content_root` is `ObjectId::for_bytes` over a label string. This module is the
//! other half: a plan that says how many bytes each of the 10,000 files carries, so
//! a row can construct that content and save it.
//!
//! **What is matched.** The v0.1.6 fixture (`synthetic-small-heavy-v2`,
//! `benchmark/fs-bench-pro/workload/main.rs`) declares 10,000 regular files over
//! 100 directories; 100 empty, 7,899 tiny, 1,500 small, 500 medium and one anchor
//! of 100,000,000 bytes; 300,000,000 logical bytes in total. All of those are
//! reproduced here exactly, and `plan` returns an error rather than a different
//! shape if the declared counts cannot be placed.
//!
//! **What is not matched, and why.** v0.1.6 permutes its class bands with a
//! SHA-256 sort key. This harness has no SHA-256 dependency and `AGENTS.md` §4
//! forbids adding one, so the band permutation here is derived from the harness's
//! own `fixture::noise` primitive. The arithmetic that turns weights into sizes is
//! the same largest-remainder pass, but the *permutation* differs, so individual
//! paths do not receive the sizes they receive in v0.1.6. The band counts, the tree
//! shape and the byte total are identical; the per-path assignment is a declared
//! difference, not a hidden one.
//!
//! The declared ranges are **weight bands, not size caps**: v0.1.6's own declared
//! weights sum to 102,555,546 bytes and the plan is then scaled up to the declared
//! logical total, so a "tiny" file there is larger than 8 bytes. Sizes here are
//! therefore not confined to the bands either.

use layerfs_content::ObjectId;

use crate::fixture;

/// Files one directory holds, so the plan's paths match the tree's.
pub const FILES_PER_DIRECTORY: u64 = 100;

/// Tiny band: count, inclusive lower weight, inclusive upper weight.
pub const TINY: (u64, u64, u64) = (7_899, 1, 8);
/// Small band.
pub const SMALL: (u64, u64, u64) = (1_500, 32, 256);
/// Medium band.
pub const MEDIUM: (u64, u64, u64) = (500, 1_024, 8_192);

/// Empty files the fixture declares.
pub const EMPTY_FILES: u64 = 100;
/// Anchor files the fixture declares.
pub const ANCHOR_FILES: u64 = 1;
/// Bytes the single anchor carries.
pub const ANCHOR_BYTES: u64 = 100_000_000;

/// Declared class counts, in band order: empty, tiny, small, medium, anchor.
pub const DECLARED_CLASSES: [u64; 5] =
    [EMPTY_FILES, TINY.0, SMALL.0, MEDIUM.0, ANCHOR_FILES];

/// One file's declared position and size.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedFile {
    /// Directory index, `0..directories`.
    pub directory: u32,
    /// Serial the tree gives this file, matching `Recipe::file_serial`.
    pub serial: u64,
    /// Bytes this file carries. `0` for an empty file.
    pub size: u64,
    /// True for the single anchor.
    pub anchor: bool,
    /// Band this file was drawn from: 0 empty, 1 tiny, 2 small, 3 medium,
    /// 4 anchor. Not a size class — the bands are relative weights.
    pub class: u64,
}

/// The whole plan for one `namespace-10000` row.
#[derive(Clone, Debug)]
pub struct BytePlan {
    /// Every file, ordered by directory then ordinal, as the tree orders paths.
    pub files: Vec<PlannedFile>,
    /// Sum of `files[*].size`. The row's declared content total.
    pub total_bytes: u64,
}

impl BytePlan {
    /// Files carrying content.
    pub fn non_empty(&self) -> usize {
        self.files.iter().filter(|file| file.size > 0).count()
    }

    /// Largest single file.
    pub fn largest(&self) -> u64 {
        self.files.iter().map(|file| file.size).max().unwrap_or(0)
    }

    /// Band counts, for the self-check.
    pub fn class_counts(&self) -> [u64; 5] {
        let mut counts = [0_u64; 5];
        for file in &self.files {
            let bucket = if file.anchor {
                4
            } else if file.size == 0 {
                0
            } else {
                match file.class {
                    1 => 1,
                    2 => 2,
                    _ => 3,
                }
            };
            counts[bucket] += 1;
        }
        counts
    }
}

/// Relative weight of the `role`-th member of a band, as v0.1.6 computes it.
fn relative_weight(lower: u64, upper: u64, role: u64, count: u64) -> Result<u64, String> {
    let width = upper
        .checked_sub(lower)
        .and_then(|value| value.checked_add(1))
        .ok_or("namespace weight width")?;
    let numerator = u128::from(
        role.checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or("namespace weight numerator")?,
    )
    .checked_mul(u128::from(width))
    .ok_or("namespace weight multiplication")?;
    let denominator = u128::from(count.checked_mul(2).ok_or("namespace weight denominator")?);
    lower
        .checked_add(u64::try_from(numerator / denominator).map_err(|_| "namespace weight")?)
        .ok_or_else(|| "namespace weight".to_string())
}

/// Builds the byte plan for `entries` files over `directories` directories.
pub fn plan(
    entries: u32,
    directories: u32,
    seed: u64,
    total_bytes: u64,
) -> Result<BytePlan, String> {
    let entries = u64::from(entries);
    let directories = u64::from(directories).max(1);
    if entries == 0 {
        return Err("namespace plan needs at least one file".to_string());
    }
    let declared: u64 = DECLARED_CLASSES.iter().sum();
    if entries < declared {
        return Err(format!(
            "namespace plan needs at least {declared} files, asked for {entries}"
        ));
    }

    // Serials follow `fs_fixture`: root is 1, directories are 2.., files follow.
    let first_file_serial = 2 + directories;

    // Bands: the declared counts, then any surplus as more tiny files, so a larger
    // `entries` degrades into a bigger tiny band rather than a different shape.
    let surplus = entries - declared;
    let mut bands: Vec<(u64, u64)> = Vec::with_capacity(entries as usize);
    bands.extend(std::iter::repeat_n((0_u64, 0_u64), EMPTY_FILES as usize));
    for role in 0..TINY.0 {
        bands.push((1, relative_weight(TINY.1, TINY.2, role, TINY.0)?));
    }
    for role in 0..SMALL.0 {
        bands.push((2, relative_weight(SMALL.1, SMALL.2, role, SMALL.0)?));
    }
    for role in 0..MEDIUM.0 {
        bands.push((3, relative_weight(MEDIUM.1, MEDIUM.2, role, MEDIUM.0)?));
    }
    for role in 0..surplus {
        bands.push((1, relative_weight(TINY.1, TINY.2, role, surplus.max(1))?));
    }
    // The anchor occupies one slot of its own; its position is permuted like every
    // other band, and its size comes from ANCHOR_BYTES rather than from a weight.
    bands.push((4, 0));
    if bands.len() as u64 != entries {
        return Err("namespace band cardinality".to_string());
    }

    // A deterministic permutation, so a path's band is a function of the seed
    // rather than of the declaration order. `fixture::noise` is the harness's own
    // primitive and the declared substitute for v0.1.6's SHA-256 sort key.
    let keys = fixture::noise(entries * 8, seed ^ 0x4e53_3130_3030_3030);
    let mut order: Vec<u64> = (0..entries).collect();
    order.sort_by_key(|index| {
        let at = (*index as usize) * 8;
        let mut slot = [0_u8; 8];
        slot.copy_from_slice(&keys[at..at + 8]);
        (u64::from_le_bytes(slot), *index)
    });
    let mut assigned: Vec<(u64, u64)> = vec![(0, 0); entries as usize];
    for (slot, source) in order.into_iter().enumerate() {
        assigned[slot] = bands[source as usize];
    }

    // The anchor: the first non-empty slot from a seed-derived offset, so it is
    // not always the first path.
    let anchor_bytes = ANCHOR_BYTES.min(total_bytes);
    let offset = {
        let bytes = fixture::noise(8, seed ^ 0x414e_4348_4f52_0001);
        u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes")) % entries
    };
    let anchor_slot = (offset..offset + entries)
        .map(|value| value % entries)
        .find(|slot| assigned[*slot as usize].0 == 4)
        .ok_or("namespace anchor slot")?;

    // Sizes: one Hamilton pass over the whole pool, exactly the arithmetic
    // v0.1.6 applies — `size = 1 + floor(distributable * weight / weight_sum)`,
    // then the remaining bytes to the largest remainders.
    let weight_sum: u64 = assigned
        .iter()
        .enumerate()
        .filter(|(slot, (class, _))| *slot as u64 != anchor_slot && *class != 0 && *class != 4)
        .map(|(_, (_, weight))| *weight)
        .sum();
    if weight_sum == 0 {
        return Err("namespace weight sum".to_string());
    }
    let positive = assigned
        .iter()
        .enumerate()
        .filter(|(slot, (class, _))| *slot as u64 != anchor_slot && *class != 0 && *class != 4)
        .count() as u64;
    let distributable = total_bytes
        .checked_sub(anchor_bytes)
        .and_then(|value| value.checked_sub(positive))
        .ok_or("namespace byte budget")?;

    let mut sizes = vec![0_u64; entries as usize];
    let mut remainders: Vec<(usize, u64)> = Vec::with_capacity(assigned.len());
    let mut floor_sum = 0_u64;
    for (slot, (class, weight)) in assigned.iter().enumerate() {
        if slot as u64 == anchor_slot {
            sizes[slot] = anchor_bytes;
            continue;
        }
        if *class == 0 || *class == 4 {
            continue;
        }
        let product = u128::from(distributable) * u128::from(*weight);
        let floor = u64::try_from(product / u128::from(weight_sum))
            .map_err(|_| "namespace floor overflow".to_string())?;
        let remainder = u64::try_from(product % u128::from(weight_sum))
            .map_err(|_| "namespace remainder overflow".to_string())?;
        sizes[slot] = 1 + floor;
        floor_sum += floor;
        remainders.push((slot, remainder));
    }
    let extra = distributable
        .checked_sub(floor_sum)
        .ok_or("namespace largest remainder")?;
    if extra > remainders.len() as u64 {
        return Err("namespace largest remainder count".to_string());
    }
    remainders.sort_by(|(left_slot, left), (right_slot, right)| {
        right.cmp(left).then_with(|| left_slot.cmp(right_slot))
    });
    for (slot, _) in remainders.into_iter().take(extra as usize) {
        sizes[slot] += 1;
    }

    let mut files = Vec::with_capacity(entries as usize);
    for position in 0..entries {
        let directory = position % directories;
        let ordinal = position / directories;
        let index = directory * FILES_PER_DIRECTORY + ordinal;
        files.push(PlannedFile {
            directory: u32::try_from(directory).map_err(|_| "namespace directory index")?,
            serial: first_file_serial + index,
            size: sizes[position as usize],
            anchor: position == anchor_slot,
            class: assigned[position as usize].0,
        });
    }
    let total = files.iter().map(|file| file.size).sum();
    if total != total_bytes {
        return Err(format!(
            "namespace plan total {total} does not equal the declared {total_bytes}"
        ));
    }
    Ok(BytePlan {
        files,
        total_bytes: total,
    })
}

/// Canonical path label of the `index`-th file in `directory`.
pub fn content_label(directory: u32, index: u64) -> String {
    format!("d{directory:04}/f{index:06}")
}

/// Identity a planned file's content is declared under, before construction.
pub fn declared_content_root(seed: u64, directory: u32, index: u64) -> ObjectId {
    ObjectId::for_bytes(
        format!(
            "layerfs/fs-bench/namespace-10000/{seed}/{}",
            content_label(directory, index)
        )
        .as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRIES: u32 = 10_000;
    const DIRECTORIES: u32 = 100;
    const TOTAL: u64 = 300_000_000;

    #[test]
    fn plan_reproduces_the_declared_fixture() {
        let plan = plan(ENTRIES, DIRECTORIES, 1, TOTAL).expect("plan");
        assert_eq!(plan.files.len(), ENTRIES as usize);
        assert_eq!(plan.class_counts(), DECLARED_CLASSES);
        assert_eq!(plan.total_bytes, TOTAL);
        assert_eq!(plan.largest(), ANCHOR_BYTES);
        assert_eq!(plan.non_empty(), (ENTRIES - EMPTY_FILES as u32) as usize);
    }

    #[test]
    fn anchor_is_unique_and_carries_the_declared_bytes() {
        let plan = plan(ENTRIES, DIRECTORIES, 1, TOTAL).expect("plan");
        let anchors: Vec<_> = plan.files.iter().filter(|file| file.anchor).collect();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].size, ANCHOR_BYTES);
    }

    #[test]
    fn directories_and_serials_match_the_tree_recipe() {
        let plan = plan(ENTRIES, DIRECTORIES, 1, TOTAL).expect("plan");
        let recipe = crate::ops::fs_fixture::Recipe {
            profile: "binary-v1",
            entries: ENTRIES,
            directories: DIRECTORIES,
            seed: 1,
        };
        let prepared = recipe.prepare();
        assert_eq!(plan.files.len(), prepared.files.len());
        let directories: std::collections::BTreeSet<u32> =
            plan.files.iter().map(|file| file.directory).collect();
        assert_eq!(directories.len(), DIRECTORIES as usize);
        let declared: std::collections::BTreeSet<u64> =
            prepared.files.iter().map(|(_, serial)| *serial).collect();
        for file in &plan.files {
            assert!(
                declared.contains(&file.serial),
                "planned serial {} is not a tree file serial",
                file.serial
            );
        }
    }

    #[test]
    fn plan_is_deterministic_in_the_seed() {
        let left = plan(ENTRIES, DIRECTORIES, 7, TOTAL).expect("plan");
        let right = plan(ENTRIES, DIRECTORIES, 7, TOTAL).expect("plan");
        let left_sizes: Vec<u64> = left.files.iter().map(|file| file.size).collect();
        let right_sizes: Vec<u64> = right.files.iter().map(|file| file.size).collect();
        assert_eq!(left_sizes, right_sizes);
        let other = plan(ENTRIES, DIRECTORIES, 8, TOTAL).expect("plan");
        let other_sizes: Vec<u64> = other.files.iter().map(|file| file.size).collect();
        assert_ne!(left_sizes, other_sizes, "a different seed must permute");
        assert_eq!(other.total_bytes, TOTAL);
    }

    #[test]
    fn a_short_entry_count_is_refused_rather_than_reshaped() {
        assert!(plan(999, DIRECTORIES, 1, TOTAL).is_err());
    }
}

