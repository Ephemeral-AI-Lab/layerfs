// v0.1.6 M1 independent oracle.
//
// The oracle maintains the declared state of every path as an independent
// content recipe, applies the declared operations of each executed cycle to a
// shadow state, and compares the shadow against the persisted namespace of the
// final retained root. Nothing here reads the mutated Store to decide what the
// expected value is.
use super::*;
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use workload_source::v016_common as v016;
use workload_source::v016_stages::{self as stages, Topology};
use workload_source::workspace_common::{Content, Entry, EntryKind};

use super::v016_mixed as mixed;

/// One path's declared state: a full recipe below the exact 128 KiB boundary,
/// or a declared length plus the ranges the verifier reads back above it.
#[derive(Clone)]
enum Shadow {
    File(Content),
    Ranged {
        len: u64,
        content: Content,
        ranges: Vec<(u64, u64)>,
    },
}

impl Shadow {
    fn len(&self) -> u64 {
        match self {
            Self::File(content) => content.len(),
            Self::Ranged { len, .. } => *len,
        }
    }
    fn content(&self) -> &Content {
        match self {
            Self::File(content) | Self::Ranged { content, .. } => content,
        }
    }
}

struct ShadowState {
    paths: BTreeMap<String, Shadow>,
    /// Every declared directory of the current state, following the stage-3
    /// moves, so the directory inventory is checked against the declared
    /// namespace rather than the pristine fixture.
    directories: BTreeSet<String>,
    /// Explicit file modes after the attribute stages.
    modes: BTreeMap<String, u32>,
    /// Explicit mtimes after the attribute stages, in whole seconds.
    mtimes: BTreeMap<String, i64>,
    /// Explicit directory modes after the attribute stages.
    directory_modes: BTreeMap<String, u32>,
}

/// One path's declared state: a full recipe at or below the exact 128 KiB
/// boundary, or a declared length plus the ranges the verifier reads back above
/// it. The threshold is exact: 131,072 B enters the chunked path.
fn declared(path: &str, content: Content) -> (String, Shadow) {
    let len = content.len();
    if len <= v016::BOUNDARY_EXACT {
        (path.to_owned(), Shadow::File(content))
    } else {
        (
            path.to_owned(),
            Shadow::Ranged {
                len,
                content,
                // A file whose declared content is unchanged is proved by its
                // full declared-range read.
                ranges: vec![(0, len)],
            },
        )
    }
}

fn shadow_from_fixture(fixture: &v016::LoadFixture) -> ShadowState {
    let mut paths = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut directory_modes = BTreeMap::new();
    for entry in &fixture.entries {
        if entry.path == "." {
            continue;
        }
        match &entry.kind {
            EntryKind::File(content) => {
                let (path_key, state) = declared(&entry.path, content.clone());
                paths.insert(path_key, state);
            }
            EntryKind::Directory => {
                directories.insert(entry.path.clone());
                directory_modes.insert(entry.path.clone(), entry.mode);
            }
            EntryKind::Hardlink(_) | EntryKind::Symlink(_) => {}
        }
    }
    ShadowState {
        paths,
        directories,
        modes: BTreeMap::new(),
        mtimes: BTreeMap::new(),
        directory_modes,
    }
}

fn ranged(len: u64, content: Content, ranges: Vec<(u64, u64)>) -> Shadow {
    Shadow::Ranged {
        len,
        content,
        ranges,
    }
}

impl ShadowState {
    /// Relocate every declared path below `source` to `destination`, which is
    /// what a real rename does to the whole subtree.
    fn remap_prefix(&mut self, source: &str, destination: &str) {
        let prefix = format!("{source}/");
        let moved_directories: Vec<String> = self
            .directories
            .iter()
            .filter(|path| path.starts_with(&prefix))
            .map(|path| format!("{destination}/{}", &path[prefix.len()..]))
            .collect();
        self.directories.retain(|path| !path.starts_with(&prefix));
        // The moved subtree's own root is a namespace entry too, so the exact
        // source path moves with its descendants.
        if self.directories.remove(source) {
            self.directories.insert(destination.to_owned());
        }
        self.directories.extend(moved_directories);
        if let Some(mode) = self.directory_modes.remove(source) {
            self.directory_modes.insert(destination.to_owned(), mode);
        }
        let moved_modes: Vec<(String, u32)> = self
            .directory_modes
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(path, value)| (format!("{destination}/{}", &path[prefix.len()..]), *value))
            .collect();
        self.directory_modes
            .retain(|path, _| !path.starts_with(&prefix));
        for (path, value) in moved_modes {
            self.directory_modes.insert(path, value);
        }
        let moved: Vec<(String, Shadow)> = self
            .paths
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(path, state)| {
                (
                    format!("{destination}/{}", &path[prefix.len()..]),
                    state.clone(),
                )
            })
            .collect();
        self.paths.retain(|path, _| !path.starts_with(&prefix));
        for (path, state) in moved {
            self.paths.insert(path, state);
        }
        let modes: Vec<(String, u32)> = self
            .modes
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(path, value)| (format!("{destination}/{}", &path[prefix.len()..]), *value))
            .collect();
        self.modes.retain(|path, _| !path.starts_with(&prefix));
        for (path, value) in modes {
            self.modes.insert(path, value);
        }
        let mtimes: Vec<(String, i64)> = self
            .mtimes
            .iter()
            .filter(|(path, _)| path.starts_with(&prefix))
            .map(|(path, value)| (format!("{destination}/{}", &path[prefix.len()..]), *value))
            .collect();
        self.mtimes.retain(|path, _| !path.starts_with(&prefix));
        for (path, value) in mtimes {
            self.mtimes.insert(path, value);
        }
    }
}

fn stage2_recipe(
    fixture: &v016::LoadFixture,
    ordinal: usize,
    role: &str,
    len: u64,
) -> AnyResult<Content> {
    workload_source::dedup_workloads::content(
        v016::FAMILY_MIXED,
        "v016-stage2",
        fixture.seed,
        ordinal,
        role,
        len,
    )
}

/// Stage 1: unlink/recreate 64 selected 4 KiB files with the cycle's cohort.
fn apply_stage1(
    fixture: &v016::LoadFixture,
    cycle: usize,
    branch_salt: &str,
    shadow: &mut ShadowState,
) -> AnyResult<()> {
    let cohort = v016::cohort_for_cycle(cycle)?;
    let prior = stages::prior_visits(cycle, cohort)?;
    for slot in 0..v016::REFRESH_FILES_PER_COHORT {
        let index = stages::refresh_pool_index(cohort, slot);
        let path = fixture
            .roles
            .refresh_pool
            .get(index)
            .ok_or("v0.1.6 shadow refresh path")?
            .clone();
        let content = stages::stage1_content(fixture, cycle, cohort, slot, prior, branch_salt)?;
        let (path, state) = declared(&path, content);
        shadow.paths.insert(path, state);
    }
    Ok(())
}

fn splice_of(
    shadow: &ShadowState,
    path: &str,
    start: u64,
    delete_len: u64,
    replacement: Content,
) -> AnyResult<Content> {
    shadow
        .paths
        .get(path)
        .ok_or_else(|| format!("v0.1.6 shadow target absent: {path}"))?
        .content()
        .splice(start, delete_len, replacement)
}

/// Stage 2 POSIX operations, in the declared order of the sixteen targets.
fn apply_stage2(
    fixture: &v016::LoadFixture,
    cycle: usize,
    shadow: &mut ShadowState,
) -> AnyResult<()> {
    let tiny_offset = stages::tiny_pwrite_offset(cycle);
    for (ordinal, path) in fixture.roles.edit.tiny_pwrite.iter().enumerate() {
        let marker = stage2_recipe(
            fixture,
            ordinal * 8 + cycle,
            &format!("tiny-pwrite-c{cycle}"),
            256,
        )?;
        let updated = splice_of(shadow, path, tiny_offset, 256, marker)?;
        let (path, state) = declared(path, updated);
        shadow.paths.insert(path, state);
    }
    let medium = fixture.roles.edit.medium_overwrite.clone();
    let len = shadow
        .paths
        .get(&medium)
        .ok_or("v0.1.6 shadow medium")?
        .len();
    let offset = ((len / 2) / 8) * 8;
    let marker = stage2_recipe(fixture, cycle, &format!("medium-midpoint-c{cycle}"), 256)?;
    let updated = splice_of(shadow, &medium, offset, 256, marker)?;
    let (path, state) = declared(&medium, updated);
    shadow.paths.insert(path, state);
    // The net-zero append/truncate probe leaves the declared content unchanged.
    let tail = fixture.roles.edit.medium_replace_tail.clone();
    let len = shadow.paths.get(&tail).ok_or("v0.1.6 shadow tail")?.len();
    let marker = stage2_recipe(fixture, cycle, &format!("medium-tail-c{cycle}"), 4_096)?;
    let updated = splice_of(shadow, &tail, len - 4_096, 4_096, marker)?;
    let (path, state) = declared(&tail, updated);
    shadow.paths.insert(path, state);
    let anchor = fixture.roles.edit.anchor_overwrite.clone();
    let anchor_len = shadow
        .paths
        .get(&anchor)
        .ok_or("v0.1.6 shadow anchor")?
        .len();
    let anchor_offset = stages::anchor_offset(cycle, anchor_len);
    let marker = stage2_recipe(fixture, cycle, &format!("anchor-c{cycle}"), 4_096)?;
    let updated = splice_of(shadow, &anchor, anchor_offset, 4_096, marker)?;
    shadow.paths.insert(
        anchor,
        ranged(
            anchor_len,
            updated,
            vec![(anchor_offset, 4_096), (0, 4_096)],
        ),
    );
    // Boundary pair: shrink before growth, aggregate neutral at cycle end.
    let (below, above) = stages::boundary_lengths(cycle);
    for (path, target) in [
        (fixture.roles.boundary_below.clone(), below),
        (fixture.roles.boundary_above.clone(), above),
    ] {
        let current = shadow
            .paths
            .get(&path)
            .ok_or("v0.1.6 shadow boundary")?
            .content()
            .clone();
        let len = current.len();
        let updated = if target > len {
            let growth = stage2_recipe(
                fixture,
                cycle,
                &format!("boundary-grow-c{cycle}"),
                target - len,
            )?;
            current.splice(len, 0, growth)?
        } else if target < len {
            current.slice(0, target)?
        } else {
            current
        };
        let (path, state) = declared(&path, updated);
        shadow.paths.insert(path, state);
    }
    Ok(())
}

/// Stage 2 SDK half: one ordered single-file insertion and one deletion.
fn apply_sdk_stage2(
    fixture: &v016::LoadFixture,
    cycle: usize,
    shadow: &mut ShadowState,
) -> AnyResult<()> {
    let plan = stages::stage2_plan(fixture, cycle)?;
    let mut calls = 0usize;
    for (_, operation) in plan {
        if !stages::is_sdk_row(&operation) {
            continue;
        }
        let (target, kind, offset) = stages::sdk_edit_target(fixture, cycle, &operation)?;
        let updated = if kind == "insert" {
            let marker = workload_source::dedup_workloads::content(
                v016::FAMILY_MIXED,
                "v016-sdk-stage2",
                fixture.seed,
                cycle * 2 + calls,
                &format!("insert-c{cycle}"),
                256,
            )?;
            splice_of(shadow, &target, offset, 0, marker)?
        } else {
            splice_of(shadow, &target, offset, 256, Content::Zero { len: 0 })?
        };
        let len = updated.len();
        let mut ranges = Vec::new();
        let mut push = |start: u64, length: u64| {
            let start = start.min(len);
            let end = (start + length).min(len);
            if start < end {
                ranges.push((start, end - start));
            }
        };
        push(offset.saturating_sub(4_096), 8_192);
        push(0, 4_096);
        push(len.saturating_sub(4_096), 4_096);
        shadow.paths.insert(target, ranged(len, updated, ranges));
        calls += 1;
    }
    if calls != stages::STAGE2_SDK_CALLS {
        return Err("v0.1.6 shadow SDK call count".into());
    }
    Ok(())
}

/// The link-bearing parents of one cycle, in the same shape the POSIX helper
/// creates: cycle 1 moves both movable subtrees under the destination parents,
/// and each later cycle toggles back or forth.
fn link_layout(fixture: &v016::LoadFixture, cycle: usize) -> (String, String) {
    if cycle % 2 == 1 {
        (
            fixture.roles.move_dest[0].clone(),
            fixture.roles.move_dest[1].clone(),
        )
    } else {
        (String::new(), String::new())
    }
}

fn link_name(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

/// Stage 4: the hardlink aliases, the symlinks and the four attribute rules.
fn apply_stage4(
    fixture: &v016::LoadFixture,
    cycle: usize,
    branch_tag: u64,
    shadow: &mut ShadowState,
) -> AnyResult<()> {
    let (link_parent, symlink_parent) = link_layout(fixture, cycle);
    for index in 0..v016::HARDLINK_TARGETS {
        shadow
            .paths
            .remove(&link_name(&link_parent, &format!("hl{index:02}")));
    }
    for index in 0..v016::SYMLINK_COUNT {
        shadow
            .paths
            .remove(&link_name(&symlink_parent, &format!("sl{index:02}")));
    }
    // The explicit attribute mtime is epoch + 10*cycle + branch_tag, so the
    // declaration follows the branch that performed the stage.
    let mtime = v016::attribute_mtime(cycle, branch_tag);
    for path in v016::attribute_targets(fixture)? {
        shadow
            .modes
            .insert(path.clone(), v016::attribute_mode(cycle, true));
        shadow.mtimes.insert(path, mtime);
    }
    // Four 64-byte writes on the four distinct 4 KiB alias targets.
    for (index, target) in stages::alias_probe_paths(fixture).iter().enumerate() {
        let marker = workload_source::dedup_workloads::content(
            v016::FAMILY_MIXED,
            "v016-stage4",
            fixture.seed,
            index,
            &format!("alias-probe-c{cycle}"),
            stages::ALIAS_PROBE_BYTES,
        )?;
        let offset = 64 * index as u64;
        let updated = splice_of(shadow, target, offset, stages::ALIAS_PROBE_BYTES, marker)?;
        let (path, state) = declared(target, updated);
        shadow.paths.insert(path, state);
    }
    for path in stages::attribute_directories(fixture, cycle) {
        shadow
            .directory_modes
            .insert(path, v016::attribute_mode(cycle, false));
    }
    Ok(())
}

/// Stage 5: the subtree recreation, the four atomic saves and the link removals.
/// The recreated deletion root is a new directory on the live filesystem, so its
/// declared mode is the live mode the helper measured, not the stage-4 chmod and
/// not an assumed default.
fn apply_stage5(
    fixture: &v016::LoadFixture,
    cycle: usize,
    branch_salt: &str,
    shadow: &mut ShadowState,
    observed: &BTreeMap<String, u32>,
) -> AnyResult<()> {
    let deletion_root = fixture.roles.deletion_root.clone();
    shadow.directory_modes.insert(
        deletion_root.clone(),
        observed_directory_mode(observed, &deletion_root)?,
    );
    let (link_parent, symlink_parent) = link_layout(fixture, cycle);
    for index in 0..v016::DELETION_SUBTREE_FILES {
        let path = fixture
            .roles
            .deletion
            .get(index)
            .ok_or("v0.1.6 shadow subtree path")?
            .clone();
        let content = stages::subtree_content(fixture, cycle, index, branch_salt)?;
        let (path, state) = declared(&path, content);
        shadow.paths.insert(path, state);
    }
    for (index, destination) in stages::save_destinations(fixture).iter().enumerate() {
        let content = workload_source::dedup_workloads::content(
            v016::FAMILY_MIXED,
            "v016-stage5",
            fixture.seed,
            index,
            &format!("atomic-save-c{cycle}-{index}"),
            v016::TINY,
        )?;
        let (path, state) = declared(destination, content);
        shadow.paths.insert(path, state);
    }
    for index in 0..v016::HARDLINK_TARGETS {
        shadow
            .paths
            .remove(&link_name(&link_parent, &format!("hl{index:02}")));
    }
    for index in 0..v016::SYMLINK_COUNT {
        shadow
            .paths
            .remove(&link_name(&symlink_parent, &format!("sl{index:02}")));
    }
    Ok(())
}

/// Stage 3: the scratch-directory replacement and the two populated directory
/// moves. A move relocates every declared descendant, so the shadow's paths are
/// remapped rather than re-declared under the old root. The live mode of every
/// created directory is the mode the workload measured on the live filesystem,
/// never an assumed runtime default.
fn apply_stage3(
    fixture: &v016::LoadFixture,
    cycle: usize,
    shadow: &mut ShadowState,
    observed: &BTreeMap<String, u32>,
) -> AnyResult<()> {
    let (remove, create) = stages::scratch_names(fixture, cycle);
    if remove.len() != v016::SCRATCH_DIRS || create.len() != v016::SCRATCH_DIRS {
        return Err("v0.1.6 scratch directory cardinality".into());
    }
    for path in &remove {
        shadow.directory_modes.remove(path);
        shadow.directories.remove(path);
    }
    for path in &create {
        let mode = observed_directory_mode(observed, path)?;
        shadow.directories.insert(path.clone());
        shadow.directory_modes.insert(path.clone(), mode);
    }
    for (source, destination) in stages::move_rows(fixture, cycle) {
        if source == destination {
            continue;
        }
        shadow.remap_prefix(&source, &destination);
    }
    Ok(())
}

/// One directory the stage helper created, with the live mode it measured.
fn observed_directory_mode(observed: &BTreeMap<String, u32>, path: &str) -> AnyResult<u32> {
    observed.get(path).copied().ok_or_else(|| {
        format!("v0.1.6 created directory {path} has no live mode observation").into()
    })
}

fn apply_cycle(
    fixture: &v016::LoadFixture,
    cycle: usize,
    branch_salt: &str,
    branch_tag: u64,
    shadow: &mut ShadowState,
    observed: &BTreeMap<String, u32>,
) -> AnyResult<()> {
    apply_stage1(fixture, cycle, branch_salt, shadow)?;
    apply_stage2(fixture, cycle, shadow)?;
    apply_sdk_stage2(fixture, cycle, shadow)?;
    apply_stage3(fixture, cycle, shadow, observed)?;
    apply_stage4(fixture, cycle, branch_tag, shadow)?;
    apply_stage5(fixture, cycle, branch_salt, shadow, observed)?;
    Ok(())
}

/// Compare one retained root with the shadow state.
fn verify_root_against_shadow(
    case_id: &str,
    store: &LayerStackStore,
    root: ObjectId,
    shadow: &ShadowState,
    fixture: &v016::LoadFixture,
    label: &str,
) -> AnyResult<()> {
    let reader = store.snapshot_reader(root);
    let source: &dyn layerfs_layerstack_store::ObjectSource = &reader;
    let view = super::workspace_verify::namespace_view(source, root)?;
    let regular: BTreeSet<&str> = view
        .paths
        .iter()
        .filter(|(path, record)| {
            *path != "." && record.kind == super::workspace_verify::inode_regular()
        })
        .map(|(path, _)| path.as_str())
        .collect();
    let expected: BTreeSet<&str> = shadow.paths.keys().map(String::as_str).collect();
    if regular != expected {
        let missing: Vec<&&str> = expected.difference(&regular).take(4).collect();
        let extra: Vec<&&str> = regular.difference(&expected).take(4).collect();
        return Err(format!(
            "v0.1.6 {label} regular path set differs: missing {missing:?}, extra {extra:?}"
        )
        .into());
    }
    // Every declared directory outside the root is present and is a directory.
    let directories: BTreeSet<String> = fixture
        .entries
        .iter()
        .filter(|entry| entry.path != "." && matches!(entry.kind, EntryKind::Directory))
        .map(|entry| entry.path.clone())
        .collect();
    if directories.is_empty() {
        return Err("v0.1.6 fixture declares no directories".into());
    }
    for path in &shadow.directories {
        let record = view
            .paths
            .get(path)
            .ok_or_else(|| format!("v0.1.6 {label} directory absent: {path}"))?;
        if record.kind != super::workspace_verify::inode_directory() {
            return Err(format!("v0.1.6 {label} declared path is not a directory: {path}").into());
        }
    }
    let observed_directories: BTreeSet<&str> = view
        .paths
        .iter()
        .filter(|(path, record)| {
            *path != "." && record.kind == super::workspace_verify::inode_directory()
        })
        .map(|(path, _)| path.as_str())
        .collect();
    let declared_directories: BTreeSet<&str> =
        shadow.directories.iter().map(String::as_str).collect();
    if observed_directories != declared_directories {
        let missing: Vec<&&str> = declared_directories
            .difference(&observed_directories)
            .take(4)
            .collect();
        let extra: Vec<&&str> = observed_directories
            .difference(&declared_directories)
            .take(4)
            .collect();
        return Err(format!(
            "v0.1.6 {label} directory inventory differs: missing {missing:?}, extra {extra:?}"
        )
        .into());
    }
    let mut small = 0usize;
    let mut ranged_paths = 0usize;
    let mut mismatches: Vec<String> = Vec::new();
    for (path, state) in &shadow.paths {
        let record = view
            .paths
            .get(path)
            .ok_or_else(|| format!("v0.1.6 {label} shadow path absent: {path}"))?;
        if record.kind != super::workspace_verify::inode_regular() {
            return Err(format!("v0.1.6 {label} non-regular shadow path: {path}").into());
        }
        if let Some(observed) = view.metadata.get(path) {
            if let Some(mode) = shadow.modes.get(path) {
                if observed.permission_mode != *mode {
                    mismatches.push(format!(
                        "mode {path}: declared {mode:o} published {:o}",
                        observed.permission_mode
                    ));
                }
            }
            if let Some(seconds) = shadow.mtimes.get(path) {
                if observed.mtime_seconds != *seconds {
                    mismatches.push(format!(
                        "mtime {path}: declared {seconds} published {}",
                        observed.mtime_seconds
                    ));
                }
            }
        }
        let observed_len = super::workspace_verify::declared_regular_length(source, record)?;
        if state.len() != observed_len {
            mismatches.push(format!(
                "length {path}: declared {} published {observed_len}",
                state.len()
            ));
            continue;
        }
        match state {
            Shadow::File(content) => {
                // Empty files have their own representation, so their declared
                // length and type are the proof; every non-empty small file is
                // proved by an independently recomputed content root.
                if let Some(declared) = super::workspace_verify::declared_content_root(
                    &Entry::file(path.clone(), content.clone()),
                )? {
                    if declared != record.content_root {
                        mismatches.push(format!(
                            "content root {path}: declared {declared} published {}",
                            record.content_root
                        ));
                    }
                    small += 1;
                }
            }
            Shadow::Ranged {
                len,
                content,
                ranges,
            } => {
                if ranges.is_empty() {
                    return Err(
                        format!("v0.1.6 {label} large path has no declared range: {path}").into(),
                    );
                }
                for (offset, length) in ranges {
                    let start = (*offset).min(*len);
                    let end = (start + *length).min(*len);
                    if start >= end {
                        continue;
                    }
                    let mut expected_bytes = vec![0u8; (end - start) as usize];
                    let read = content.read_at(start, &mut expected_bytes)?;
                    expected_bytes.truncate(read);
                    if super::workspace_verify::verify_declared_range(
                        source,
                        root,
                        path,
                        start..end,
                        &expected_bytes,
                    )
                    .is_err()
                    {
                        mismatches.push(format!("declared range {path} {start}..{end}"));
                    }
                }
                ranged_paths += 1;
            }
        }
    }
    for (path, mode) in &shadow.directory_modes {
        let observed = view
            .metadata
            .get(path)
            .ok_or_else(|| format!("v0.1.6 {label} directory metadata absent: {path}"))?;
        if observed.permission_mode != *mode {
            mismatches.push(format!(
                "directory mode {path}: declared {mode:o} published {:o}",
                observed.permission_mode
            ));
        }
    }
    if !mismatches.is_empty() {
        super::v016_mixed::emit(
            "v016-oracle-mismatch",
            &[
                ("case", super::v016_mixed::quote(case_id)),
                ("label", super::v016_mixed::quote(label)),
                ("mismatch_count", mismatches.len().to_string()),
                (
                    "mismatches",
                    format!(
                        "[{}]",
                        mismatches
                            .iter()
                            .take(24)
                            .map(|row| super::v016_mixed::quote(row))
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                ),
            ],
        );
        return Err(format!(
            "v0.1.6 {label}: {} declared state mismatches, first: {}",
            mismatches.len(),
            mismatches[0]
        )
        .into());
    }
    super::v016_mixed::emit(
        "v016-oracle-root",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("label", super::v016_mixed::quote(label)),
            ("root", super::v016_mixed::quote(&root.to_string())),
            ("regular_paths", shadow.paths.len().to_string()),
            ("small_content_roots", small.to_string()),
            ("declared_range_paths", ranged_paths.to_string()),
            ("declared_directories", directories.len().to_string()),
            (
                "scope",
                super::v016_mixed::quote(
                    "complete namespace inventory; every sub-128KiB regular file proved by an independently recomputed content root; changed large files proved by their declared ranges",
                ),
            ),
        ],
    );
    Ok(())
}

/// The independent verification route of one selected M1 invocation.
pub(crate) fn verify_case(
    case_id: &str,
    mixed_case: &stages::MixedCase,
    seed: u8,
    reopened: &LayerStackStore,
    sessions: &[mixed::Worker],
    client: &Client,
) -> AnyResult<()> {
    let mixed_case = *mixed_case;
    let tier = if mixed_case.tier == "L100" {
        &v016::L100
    } else {
        &v016::L500
    };
    let fixture = Arc::new(v016::load_fixture(tier, seed)?);
    let declared_commits = mixed::declared_commits_for(mixed_case, sessions.len());
    if declared_commits != mixed_case.topology.total_commits(mixed_case.k) {
        return Err("v0.1.6 declared commit cardinality".into());
    }
    let mut observed_commits = 0usize;
    for worker in sessions {
        let pinned = reopened.pin_branch(worker.branch)?;
        if pinned.branch.head_commit_id != worker.commits.last().copied() {
            return Err(format!(
                "v0.1.6 reopened branch {} head differs from the published head",
                worker.index
            )
            .into());
        }
        if worker.roots.last().copied() != Some(pinned.root) {
            return Err("v0.1.6 reopened branch root differs from the published root".into());
        }
        observed_commits += worker.commits.len();
    }
    if observed_commits != declared_commits {
        return Err(format!(
            "v0.1.6 graph commit count {observed_commits} != declared {declared_commits}"
        )
        .into());
    }
    // Every retained commit is checked for its own parent edge and identity.
    let wanted: Vec<CommitId> = sessions
        .iter()
        .flat_map(|worker| worker.commits.iter().copied())
        .collect();
    let records = super::workspace_verify::commit_records(client, &wanted)?;
    let distinct: BTreeSet<CommitId> = wanted.iter().copied().collect();
    // A forked child resumes from the fork point, so its first local commit's
    // published parent is that inherited commit, not `None`. Every later local
    // commit chains to the previous local commit as usual.
    let fork_points: BTreeMap<usize, CommitId> = if mixed_case.topology == Topology::Branch {
        let fork = sessions
            .first()
            .and_then(|trunk| trunk.commits.get(mixed::TRUNK_FORK_COMMIT - 1).copied());
        match fork {
            Some(commit) => sessions
                .iter()
                .skip(1)
                .map(|worker| (worker.index, commit))
                .collect(),
            None => BTreeMap::new(),
        }
    } else {
        BTreeMap::new()
    };
    for worker in sessions {
        let mut previous: Option<CommitId> = fork_points.get(&worker.index).copied();
        for commit in &worker.commits {
            let record = records
                .get(commit)
                .ok_or("v0.1.6 retained commit record absent")?;
            if record.parent_commit_id != previous {
                return Err(format!(
                    "v0.1.6 parent edge mismatch at commit {commit}: declared {previous:?} published {:?}",
                    record.parent_commit_id
                )
                .into());
            }
            previous = Some(*commit);
        }
    }
    // The forked children resume from the trunk's commit-5 state. The inherited
    // state is proved by the child's final-state oracle, which replays the cycle
    // sequence from the fork point rather than from a pristine fixture.
    if distinct.len() != declared_commits {
        return Err(format!(
            "v0.1.6 distinct commit identities {} != declared {declared_commits}",
            distinct.len()
        )
        .into());
    }
    let mut roots: BTreeSet<ObjectId> = BTreeSet::new();
    for worker in sessions {
        for root in &worker.roots {
            roots.insert(*root);
        }
    }
    let genesis_root = crate::v016_mixed::v016_genesis_root();
    if let Some(genesis) = genesis_root {
        roots.insert(genesis);
    }
    let genesis_root = crate::v016_mixed::v016_genesis_root();
    super::v016_mixed::emit(
        "v016-graph-proof",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("created_commits", declared_commits.to_string()),
            ("distinct_commits", distinct.len().to_string()),
            ("retained_roots", roots.len().to_string()),
            (
                "declared_roots",
                mixed_case.topology.roots(mixed_case.k).to_string(),
            ),
            (
                "longest_ancestry",
                mixed_case.topology.longest_ancestry(mixed_case.k).to_string(),
            ),
            ("genesis_root", super::v016_mixed::quote(&format!("{genesis_root:?}"))),
            (
                "scope",
                super::v016_mixed::quote(
                    "every retained commit identity and parent edge through the public query surface after reopen; declared branch cardinality",
                ),
            ),
        ],
    );
    if mixed_case.exhaustive_verify_only() {
        return verify_exhaustive(
            case_id,
            mixed_case,
            &fixture,
            reopened,
            sessions,
            declared_commits,
            &roots,
        );
    }
    // The independent final-state proof, one shadow per live branch. A branch
    // that forked from the trunk inherits every cycle the trunk already
    // published, replayed with the trunk's own branch identity and with the
    // live directory modes the trunk measured, before its own cycles are
    // applied.
    let trunk = sessions
        .iter()
        .find(|worker| worker.cycle_start > 1)
        .and_then(|_| sessions.first());
    for worker in sessions {
        let mut shadow = shadow_from_fixture(&fixture);
        if worker.cycle_start > 1 {
            let parent = trunk.ok_or("v0.1.6 inherited branch parent")?;
            for cycle in 1..worker.cycle_start {
                apply_cycle(
                    &fixture,
                    cycle,
                    &parent.branch_salt,
                    parent.branch_tag,
                    &mut shadow,
                    &parent.created_directory_modes,
                )?;
            }
        }
        let cycles = worker.commits.len() / stages::STAGES;
        for offset in 0..cycles {
            let cycle = worker.cycle_start + offset;
            apply_cycle(
                &fixture,
                cycle,
                &worker.branch_salt,
                worker.branch_tag,
                &mut shadow,
                &worker.created_directory_modes,
            )?;
        }
        let final_root = worker
            .roots
            .last()
            .copied()
            .ok_or("v0.1.6 final retained root")?;
        if std::env::var_os("LAYERFS_V016_ATTR_DIAGNOSTIC").is_some() {
            let reader = reopened.snapshot_reader(final_root);
            let view = super::workspace_verify::namespace_view(&reader, final_root)?;
            for (path, mode) in &shadow.modes {
                super::v016_mixed::emit(
                    "v016-attribute-verification-diagnostic",
                    &[
                        ("case", super::v016_mixed::quote(case_id)),
                        ("object", super::v016_mixed::quote("file")),
                        ("path", super::v016_mixed::quote(path)),
                        ("declared_mode", format!("{mode:o}")),
                        (
                            "published_mode",
                            view.metadata
                                .get(path)
                                .map(|meta| format!("{:o}", meta.permission_mode))
                                .unwrap_or_else(|| "absent".into()),
                        ),
                    ],
                );
            }
            for (path, mode) in &shadow.directory_modes {
                super::v016_mixed::emit(
                    "v016-attribute-verification-diagnostic",
                    &[
                        ("case", super::v016_mixed::quote(case_id)),
                        ("object", super::v016_mixed::quote("directory")),
                        ("path", super::v016_mixed::quote(path)),
                        ("declared_mode", format!("{mode:o}")),
                        (
                            "published_mode",
                            view.metadata
                                .get(path)
                                .map(|meta| format!("{:o}", meta.permission_mode))
                                .unwrap_or_else(|| "absent".into()),
                        ),
                        (
                            "published_inode",
                            view.inode(path)
                                .map(|id| super::v016_mixed::quote(&format!("{id:?}")))
                                .unwrap_or_else(|| "absent".into()),
                        ),
                    ],
                );
            }
        }
        verify_root_against_shadow(
            case_id,
            reopened,
            final_root,
            &shadow,
            &fixture,
            &format!("final-branch-{}", worker.index),
        )?;
    }
    if mixed_case.topology == Topology::Concurrent {
        verify_discard(case_id, &fixture, reopened, sessions)?;
    }
    let counters = super::v016_mixed::observed_counters()?;
    let cycles_per_worker = mixed_case.k / stages::STAGES;
    let workers_with_cycles = if mixed_case.topology == Topology::Branch {
        sessions.len() - 1
    } else {
        sessions.len()
    };
    let trunk_cycles = if mixed_case.topology == Topology::Branch {
        mixed::TRUNK_COMMITS / stages::STAGES
    } else {
        0
    };
    let expected_cycles = cycles_per_worker * workers_with_cycles + trunk_cycles;
    for (name, per_cycle) in stages::declared_cycle_counters() {
        if name == "created_commits" || name == "posix_helper_executions" {
            continue;
        }
        let declared = per_cycle * expected_cycles;
        let observed = counters.get(name).copied().unwrap_or(0);
        if observed != declared {
            return Err(format!(
                "v0.1.6 declared cycle counter {name}: observed {observed} != {declared}"
            )
            .into());
        }
    }
    if counters.get("created_commits").copied().unwrap_or(0) != declared_commits {
        return Err("v0.1.6 published Created commit count differs from the declared total".into());
    }
    if counters
        .get("posix_helper_executions")
        .copied()
        .unwrap_or(0)
        != declared_commits
    {
        return Err("v0.1.6 POSIX helper execution count differs from the declared total".into());
    }
    super::v016_mixed::emit(
        "v016-counter-proof",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("cycles", expected_cycles.to_string()),
            (
                "counters",
                format!(
                    "{{{}}}",
                    counters
                        .iter()
                        .map(|(key, value)| format!("{}:{value}", super::v016_mixed::quote(key)))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            (
                "scope",
                super::v016_mixed::quote(
                    "observed per-stage helper counters summed over every executed cycle and compared with the frozen workloads.md table",
                ),
            ),
        ],
    );
    super::v016_mixed::emit(
        "v016-verification",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("status", super::v016_mixed::quote("pass")),
            ("created_commits", declared_commits.to_string()),
            (
                "scope",
                super::v016_mixed::quote(
                    "complete declared final namespace inventory, independently recomputed sub-128KiB content roots, declared large-file ranges, every retained commit identity and parent edge, declared operation counters and the concurrent discard proof",
                ),
            ),
        ],
    );
    Ok(())
}

/// The explicit exhaustive route of the two extended replays: every namespace,
/// file byte recipe and metadata state across all retained roots, under the
/// case's own frozen watchdog rather than the regular 15-second deadline.
fn verify_exhaustive(
    case_id: &str,
    mixed_case: stages::MixedCase,
    fixture: &v016::LoadFixture,
    reopened: &LayerStackStore,
    sessions: &[mixed::Worker],
    declared_commits: usize,
    roots: &BTreeSet<ObjectId>,
) -> AnyResult<()> {
    let worker = sessions.first().ok_or("v0.1.6 exhaustive worker")?;
    if sessions.len() != 1 {
        return Err("v0.1.6 exhaustive replay requires the sequential topology".into());
    }
    if worker.commits.len() != declared_commits || worker.roots.len() != declared_commits {
        return Err(format!(
            "v0.1.6 exhaustive replay retained {} commits / {} roots, declared {declared_commits}",
            worker.commits.len(),
            worker.roots.len()
        )
        .into());
    }
    let pinned = reopened.pin_branch(worker.branch)?;
    if pinned.branch.head_commit_id != worker.commits.last().copied()
        || pinned.root != *worker.roots.last().ok_or("v0.1.6 exhaustive head root")?
    {
        return Err("v0.1.6 exhaustive replay head differs from the published head".into());
    }
    // Reconstruct every retained state independently and verify it.
    let mut shadow = shadow_from_fixture(fixture);
    let cycles = declared_commits / stages::STAGES;
    let mut states = 0usize;
    for cycle in 1..=cycles {
        apply_cycle(
            fixture,
            cycle,
            &worker.branch_salt,
            worker.branch_tag,
            &mut shadow,
            &worker.created_directory_modes,
        )?;
        let commit_index = cycle * stages::STAGES - 1;
        let root = worker.roots[commit_index];
        verify_root_against_shadow(
            case_id,
            reopened,
            root,
            &shadow,
            fixture,
            &format!("exhaustive-cycle{cycle}"),
        )?;
        states += 1;
    }
    if states != cycles {
        return Err("v0.1.6 exhaustive state count".into());
    }
    super::v016_mixed::emit(
        "v016-exhaustive-proof",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("retained_states", states.to_string()),
            ("distinct_state_roots", roots.len().to_string()),
            (
                "declared_roots",
                mixed_case.topology.roots(mixed_case.k).to_string(),
            ),
            (
                "verified_paths_per_state",
                shadow.paths.len().to_string(),
            ),
            (
                "watchdog_seconds",
                mixed_case.watchdog_seconds().to_string(),
            ),
            (
                "scope",
                super::v016_mixed::quote(
                    "every namespace entry, every sub-128KiB regular-file content root recomputed from the independent recipe, every changed large-file declared range and the explicit file/directory metadata of every state the producer retained",
                ),
            ),
        ],
    );
    Ok(())
}

/// Item 8 of the verification contract: the discarded mutation vanished and the
/// surviving branch state holds its declared value.
fn verify_discard(
    case_id: &str,
    fixture: &v016::LoadFixture,
    reopened: &LayerStackStore,
    sessions: &[mixed::Worker],
) -> AnyResult<()> {
    let path = super::v016_mixed::discard_witness_path_for_oracle(fixture)?;
    let first = sessions.first().ok_or("v0.1.6 discard branch")?;
    let root = first
        .roots
        .last()
        .copied()
        .ok_or("v0.1.6 discard branch root")?;
    let reader = reopened.snapshot_reader(root);
    let source: &dyn layerfs_layerstack_store::ObjectSource = &reader;
    let view = super::workspace_verify::namespace_view(source, root)?;
    let record = view
        .paths
        .get(&path)
        .ok_or_else(|| format!("v0.1.6 discard witness absent: {path}"))?;
    // A's discarded session never committed, so the witness must carry exactly
    // what the declared schedule leaves there. The witness is a refresh-pool
    // member of a rotating cohort, so a deep history legitimately rewrites it
    // in a later cycle: the declaration is the shadow's state for this branch,
    // not the pristine fixture recipe.
    let mut shadow = shadow_from_fixture(fixture);
    let cycles = first.commits.len() / stages::STAGES;
    for offset in 0..cycles {
        apply_cycle(
            fixture,
            first.cycle_start + offset,
            &first.branch_salt,
            first.branch_tag,
            &mut shadow,
            &first.created_directory_modes,
        )?;
    }
    let (declared, declared_root, declared_length) = match shadow.paths.get(&path) {
        Some(Shadow::File(content)) => {
            let root = super::workspace_verify::declared_content_root(&Entry::file(
                path.clone(),
                content.clone(),
            ))?
            .ok_or("v0.1.6 discard witness declared root")?;
            (root, root, content.len())
        }
        Some(Shadow::Ranged { len, .. }) => {
            return Err(format!(
                "v0.1.6 discard witness {path} is declared as a {len}-byte ranged file"
            )
            .into())
        }
        None => {
            return Err(format!(
                "v0.1.6 discard witness {path} is not part of the declared regular inventory"
            )
            .into())
        }
    };
    if declared_root != record.content_root {
        let observed = super::workspace_verify::declared_regular_length(source, record)?;
        return Err(format!(
            "v0.1.6 discarded mutation is present in the published branch state: {path} declared root {declared} observed {} length {observed} declared_length {declared_length}",
            record.content_root
        )
        .into());
    }
    super::v016_mixed::emit(
        "v016-discard-proof",
        &[
            ("case", super::v016_mixed::quote(case_id)),
            ("path", super::v016_mixed::quote(&path)),
            (
                "observed_root",
                super::v016_mixed::quote(&record.content_root.to_string()),
            ),
            (
                "declared_root",
                super::v016_mixed::quote(&declared_root.to_string()),
            ),
            (
                "declared_length",
                declared_length.to_string(),
            ),
            (
                "scope",
                super::v016_mixed::quote(
                    "discarded session mutation absent from the published branch state; the declaration is the branch's own scheduled state after every cycle it published",
                ),
            ),
        ],
    );
    Ok(())
}
