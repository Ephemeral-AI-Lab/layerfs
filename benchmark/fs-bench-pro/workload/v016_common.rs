// Single implementation of the v0.1.6 load-bearing fixture, M1 recipes and graph cardinality.
//
// Families, the host orchestrator and the workload helper all read these numbers
// from here, so the roadmap `cases.json` has exactly one executable counterpart
// and the fixture algebra can be checked without the product.
use super::workspace_common::{self as common, Content, Entry, EntryKind};
use super::{dedup_workloads as d, Result};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const MIB: u64 = 1_048_576;

pub(crate) const FAMILY_HISTORY: &str = "dedup_branch_history";
pub(crate) const FAMILY_BOUNDARY: &str = "file_size_transition";
pub(crate) const FAMILY_MIXED: &str = "mixed_load_bearing";
pub(crate) const FAMILY_WORKSPACE: &str = "multi_workspace_development";
pub(crate) const FAMILY_BRANCH: &str = "branch_development";

// ------------------------------------------------------------- byte algebra

pub(crate) const BOUNDARY_BELOW: u64 = 131_071;
pub(crate) const BOUNDARY_EXACT: u64 = 131_072;
pub(crate) const BOUNDARY_ABOVE: u64 = 131_073;
pub(crate) const TINY: u64 = 4_096;
pub(crate) const WITNESS: u64 = 4_096;

pub(crate) const DIRECTORY_NAME_CAP: usize = 100;
/// The initial directory mode of every declared directory.
pub(crate) const DEFAULT_DIRECTORY_MODE: u32 = 0o750;
pub(crate) const RESERVED_PATHS: usize = 16;
pub(crate) const RESERVED_BYTES: u64 = 65_536;

pub(crate) const REFRESH_COHORTS: usize = 16;
pub(crate) const REFRESH_FILES_PER_COHORT: usize = 64;
pub(crate) const REFRESH_POOL_FILES: usize = REFRESH_COHORTS * REFRESH_FILES_PER_COHORT;
pub(crate) const MOVE_SUBTREE_FILES: usize = 32;
pub(crate) const DELETION_SUBTREE_FILES: usize = 32;
pub(crate) const HARDLINK_TARGETS: usize = 8;
pub(crate) const SYMLINK_COUNT: usize = 4;
pub(crate) const EDIT_TARGET_FILES: usize = 8;
pub(crate) const ATTR_EXTRA_FILES: usize = 8;
pub(crate) const SCRATCH_DIRS: usize = 8;
pub(crate) const MOVE_DEST_DIRS: usize = 2;
pub(crate) const MOVE_DESTS: usize = 2;
/// Stage 4 adds twelve names and stage 5 adds one live atomic-save temporary.
pub(crate) const MAX_EXTRA_PATHS: usize = HARDLINK_TARGETS + SYMLINK_COUNT + 1;
pub(crate) const MAX_EXTRA_BYTES: u64 =
    HARDLINK_TARGETS as u64 * TINY + SYMLINK_COUNT as u64 * 64 + TINY;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileClass {
    pub(crate) count: usize,
    pub(crate) size: u64,
}

/// One registered load-bearing size. The class table is the exact
/// `fixtures.md` distribution in declaration order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LoadTier {
    /// Registered data directories excluding root.
    pub(crate) data_dirs: usize,
    pub(crate) empty: usize,
    pub(crate) tiny: usize,
    pub(crate) medium_low: usize,
    pub(crate) medium_low_len: u64,
    pub(crate) medium_high: usize,
    pub(crate) medium_high_len: u64,
    pub(crate) large: usize,
    pub(crate) anchors: usize,
    pub(crate) anchor_len: u64,
}

pub(crate) const L100: LoadTier = LoadTier {
    data_dirs: 53,
    empty: 34,
    tiny: 3_950,
    medium_low: 706,
    medium_low_len: 28_146,
    medium_high: 244,
    medium_high_len: 28_147,
    large: 46,
    anchors: 1,
    anchor_len: 8 * MIB,
};

pub(crate) const L500: LoadTier = LoadTier {
    data_dirs: 303,
    empty: 284,
    tiny: 23_700,
    medium_low: 3_736,
    medium_low_len: 22_566,
    medium_high: 2_064,
    medium_high_len: 22_567,
    large: 195,
    anchors: 2,
    anchor_len: 32 * MIB,
};

impl LoadTier {
    pub(crate) fn classes(&self) -> [FileClass; 9] {
        [
            FileClass { count: self.empty, size: 0 },
            FileClass { count: self.tiny, size: TINY },
            FileClass { count: self.medium_low, size: self.medium_low_len },
            FileClass { count: self.medium_high, size: self.medium_high_len },
            FileClass { count: 1, size: BOUNDARY_BELOW },
            FileClass { count: 1, size: BOUNDARY_EXACT },
            FileClass { count: 1, size: BOUNDARY_ABOVE },
            FileClass { count: self.large, size: MIB },
            FileClass { count: self.anchors, size: self.anchor_len },
        ]
    }

    pub(crate) fn initial_paths(&self) -> usize {
        self.classes().iter().map(|class| class.count).sum()
    }

    pub(crate) fn initial_bytes(&self) -> u64 {
        self.classes().iter().map(|class| class.count as u64 * class.size).sum()
    }

    pub(crate) fn max_paths(&self) -> usize {
        self.initial_paths() + RESERVED_PATHS
    }

    pub(crate) fn max_bytes(&self) -> u64 {
        self.initial_bytes() + RESERVED_BYTES
    }

    /// Directories excluding root: the registered data directories plus two
    /// move destination parents and eight alternating scratch directories.
    pub(crate) fn max_dirs(&self) -> usize {
        self.data_dirs + MOVE_DEST_DIRS + SCRATCH_DIRS
    }
}

pub(crate) fn data_dir(index: usize) -> String {
    format!("d{index:03}")
}

pub(crate) fn file_name(index: usize) -> String {
    format!("f{index:05}")
}

pub(crate) fn ordinal_path(ordinal: usize) -> String {
    format!(
        "{}/{}",
        data_dir(ordinal / DIRECTORY_NAME_CAP),
        file_name(ordinal % DIRECTORY_NAME_CAP)
    )
}

pub(crate) fn pool_path(dir_index: usize, slot: usize) -> String {
    format!("{}/{}", data_dir(dir_index), file_name(slot))
}

// ------------------------------------------------------------------ fixture

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Witnesses {
    pub(crate) empty: String,
    pub(crate) tiny: [String; 2],
    pub(crate) medium: [String; 2],
    pub(crate) boundary_exact: String,
    pub(crate) large: String,
    pub(crate) anchors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EditTargets {
    pub(crate) tiny_pwrite: Vec<String>,
    pub(crate) medium_overwrite: String,
    pub(crate) medium_net_zero: String,
    pub(crate) medium_replace_tail: String,
    pub(crate) sdk_insert: String,
    pub(crate) sdk_delete: String,
    pub(crate) anchor_overwrite: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LinkRoles {
    pub(crate) hardlinks: Vec<(String, String)>,
    pub(crate) symlinks: Vec<(String, String, SymlinkKind)>,
    pub(crate) alias_writes: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymlinkKind {
    Resolvable,
    Dangling,
    SelfLoop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Roles {
    pub(crate) refresh_pool: Vec<String>,
    pub(crate) move_a: Vec<String>,
    pub(crate) move_b: Vec<String>,
    pub(crate) deletion: Vec<String>,
    pub(crate) link_targets: Vec<String>,
    pub(crate) edit_targets: Vec<String>,
    pub(crate) attr_extra: Vec<String>,
    pub(crate) witnesses: Witnesses,
    pub(crate) move_a_root: String,
    pub(crate) move_b_root: String,
    pub(crate) deletion_root: String,
    pub(crate) move_dest: Vec<String>,
    pub(crate) scratch: Vec<String>,
    pub(crate) boundary_below: String,
    pub(crate) boundary_above: String,
    pub(crate) edit: EditTargets,
    pub(crate) links: LinkRoles,
}

pub(crate) struct LoadFixture {
    pub(crate) tier: LoadTier,
    pub(crate) seed: u8,
    pub(crate) roles: Roles,
    pub(crate) entries: Vec<Entry>,
}

impl LoadFixture {
    pub(crate) fn by_path(&self) -> BTreeMap<&str, &Entry> {
        self.entries.iter().map(|entry| (entry.path.as_str(), entry)).collect()
    }

    pub(crate) fn entry(&self, path: &str) -> Result<&Entry> {
        self.by_path()
            .get(path)
            .copied()
            .ok_or_else(|| format!("v0.1.6 path absent from fixture: {path}").into())
    }

    pub(crate) fn content(&self, path: &str) -> Result<&Content> {
        match &self.entry(path)?.kind {
            EntryKind::File(content) => Ok(content),
            _ => Err(format!("v0.1.6 path is not a regular file: {path}").into()),
        }
    }

    pub(crate) fn len(&self, path: &str) -> Result<u64> {
        Ok(self.content(path)?.len())
    }

    /// Every stage-2 target, in declared order (sixteen entries).
    pub(crate) fn main_edit_targets(&self) -> Vec<String> {
        let mut rows = self.roles.edit.tiny_pwrite.clone();
        rows.push(self.roles.edit.medium_overwrite.clone());
        rows.push(self.roles.edit.medium_net_zero.clone());
        rows.push(self.roles.edit.medium_replace_tail.clone());
        rows.push(self.roles.edit.sdk_insert.clone());
        rows.push(self.roles.edit.sdk_delete.clone());
        rows.push(self.roles.edit.anchor_overwrite.clone());
        rows.push(self.roles.boundary_below.clone());
        rows.push(self.roles.boundary_above.clone());
        rows
    }
}

/// Even ordinals use a deterministic seeded generator; odd ordinals use the
/// existing seeded pseudorandom generator. Content class, seed and role are
/// part of fixture identity.
pub(crate) fn background_recipe(seed: u8, role: &str, ordinal: usize, len: u64) -> Result<Content> {
    d::content(FAMILY_MIXED, "v016-background", seed, ordinal, role, len)
}

/// Refresh recipes carry the cohort and the content class. The first two
/// members of every group of four are the exact recurrent pair.
pub(crate) fn refresh_recipe(seed: u8, index: usize) -> Result<Content> {
    let cohort = index / REFRESH_FILES_PER_COHORT;
    let slot = index % REFRESH_FILES_PER_COHORT;
    let class = match slot % 4 {
        0 | 1 => "recurring",
        2 => "local",
        _ => "unique0",
    };
    d::content(
        FAMILY_MIXED,
        "v016-refresh",
        seed,
        index,
        &format!("{class}-c{cohort}"),
        TINY,
    )
}

/// Deterministic fixture generation.
///
/// One ordinal space and one segment list drive both the byte classes and the
/// paths, so the class multiset and the packed layout cannot disagree. Layout:
///
/// * the tiny class's first 1,136 ordinals belong to the role segments: 1,024
///   refresh files plus the deletion subtree and the two movable subtrees;
/// * the refresh pool packs 100 names per directory;
/// * the deletion subtree and the two movable subtrees each own one directory
///   whose remaining 68 slots stay unused;
/// * every other class packs 100 names per directory from `d000`, then two move
///   destination parents and eight scratch directories follow.
pub(crate) fn load_fixture(tier: &LoadTier, seed: u8) -> Result<LoadFixture> {
    if !(1..=3).contains(&seed) {
        return Err("v0.1.6 seed must be 1, 2 or 3".into());
    }
    let classes = tier.classes();
    let tiny_role_files = REFRESH_POOL_FILES + DELETION_SUBTREE_FILES + 2 * MOVE_SUBTREE_FILES;
    if classes[1].count < tiny_role_files {
        return Err("v0.1.6 tiny class smaller than its role segments".into());
    }
    let total: usize = classes.iter().map(|class| class.count).sum();
    if total != tier.initial_paths() {
        return Err(format!("v0.1.6 class multiset {total} != {}", tier.initial_paths()).into());
    }
    let mut class_base = [0usize; 9];
    let mut cursor = 0usize;
    for (index, class) in classes.iter().enumerate() {
        class_base[index] = cursor;
        cursor += class.count;
    }

    // Ordinary classes in declaration order. The tiny class's role ordinals are
    // written separately below, from the dedicated directories.
    let mut entries: Vec<Entry> = Vec::with_capacity(total + tier.max_dirs() + 1);
    entries.push(Entry::directory("."));
    let mut ordinal = 0usize;
    let mut written = 0usize;
    for (class_index, class) in classes.iter().enumerate() {
        for index in 0..class.count {
            if class_index == 1 && index < tiny_role_files {
                ordinal += 1;
                continue;
            }
            let content = if class.size == 0 {
                Content::Zero { len: 0 }
            } else {
                background_recipe(seed, "background", ordinal, class.size)?
            };
            entries.push(Entry::file(ordinal_path(written), content));
            ordinal += 1;
            written += 1;
        }
    }
    if ordinal != total {
        return Err(format!("v0.1.6 ordinal accounting {ordinal} != {total}").into());
    }
    // The packed classes occupy the first `packed_dirs` directories; the role
    // directories follow them.
    let packed_dirs = written.div_ceil(DIRECTORY_NAME_CAP);

    // The role directories follow the packed classes: the refresh pool, then
    // the deletion subtree, then the two movable subtrees.
    let pool_dirs = REFRESH_POOL_FILES.div_ceil(DIRECTORY_NAME_CAP);
    let pool_base = packed_dirs;
    let subtree_base = pool_base + pool_dirs;
    let deletion_root = data_dir(subtree_base);
    let move_root_b = data_dir(subtree_base + 1);
    let move_root_a = data_dir(subtree_base + 2);
    let refresh_pool: Vec<String> = (0..REFRESH_POOL_FILES)
        .map(|index| pool_path(pool_base + index / DIRECTORY_NAME_CAP, index % DIRECTORY_NAME_CAP))
        .collect();
    for (index, path) in refresh_pool.iter().enumerate() {
        entries.push(Entry::file(path.clone(), refresh_recipe(seed, index)?));
    }
    let deletion: Vec<String> = (0..DELETION_SUBTREE_FILES)
        .map(|index| pool_path(subtree_base, index))
        .collect();
    let move_b: Vec<String> = (0..MOVE_SUBTREE_FILES)
        .map(|index| pool_path(subtree_base + 1, index))
        .collect();
    let move_a: Vec<String> = (0..MOVE_SUBTREE_FILES)
        .map(|index| pool_path(subtree_base + 2, index))
        .collect();
    for (index, path) in deletion.iter().enumerate() {
        entries.push(Entry::file(
            path.clone(),
            background_recipe(seed, "deletion", index, TINY)?,
        ));
    }
    for (index, path) in move_b.iter().enumerate() {
        entries.push(Entry::file(
            path.clone(),
            background_recipe(seed, "move-b", index, TINY)?,
        ));
    }
    for (index, path) in move_a.iter().enumerate() {
        entries.push(Entry::file(
            path.clone(),
            background_recipe(seed, "move-a", index, TINY)?,
        ));
    }
    let data_dirs = subtree_base + 3;
    if data_dirs != tier.data_dirs {
        return Err(format!(
            "v0.1.6 layout uses {data_dirs} data directories, tier declares {}",
            tier.data_dirs
        )
        .into());
    }
    let move_dest = vec![data_dir(data_dirs), data_dir(data_dirs + 1)];
    let scratch: Vec<String> = (0..SCRATCH_DIRS)
        .map(|index| data_dir(data_dirs + MOVE_DEST_DIRS + index))
        .collect();
    // Every registered data directory is an explicit entry, so the fixture
    // declares the complete namespace inventory.
    for index in 0..data_dirs {
        entries.push(Entry::directory(data_dir(index)));
    }
    for path in move_dest.iter().chain(scratch.iter()) {
        entries.push(Entry::directory(path.clone()));
    }

    // Role pools bind to logical IDs, never to directory iteration order. The
    // tiny class reserves its first 1,234 ordinals (the 1,136 role files plus a
    // 98-slot headroom), so the link, edit and attribute pools begin at the
    // tiny class ordinal 1,234.
    let tiny_file = |class_ordinal: usize| -> Result<String> {
        if class_ordinal < tiny_role_files || class_ordinal >= classes[1].count {
            return Err(format!("v0.1.6 tiny role ordinal {class_ordinal} out of range").into());
        }
        Ok(ordinal_path(
            class_base[0] + classes[0].count + class_ordinal - tiny_role_files,
        ))
    };
    let link_targets: Vec<String> = (0..HARDLINK_TARGETS)
        .map(|index| tiny_file(1_136 + index))
        .collect::<Result<_>>()?;
    let edit_targets: Vec<String> = (0..EDIT_TARGET_FILES)
        .map(|index| tiny_file(1_144 + index))
        .collect::<Result<_>>()?;
    let attr_extra: Vec<String> = (0..ATTR_EXTRA_FILES)
        .map(|index| tiny_file(1_152 + index))
        .collect::<Result<_>>()?;
    let tiny_pwrite: Vec<String> = (0..8)
        .map(|index| tiny_file(1_160 + index))
        .collect::<Result<_>>()?;

    // Witnesses and the remaining targets are ordinary class members. The
    // packed layout skips the tiny class's reserved role ordinals, so a class
    // ordinal after the first class maps into the packed space by subtracting
    // those reservations.
    let packed = |class: usize, offset: usize| -> String {
        // Class ordinals below the tiny class's reservation count are already
        // inside the packed space.
        let base = class_base[class].saturating_sub(tiny_role_files.min(class_base[class]));
        ordinal_path(base + offset)
    };
    let witnesses = Witnesses {
        empty: ordinal_path(class_base[0]),
        tiny: [
            packed(1, 1_400),
            packed(1, 1_401),
        ],
        medium: [packed(2, 2), packed(3, 1)],
        boundary_exact: packed(4, 1),
        large: packed(7, 1),
        anchors: (0..tier.anchors).map(|index| packed(8, index)).collect(),
    };
    let edit = EditTargets {
        tiny_pwrite,
        medium_overwrite: packed(2, 0),
        medium_net_zero: packed(2, 1),
        medium_replace_tail: packed(3, 0),
        sdk_insert: packed(7, 0),
        sdk_delete: packed(7, 2),
        anchor_overwrite: packed(8, 0),
    };
    let links = link_roles_of(&move_dest, &link_targets)?;
    let fixture = LoadFixture {
        tier: *tier,
        seed,
        roles: Roles {
            refresh_pool,
            move_a,
            move_b,
            deletion,
            link_targets,
            edit_targets,
            attr_extra,
            witnesses,
            move_a_root: move_root_a,
            move_b_root: move_root_b,
            deletion_root,
            move_dest,
            scratch,
            boundary_below: packed(4, 0),
            boundary_above: packed(6, 0),
            edit,
            links,
        },
        entries,
    };
    validate_load_fixture(&fixture)?;
    Ok(fixture)
}

fn link_roles_of(move_dest: &[String], link_targets: &[String]) -> Result<LinkRoles> {
    let mut hardlinks = Vec::new();
    for (index, target) in link_targets.iter().enumerate() {
        hardlinks.push((format!("{}/hl{index:02}", move_dest[0]), target.clone()));
    }
    let mut symlinks = Vec::new();
    for (index, target) in link_targets.iter().enumerate().take(SYMLINK_COUNT) {
        let kind = match index {
            0 | 1 => SymlinkKind::Resolvable,
            2 => SymlinkKind::Dangling,
            _ => SymlinkKind::SelfLoop,
        };
        let link_path = format!("{}/sl{index:02}", move_dest[1]);
        let target = match kind {
            // Relative to the link's own parent: one level below the fixture
            // root while the populated directory sits under `move_dest`, and at
            // the fixture root in the cycles that keep it in place.
            SymlinkKind::Resolvable => format!("../{target}"),
            SymlinkKind::Dangling => "ghost-v016".to_string(),
            SymlinkKind::SelfLoop => ".".to_string(),
        };
        symlinks.push((link_path, target, kind));
    }
    Ok(LinkRoles {
        hardlinks,
        symlinks,
        alias_writes: link_targets.iter().take(4).cloned().collect(),
    })
}

/// Validate the exact fixture contract. Product-free and deterministic.
pub(crate) fn validate_load_fixture(fixture: &LoadFixture) -> Result<()> {
    let tier = fixture.tier;
    let files = fixture
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
        .count();
    if files != tier.initial_paths() {
        return Err(format!("v0.1.6 file count {files} != {}", tier.initial_paths()).into());
    }
    let bytes = common::validate_entries(&fixture.entries)?;
    if bytes != tier.initial_bytes() {
        return Err(format!("v0.1.6 logical bytes {bytes} != {}", tier.initial_bytes()).into());
    }
    if tier.initial_paths() + RESERVED_PATHS != tier.max_paths()
        || tier.initial_bytes() + RESERVED_BYTES != tier.max_bytes()
    {
        return Err("v0.1.6 reserved-name arithmetic".into());
    }
    if tier.initial_paths() + MAX_EXTRA_PATHS > tier.max_paths()
        || tier.initial_bytes() + MAX_EXTRA_BYTES > tier.max_bytes()
    {
        return Err("v0.1.6 stage high-water exceeds the reserved capacity".into());
    }
    let declared: BTreeSet<&str> = fixture
        .entries
        .iter()
        .flat_map(|entry| match &entry.kind {
            EntryKind::Directory => Some(entry.path.as_str()),
            EntryKind::File(_) => entry.path.split('/').next(),
            _ => None,
        })
        .collect();
    if declared.len() - 1 != tier.max_dirs() {
        return Err(format!(
            "v0.1.6 directory count {} != {}",
            declared.len() - 1,
            tier.max_dirs()
        )
        .into());
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut per_dir: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &fixture.entries {
        if !seen.insert(entry.path.as_str()) {
            return Err(format!("v0.1.6 duplicate path {}", entry.path).into());
        }
        if entry.path.split('/').count() > 3 {
            return Err(format!("v0.1.6 path depth for {}", entry.path).into());
        }
        if matches!(entry.kind, EntryKind::File(_)) {
            let dir = entry.path.split('/').next().ok_or("v0.1.6 path shape")?;
            *per_dir.entry(dir).or_default() += 1;
        }
    }
    if per_dir.values().any(|count| *count > DIRECTORY_NAME_CAP) {
        return Err("v0.1.6 data directory name cap exceeded".into());
    }
    let mut pool: BTreeSet<&str> = BTreeSet::new();
    for (label, rows) in [
        ("refresh", &fixture.roles.refresh_pool),
        ("move-a", &fixture.roles.move_a),
        ("move-b", &fixture.roles.move_b),
        ("deletion", &fixture.roles.deletion),
        ("link", &fixture.roles.link_targets),
        ("edit", &fixture.roles.edit_targets),
        ("attr", &fixture.roles.attr_extra),
    ] {
        for path in rows {
            if !pool.insert(path.as_str()) {
                return Err(format!("v0.1.6 role pool overlap at {label}:{path}").into());
            }
        }
    }
    let mut main: BTreeSet<String> = BTreeSet::new();
    for path in fixture.main_edit_targets() {
        if !main.insert(path.clone()) {
            return Err(format!("v0.1.6 duplicate stage-2 target {path}").into());
        }
    }
    if main.len() != 16 {
        return Err("v0.1.6 stage-2 target cardinality".into());
    }
    // Every declared role path must exist in the entry list and every parent
    // directory must be declared, so a stage cannot fail on a missing path.
    let declared_paths: BTreeSet<&str> =
        fixture.entries.iter().map(|entry| entry.path.as_str()).collect();
    let directories: BTreeSet<&str> = fixture
        .entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Directory))
        .map(|entry| entry.path.as_str())
        .collect();
    let mut declared_roles: Vec<(&str, &String)> = Vec::new();
    for (label, rows) in [
        ("refresh", &fixture.roles.refresh_pool),
        ("move-a", &fixture.roles.move_a),
        ("move-b", &fixture.roles.move_b),
        ("deletion", &fixture.roles.deletion),
        ("link", &fixture.roles.link_targets),
        ("edit", &fixture.roles.edit_targets),
        ("attr", &fixture.roles.attr_extra),
    ] {
        for path in rows {
            declared_roles.push((label, path));
        }
    }
    let stage2_targets = fixture.main_edit_targets();
    for path in &stage2_targets {
        declared_roles.push(("stage2", path));
    }
    for path in fixture.roles.move_dest.iter().chain(&fixture.roles.scratch) {
        if !directories.contains(path.as_str()) {
            return Err(format!("v0.1.6 role directory is not declared: {path}").into());
        }
    }
    for (label, path) in declared_roles {
        if !declared_paths.contains(path.as_str()) {
            return Err(format!("v0.1.6 role path is not declared ({label}): {path}").into());
        }
        let (parent, _) = path
            .rsplit_once('/')
            .ok_or_else(|| format!("v0.1.6 role path has no parent: {path}"))?;
        if !directories.contains(parent) {
            return Err(format!("v0.1.6 role parent is not declared: {parent}").into());
        }
    }
    for entry in &fixture.entries {
        if entry.path == "." || matches!(entry.kind, EntryKind::Directory) {
            continue;
        }
        let (parent, _) = entry
            .path
            .rsplit_once('/')
            .ok_or_else(|| format!("v0.1.6 entry path has no parent: {}", entry.path))?;
        if !directories.contains(parent) {
            return Err(format!("v0.1.6 entry parent is not declared: {}", entry.path).into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- M1 recipes

pub(crate) const CYCLE_COMMITS: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CycleCounters {
    pub(crate) created_commits: usize,
    pub(crate) regular_unlinks: usize,
    pub(crate) ordinary_regular_creates: usize,
    pub(crate) temporary_regular_creates: usize,
    pub(crate) rename_overwrites: usize,
    pub(crate) main_edit_targets: usize,
    pub(crate) sdk_edit_calls: usize,
    pub(crate) sdk_edit_members: usize,
    pub(crate) sdk_batch_calls: usize,
    pub(crate) hardlink_creates: usize,
    pub(crate) hardlink_unlinks: usize,
    pub(crate) symlink_creates: usize,
    pub(crate) symlink_unlinks: usize,
    pub(crate) alias_probe_writes: usize,
    pub(crate) unlinked_descriptor_writes: usize,
    pub(crate) populated_directory_moves: usize,
    pub(crate) directory_creates: usize,
    pub(crate) directory_removes: usize,
    pub(crate) file_chmod: usize,
    pub(crate) directory_chmod: usize,
    pub(crate) explicit_mtime_calls: usize,
    pub(crate) expected_eexist_probes: usize,
    pub(crate) posix_helper_executions: usize,
}

pub(crate) const CYCLE: CycleCounters = CycleCounters {
    created_commits: 5,
    // Explicit `unlink` calls: sixty-four refresh paths in stage 1, the
    // thirty-two subtree members in stage 5, and the twelve persisted links
    // the stage-5 recipe removes.
    regular_unlinks: 108,
    ordinary_regular_creates: 96,
    temporary_regular_creates: 4,
    rename_overwrites: 4,
    main_edit_targets: 16,
    sdk_edit_calls: 2,
    sdk_edit_members: 2,
    sdk_batch_calls: 0,
    hardlink_creates: 8,
    hardlink_unlinks: 8,
    symlink_creates: 4,
    symlink_unlinks: 4,
    alias_probe_writes: 4,
    unlinked_descriptor_writes: 1,
    populated_directory_moves: 2,
    directory_creates: 9,
    directory_removes: 9,
    file_chmod: 16,
    directory_chmod: 4,
    explicit_mtime_calls: 16,
    expected_eexist_probes: 1,
    posix_helper_executions: 5,
};

/// The refresh cohort for cycle `c` (1-based). The first two cycles both visit
/// cohort 0, so K10 already contains temporal recurrence.
pub(crate) fn cohort_for_cycle(cycle: usize) -> Result<usize> {
    if cycle == 0 {
        return Err("v0.1.6 cycles are 1-based".into());
    }
    Ok(if cycle % 2 == 1 { 0 } else { (cycle / 2 - 1) % REFRESH_COHORTS })
}

/// Per-cohort visit count after cycles `1..=cycle`.
pub(crate) fn cohort_visit_counts(cycle: usize) -> Result<Vec<usize>> {
    let mut visits = vec![0usize; REFRESH_COHORTS];
    for current in 1..=cycle {
        visits[cohort_for_cycle(current)?] += 1;
    }
    Ok(visits)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RefreshClass {
    Recurring,
    Local,
    Unique,
}

pub(crate) fn refresh_class(slot: usize) -> RefreshClass {
    match slot % 4 {
        0 | 1 => RefreshClass::Recurring,
        2 => RefreshClass::Local,
        _ => RefreshClass::Unique,
    }
}

/// The recurrent value for a cohort whose prior visit count is `visits`:
/// initial A, first visit B, second A. A historical fork inherits the count.
pub(crate) fn recurrent_value(prior_visits: usize) -> u8 {
    if prior_visits % 2 == 0 { 1 } else { 0 }
}

/// Stage-1 content for one cycle. Unique content carries the cycle and the
/// branch salt; recurrent content deliberately carries neither.
pub(crate) fn stage1_contents(
    fixture: &LoadFixture,
    cycle: usize,
    branch_salt: &str,
) -> Result<Vec<(String, Content)>> {
    let visits = cohort_visit_counts(cycle)?;
    let cohort = cohort_for_cycle(cycle)?;
    let value = recurrent_value(visits[cohort] - 1);
    let mut rows = Vec::with_capacity(REFRESH_FILES_PER_COHORT);
    for slot in 0..REFRESH_FILES_PER_COHORT {
        let index = cohort * REFRESH_FILES_PER_COHORT + slot;
        let path = fixture.roles.refresh_pool[index].clone();
        let role = match refresh_class(slot) {
            RefreshClass::Recurring => {
                format!("recurring-{}-c{cohort}", if value == 0 { "a" } else { "b" })
            }
            RefreshClass::Local => format!("local-c{cohort}"),
            RefreshClass::Unique => format!("unique-c{cycle}-{branch_salt}"),
        };
        rows.push((
            path,
            d::content(FAMILY_MIXED, "v016-refresh", fixture.seed, index, &role, TINY)?,
        ));
    }
    Ok(rows)
}

/// The sixteen chmod and mtime targets: the eight canonical hardlink targets
/// plus eight additional tiny files.
pub(crate) fn attribute_targets(fixture: &LoadFixture) -> Result<Vec<String>> {
    let mut rows = fixture.roles.link_targets.clone();
    rows.extend(fixture.roles.attr_extra.iter().cloned());
    if rows.len() != CYCLE.file_chmod {
        return Err("v0.1.6 attribute target cardinality".into());
    }
    Ok(rows)
}

/// Explicit attribute-stage modes alternate 0600/0640 for files, 0700/0750 for
/// directories, so a stage-4 mutation always differs from the initial 0640/0750.
pub(crate) fn attribute_mode(cycle: usize, file: bool) -> u32 {
    match (file, cycle % 2 == 1) {
        (true, true) => 0o600,
        (true, false) => 0o640,
        (false, true) => 0o700,
        (false, false) => 0o750,
    }
}

pub(crate) const ATTRIBUTE_MTIME_NS: u32 = 123_456_789;

pub(crate) fn attribute_mtime(cycle: usize, branch_tag: u64) -> i64 {
    1_700_000_000 + 10 * cycle as i64 + branch_tag as i64
}

/// Stage-3 scratch names for one cycle. The eight scratch directories are
/// removed before their eight replacement names are created; odd cycles remove
/// the registered names and create the alternating set, even cycles remove the
/// alternates and restore the registered names. Every cycle therefore ends with
/// exactly eight scratch directories in place.
pub(crate) fn scratch_for_cycle(fixture: &LoadFixture, cycle: usize) -> (Vec<String>, Vec<String>) {
    let names = fixture.roles.scratch.clone();
    let alternate: Vec<String> = names
        .iter()
        .map(|path| format!("{path}x"))
        .collect();
    if cycle % 2 == 1 {
        (names, alternate)
    } else {
        (alternate, names)
    }
}

/// Stage-3 move target for one cycle: odd cycles move the populated subtree
/// under its destination parent, even cycles move it back.
pub(crate) fn move_targets(fixture: &LoadFixture, cycle: usize) -> (String, String) {
    if cycle % 2 == 1 {
        (
            format!("{}/{}", fixture.roles.move_dest[0], fixture.roles.move_a_root),
            format!("{}/{}", fixture.roles.move_dest[1], fixture.roles.move_b_root),
        )
    } else {
        (
            fixture.roles.move_a_root.clone(),
            fixture.roles.move_b_root.clone(),
        )
    }
}

// -------------------------------------------------------------- graph facts

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Topology {
    Sequential,
    Concurrent,
    Branch,
    Four,
}

impl Topology {
    pub(crate) fn branches(self) -> usize {
        match self {
            Self::Sequential => 1,
            Self::Concurrent => 2,
            Self::Branch => 3,
            Self::Four => 4,
        }
    }

    pub(crate) fn trunk_commits(self) -> usize {
        match self {
            Self::Branch => 10,
            _ => 0,
        }
    }

    pub(crate) fn local_commits(self, k: usize, branch: usize) -> Result<usize> {
        if branch >= self.branches() {
            return Err("v0.1.6 branch ordinal out of range".into());
        }
        Ok(match self {
            Self::Branch if branch == 0 => 10,
            _ => k,
        })
    }

    pub(crate) fn total_commits(self, k: usize) -> Result<usize> {
        let mut total = 0;
        for branch in 0..self.branches() {
            total += self.local_commits(k, branch)?;
        }
        Ok(total)
    }

    /// Retained roots: the initial Layer plus every newly created Commit.
    pub(crate) fn roots(self, k: usize) -> Result<usize> {
        Ok(1 + self.total_commits(k)?)
    }

    pub(crate) fn longest_ancestry(self, k: usize) -> usize {
        match self {
            Self::Branch => 5 + k,
            _ => k,
        }
    }
}

pub(crate) fn tier_of_case(id: &str) -> Result<&'static LoadTier> {
    if id.contains("100mb-5000") {
        Ok(&L100)
    } else if id.contains("500mb-30000") {
        Ok(&L500)
    } else {
        Err(format!("v0.1.6 case ID does not name a load tier: {id}").into())
    }
}

/// The registered fixture of one v0.1.6 M1 case.
pub(crate) fn fixture_for_case(id: &str, seed: u8) -> Result<Vec<Entry>> {
    Ok(load_fixture(tier_of_case(id)?, seed)?.entries)
}

pub(crate) fn self_check() -> Result<()> {
    for tier in [&L100, &L500] {
        for seed in 1..=3 {
            validate_load_fixture(&load_fixture(tier, seed)?)?;
        }
    }

    if cohort_for_cycle(1)? != 0 || cohort_for_cycle(2)? != 0 {
        return Err("v0.1.6 K10 recurrence prefix".into());
    }
    let cohorts: BTreeSet<usize> = (1..=20).map(cohort_for_cycle).collect::<Result<_>>()?;
    if cohorts.len() < 2 {
        return Err("v0.1.6 K100 must rotate beyond the hot cohort".into());
    }
    // Every requested recurrent write must differ from the value it replaces.
    let mut visits = [0usize; REFRESH_COHORTS];
    let mut state = [0u8; REFRESH_COHORTS];
    for cycle in 1..=100 {
        let cohort = cohort_for_cycle(cycle)?;
        let value = recurrent_value(visits[cohort]);
        if value == state[cohort] {
            return Err(format!("v0.1.6 recurrent no-op in cycle {cycle}").into());
        }
        state[cohort] = value;
        visits[cohort] += 1;
    }
    for topology in [
        Topology::Sequential,
        Topology::Concurrent,
        Topology::Branch,
        Topology::Four,
    ] {
        let expected: [(usize, usize, usize, usize); 2] = match topology {
            Topology::Sequential => [(10, 10, 11, 10), (100, 100, 101, 100)],
            Topology::Concurrent => [(10, 20, 21, 10), (100, 200, 201, 100)],
            Topology::Branch => [(10, 30, 31, 15), (100, 210, 211, 105)],
            Topology::Four => [(10, 40, 41, 10), (100, 400, 401, 100)],
        };
        for (k, commits, roots, ancestry) in expected {
            if topology.total_commits(k)? != commits
                || topology.roots(k)? != roots
                || topology.longest_ancestry(k) != ancestry
            {
                return Err(format!("v0.1.6 topology cardinality {topology:?} K{k}").into());
            }
        }
    }
    // Every registered M1 case resolves to a tier and a valid fixture.
    for id in super::v016_stages::MIXED_IDS
        .into_iter()
        .chain(super::v016_stages::WORKSPACE_IDS)
        .chain(super::v016_stages::BRANCH_IDS)
        .chain(super::v016_stages::EXTENDED_IDS)
    {
        if super::v016_stages::mixed_case(id)?.is_none() {
            return Err(format!("v0.1.6 case {id} has no declared topology").into());
        }
        let fixture = load_fixture(tier_of_case(id)?, 1)?;
        validate_load_fixture(&fixture)?;
    }
    for tier in [&L100, &L500] {
        let fixture = load_fixture(tier, 1)?;
        let live = tier.initial_paths()
            + fixture.roles.links.hardlinks.len()
            + fixture.roles.links.symlinks.len()
            + 1;
        if live > tier.max_paths() {
            return Err("v0.1.6 stage-5 live path cap".into());
        }
        let bytes = tier.initial_bytes()
            + fixture.roles.links.hardlinks.len() as u64 * TINY
            + fixture
                .roles
                .links
                .symlinks
                .iter()
                .map(|(_, target, _)| target.len() as u64)
                .sum::<u64>()
            + TINY;
        if bytes > tier.max_bytes() {
            return Err("v0.1.6 stage-5 live byte cap".into());
        }
    }
    for cycle in 1..=20 {
        if attribute_mode(cycle, true) == 0o640 && cycle % 2 == 1 {
            return Err("v0.1.6 attribute toggle".into());
        }
        if attribute_mtime(cycle, 0) == 1_700_000_000 {
            return Err("v0.1.6 explicit mtime recipe".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn v016_contract() {
        super::self_check().unwrap();
    }

    #[test]
    fn load_fixture_role_pools() {
        for tier in [&super::L100, &super::L500] {
            let fixture = super::load_fixture(tier, 1).unwrap();
            assert_eq!(fixture.roles.links.hardlinks.len(), 8);
            assert_eq!(fixture.roles.links.alias_writes.len(), 4);
            assert_eq!(super::attribute_targets(&fixture).unwrap().len(), 16);
            assert_eq!(fixture.roles.refresh_pool.len(), 1024);
            assert_eq!(fixture.roles.move_a.len(), 32);
            assert_eq!(fixture.roles.deletion.len(), 32);
            assert_eq!(fixture.main_edit_targets().len(), 16);
            // Odd cycles remove the registered scratch directories and create
            // their `x` counterparts; even cycles do the exact reverse, so the
            // two sets alternate and never coincide.
            let registered = fixture.roles.scratch.clone();
            let alternate: Vec<String> = registered
                .iter()
                .map(|path| format!("{path}x"))
                .collect();
            let (remove, create) = super::scratch_for_cycle(&fixture, 1);
            assert_eq!(remove, registered);
            assert_eq!(create, alternate);
            let (remove, create) = super::scratch_for_cycle(&fixture, 2);
            assert_eq!(remove, alternate);
            assert_eq!(create, registered);
            assert_ne!(create, remove);
            // The recurrent pair in every group of four is exact across branches.
            let a = super::stage1_contents(&fixture, 1, "a").unwrap();
            let b = super::stage1_contents(&fixture, 1, "b").unwrap();
            for (index, (left, right)) in a.iter().zip(b.iter()).enumerate() {
                if matches!(super::refresh_class(index % 64), super::RefreshClass::Unique) {
                    assert_ne!(left.1.digest().unwrap(), right.1.digest().unwrap());
                } else {
                    assert_eq!(left.1.digest().unwrap(), right.1.digest().unwrap());
                }
            }
        }
    }

    #[test]
    fn fixture_ordinals_are_unique_and_inside_the_caps() {
        for tier in [&super::L100, &super::L500] {
            for seed in 1..=3 {
                let fixture = super::load_fixture(tier, seed).unwrap();
                let paths: std::collections::BTreeSet<&str> =
                    fixture.entries.iter().map(|entry| entry.path.as_str()).collect();
                assert_eq!(paths.len(), fixture.entries.len());
            }
        }
    }
}
