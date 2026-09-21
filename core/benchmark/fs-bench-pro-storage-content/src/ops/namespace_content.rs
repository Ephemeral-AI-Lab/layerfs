//! The `namespace-*` byte plans: the same *work* as the v0.1.6 fixtures.
//!
//! `c1.fs.build-scale`'s recipe is a pure function of `(profile, entries,
//! directories, seed)` and materializes no file content at all — every file's
//! `content_root` is `ObjectId::for_bytes` over a label string. This module is the
//! other half: a plan that says how many bytes each file carries, so a row can
//! construct that content and save it.
//!
//! **Two declarations, both ported rather than invented.** The reference harness
//! (`benchmark/fs-bench-pro/families/init_namespace/mod.rs`) registers four
//! `NamespaceScenario`s, and each one is a `(files, directories, logical bytes,
//! anchors, band mix)` tuple rather than a total that scales. This module carries
//! two of them, because they are the two this harness registers rows for:
//!
//! | | `Declaration::TEN_THOUSAND` | `Declaration::LARGE` |
//! | --- | ---: | ---: |
//! | reference id | `namespace-10000` | `namespace-100000` |
//! | reference alias | `namespace-10000-files-300mb` | `namespace-100000-files-500mb` |
//! | files | 10,000 | 100,000 |
//! | directories | 100 | 1,000 |
//! | logical bytes | 300,000,000 | 500,000,000 |
//! | anchors | 1 × 100,000,000 | **2 × 100,000,000** |
//! | empty / tiny / small / medium | 100 / 7,899 / 1,500 / 500 | 1,000 / 78,998 / 15,000 / 5,000 |
//!
//! **The large declaration is a scaled declaration, not a fallback.** `1,000 +
//! 78,998 + 15,000 + 5,000 + 2 = 100,000` exactly; empty (100 → 1,000), small
//! (1,500 → 15,000) and medium (500 → 5,000) scale exactly ×10, the anchor
//! count goes **1 → 2** rather than ×10, and **tiny is the balancing band**:
//! 78,998 is eight more than 7,899 × 10. `anchor_bytes` is *per anchor*: the
//! reference computes its scenario total as
//! `anchor_files.checked_mul(anchor_bytes)` (`benchmark/fs-bench-pro/src/main.rs`),
//! so the large declaration carries 200,000,000 bytes of anchors and distributes
//! 300,000,000 over the rest.
//!
//! **What is matched.** The reference declares each scenario's class counts, its
//! tree shape, its anchor size and its byte total, and the equivalent declaration
//! here reproduces all of them: [`Declaration::classes`] must equal the declared
//! band counts and [`plan`] refuses rather than reshaping if they cannot be placed.
//!
//! **What is not matched, and why.** The reference permutes its class bands with a
//! SHA-256 sort key. This harness has no SHA-256 dependency and `AGENTS.md` §4
//! forbids adding one, so the band permutation here is derived from the harness's
//! own `fixture::noise` primitive. The arithmetic that turns weights into sizes is
//! the same largest-remainder pass, but the *permutation* differs, so individual
//! paths do not receive the sizes they receive in the reference. The band counts,
//! the tree shape and the byte total are identical; the per-path assignment is a
//! declared difference, not a hidden one.
//!
//! The declared ranges are **weight bands, not size caps**: the reference's own
//! declared weights sum to 102,555,546 bytes and the plan is then scaled up to the
//! declared logical total, so a "tiny" file there is larger than 8 bytes. Sizes
//! here are therefore not confined to the bands either.

use layerfs_content::ObjectId;

use crate::fixture;

/// Files one directory holds, so the plan's paths, the tree recipe's batches and
/// the driver's noise index all agree on one index space.
///
/// **This constant is load-bearing at every entry count, not only at 10,000.** The
/// index of a file is `directory * FILES_PER_DIRECTORY + ordinal` with
/// `ordinal = position / directories`, so a declaration whose `directories` is not
/// `entries / FILES_PER_DIRECTORY` reuses an index. At `entries = 100,000` with 100
/// directories the ordinal runs to 999 and position 10,000 reuses index 100: 100,000
/// files collapse onto 10,000 distinct serials. The 1,000-directory declaration is
/// what keeps the index space injective, which is why [`Declaration::LARGE`] declares it.
pub const FILES_PER_DIRECTORY: u64 = 100;

/// Tiny band: count, inclusive lower weight, inclusive upper weight.
pub const TINY: (u64, u64, u64) = (7_899, 1, 8);
/// Small band.
pub const SMALL: (u64, u64, u64) = (1_500, 32, 256);
/// Medium band.
pub const MEDIUM: (u64, u64, u64) = (500, 1_024, 8_192);

/// Empty files the ten-thousand-entry fixture declares.
pub const EMPTY_FILES: u64 = 100;
/// Anchor files the ten-thousand-entry fixture declares.
pub const ANCHOR_FILES: u64 = 1;
/// Bytes a single anchor carries. Per anchor file, never per declaration.
pub const ANCHOR_BYTES: u64 = 100_000_000;

/// Declared class counts, in band order: empty, tiny, small, medium, anchor.
pub const DECLARED_CLASSES: [u64; 5] =
    [EMPTY_FILES, TINY.0, SMALL.0, MEDIUM.0, ANCHOR_FILES];

/// The band mix one declaration states.
///
/// A band is `(count, inclusive lower weight, inclusive upper weight)`, exactly as
/// the reference states it. The mix is data rather than a constant because the
/// 100,000-entry declaration is a *different* mix at every band rather than the
/// 10,000-entry one repeated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bands {
    /// Empty files: zero bytes, and a slot each.
    pub empty: u64,
    /// Tiny band.
    pub tiny: (u64, u64, u64),
    /// Small band.
    pub small: (u64, u64, u64),
    /// Medium band.
    pub medium: (u64, u64, u64),
    /// Anchor files. Each carries [`Declaration::anchor_bytes`].
    pub anchors: u64,
}

/// The reference harness's 10,000-entry band mix (`namespace-10000`).
pub const TEN_THOUSAND_BANDS: Bands = Bands {
    empty: EMPTY_FILES,
    tiny: TINY,
    small: SMALL,
    medium: MEDIUM,
    anchors: ANCHOR_FILES,
};

/// The reference harness's 100,000-entry band mix (`namespace-100000`).
///
/// Ported field for field from
/// `benchmark/fs-bench-pro/families/init_namespace/mod.rs`; see the module header
/// for the worked arithmetic. The core port's fallback rule cannot produce this
/// mix, because its declared classes are the 10,000-entry ones and everything past
/// them lands in tiny: at `entries = 100,000` it would place 97,899 tiny files and
/// one anchor where the reference places 78,998 tiny files and two.
pub const LARGE_BANDS: Bands = Bands {
    empty: 1_000,
    tiny: (78_998, 1, 8),
    small: (15_000, 32, 256),
    medium: (5_000, 1_024, 8_192),
    anchors: 2,
};

/// What one `namespace-*` row declares: a tree shape, a byte total, and a band mix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Declaration {
    /// Files the plan places.
    pub entries: u32,
    /// Directories the tree places them in; must divide `entries` into
    /// [`FILES_PER_DIRECTORY`]-sized index runs.
    pub directories: u32,
    /// Total logical bytes, anchors included.
    pub total_bytes: u64,
    /// The band mix.
    pub bands: Bands,
    /// Bytes **each** anchor file carries.
    pub anchor_bytes: u64,
}

impl Declaration {
    /// The reference harness's 10,000-entry declaration at its own total.
    pub const TEN_THOUSAND: Self = Self {
        entries: 10_000,
        directories: 100,
        total_bytes: crate::ops::pipeline::NAMESPACE_SCALE_BYTES,
        bands: TEN_THOUSAND_BANDS,
        anchor_bytes: ANCHOR_BYTES,
    };

    /// The reference harness's 100,000-entry declaration at its own total.
    ///
    /// 500,000,000 is **decimal** and is the reference scenario's own number. It is
    /// not `c1.fs.build-scale`'s 100,000-entry rung, which declares 524,288,000
    /// (500 MiB, `c1_fs_build.rs`): both are 100,000 files over 1,000 directories,
    /// they differ in bytes, and this row ports the reference's.
    pub const LARGE: Self = Self {
        entries: 100_000,
        directories: 1_000,
        total_bytes: 500_000_000,
        bands: LARGE_BANDS,
        anchor_bytes: ANCHOR_BYTES,
    };

    /// Declared class counts, in band order: empty, tiny, small, medium, anchor.
    pub const fn classes(&self) -> [u64; 5] {
        [
            self.bands.empty,
            self.bands.tiny.0,
            self.bands.small.0,
            self.bands.medium.0,
            self.bands.anchors,
        ]
    }

    /// Sum of [`Declaration::classes`].
    pub const fn declared(&self) -> u64 {
        self.bands.empty
            + self.bands.tiny.0
            + self.bands.small.0
            + self.bands.medium.0
            + self.bands.anchors
    }

    /// Bytes the declaration's anchors carry together.
    ///
    /// Per anchor, multiplied by the count, exactly as the reference computes its
    /// scenario total. [`plan`] clamps the product to the declared total and
    /// refuses a declaration the clamped anchors leave no room for.
    pub const fn anchor_total(&self) -> u64 {
        match self.bands.anchors.checked_mul(self.anchor_bytes) {
            Some(total) => total,
            None => u64::MAX,
        }
    }
}

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

/// Builds the byte plan for one declaration.
pub fn plan(declaration: &Declaration, seed: u64) -> Result<BytePlan, String> {
    let entries = u64::from(declaration.entries);
    let directories = u64::from(declaration.directories).max(1);
    let total_bytes = declaration.total_bytes;
    if entries == 0 {
        return Err("namespace plan needs at least one file".to_string());
    }
    let declared = declaration.declared();
    if entries < declared {
        return Err(format!(
            "namespace plan needs at least {declared} files, asked for {entries}"
        ));
    }
    // The index space is `directory * FILES_PER_DIRECTORY + ordinal` with
    // `ordinal = position / directories`, so it is injective only while one
    // directory holds no more than `FILES_PER_DIRECTORY` files. A declaration that
    // asks for more reuses an index, which gives two files the same serial and the
    // same constructed-content identity. Refused here rather than discovered as a
    // silently smaller fixture - the failure mode the 100,000-entry row would hit
    // if it kept the 10,000-entry row's 100 directories.
    let per_directory = entries.div_ceil(directories);
    if per_directory > FILES_PER_DIRECTORY {
        return Err(format!(
            "namespace plan index space wraps: {entries} files over {directories} \
             directories is {per_directory} per directory against a \
             FILES_PER_DIRECTORY of {FILES_PER_DIRECTORY}"
        ));
    }

    // Serials follow `fs_fixture`: root is 1, directories are 2.., files follow.
    let first_file_serial = 2 + directories;

    // Bands: the declared counts, then any surplus as more tiny files. The surplus
    // is empty for every declaration this module ports - the 100,000-entry mix is a
    // *scaled* mix, not the 10,000-entry one with 90,000 files left over - but the
    // route stays so a declaration that states fewer classes than it has files is
    // still placed rather than refused.
    let (empty, tiny, small, medium) = (
        declaration.bands.empty,
        declaration.bands.tiny,
        declaration.bands.small,
        declaration.bands.medium,
    );
    let anchors = declaration.bands.anchors;
    let surplus = entries - declared;
    let mut bands: Vec<(u64, u64)> = Vec::with_capacity(entries as usize);
    bands.extend(std::iter::repeat_n((0_u64, 0_u64), empty as usize));
    for role in 0..tiny.0 {
        bands.push((1, relative_weight(tiny.1, tiny.2, role, tiny.0)?));
    }
    for role in 0..small.0 {
        bands.push((2, relative_weight(small.1, small.2, role, small.0)?));
    }
    for role in 0..medium.0 {
        bands.push((3, relative_weight(medium.1, medium.2, role, medium.0)?));
    }
    for role in 0..surplus {
        bands.push((1, relative_weight(tiny.1, tiny.2, role, surplus.max(1))?));
    }
    // Each anchor occupies a slot of its own; their positions are permuted like
    // every other band, and each size comes from `anchor_bytes` rather than from a
    // weight. The count is the declaration's, so a declaration with two anchors
    // places two.
    bands.extend(std::iter::repeat_n((4_u64, 0_u64), anchors as usize));
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

    // The anchors. Each takes the first free slot that carries its class, from a
    // seed-derived offset, so a declaration with one anchor behaves exactly as it
    // did when the anchor was singular and a declaration with two places two
    // distinct paths. The walk is bounded by `entries` slots, so a declaration that
    // declares more anchors than it can place is refused rather than looping.
    let anchor_bytes = declaration.anchor_bytes.min(total_bytes);
    let anchor_total = anchors.saturating_mul(anchor_bytes).min(total_bytes);
    let offset = {
        let bytes = fixture::noise(8, seed ^ 0x414e_4348_4f52_0001);
        u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes")) % entries
    };
    let mut anchor_slots: Vec<usize> = Vec::with_capacity(anchors as usize);
    for step in 0..entries {
        if anchor_slots.len() as u64 == anchors {
            break;
        }
        let slot = ((offset + step) % entries) as usize;
        if assigned[slot].0 == 4 && !anchor_slots.contains(&slot) {
            anchor_slots.push(slot);
        }
    }
    if anchor_slots.len() as u64 != anchors {
        return Err(format!(
            "namespace anchor slots: {anchors} declared, {} placed",
            anchor_slots.len()
        ));
    }

    // Sizes: one Hamilton pass over the whole pool, exactly the arithmetic
    // v0.1.6 applies — `size = 1 + floor(distributable * weight / weight_sum)`,
    // then the remaining bytes to the largest remainders. The anchors are excluded
    // from the pool and their bytes are subtracted **as a total**, because
    // `anchor_bytes` is per anchor: the reference computes its scenario total as
    // `anchor_files * anchor_bytes`.
    let weight_sum: u64 = assigned
        .iter()
        .enumerate()
        .filter(|(slot, (class, _))| !anchor_slots.contains(slot) && *class != 0 && *class != 4)
        .map(|(_, (_, weight))| *weight)
        .sum();
    if weight_sum == 0 {
        return Err("namespace weight sum".to_string());
    }
    let positive = assigned
        .iter()
        .enumerate()
        .filter(|(slot, (class, _))| !anchor_slots.contains(slot) && *class != 0 && *class != 4)
        .count() as u64;
    let distributable = total_bytes
        .checked_sub(anchor_total)
        .and_then(|value| value.checked_sub(positive))
        .ok_or("namespace byte budget")?;

    let mut sizes = vec![0_u64; entries as usize];
    let mut remainders: Vec<(usize, u64)> = Vec::with_capacity(assigned.len());
    let mut floor_sum = 0_u64;
    for (slot, (class, weight)) in assigned.iter().enumerate() {
        if anchor_slots.contains(&slot) {
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
            anchor: anchor_slots.contains(&(position as usize)),
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
        let plan = plan(&Declaration::TEN_THOUSAND, 1).expect("plan");
        assert_eq!(plan.files.len(), ENTRIES as usize);
        assert_eq!(plan.class_counts(), DECLARED_CLASSES);
        assert_eq!(plan.total_bytes, TOTAL);
        assert_eq!(plan.largest(), ANCHOR_BYTES);
        assert_eq!(plan.non_empty(), (ENTRIES - EMPTY_FILES as u32) as usize);
    }

    #[test]
    fn anchor_is_unique_and_carries_the_declared_bytes() {
        let plan = plan(&Declaration::TEN_THOUSAND, 1).expect("plan");
        let anchors: Vec<_> = plan.files.iter().filter(|file| file.anchor).collect();
        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].size, ANCHOR_BYTES);
    }

    #[test]
    fn directories_and_serials_match_the_tree_recipe() {
        let plan = plan(&Declaration::TEN_THOUSAND, 1).expect("plan");
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
        let left = plan(&Declaration::TEN_THOUSAND, 7).expect("plan");
        let right = plan(&Declaration::TEN_THOUSAND, 7).expect("plan");
        let left_sizes: Vec<u64> = left.files.iter().map(|file| file.size).collect();
        let right_sizes: Vec<u64> = right.files.iter().map(|file| file.size).collect();
        assert_eq!(left_sizes, right_sizes);
        let other = plan(&Declaration::TEN_THOUSAND, 8).expect("plan");
        let other_sizes: Vec<u64> = other.files.iter().map(|file| file.size).collect();
        assert_ne!(left_sizes, other_sizes, "a different seed must permute");
        assert_eq!(other.total_bytes, TOTAL);
    }

    #[test]
    fn a_short_entry_count_is_refused_rather_than_reshaped() {
        let short = Declaration {
            entries: 999,
            ..Declaration::TEN_THOUSAND
        };
        assert!(plan(&short, 1).is_err());
    }

    /// The 100,000-entry declaration is placed at its own bands and its own total,
    /// and it is not the 10,000-entry shape repeated.
    ///
    /// This is the assertion that separates the port from the port's old fallback:
    /// at `entries = 100,000` the 10,000-entry class declaration would place 97,899
    /// tiny files and one anchor, where the reference declares 78,998 tiny files and
    /// two anchors over 200,000,000 bytes.
    #[test]
    fn the_large_declaration_is_the_scaled_mix_not_the_small_one_repeated() {
        let large = plan(&Declaration::LARGE, 1).expect("plan");
        assert_eq!(large.files.len(), 100_000);
        assert_eq!(large.class_counts(), Declaration::LARGE.classes());
        assert_eq!(large.class_counts(), [1_000, 78_998, 15_000, 5_000, 2]);
        assert_eq!(large.total_bytes, 500_000_000);
        assert_eq!(large.largest(), ANCHOR_BYTES);
        assert_eq!(large.non_empty(), 100_000 - 1_000);
        let anchors: Vec<_> = large.files.iter().filter(|file| file.anchor).collect();
        assert_eq!(anchors.len(), 2, "two anchors, not one");
        assert_eq!(
            anchors.iter().map(|file| file.size).sum::<u64>(),
            Declaration::LARGE.anchor_total(),
            "the two anchors carry 2 x 100,000,000 bytes together"
        );
        assert_eq!(Declaration::LARGE.anchor_total(), 200_000_000);

        // The bands are a scaled mix, and only three of the five are an exact x10:
        // empty (100 -> 1,000), small (1,500 -> 15,000) and medium (500 -> 5,000)
        // scale exactly, the anchor goes 1 -> 2 rather than x10, and **tiny is the
        // balancing band**: 100,000 - 1,000 - 15,000 - 5,000 - 2 = 78,998, which is
        // eight more than 7,899 x 10 = 78,990. Asserting a flat x10 here failed when
        // it was first written, which is how the eight files were noticed; the
        // declaration is the reference's and this arithmetic is the check on it.
        let tenth = plan(&Declaration::TEN_THOUSAND, 1).expect("plan");
        let tens = tenth.class_counts();
        assert_eq!(Declaration::LARGE.classes()[0], tens[0] * 10, "empty scales x10");
        assert_eq!(Declaration::LARGE.classes()[2], tens[2] * 10, "small scales x10");
        assert_eq!(Declaration::LARGE.classes()[3], tens[3] * 10, "medium scales x10");
        assert_eq!(
            Declaration::LARGE.classes()[1],
            tens[1] * 10 + 8,
            "tiny is the balancing band: eight more than x10"
        );
        assert_eq!(
            Declaration::LARGE.classes()[4],
            2,
            "the anchor count goes 1 -> 2, not x10"
        );
        assert_eq!(
            Declaration::LARGE.classes().iter().sum::<u64>(),
            100_000,
            "the five bands must account for every file exactly once"
        );
    }

    /// The large declaration's serials are injective over its own tree, because it
    /// declares 1,000 directories rather than 100.
    #[test]
    fn the_large_declaration_holds_one_hundred_files_per_directory() {
        let plan = plan(&Declaration::LARGE, 1).expect("plan");
        let recipe = crate::ops::fs_fixture::Recipe {
            profile: "binary-v1",
            entries: Declaration::LARGE.entries,
            directories: Declaration::LARGE.directories,
            seed: 1,
        };
        let prepared = recipe.prepare();
        assert_eq!(plan.files.len(), prepared.files.len());
        let directories: std::collections::BTreeSet<u32> =
            plan.files.iter().map(|file| file.directory).collect();
        assert_eq!(directories.len(), 1_000);
        let declared: std::collections::BTreeSet<u64> = prepared
            .files
            .iter()
            .map(|(_, serial)| *serial)
            .collect();
        assert_eq!(declared.len(), 100_000, "the tree itself must be injective");
        for file in &plan.files {
            assert!(
                declared.contains(&file.serial),
                "planned serial {} is not a tree file serial",
                file.serial
            );
        }
        let serials: std::collections::BTreeSet<u64> =
            plan.files.iter().map(|file| file.serial).collect();
        assert_eq!(serials.len(), 100_000, "planned serials must be injective");
    }

    /// The index space is a property of the declaration, and a wrapped one is
    /// refused rather than silently collapsed onto fewer serials.
    #[test]
    fn a_declaration_that_wraps_the_index_space_is_refused() {
        let wrapped = Declaration {
            directories: 100,
            ..Declaration::LARGE
        };
        let error = plan(&wrapped, 1).expect_err("100,000 files over 100 directories wraps");
        assert!(
            error.contains("index space wraps"),
            "the refusal names the cause: {error}"
        );
    }

    /// A declaration whose anchors alone exceed its total has no pool to distribute.
    #[test]
    fn a_declaration_whose_anchors_do_not_fit_is_refused() {
        let tiny = Declaration {
            total_bytes: 150_000_000,
            ..Declaration::LARGE
        };
        assert!(plan(&tiny, 1).is_err(), "two 100 MB anchors need 200 MB");
    }
}

