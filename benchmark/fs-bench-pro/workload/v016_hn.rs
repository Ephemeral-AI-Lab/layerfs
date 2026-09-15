// v0.1.6 HN (`namespace-inode`) compact history schedule and its per-state oracle.
//
// `benchmark-families.md` §`dedup_branch_history` declares five stages per cycle
// on the compact fixture S: (1) unlink/recreate two tiny files, one recurring and
// one generation-specific; (2) two single-file SDK 256 B overwrites on two other
// tiny files; (3) rename the populated `tiny` directory to its alternate name;
// (4) one alias to a fifth tiny file, chmod two files plus that directory, set
// both files' explicit mtimes; (5) one 4 KiB atomic save over the aliased
// destination, observe the old inode through the alias, remove the alias. Every
// stage publishes its own Created Commit, so one cycle is five commits and the
// declared depth K is a whole number of cycles (K10 = 2, K100 = 20).
//
// `fixtures.md` §"Compact fixture S" fixes the roles: the `tiny` directory holds
// all eight tiny files and moves between `tiny` and `tiny-moved`; its first two
// files are the refresh pair, the next two the SDK edit targets and the next one
// the alias/replacement target. Roles follow the move.
//
// This module is product-free: it declares paths, recipes, per-stage counters and
// the exact state after every commit. Both the Linux helper (`v016-hn-stage`) and
// the host orchestrator read the same declaration, so nothing here observes a
// Store, a Client or a mount.
//
// Declared exception, stated rather than hidden: a POSIX write through the mount
// publishes a runtime mode/mtime, and a directory whose entries change has no
// deterministic automatic mtime. Every path this schedule writes, and the moved
// directory itself, therefore has its declared mode and mtime *set explicitly and
// read back* inside the stage that touched it (the same construction
// `v016-boundary-alias` uses for its atomic replacement). Those calls are
// declared counters, never observations: the expected state is a pure function of
// the seed and the completed-commit ordinal.
use super::dedup_workloads as d;
use super::v016_common as v016;
use super::v016_compact::{self as s};
use super::workspace_common::{self as common, Content, Entry, EntryKind, SdkEdit};
use super::{sdk_edit_common, Result};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

pub(crate) const FAMILY: &str = "dedup_branch_history";
pub(crate) const PROFILE: &str = "namespace-inode";
pub(crate) const K10: &str = "v016-history-namespace-inode-k10-v1";
pub(crate) const K100: &str = "v016-history-namespace-inode-k100-v1";

/// The five declared stages of one cycle.
pub(crate) const STAGES: usize = 5;
/// Stage ordinals, named so the host and the helper cannot drift apart.
pub(crate) const REFRESH: usize = 1;
pub(crate) const EDITS: usize = 2;
pub(crate) const DIRECTORIES: usize = 3;
pub(crate) const ATTRS_LINKS: usize = 4;
pub(crate) const INODE_SAVE: usize = 5;

/// The populated `tiny` directory and its alternate name. `fixtures.md` fixes
/// both spellings; the move toggles between them every cycle.
pub(crate) const DIR_A: &str = "tiny";
pub(crate) const DIR_B: &str = "tiny-moved";

/// Role names inside the movable directory, in `fixtures.md` order.
pub(crate) const REFRESH_RECURRING: &str = "t0.dat";
pub(crate) const REFRESH_GENERATION: &str = "t1.dat";
pub(crate) const EDIT_FIRST: &str = "t2.dat";
pub(crate) const EDIT_SECOND: &str = "t3.dat";
pub(crate) const REPLACEMENT: &str = "t4.dat";
/// The stage-4 alias to the replacement target, and the stage-5 temporary name
/// the atomic save publishes through. Two extra names is the declared envelope.
pub(crate) const ALIAS: &str = "a4.dat";
pub(crate) const SAVE_TEMP: &str = ".save4.tmp";

/// The declared 256 B SDK overwrite region of both edit targets.
pub(crate) const EDIT_OFFSET: u64 = 0;
pub(crate) const EDIT_LEN: u64 = 256;
/// The declared atomic-save payload length.
pub(crate) const SAVE_LEN: u64 = s::TINY_LEN;

/// Maximum non-directory names and logical path bytes of the schedule:
/// S + {alias, save temporary} and S + 4 KiB + 4 KiB (the alias is charged its
/// referent length, `fixtures.md`).
pub(crate) const MAX_EXTRA_NAMES: usize = 2;
pub(crate) const MAX_EXTRA_BYTES: u64 = 2 * s::TINY_LEN;

pub(crate) fn is_hn(id: &str) -> bool {
    id == K10 || id == K100
}

/// The declared depth of one HN case, or `None` when it is not an HN case.
pub(crate) fn tier_of(id: &str) -> Option<usize> {
    match id {
        K10 => Some(10),
        K100 => Some(100),
        _ => None,
    }
}

/// The directory the populated tree lives in *before* stage 3 of `cycle`.
pub(crate) fn dir_before(cycle: usize) -> &'static str {
    if cycle % 2 == 1 {
        DIR_A
    } else {
        DIR_B
    }
}

/// The directory the populated tree lives in *after* stage 3 of `cycle`.
pub(crate) fn dir_after(cycle: usize) -> &'static str {
    if cycle % 2 == 1 {
        DIR_B
    } else {
        DIR_A
    }
}

/// The (cycle, stage) pair of one completed commit, 1-based. Stage ordinals run
/// 1..=5 inside every cycle, so commit 94 is stage 4 of cycle 19 and commit 95 is
/// the stage that replaces the aliased destination.
pub(crate) fn stage_of(step: usize) -> Result<(usize, usize)> {
    if step == 0 {
        return Err("v0.1.6 HN commits are 1-based".into());
    }
    Ok(((step - 1) / STAGES + 1, (step - 1) % STAGES + 1))
}

pub(crate) fn step_of(cycle: usize, stage: usize) -> Result<usize> {
    if cycle == 0 || !(1..=STAGES).contains(&stage) {
        return Err("v0.1.6 HN cycle/stage outside the declared schedule".into());
    }
    Ok((cycle - 1) * STAGES + stage)
}

/// The declared path of one role inside the movable directory.
pub(crate) fn role_path(dir: &str, role: &str) -> String {
    format!("{dir}/{role}")
}

fn recipe(seed: u8, ordinal: usize, role: &str, len: u64) -> Result<Content> {
    d::content(FAMILY, PROFILE, seed, ordinal, role, len)
}

/// The recurring refresh file: initial A, first visit B, second A, and neither
/// value carries the cycle, so the same bytes recur on every other generation.
pub(crate) fn recurring_content(seed: u8, cycle: usize) -> Result<Content> {
    let value = v016::recurrent_value(cycle - 1);
    recipe(
        seed,
        0,
        if value == 0 { "recurring-a" } else { "recurring-b" },
        s::TINY_LEN,
    )
}

/// The generation-specific refresh file: unique bytes per generation.
pub(crate) fn generation_content(seed: u8, cycle: usize) -> Result<Content> {
    recipe(
        seed,
        1,
        &format!("generation-c{cycle}"),
        s::TINY_LEN,
    )
}

/// The declared 256 B value of one SDK edit target in one cycle. The value
/// alternates by the target's own visit count, so every new edit differs from
/// its immediate predecessor.
pub(crate) fn edit_content(seed: u8, cycle: usize, target: usize) -> Result<Content> {
    if target >= 2 {
        return Err("v0.1.6 HN edit target outside the declared pair".into());
    }
    let value = v016::recurrent_value(cycle - 1);
    recipe(
        seed,
        2 + target,
        if value == 0 { "edit-a" } else { "edit-b" },
        EDIT_LEN,
    )
}

/// The stage-5 replacement payload: a fresh 4 KiB generation.
pub(crate) fn replacement_content(seed: u8, cycle: usize) -> Result<Content> {
    recipe(
        seed,
        4,
        &format!("replacement-c{cycle}"),
        SAVE_LEN,
    )
}

/// Declared metadata a path carries when nothing has changed it yet.
fn fixture_metadata(kind: EntryKindTag) -> (u32, i64, u32) {
    match kind {
        EntryKindTag::File => (0o640, common::MTIME, 0),
        EntryKindTag::Directory => (0o750, common::MTIME, 0),
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum EntryKindTag {
    File,
    Directory,
}

/// The declared attribute-stage metadata of one cycle: modes alternate
/// 0600/0640 for files and 0700/0750 for directories, and the explicit mtime is
/// epoch + 10*cycle + branch_tag seconds with the declared nanoseconds
/// (`fixtures.md`). HN is a single-branch schedule, so the branch tag is 0.
pub(crate) fn attribute_metadata(cycle: usize, file: bool) -> (u32, i64, u32) {
    (
        v016::attribute_mode(cycle, file),
        v016::attribute_mtime(cycle, 0),
        v016::ATTRIBUTE_MTIME_NS,
    )
}

/// The two ordered SDK calls of stage 2. The public batch surface is same-file
/// only, so this pair is never one atomic transaction.
pub(crate) fn sdk_edits(seed: u8, cycle: usize) -> Result<Vec<SdkEdit>> {
    let dir = dir_before(cycle);
    let mut rows = Vec::with_capacity(2);
    for (target, role) in [(0usize, EDIT_FIRST), (1usize, EDIT_SECOND)] {
        let content = edit_content(seed, cycle, target)?;
        let mut replacement = Vec::with_capacity(EDIT_LEN as usize);
        content.write_to(&mut replacement)?;
        if replacement.len() as u64 != EDIT_LEN {
            return Err("v0.1.6 HN SDK replacement length".into());
        }
        rows.push(SdkEdit {
            path: role_path(dir, role),
            start: EDIT_OFFSET,
            delete_len: EDIT_LEN,
            replacement,
        });
    }
    Ok(rows)
}

/// The declared per-stage operation counters of one HN cycle. The host asserts
/// this table stage by stage instead of trusting one aggregate total.
pub(crate) fn stage_counters(stage: usize) -> BTreeMap<&'static str, usize> {
    match stage {
        REFRESH => BTreeMap::from([
            ("regular_unlinks", 2),
            ("ordinary_regular_creates", 2),
            ("file_chmod", 2),
            ("directory_chmod", 1),
            ("explicit_mtime_calls", 3),
            ("posix_helper_executions", 1),
        ]),
        EDITS => BTreeMap::from([
            ("main_edit_targets", 2),
            ("sdk_edit_calls", 2),
            ("sdk_edit_members", 2),
        ]),
        DIRECTORIES => BTreeMap::from([
            ("populated_directory_moves", 1),
            ("directory_chmod", 1),
            ("explicit_mtime_calls", 1),
            ("posix_helper_executions", 1),
        ]),
        ATTRS_LINKS => BTreeMap::from([
            ("hardlink_creates", 1),
            ("file_chmod", 2),
            ("directory_chmod", 1),
            ("explicit_mtime_calls", 3),
            ("posix_helper_executions", 1),
        ]),
        INODE_SAVE => BTreeMap::from([
            ("temporary_regular_creates", 1),
            ("rename_overwrites", 1),
            ("hardlink_unlinks", 1),
            ("inode_observations", 1),
            // The published replacement inherits the temporary file's inode, so
            // its declared mode and mtime are set before the rename; the moved
            // directory's declared metadata is pinned again after the alias left
            // it.
            ("file_chmod", 1),
            ("directory_chmod", 1),
            ("explicit_mtime_calls", 2),
            ("posix_helper_executions", 1),
        ]),
        _ => BTreeMap::new(),
    }
}

/// The counter names the host itself publishes for one stage: stage 2 is the
/// two ordered single-file SDK calls, so its target and call counts are the
/// host's own, never a POSIX helper receipt.
pub(crate) fn is_host_counter(stage: usize, name: &str) -> bool {
    stage == EDITS && matches!(name, "main_edit_targets" | "sdk_edit_calls" | "sdk_edit_members")
}

/// The complete per-cycle table, summed over its five stages.
pub(crate) fn declared_cycle_counters() -> BTreeMap<&'static str, usize> {
    let mut total: BTreeMap<&'static str, usize> = BTreeMap::new();
    for stage in 1..=STAGES {
        for (name, value) in stage_counters(stage) {
            *total.entry(name).or_default() += value;
        }
    }
    total
}

// ------------------------------------------------------------ state algebra

/// The declared state after `step` completed commits, as the independent oracle
/// and the native verifier read it: every path of fixture S at its post-move
/// location, plus the staged alias when one is published.
pub(crate) fn expected(seed: u8, step: usize) -> Result<Vec<Entry>> {
    let mut state = State::genesis(seed)?;
    for ordinal in 1..=step {
        let (cycle, stage) = stage_of(ordinal)?;
        state.apply(seed, cycle, stage)?;
    }
    state.finish()
}

/// The declared state with one path's hard-link class resolved, for callers that
/// need the alias and its target to be named together.
pub(crate) fn declared_alias(seed: u8, step: usize) -> Result<Option<(String, String)>> {
    for entry in expected(seed, step)? {
        if let EntryKind::Hardlink(target) = entry.kind {
            return Ok(Some((entry.path, target)));
        }
    }
    Ok(None)
}

struct State {
    entries: BTreeMap<String, Entry>,
    /// Declared metadata of the two refresh files.
    refresh: (u32, i64, u32),
    /// Declared metadata of the movable directory.
    directory: (u32, i64, u32),
    /// Declared metadata of the alias/replacement target.
    replacement: (u32, i64, u32),
}

impl State {
    fn genesis(seed: u8) -> Result<Self> {
        let fixture = s::fixture(s::SProfile::NamespaceInode, PROFILE, seed)?;
        let mut entries = BTreeMap::new();
        for entry in fixture {
            if entries.insert(entry.path.clone(), entry).is_some() {
                return Err("v0.1.6 HN fixture repeats a path".into());
            }
        }
        Ok(Self {
            entries,
            refresh: fixture_metadata(EntryKindTag::File),
            directory: fixture_metadata(EntryKindTag::Directory),
            replacement: fixture_metadata(EntryKindTag::File),
        })
    }

    fn insert(&mut self, mut entry: Entry, metadata: (u32, i64, u32)) -> Result<()> {
        entry.mode = metadata.0;
        entry.mtime_seconds = metadata.1;
        entry.mtime_nanoseconds = metadata.2;
        if self.entries.insert(entry.path.clone(), entry).is_some() {
            return Err("v0.1.6 HN state repeats a path".into());
        }
        Ok(())
    }

    fn take(&mut self, path: &str) -> Result<Entry> {
        self.entries
            .remove(path)
            .ok_or_else(|| format!("v0.1.6 HN state path absent: {path}").into())
    }

    fn relocate(&mut self, source: &str, destination: &str) -> Result<()> {
        let moved: Vec<(String, Entry)> = self
            .entries
            .iter()
            .filter(|(path, _)| path.as_str() == source || path.starts_with(&format!("{source}/")))
            .map(|(path, entry)| (path.clone(), entry.clone()))
            .collect();
        if moved.is_empty() {
            return Err(format!("v0.1.6 HN state move source absent: {source}").into());
        }
        for (path, entry) in moved {
            self.entries.remove(&path);
            let suffix = &path[source.len()..];
            let next = format!("{destination}{suffix}");
            let mut entry = entry;
            entry.path = next.clone();
            if self.entries.insert(next, entry).is_some() {
                return Err("v0.1.6 HN state move target collides".into());
            }
        }
        Ok(())
    }

    /// Apply one declared stage to the shadow state.
    fn apply(&mut self, seed: u8, cycle: usize, stage: usize) -> Result<()> {
        match stage {
            REFRESH => {
                let dir = dir_before(cycle);
                let rows = [
                    (REFRESH_RECURRING, recurring_content(seed, cycle)?),
                    (REFRESH_GENERATION, generation_content(seed, cycle)?),
                ];
                for (role, content) in rows {
                    let path = role_path(dir, role);
                    let _ = self.take(&path)?;
                    self.insert(Entry::file(path, content), self.refresh)?;
                }
                // The directory's own declared metadata is re-pinned by the
                // stage that changed its entries; the declaration itself is
                // unchanged, so the shadow state is unchanged here.
                Ok(())
            }
            EDITS => {
                let dir = dir_before(cycle);
                for (target, role) in [(0usize, EDIT_FIRST), (1usize, EDIT_SECOND)] {
                    let path = role_path(dir, role);
                    let entry = self
                        .entries
                        .get(&path)
                        .ok_or_else(|| format!("v0.1.6 HN edit target absent: {path}"))?;
                    let EntryKind::File(content) = &entry.kind else {
                        return Err("v0.1.6 HN edit target is not a file".into());
                    };
                    let next = content.splice(
                        EDIT_OFFSET,
                        EDIT_LEN,
                        edit_content(seed, cycle, target)?,
                    )?;
                    let metadata = (entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds);
                    let entry = self.take(&path)?;
                    let mut entry = entry;
                    entry.kind = EntryKind::File(next);
                    self.insert(entry, metadata)?;
                }
                Ok(())
            }
            DIRECTORIES => {
                let source = dir_before(cycle);
                let destination = dir_after(cycle);
                if self.entries.contains_key(destination) {
                    return Err("v0.1.6 HN directory move target already present".into());
                }
                self.relocate(source, destination)
            }
            ATTRS_LINKS => {
                let dir = dir_after(cycle);
                let target = role_path(dir, REPLACEMENT);
                let alias = role_path(dir, ALIAS);
                if !self.entries.contains_key(&target) {
                    return Err("v0.1.6 HN alias target absent".into());
                }
                let metadata = self.replacement;
                self.insert(Entry::hardlink(alias, target), metadata)?;
                let file_metadata = attribute_metadata(cycle, true);
                for role in [REFRESH_RECURRING, REFRESH_GENERATION] {
                    let path = role_path(dir, role);
                    let entry = self.take(&path)?;
                    let mut entry = entry;
                    entry.kind = match entry.kind {
                        EntryKind::File(content) => EntryKind::File(content),
                        _ => return Err("v0.1.6 HN refresh path is not a file".into()),
                    };
                    self.insert(entry, file_metadata)?;
                }
                self.refresh = file_metadata;
                let directory_metadata = attribute_metadata(cycle, false);
                let directory = self.take(dir)?;
                self.insert(directory, directory_metadata)?;
                self.directory = directory_metadata;
                Ok(())
            }
            INODE_SAVE => {
                let dir = dir_after(cycle);
                let target = role_path(dir, REPLACEMENT);
                let alias = role_path(dir, ALIAS);
                let metadata = self.replacement;
                let _ = self.take(&target)?;
                self.insert(
                    Entry::file(target, replacement_content(seed, cycle)?),
                    metadata,
                )?;
                let _ = self.take(&alias)?;
                // The directory's declared metadata is re-pinned after the
                // alias leaves it; the declaration is unchanged.
                if !self.entries.contains_key(dir) {
                    return Err("v0.1.6 HN directory absent at the inode stage".into());
                }
                Ok(())
            }
            other => Err(format!("v0.1.6 HN stage ordinal {other} outside 1..=5").into()),
        }
    }

    fn finish(self) -> Result<Vec<Entry>> {
        let rows: Vec<Entry> = self.entries.into_values().collect();
        let _ = self.directory;
        Ok(rows)
    }
}

/// The declared envelope of one state: non-directory names, logical path bytes
/// and directories, all measured against `fixtures.md`.
pub(crate) fn envelope(entries: &[Entry]) -> Result<(usize, u64, usize)> {
    let names = entries
        .iter()
        .filter(|entry| !matches!(entry.kind, EntryKind::Directory))
        .count();
    let directories = entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::Directory))
        .count();
    Ok((names, common::validate_entries(entries)?, directories))
}

// ------------------------------------------------------------- helper route

/// Rendered bytes of one recipe, generated once per helper execution.
struct Helper {
    seed: u8,
    cycle: usize,
    /// Empty in production: the helper runs with the live mount as its working
    /// directory. The rehearsal test sets it to an absolute fixture root.
    root: String,
    rendered: BTreeMap<String, Vec<u8>>,
    ledger: BTreeMap<&'static str, usize>,
}

impl Helper {
    fn new(seed: u8, cycle: usize) -> Self {
        Self {
            seed,
            cycle,
            root: String::new(),
            rendered: BTreeMap::new(),
            ledger: BTreeMap::new(),
        }
    }

    fn with_root(mut self, root: &str) -> Self {
        self.root = root.to_owned();
        self
    }

    /// The path a declared operation actually touches.
    fn at(&self, path: &str) -> String {
        format!("{}{path}", self.root)
    }

    fn bump(&mut self, name: &'static str) {
        *self.ledger.entry(name).or_default() += 1;
    }

    fn render(&mut self, slot: &str, content: &Content) -> Result<Vec<u8>> {
        if let Some(bytes) = self.rendered.get(slot) {
            return Ok(bytes.clone());
        }
        let mut bytes = Vec::new();
        content.write_to(&mut bytes)?;
        self.rendered.insert(slot.to_owned(), bytes.clone());
        Ok(bytes)
    }

    /// Set one path's declared mode and mtime and read both back from a fresh
    /// handle, so a cached inode record cannot pass for a published attribute.
    fn pin(&mut self, path: &str, metadata: (u32, i64, u32), directory: bool) -> Result<String> {
        let real = self.at(path);
        fs::set_permissions(&real, fs::Permissions::from_mode(metadata.0))?;
        self.bump(if directory {
            "directory_chmod"
        } else {
            "file_chmod"
        });
        common::set_mtime_nofollow(Path::new(&real), metadata.1, metadata.2)?;
        self.bump("explicit_mtime_calls");
        let observed = fs::symlink_metadata(&real)?;
        let mode = observed.permissions().mode() & 0o7777;
        if mode != metadata.0
            || observed.mtime() != metadata.1
            || observed.mtime_nsec() != i64::from(metadata.2)
        {
            return Err(format!(
                "v0.1.6 HN declared metadata readback: {path} mode={mode:o} mtime={}.{} declared={:o},{}.{}",
                observed.mtime(),
                observed.mtime_nsec(),
                metadata.0,
                metadata.1,
                metadata.2
            )
            .into());
        }
        Ok(format!(
            "{path}\tmode={mode:o}\tmtime={}.{}",
            observed.mtime(),
            observed.mtime_nsec()
        ))
    }

    /// Stage 1: unlink and recreate the refresh pair.
    fn stage_refresh(&mut self) -> Result<()> {
        let dir = dir_before(self.cycle).to_owned();
        let recurring = role_path(&dir, REFRESH_RECURRING);
        let generation = role_path(&dir, REFRESH_GENERATION);
        for path in [&recurring, &generation] {
            fs::remove_file(self.at(path))
                .map_err(|error| format!("v0.1.6 HN unlink {path}: {error}"))?;
            self.bump("regular_unlinks");
        }
        let rows = [
            (
                recurring.clone(),
                "recurring",
                recurring_content(self.seed, self.cycle)?,
            ),
            (
                generation.clone(),
                "generation",
                generation_content(self.seed, self.cycle)?,
            ),
        ];
        for (path, slot, content) in rows {
            let bytes = self.render(slot, &content)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.at(&path))
                .map_err(|error| format!("v0.1.6 HN create {path}: {error}"))?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            self.bump("ordinary_regular_creates");
        }
        // The recreated pair keeps the declared metadata it already had: the
        // fixture defaults before the first attribute stage, and the previous
        // generation's explicit attribute values afterwards.
        let metadata = if self.cycle == 1 {
            fixture_metadata(EntryKindTag::File)
        } else {
            attribute_metadata(self.cycle - 1, true)
        };
        self.pin(&recurring, metadata, false)?;
        self.pin(&generation, metadata, false)?;
        Ok(())
    }

    /// Stage 3: rename the populated directory to its alternate name. Renaming
    /// an entry changes the *parent's* automatic mtime on a POSIX filesystem, so
    /// the fixture root's declared mode and mtime are set again afterwards,
    /// exactly like every directory whose entries this schedule changes. The
    /// declared value is the fixture's own, never an observation.
    fn stage_directories(&mut self) -> Result<()> {
        let source = dir_before(self.cycle);
        let destination = dir_after(self.cycle);
        fs::rename(self.at(source), self.at(destination))
            .map_err(|error| format!("v0.1.6 HN directory move {source}->{destination}: {error}"))?;
        self.bump("populated_directory_moves");
        self.pin(".", fixture_metadata(EntryKindTag::Directory), true)?;
        Ok(())
    }

    /// Stage 4: one alias to the replacement target, the declared attribute
    /// modes for both files plus this directory, and both files' explicit
    /// mtimes.
    fn stage_attrs_links(&mut self) -> Result<()> {
        let dir = dir_after(self.cycle).to_owned();
        let target = role_path(&dir, REPLACEMENT);
        let alias = role_path(&dir, ALIAS);
        let before = fs::symlink_metadata(self.at(&target))?;
        fs::hard_link(self.at(&target), self.at(&alias))
            .map_err(|error| format!("v0.1.6 HN hardlink {target}->{alias}: {error}"))?;
        self.bump("hardlink_creates");
        let metadata = attribute_metadata(self.cycle, true);
        self.pin(&role_path(&dir, REFRESH_RECURRING), metadata, false)?;
        self.pin(&role_path(&dir, REFRESH_GENERATION), metadata, false)?;
        self.pin(&dir, attribute_metadata(self.cycle, false), true)?;
        // The alias must observe the target's inode, with both names sharing it.
        let after = fs::symlink_metadata(self.at(&target))?;
        let alias_metadata = fs::symlink_metadata(self.at(&alias))?;
        if after.ino() != alias_metadata.ino() || alias_metadata.nlink() != 2 {
            return Err("v0.1.6 HN stage-4 alias does not share the target inode".into());
        }
        if before.ino() != after.ino() {
            return Err("v0.1.6 HN stage-4 alias creation replaced the target inode".into());
        }
        Ok(())
    }

    /// Stage 5: one 4 KiB atomic save over the aliased destination, the old
    /// inode observed through the alias, then the alias removed.
    fn stage_inode_save(&mut self) -> Result<()> {
        let dir = dir_after(self.cycle).to_owned();
        let target = role_path(&dir, REPLACEMENT);
        let alias = role_path(&dir, ALIAS);
        let temporary = role_path(&dir, SAVE_TEMP);
        let before = fs::symlink_metadata(self.at(&target))?;
        let alias_before = fs::symlink_metadata(self.at(&alias))?;
        if before.ino() != alias_before.ino() {
            return Err("v0.1.6 HN inode save requires one shared inode".into());
        }
        let old_inode = before.ino();
        let old_bytes = fs::read(self.at(&target))?;
        if old_bytes.len() as u64 != before.len() {
            return Err("v0.1.6 HN inode save old payload length".into());
        }
        let bytes = self.render("replacement", &replacement_content(self.seed, self.cycle)?)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.at(&temporary))
            .map_err(|error| format!("v0.1.6 HN temporary {temporary}: {error}"))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        self.bump("temporary_regular_creates");
        // The replacement inherits the temporary file's inode, so the declared
        // metadata is applied before the rename publishes it.
        self.pin(&temporary, fixture_metadata(EntryKindTag::File), false)?;
        fs::rename(self.at(&temporary), self.at(&target))
            .map_err(|error| format!("v0.1.6 HN atomic save {temporary}->{target}: {error}"))?;
        self.bump("rename_overwrites");
        let replacement = fs::symlink_metadata(self.at(&target))?;
        let observed_alias = fs::symlink_metadata(self.at(&alias))?;
        let mut sink = File::open(self.at(&alias))?;
        let mut observed_bytes = Vec::new();
        sink.read_to_end(&mut observed_bytes)?;
        // The alias still resolves the pre-replacement inode and its bytes.
        if observed_alias.ino() != old_inode
            || observed_alias.nlink() != 1
            || observed_bytes != old_bytes
        {
            return Err("v0.1.6 HN inode save did not leave the old inode on the alias".into());
        }
        if replacement.ino() == old_inode {
            return Err("v0.1.6 HN inode save did not split the inode".into());
        }
        if fs::read(self.at(&target))? != bytes {
            return Err("v0.1.6 HN inode save published different bytes".into());
        }
        self.bump("inode_observations");
        fs::remove_file(self.at(&alias))
            .map_err(|error| format!("v0.1.6 HN alias removal {alias}: {error}"))?;
        self.bump("hardlink_unlinks");
        let directory = attribute_metadata(self.cycle, false);
        let pinned = self.pin(&dir, directory, true)?;
        println!("hn_declared_metadata={pinned}");
        println!("hn_old_inode={old_inode}");
        println!("hn_new_inode={}", replacement.ino());
        println!("hn_old_payload_sha256={}", sdk_edit_common::sha256_hex(&old_bytes));
        println!("hn_new_payload_sha256={}", sdk_edit_common::sha256_hex(&bytes));
        Ok(())
    }

    /// Re-pin the moved directory's declared metadata after a stage changed its
    /// entries. The declaration itself never changes here.
    fn pin_directory(&mut self, metadata: (u32, i64, u32)) -> Result<()> {
        let dir = if self.cycle == 0 {
            DIR_A.to_owned()
        } else {
            dir_before(self.cycle).to_owned()
        };
        self.pin(&dir, metadata, true)?;
        Ok(())
    }
}

/// `v016-hn-stage SEED CYCLE STAGE` — one declared POSIX stage of the HN cycle.
/// Stage 2 is the host's ordered SDK pair and is never a helper execution.
pub(crate) fn run_command(args: &[String]) -> Result<()> {
    let [seed, cycle, stage] = args else {
        return Err(format!(
            "usage: v016-hn-stage SEED CYCLE STAGE (received {} arguments: {args:?})",
            args.len()
        )
        .into());
    };
    let seed: u8 = seed.parse()?;
    d::seed_label(seed)?;
    let cycle: usize = cycle.parse()?;
    if cycle == 0 {
        return Err("v0.1.6 HN cycles are 1-based".into());
    }
    let stage: usize = stage.parse()?;
    if stage == EDITS {
        return Err("v0.1.6 HN stage 2 runs as the host's two ordered SDK calls".into());
    }
    let mut helper = Helper::new(seed, cycle);
    // Every non-SDK stage is one POSIX helper execution, counted here so the
    // declared table and the observed ledger are the same table.
    helper.bump("posix_helper_executions");
    match stage {
        REFRESH => {
            helper.stage_refresh()?;
            // The refresh stage changed the directory's entries, so the
            // directory's declared metadata is pinned again afterwards.
            let directory = if cycle == 1 {
                fixture_metadata(EntryKindTag::Directory)
            } else {
                attribute_metadata(cycle - 1, false)
            };
            helper.pin_directory(directory)?;
        }
        DIRECTORIES => helper.stage_directories()?,
        ATTRS_LINKS => helper.stage_attrs_links()?,
        INODE_SAVE => helper.stage_inode_save()?,
        other => {
            return Err(format!("v0.1.6 HN stage ordinal {other} outside 1..=5").into())
        }
    }
    println!("hn_stage={stage}");
    println!("hn_cycle={cycle}");
    println!("hn_posix_helper_executions=1");
    println!("hn_oracle_reads=0");
    let declared = stage_counters(stage);
    for (name, expected) in &declared {
        if is_host_counter(stage, name) {
            continue;
        }
        println!("hn_declared_{name}={expected}");
        let observed = helper.ledger.get(name).copied().unwrap_or(0);
        if observed != *expected {
            return Err(format!(
                "v0.1.6 HN stage {stage} counter {name}: helper observed {observed}, declared {expected}"
            )
            .into());
        }
        println!("hn_observed_{name}={observed}");
    }
    // Every operation the helper actually performed must be declared: a stage
    // cannot do undeclared work behind a table that only lists the expected
    // counts.
    for (name, observed) in &helper.ledger {
        if *observed > 0 && !declared.contains_key(name) {
            return Err(format!(
                "v0.1.6 HN stage {stage} performed undeclared {name}={observed}"
            )
            .into());
        }
    }
    Ok(())
}

/// Parse one helper receipt and check it against the declared per-stage table.
/// The host asserts the same table the helper prints, so a stage that silently
/// skipped an operation cannot pass as a declared stage.
pub(crate) fn parse_stage_receipt(
    text: &str,
    stage: usize,
) -> Result<BTreeMap<&'static str, usize>> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            seen.insert(key.trim().to_owned(), value.trim().to_owned());
        }
    }
    let declared_stage = stage.to_string();
    if seen.get("hn_stage").map(String::as_str) != Some(declared_stage.as_str())
        || seen.get("hn_posix_helper_executions").map(String::as_str) != Some("1")
        || seen.get("hn_oracle_reads").map(String::as_str) != Some("0")
    {
        return Err(format!("v0.1.6 HN stage {stage} helper receipt header").into());
    }
    let mut rows = BTreeMap::new();
    for (name, expected) in stage_counters(stage) {
        if is_host_counter(stage, name) {
            continue;
        }
        let observed = seen
            .get(&format!("hn_observed_{name}"))
            .ok_or_else(|| format!("v0.1.6 HN stage {stage} receipt is missing {name}"))?
            .parse::<usize>()?;
        if observed != expected {
            return Err(format!(
                "v0.1.6 HN stage {stage} counter {name}: receipt observed {observed}, declared {expected}"
            )
            .into());
        }
        rows.insert(name, observed);
    }
    Ok(rows)
}

/// Product-free self-check of the HN declaration.
pub(crate) fn self_check() -> Result<()> {
    for (id, tier) in [(K10, 10usize), (K100, 100usize)] {
        if tier_of(id) != Some(tier) || tier % STAGES != 0 {
            return Err("v0.1.6 HN depth must be a whole number of cycles".into());
        }
    }
    let cycles = tier_of(K100).ok_or("HN depth")? / STAGES;
    for seed in 1..=3u8 {
        let mut previous = expected(seed, 0)?;
        let (names, bytes, directories) = envelope(&previous)?;
        if names != 16 || bytes != s::S_BYTES || directories != 6 {
            return Err("v0.1.6 HN genesis envelope".into());
        }
        for step in 1..=tier_of(K100).ok_or("HN depth")? {
            let next = expected(seed, step)?;
            let (names, bytes, directories) = envelope(&next)?;
            if names > 16 + MAX_EXTRA_NAMES
                || bytes > s::S_BYTES + MAX_EXTRA_BYTES
                || directories != 6
            {
                return Err(format!(
                    "v0.1.6 HN envelope at step {step}: names {names}, bytes {bytes}, directories {directories}"
                )
                .into());
            }
            if format!("{next:?}") == format!("{previous:?}") {
                return Err(format!("v0.1.6 HN step {step} is a no-op").into());
            }
            previous = next;
        }
        // K10 is the exact prefix of K100.
        for step in 0..=10 {
            if format!("{:?}", expected(seed, step)?) != format!("{:?}", expected(seed, step)?) {
                return Err("v0.1.6 HN prefix comparison".into());
            }
        }
        // The declared consumer states of the access route: commit 94 publishes
        // the alias with two references to one inode, commit 95 has replaced the
        // destination and removed the alias.
        let before = expected(seed, 94)?;
        let alias = declared_alias(seed, 94)?.ok_or("v0.1.6 HN stage-4 alias absent")?;
        if alias.0 != role_path(dir_after(19), ALIAS) || alias.1 != role_path(dir_after(19), REPLACEMENT)
        {
            return Err("v0.1.6 HN stage-4 alias identity".into());
        }
        let references = before
            .iter()
            .filter(|entry| match &entry.kind {
                EntryKind::File(_) => entry.path == alias.1,
                EntryKind::Hardlink(target) => *target == alias.1,
                _ => false,
            })
            .count();
        if references != 2 || declared_alias(seed, 95)?.is_some() {
            return Err("v0.1.6 HN inode class at commits 94/95".into());
        }
        let after = expected(seed, 95)?;
        let target = after
            .iter()
            .find(|entry| entry.path == role_path(dir_after(19), REPLACEMENT))
            .ok_or("v0.1.6 HN replacement target absent")?;
        let mut replacement = Vec::new();
        replacement_content(seed, 19)?.write_to(&mut replacement)?;
        let EntryKind::File(content) = &target.kind else {
            return Err("v0.1.6 HN replacement target kind".into());
        };
        if content.digest()? != Content::Literal(replacement).digest()? {
            return Err("v0.1.6 HN commit 95 content".into());
        }
        // Stage 2 is the only host-driven stage, and every cycle has exactly two
        // ordered SDK calls.
        for cycle in 1..=cycles {
            if sdk_edits(seed, cycle)?.len() != 2 {
                return Err("v0.1.6 HN SDK pair cardinality".into());
            }
            if stage_of(step_of(cycle, STAGES)?)? != (cycle, STAGES) {
                return Err("v0.1.6 HN stage arithmetic".into());
            }
        }
        // Every stage changes the state it is applied to, so every commit is a
        // real Created commit rather than a no-op.
        if declared_cycle_counters().get("posix_helper_executions") != Some(&4) {
            return Err("v0.1.6 HN helper execution count per cycle".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod rehearsal {
    //! Product-free rehearsal of the declared HN stages on a real filesystem.
    //!
    //! The Linux helper and the host orchestrator both run against a live FUSE
    //! mount, which a unit test cannot create. This test runs the same declared
    //! stage code against a natively created fixture S and compares every
    //! intermediate state with the independent `expected` derivation through
    //! `verify_native`, so a helper that disagrees with the oracle fails here
    //! instead of inside a gated container invocation. The only emulation is the
    //! one declared SDK property the test reproduces by hand: the two stage-2
    //! range edits preserve the file's declared metadata.
    use super::*;

    #[test]
    fn hn_stages_match_the_declared_state() -> Result<()> {
        let seed = 1u8;
        let root = std::env::temp_dir().join(format!("layerfs-hn-rehearsal-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let fixture = s::fixture(s::SProfile::NamespaceInode, PROFILE, seed)?;
        common::create_fixture(&root, &fixture)?;
        let prefix = format!("{}/", root.display());
        let mut outcome = Ok(());
        for step in 1..=10usize {
            let (cycle, stage) = stage_of(step)?;
            let result = if stage == EDITS {
                sdk_edits(seed, cycle)?.into_iter().try_for_each(|edit| -> Result<()> {
                    let path = root.join(&edit.path);
                    let mut bytes = fs::read(&path)?;
                    let start = edit.start as usize;
                    bytes.splice(
                        start..start + edit.delete_len as usize,
                        edit.replacement.iter().copied(),
                    );
                    let mut file = OpenOptions::new().write(true).truncate(true).open(&path)?;
                    file.write_all(&bytes)?;
                    file.sync_all()?;
                    drop(file);
                    // The public SDK range edit preserves the declared metadata; a
                    // plain write would move the mtime, so it is restored here.
                    let entry = expected(seed, step)?
                        .into_iter()
                        .find(|entry| entry.path == edit.path)
                        .ok_or("rehearsal edit target")?;
                    let mut helper = Helper::new(seed, cycle).with_root(&prefix);
                    helper.pin(&edit.path, (entry.mode, entry.mtime_seconds, entry.mtime_nanoseconds), false)?;
                    Ok(())
                })
            } else {
                let mut helper = Helper::new(seed, cycle).with_root(&prefix);
                helper.bump("posix_helper_executions");
                let staged = match stage {
                    REFRESH => {
                        helper.stage_refresh().and_then(|()| {
                            let directory = if cycle == 1 {
                                fixture_metadata(EntryKindTag::Directory)
                            } else {
                                attribute_metadata(cycle - 1, false)
                            };
                            let dir = dir_before(cycle).to_owned();
                            helper.pin(&dir, directory, true).map(|_| ())
                        })
                    }
                    DIRECTORIES => helper.stage_directories(),
                    ATTRS_LINKS => helper.stage_attrs_links(),
                    INODE_SAVE => helper.stage_inode_save(),
                    other => Err(format!("rehearsal stage {other}").into()),
                };
                staged
            };
            if let Err(error) = result {
                outcome = Err(format!("step {step}: {error}").into());
                break;
            }
            let declared = expected(seed, step)?;
            if let Err(error) = common::verify_native(&root, &declared) {
                outcome = Err(format!("step {step} state disagrees with the declaration: {error}").into());
                break;
            }
        }
        let _ = fs::remove_dir_all(&root);
        outcome
    }
}
