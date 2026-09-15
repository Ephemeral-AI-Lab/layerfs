// v0.1.6 compact fixture S and the profile-specific region overrides.
//
// `fixtures.md` §"Compact fixture S" defines sixteen regular files, 2,658,304 B
// and five directories excluding root: eight 4 KiB files in `tiny`, two 64 KiB
// files in `medium`, the three threshold sizes in `boundary`, two 1 MiB files in
// `large` and one 4 KiB witness in `witness`. S is a structural envelope: the
// A/B/Z region overrides each profile declares are part of the fixture identity,
// so two profiles never share a master just because their file sizes match.
//
// The module is product-free: it composes declared recipes only.

use super::dedup_workloads as d;
use super::workspace_common::{Content, Entry, EntryKind};
use super::Result;

pub(crate) const TINY_FILES: usize = 8;
pub(crate) const MEDIUM_FILES: usize = 2;
pub(crate) const LARGE_FILES: usize = 2;
pub(crate) const TINY_LEN: u64 = 4 * 1024;
pub(crate) const MEDIUM_LEN: u64 = 64 * 1024;
pub(crate) const LARGE_LEN: u64 = 1024 * 1024;
pub(crate) const WITNESS_LEN: u64 = 4 * 1024;
pub(crate) const BELOW_LEN: u64 = 131_071;
pub(crate) const EXACT_LEN: u64 = 131_072;
pub(crate) const ABOVE_LEN: u64 = 131_073;
pub(crate) const S_BYTES: u64 = 2_658_304;

/// One declared region override: an offset, a length and the label whose
/// declared bytes replace the region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Region {
    pub(crate) offset: u64,
    pub(crate) len: u64,
    pub(crate) label: &'static str,
    /// The declared recipe ordinal of this region's A/B/Z bytes. The fixture
    /// override and every later profile edit use the same ordinal, so a revisit
    /// to a hot region writes the same declared value again.
    pub(crate) ordinal: usize,
}

/// The region overrides of one fixture-S profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SProfile {
    /// Large-hotset: the three 256 B regions of both 1 MiB files start at A.
    LargeHotset,
    /// Boundary-cycle: no overrides beyond the declared base content.
    BoundaryCycle,
    /// Namespace-inode: fixture S with the declared `tiny` directory roles and
    /// no region overrides.
    NamespaceInode,
    /// The compact branch controls: the two 256 B overwrite regions of the
    /// first medium file start at a distinct Z.
    BranchControl,
}

impl SProfile {
    pub(crate) fn validate(self, seed: u8) -> Result<()> {
        d::seed_label(seed)?;
        Ok(())
    }

    /// The profile of one registered v0.1.6 history kind.
    pub(crate) fn for_kind(kind: &str) -> Result<Self> {
        profile_of(kind)
    }

    /// The declared region overrides, in a fixed order.
    pub(crate) fn regions(self) -> Vec<(String, Vec<Region>)> {
        match self {
            SProfile::LargeHotset => (0..LARGE_FILES)
                .map(|index| {
                    (
                        s_large_path(index),
                        (0..3)
                            .map(|region| Region {
                                offset: hotset_offset(region),
                                len: d::HOTSET_REGION_LEN,
                                label: d::HOTSET_A,
                                ordinal: index * 3 + region,
                            })
                            .collect(),
                    )
                })
                .collect(),
            SProfile::BoundaryCycle | SProfile::NamespaceInode => Vec::new(),
            SProfile::BranchControl => vec![(
                s_medium_path(0),
                (0..2usize)
                    .map(|region| Region {
                        offset: region as u64 * 4_096,
                        len: d::BRANCH_REGION_LEN,
                        label: d::BRANCH_Z,
                        ordinal: region,
                    })
                    .collect(),
            )],
        }
    }
}

/// Head, middle and tail region offsets of one 1 MiB hotset file.
pub(crate) fn hotset_offset(region: usize) -> u64 {
    match region {
        0 => 0,
        1 => (LARGE_LEN - d::HOTSET_REGION_LEN) / 2,
        _ => LARGE_LEN - d::HOTSET_REGION_LEN,
    }
}

pub(crate) fn s_tiny_path(index: usize) -> String {
    format!("tiny/t{index}.dat")
}

pub(crate) fn s_medium_path(index: usize) -> String {
    format!("medium/m{index}.dat")
}

pub(crate) fn s_large_path(index: usize) -> String {
    format!("large/l{index}.dat")
}

pub(crate) fn s_boundary_path(class: &str) -> String {
    format!("boundary/{class}.dat")
}

pub(crate) fn s_witness_path() -> String {
    "witness/w.dat".to_owned()
}

/// The declared initial content of one fixture-S name, before any profile
/// region override.
fn base_content(kind: &str, seed: u8, ordinal: usize, role: &str, len: u64) -> Result<Content> {
    d::content("dedup_branch_history", kind, seed, ordinal, role, len)
}

/// The profile of one registered v0.1.6 history kind.
pub(crate) fn profile_of(kind: &str) -> Result<SProfile> {
    match kind {
        "large-hotset" => Ok(SProfile::LargeHotset),
        "boundary-cycle" => Ok(SProfile::BoundaryCycle),
        "namespace-inode" => Ok(SProfile::NamespaceInode),
        "convergent" | "descendant" => Ok(SProfile::BranchControl),
        other => Err(format!("unknown fixture-S profile {other}").into()),
    }
}

/// Apply one declared region override to a content recipe.
fn with_region(
    kind: &str,
    seed: u8,
    ordinal: usize,
    content: Content,
    region: Region,
) -> Result<Content> {
    if region.offset + region.len > content.len() {
        return Err("v0.1.6 fixture S region outside the file".into());
    }
    let patch = base_content(kind, seed, ordinal, region.label, region.len)?;
    content.splice(region.offset, region.len, patch)
}

/// The complete declared fixture S for one profile.
pub(crate) fn fixture(profile: SProfile, kind: &str, seed: u8) -> Result<Vec<Entry>> {
    profile.validate(seed)?;
    let mut entries = vec![
        Entry::directory("."),
        Entry::directory("tiny"),
        Entry::directory("medium"),
        Entry::directory("boundary"),
        Entry::directory("large"),
        Entry::directory("witness"),
    ];
    let mut regular = Vec::new();
    for index in 0..TINY_FILES {
        regular.push((
            s_tiny_path(index),
            base_content(kind, seed, index, "tiny", TINY_LEN)?,
        ));
    }
    for index in 0..MEDIUM_FILES {
        regular.push((
            s_medium_path(index),
            base_content(kind, seed, 8 + index, "medium", MEDIUM_LEN)?,
        ));
    }
    for (slot, (class, len)) in [("below", BELOW_LEN), ("exact", EXACT_LEN), ("above", ABOVE_LEN)]
        .into_iter()
        .enumerate()
    {
        regular.push((
            s_boundary_path(class),
            base_content(kind, seed, 10 + slot, "boundary", len)?,
        ));
    }
    for index in 0..LARGE_FILES {
        regular.push((
            s_large_path(index),
            base_content(kind, seed, 13 + index, "large", LARGE_LEN)?,
        ));
    }
    regular.push((
        s_witness_path(),
        base_content(kind, seed, 15, "witness", WITNESS_LEN)?,
    ));
    // The profile's region overrides replace declared bytes, so the recipe is
    // an exact declaration rather than a post-hoc observation.
    for (path, regions) in profile.regions() {
        let slot = regular
            .iter()
            .position(|(name, _)| *name == path)
            .ok_or("v0.1.6 fixture S region path")?;
        for region in regions {
            let current = regular[slot].1.clone();
            regular[slot].1 = with_region(kind, seed, region.ordinal, current, region)?;
        }
    }
    let total: u64 = regular.iter().map(|(_, content)| content.len()).sum();
    if total != S_BYTES {
        return Err(format!("v0.1.6 fixture S bytes {total} != {S_BYTES}").into());
    }
    entries.extend(regular.into_iter().map(|(path, content)| Entry::file(path, content)));
    Ok(entries)
}

/// The declared regular-file inventory of fixture S with its lengths.
pub(crate) fn declared_inventory() -> Vec<(String, u64)> {
    let mut rows = Vec::new();
    for index in 0..TINY_FILES {
        rows.push((s_tiny_path(index), TINY_LEN));
    }
    for index in 0..MEDIUM_FILES {
        rows.push((s_medium_path(index), MEDIUM_LEN));
    }
    rows.push((s_boundary_path("below"), BELOW_LEN));
    rows.push((s_boundary_path("exact"), EXACT_LEN));
    rows.push((s_boundary_path("above"), ABOVE_LEN));
    for index in 0..LARGE_FILES {
        rows.push((s_large_path(index), LARGE_LEN));
    }
    rows.push((s_witness_path(), WITNESS_LEN));
    rows
}

/// Read one declared file's bytes out of a declared inventory.
pub(crate) fn bytes_of(entries: &[Entry], path: &str) -> Result<Vec<u8>> {
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .ok_or_else(|| format!("v0.1.6 fixture S path absent: {path}"))?;
    let EntryKind::File(content) = &entry.kind else {
        return Err(format!("v0.1.6 fixture S path is not a file: {path}").into());
    };
    let mut out = Vec::with_capacity(content.len() as usize);
    content.write_to(&mut out)?;
    Ok(out)
}
