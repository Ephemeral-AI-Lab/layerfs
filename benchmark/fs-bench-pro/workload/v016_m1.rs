// v0.1.6 M1 stage helper: the five declared POSIX stages of schedule M1.
//
// One execution per POSIX stage, launched by the host through
// `Client::exec_workspace_session`. The helper runs with the live FUSE mount
// as its working directory, performs only the declared operations, closes
// every writable descriptor, and prints an exact operation receipt. It never
// reads a Store, an SDK type or a verification index.
use super::v016_common::{self as v016, LoadFixture, SymlinkKind};
use super::v016_stages::{self as stages, Stage};
use super::{dedup_workloads as d, Result};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// Declared operation classes. Counters are named explicitly so the host checks
/// the recipe instead of trusting one aggregate syscall total.
#[derive(Clone, Debug, Default)]
struct Ledger {
    unlink: usize,
    create: usize,
    create_excl_fail: usize,
    pwrite: usize,
    append: usize,
    ftruncate: usize,
    read: usize,
    fsync: usize,
    open: usize,
    close: usize,
    rename: usize,
    rename_overwrite: usize,
    hardlink: usize,
    hardlink_unlink: usize,
    symlink: usize,
    symlink_unlink: usize,
    readlink: usize,
    mkdir: usize,
    rmdir: usize,
    chmod: usize,
    directory_chmod: usize,
    mtime: usize,
    lstat: usize,
    temporary_create: usize,
    moved_directory: usize,
    alias_probe_write: usize,
    unlinked_descriptor_write: usize,
    main_edit_target: usize,
}

impl Ledger {
    fn rows(&self) -> Vec<(&'static str, usize)> {
        vec![
            ("unlink", self.unlink),
            ("create", self.create),
            ("create_excl_fail", self.create_excl_fail),
            ("pwrite", self.pwrite),
            ("append", self.append),
            ("ftruncate", self.ftruncate),
            ("read", self.read),
            ("fsync", self.fsync),
            ("open", self.open),
            ("close", self.close),
            ("rename", self.rename),
            ("rename_overwrite", self.rename_overwrite),
            ("hardlink", self.hardlink),
            ("hardlink_unlink", self.hardlink_unlink),
            ("symlink", self.symlink),
            ("symlink_unlink", self.symlink_unlink),
            ("readlink", self.readlink),
            ("mkdir", self.mkdir),
            ("rmdir", self.rmdir),
            ("chmod", self.chmod),
            ("directory_chmod", self.directory_chmod),
            ("mtime", self.mtime),
            ("lstat", self.lstat),
            ("temporary_create", self.temporary_create),
            ("moved_directory", self.moved_directory),
            ("alias_probe_write", self.alias_probe_write),
            ("unlinked_descriptor_write", self.unlinked_descriptor_write),
            ("main_edit_target", self.main_edit_target),
        ]
    }
}

#[derive(Clone, Debug)]
struct Evidence {
    key: String,
    fields: BTreeMap<String, String>,
}

struct Helper {
    fixture: LoadFixture,
    cycle: usize,
    branch_tag: u64,
    branch_salt: String,
    ledger: Ledger,
    evidence: Vec<Evidence>,
    rendered: BTreeMap<String, Vec<u8>>,
    /// Generated-content wall, reported separately from product operations.
    render_ns: u64,
}

fn set_mode(path: &str, mode: u32) -> Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn set_mtime(path: &str, seconds: i64, nanoseconds: u32) -> Result<()> {
    let file = OpenOptions::new().write(true).open(path)?;
    let base = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(u64::try_from(seconds)?))
        .ok_or("v0.1.6 mtime base")?;
    let stamp = base + std::time::Duration::from_nanos(u64::from(nanoseconds));
    file.set_times(std::fs::FileTimes::new().set_accessed(stamp).set_modified(stamp))?;
    Ok(())
}

fn h(path: &str) -> Result<String> {
    Ok(super::sdk_edit_common::sha256_hex(path.as_bytes()))
}

fn sha(bytes: &[u8]) -> String {
    super::sdk_edit_common::sha256_hex(bytes)
}

fn parent_of(path: &str) -> Result<String> {
    let (parent, _) = path
        .rsplit_once('/')
        .ok_or_else(|| format!("v0.1.6 role path has no parent: {path}"))?;
    Ok(parent.to_owned())
}

impl Helper {
    fn new(fixture: LoadFixture, cycle: usize, branch_tag: u64, branch_salt: &str) -> Self {
        Self {
            fixture,
            cycle,
            branch_tag,
            branch_salt: branch_salt.to_owned(),
            ledger: Ledger::default(),
            evidence: Vec::new(),
            rendered: BTreeMap::new(),
            render_ns: 0,
        }
    }

    fn record(&mut self, key: impl Into<String>, fields: BTreeMap<String, String>) {
        self.evidence.push(Evidence {
            key: key.into(),
            fields,
        });
    }

    fn rows(&mut self, key: &str, rows: Vec<(String, String)>) {
        for (name, value) in rows {
            self.record(
                key.to_owned(),
                BTreeMap::from([("name".to_owned(), name), ("value".to_owned(), value)]),
            );
        }
    }

    /// Rendered bytes by recipe key, so a repeated deterministic recipe is
    /// generated once per execution instead of once per write. The key is the
    /// recipe's declared identity (cycle, cohort, slot or role), never the
    /// observed filesystem state.
    fn render(&mut self, slot: &str, content: &super::workspace_common::Content) -> Result<Vec<u8>> {
        if let Some(bytes) = self.rendered.get(slot) {
            return Ok(bytes.clone());
        }
        let started = std::time::Instant::now();
        let mut bytes = Vec::new();
        content.write_to(&mut bytes)?;
        self.render_ns += started.elapsed().as_nanos() as u64;
        self.rendered.insert(slot.to_owned(), bytes.clone());
        Ok(bytes)
    }

    fn recipe(
        &mut self,
        slot: &str,
        profile: &str,
        ordinal: usize,
        role: &str,
        len: u64,
    ) -> Result<Vec<u8>> {
        let content = d::content(
            v016::FAMILY_MIXED,
            profile,
            self.fixture.seed,
            ordinal,
            role,
            len,
        )?;
        self.render(slot, &content)
    }

    /// The parent directory of each movable populated subtree as it stands at
    /// this cycle's stage 3. The alternation is a declared function of the
    /// cycle, not an observation.
    fn link_parents(&self) -> Result<(String, String)> {
        let moved = |root: &str, index: usize| -> Result<String> {
            if self.cycle % 2 == 1 {
                Ok(self.fixture.roles.move_dest[index].clone())
            } else {
                let _ = root;
                Ok(String::new())
            }
        };
        Ok((
            moved(&self.fixture.roles.move_a_root, 0)?,
            moved(&self.fixture.roles.move_b_root, 1)?,
        ))
    }

    fn hardlink_alias(&self, parent: &str, index: usize) -> String {
        if parent.is_empty() {
            format!("hl{index:02}")
        } else {
            format!("{parent}/hl{index:02}")
        }
    }

    fn symlink_path(&self, parent: &str, index: usize) -> String {
        if parent.is_empty() {
            format!("sl{index:02}")
        } else {
            format!("{parent}/sl{index:02}")
        }
    }

    // ------------------------------------------------------------ stage one

    fn stage_refresh(&mut self) -> Result<()> {
        let cycle = self.cycle;
        let cohort = v016::cohort_for_cycle(cycle)?;
        let visits = v016::cohort_visit_counts(cycle - 1)?;
        let prior = visits[cohort];
        let base = cohort * v016::REFRESH_FILES_PER_COHORT;
        let mut recurring_a = 0usize;
        let mut recurring_b = 0usize;
        for slot in 0..v016::REFRESH_FILES_PER_COHORT {
            let path = self.fixture.roles.refresh_pool[base + slot].clone();
            // Stage 1 deletes before creation.
            if Path::new(&path).exists() {
                fs::remove_file(&path)?;
                self.ledger.unlink += 1;
            }
            let content = stages::stage1_content(
                &self.fixture,
                cycle,
                cohort,
                slot,
                prior,
                &self.branch_salt,
            )?;
            let bytes = self.render(
                &format!("refresh-c{cycle}-cohort{cohort}-slot{slot}"),
                &content,
            )?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)?;
            self.ledger.open += 1;
            self.ledger.create += 1;
            file.write_all(&bytes)?;
            self.ledger.pwrite += 1;
            file.sync_all()?;
            self.ledger.fsync += 1;
            drop(file);
            self.ledger.close += 1;
            if stages::stage1_slot_role(slot) == v016::RefreshClass::Recurring {
                if v016::recurrent_value(prior) == 0 {
                    recurring_a += 1;
                } else {
                    recurring_b += 1;
                }
            }
        }
        // One bounded negative probe: O_CREAT|O_EXCL on an existing path must
        // fail with EEXIST and leave the entry unchanged.
        let probe = self.fixture.roles.refresh_pool[stages::eexist_probe_index(cohort)].clone();
        let before = fs::metadata(&probe)?;
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(_) => return Err(format!("v0.1.6 EEXIST probe created {probe}").into()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                self.ledger.create_excl_fail += 1;
            }
            Err(error) => return Err(error.into()),
        }
        let after = fs::metadata(&probe)?;
        if before.ino() != after.ino() || before.len() != after.len() {
            return Err("v0.1.6 EEXIST probe changed the entry".into());
        }
        self.record(
            "stage1",
            BTreeMap::from([
                ("cycle".to_owned(), cycle.to_string()),
                ("cohort".to_owned(), cohort.to_string()),
                ("prior_visits".to_owned(), prior.to_string()),
                ("recurring_a".to_owned(), recurring_a.to_string()),
                ("recurring_b".to_owned(), recurring_b.to_string()),
                ("local".to_owned(), "16".to_owned()),
                ("unique".to_owned(), "16".to_owned()),
                ("probe_path".to_owned(), probe),
                ("probe_ino".to_owned(), after.ino().to_string()),
                (
                    "create_names_sha256".to_owned(),
                    sha(format!("{cycle}-{cohort}").as_bytes()),
                ),
            ]),
        );
        Ok(())
    }

    fn pwrite_probe(&mut self, path: &str, offset: u64, bytes: &[u8]) -> Result<String> {
        let mut file = OpenOptions::new().read(true).write(true).open(path)?;
        self.ledger.open += 1;
        file.seek(SeekFrom::Start(offset))?;
        let mut before = vec![0u8; bytes.len()];
        file.read_exact(&mut before)?;
        self.ledger.read += 1;
        file.seek(SeekFrom::Start(offset))?;
        file.write_all(bytes)?;
        self.ledger.pwrite += 1;
        file.sync_all()?;
        self.ledger.fsync += 1;
        drop(file);
        self.ledger.close += 1;
        Ok(sha(&before))
    }

    // ------------------------------------------------------------ stage two

    fn stage_edits(&mut self) -> Result<()> {
        let cycle = self.cycle;
        let plan = stages::stage2_plan(&self.fixture, cycle)?;
        let mut rows = Vec::new();
        for (path, operation) in plan {
            if stages::is_sdk_row(&operation) {
                continue;
            }
            self.ledger.main_edit_target += 1;
            match operation {
                stages::Stage2Op::TinyPwrite { ordinal } => {
                    let offset = stages::tiny_pwrite_offset(cycle);
                    let bytes = self.recipe(
                        &format!("tiny-pwrite-{cycle}-{ordinal}"),
                        "v016-stage2",
                        ordinal * 8 + cycle,
                        &format!("tiny-pwrite-c{cycle}"),
                        256,
                    )?;
                    let before = self.pwrite_probe(&path, offset, &bytes)?;
                    rows.push((
                        format!("tiny-pwrite:{path}"),
                        format!("{offset}\t{}\t{before}", bytes.len()),
                    ));
                }
                stages::Stage2Op::MediumMidpoint => {
                    let len = fs::metadata(&path)?.len();
                    let offset = ((len / 2) / 8) * 8;
                    let bytes = self.recipe(
                        &format!("medium-midpoint-{cycle}"),
                        "v016-stage2",
                        cycle,
                        &format!("medium-midpoint-c{cycle}"),
                        256,
                    )?;
                    let before = self.pwrite_probe(&path, offset, &bytes)?;
                    rows.push((
                        format!("medium-midpoint:{path}"),
                        format!("{offset}\t{}\t{before}", bytes.len()),
                    ));
                }
                stages::Stage2Op::MediumNetZero => {
                    // Timed real writes and truncate; the probe is never
                    // optimized out even though its final content is unchanged.
                    let bytes = self.recipe(
                        &format!("medium-net-zero-{cycle}"),
                        "v016-stage2",
                        cycle,
                        &format!("medium-net-zero-c{cycle}"),
                        4_096,
                    )?;
                    let len = fs::metadata(&path)?.len();
                    let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
                    self.ledger.open += 1;
                    file.seek(SeekFrom::End(0))?;
                    file.write_all(&bytes)?;
                    self.ledger.append += 1;
                    file.sync_all()?;
                    self.ledger.fsync += 1;
                    file.set_len(len)?;
                    self.ledger.ftruncate += 1;
                    file.sync_all()?;
                    self.ledger.fsync += 1;
                    drop(file);
                    self.ledger.close += 1;
                    let after = fs::metadata(&path)?.len();
                    if after != len {
                        return Err("v0.1.6 net-zero probe changed the length".into());
                    }
                    rows.push((
                        format!("medium-net-zero:{path}"),
                        format!("{len}\t{after}\t{bytes_len}", bytes_len = bytes.len()),
                    ));
                }
                stages::Stage2Op::MediumReplaceTail => {
                    let bytes = self.recipe(
                        &format!("medium-tail-{cycle}"),
                        "v016-stage2",
                        cycle,
                        &format!("medium-tail-c{cycle}"),
                        4_096,
                    )?;
                    let len = fs::metadata(&path)?.len();
                    let mut file = OpenOptions::new().read(true).write(true).open(&path)?;
                    self.ledger.open += 1;
                    file.set_len(len - 4_096)?;
                    self.ledger.ftruncate += 1;
                    file.seek(SeekFrom::End(0))?;
                    file.write_all(&bytes)?;
                    self.ledger.append += 1;
                    file.sync_all()?;
                    self.ledger.fsync += 1;
                    drop(file);
                    self.ledger.close += 1;
                    let after = fs::metadata(&path)?.len();
                    if after != len {
                        return Err("v0.1.6 tail replacement changed the length".into());
                    }
                    rows.push((
                        format!("medium-replace-tail:{path}"),
                        format!("{len}\t{after}\t{}", sha(&bytes)),
                    ));
                }
                stages::Stage2Op::AnchorOverwrite => {
                    let len = fs::metadata(&path)?.len();
                    let offset = stages::anchor_offset(cycle, len);
                    let bytes = self.recipe(
                        &format!("anchor-{cycle}"),
                        "v016-stage2",
                        cycle,
                        &format!("anchor-c{cycle}"),
                        4_096,
                    )?;
                    let before = self.pwrite_probe(&path, offset, &bytes)?;
                    rows.push((
                        format!("anchor-overwrite:{path}"),
                        format!("{offset}\t{}\t{before}", bytes.len()),
                    ));
                }
                stages::Stage2Op::BoundaryShrink => {
                    let (below, above) = stages::boundary_lengths(cycle);
                    let current = fs::metadata(&path)?.len();
                    let mut file = OpenOptions::new().write(true).open(&path)?;
                    self.ledger.open += 1;
                    file.set_len(below)?;
                    self.ledger.ftruncate += 1;
                    file.sync_all()?;
                    self.ledger.fsync += 1;
                    drop(file);
                    self.ledger.close += 1;
                    rows.push((
                        format!("boundary-below:{path}"),
                        format!("{current}\t{below}\t{above}"),
                    ));
                }
                stages::Stage2Op::BoundaryGrow => {
                    let (below, above) = stages::boundary_lengths(cycle);
                    let current = fs::metadata(&path)?.len();
                    if above > current {
                        let bytes = self.recipe(
                            &format!("boundary-grow-{cycle}"),
                            "v016-stage2",
                            cycle,
                            &format!("boundary-grow-c{cycle}"),
                            above - current,
                        )?;
                        let mut file = OpenOptions::new().write(true).open(&path)?;
                        self.ledger.open += 1;
                        file.seek(SeekFrom::End(0))?;
                        file.write_all(&bytes)?;
                        self.ledger.append += 1;
                        file.sync_all()?;
                        self.ledger.fsync += 1;
                        drop(file);
                        self.ledger.close += 1;
                    } else if above < current {
                        let mut file = OpenOptions::new().write(true).open(&path)?;
                        self.ledger.open += 1;
                        file.set_len(above)?;
                        self.ledger.ftruncate += 1;
                        file.sync_all()?;
                        self.ledger.fsync += 1;
                        drop(file);
                        self.ledger.close += 1;
                    }
                    let after = fs::metadata(&path)?.len();
                    rows.push((
                        format!("boundary-above:{path}"),
                        format!("{current}\t{after}\t{below}"),
                    ));
                }
                stages::Stage2Op::SdkInsert | stages::Stage2Op::SdkDelete => {
                    return Err("v0.1.6 SDK rows must not run in the POSIX helper".into());
                }
            }
        }
        if self.ledger.main_edit_target != stages::STAGE2_POSIX_TARGETS {
            return Err("v0.1.6 POSIX stage-2 target cardinality".into());
        }
        self.rows("stage2-target", rows);
        Ok(())
    }

    // ---------------------------------------------------------- stage three

    fn stage_directories(&mut self) -> Result<()> {
        let cycle = self.cycle;
        let (remove, create) = stages::scratch_names(&self.fixture, cycle);
        for path in &remove {
            fs::remove_dir(path).map_err(|error| format!("rmdir {path}: {error}"))?;
            self.ledger.rmdir += 1;
        }
        let mut created_modes = Vec::new();
        for path in &create {
            fs::create_dir(path).map_err(|error| format!("mkdir {path}: {error}"))?;
            self.ledger.mkdir += 1;
            // The live mode of a directory this workload creates is a runtime
            // property, not a workload argument: `create_dir` passes the
            // process default and the runtime decides the result. It is
            // measured here so the declared state uses the observed mode
            // instead of an assumed one.
            let mode = fs::metadata(path)
                .map_err(|error| format!("mkdir mode {path}: {error}"))?
                .permissions()
                .mode()
                & 0o7777;
            if mode & 0o7000 != 0 {
                return Err(format!("v0.1.6 created directory {path} carries {mode:o}").into());
            }
            created_modes.push((path.clone(), format!("{mode:o}")));
        }
        self.rows("mkdir-mode", created_modes);
        let mut rows = Vec::new();
        for (source, destination) in stages::move_rows(&self.fixture, cycle) {
            if source == destination {
                // The subtree is already at its registered root this cycle.
                self.ledger.moved_directory += 1;
                rows.push((format!("in-place:{source}"), destination));
                continue;
            }
            fs::rename(&source, &destination)
                .map_err(|error| format!("rename {source} -> {destination}: {error}"))?;
            self.ledger.rename += 1;
            self.ledger.moved_directory += 1;
            rows.push((format!("move:{source}"), destination));
        }
        if rows.len() != 2 {
            return Err("v0.1.6 populated-directory move cardinality".into());
        }
        self.rows("stage3-move", rows);
        self.record(
            "stage3",
            BTreeMap::from([
                ("cycle".to_owned(), cycle.to_string()),
                ("removed".to_owned(), remove.len().to_string()),
                ("created".to_owned(), create.len().to_string()),
                ("create_names".to_owned(), create.join(",")),
                ("remove_names".to_owned(), remove.join(",")),
            ]),
        );
        Ok(())
    }

    // ----------------------------------------------------------- stage four

    fn stage_attrs_links(&mut self) -> Result<()> {
        let cycle = self.cycle;
        let file_mode = v016::attribute_mode(cycle, true);
        let directory_mode = v016::attribute_mode(cycle, false);
        let (link_parent, symlink_parent) = self.link_parents()?;
        let targets = self.fixture.roles.link_targets.clone();
        let mut aliases = Vec::new();
        for (index, target) in targets.iter().enumerate() {
            let alias = self.hardlink_alias(&link_parent, index);
            fs::hard_link(target, &alias).map_err(|error| {
                format!(
                    "hardlink {target} -> {alias} failed: {error} (target_exists={}, parent_exists={})",
                    Path::new(target).exists(),
                    Path::new(&link_parent).is_dir()
                )
            })?;
            self.ledger.hardlink += 1;
            aliases.push((alias, target.clone()));
        }
        let symlink_rows = stages::symlink_rows(&self.fixture);
        let mut symlinks = Vec::new();
        for (index, (_, target, kind)) in symlink_rows.iter().enumerate() {
            let link = self.symlink_path(&symlink_parent, index);
            // The declared target is relative to the fixture root; the link's
            // own parent is one level deeper while the populated directory sits
            // under a destination parent.
            let resolved_target = match kind {
                SymlinkKind::Resolvable => {
                    if symlink_parent.is_empty() {
                        target.trim_start_matches("../").to_owned()
                    } else {
                        target.clone()
                    }
                }
                _ => target.clone(),
            };
            std::os::unix::fs::symlink(&resolved_target, &link)
                .map_err(|error| format!("symlink {resolved_target} -> {link}: {error}"))?;
            self.ledger.symlink += 1;
            symlinks.push((link, resolved_target));
        }
        // Four 64-byte alias writes on the distinct 4 KiB files.
        let probe_paths = stages::alias_probe_paths(&self.fixture);
        let mut rows = Vec::new();
        for (index, target) in probe_paths.iter().enumerate() {
            let offset = 64 * index as u64;
            let bytes = self.recipe(
                &format!("alias-probe-{cycle}-{index}"),
                "v016-stage4",
                index,
                &format!("alias-probe-c{cycle}"),
                stages::ALIAS_PROBE_BYTES,
            )?;
            let before = self
                .pwrite_probe(target, offset, &bytes)
                .map_err(|error| format!("alias probe {target}: {error}"))?;
            self.ledger.alias_probe_write += 1;
            rows.push((
                format!("alias-write:{target}"),
                format!("{offset}\t{}\t{before}", bytes.len()),
            ));
        }
        self.rows("stage4-alias-write", rows);
        let mut attributes = Vec::new();
        for path in v016::attribute_targets(&self.fixture)? {
            set_mode(&path, file_mode)
                .map_err(|error| format!("chmod {path}: {error}"))?;
            self.ledger.chmod += 1;
            // A fresh handle proves the live mode, not a cached inode record.
            let readback = File::open(&path)
                .map_err(|error| format!("chmod readback open {path}: {error}"))?
                .metadata()?
                .permissions()
                .mode()
                & 0o7777;
            if readback != file_mode {
                return Err(format!(
                    "file chmod {path} readback {readback:o} != requested {file_mode:o}"
                )
                .into());
            }
            attributes.push(path);
        }
        let mut directories = Vec::new();
        for path in stages::attribute_directories(&self.fixture, cycle) {
            set_mode(&path, directory_mode)
                .map_err(|error| format!("directory chmod {path}: {error}"))?;
            self.ledger.directory_chmod += 1;
            // A directory handle closed and reopened between cycles is the
            // difficulty this stage declares; the readback is taken from the
            // path, which resolves the live inode.
            let readback = fs::metadata(&path)
                .map_err(|error| format!("directory chmod readback {path}: {error}"))?
                .permissions()
                .mode()
                & 0o7777;
            if readback != directory_mode {
                return Err(format!(
                    "directory chmod {path} readback {readback:o} != requested {directory_mode:o}"
                )
                .into());
            }
            directories.push(format!("{path}:{readback:o}"));
        }
        self.rows(
            "stage4-directory-chmod",
            directories
                .iter()
                .map(|row| {
                    let (name, value) = row.split_once(':').unwrap_or((row.as_str(), ""));
                    (name.to_owned(), value.to_owned())
                })
                .collect(),
        );
        let mtime = v016::attribute_mtime(cycle, self.branch_tag);
        for path in &attributes {
            set_mtime(path, mtime, v016::ATTRIBUTE_MTIME_NS)
                .map_err(|error| format!("mtime {path}: {error}"))?;
            self.ledger.mtime += 1;
        }
        // Expected live readlink/open behaviour is part of the workload.
        let mut symlink_receipts = Vec::new();
        for (index, (link, target)) in symlinks.iter().enumerate() {
            let observed =
                fs::read_link(link).map_err(|error| format!("readlink {link}: {error}"))?;
            self.ledger.readlink += 1;
            if observed.to_string_lossy() != *target {
                return Err(format!("v0.1.6 symlink target mismatch at {link}").into());
            }
            let kind = symlink_rows[index].2;
            let outcome = match kind {
                SymlinkKind::Resolvable => {
                    let mut file = File::open(link)?;
                    self.ledger.open += 1;
                    let mut sink = Vec::new();
                    file.read_to_end(&mut sink)?;
                    self.ledger.read += 1;
                    drop(file);
                    self.ledger.close += 1;
                    format!("read:{}", sink.len())
                }
                SymlinkKind::Dangling => {
                    let error = File::open(link)
                        .err()
                        .ok_or("v0.1.6 dangling symlink unexpectedly resolved")?;
                    format!("expected-error:{:?}", error.kind())
                }
                SymlinkKind::SelfLoop => {
                    // A self-loop resolves to its own parent directory, so the
                    // expected live outcome is a directory handle, not ELOOP.
                    let metadata = fs::metadata(link)
                        .map_err(|error| format!("self-loop stat {link}: {error}"))?;
                    if !metadata.is_dir() {
                        return Err(format!(
                            "v0.1.6 self-loop symlink resolved to a non-directory: {link}"
                        )
                        .into());
                    }
                    let mut names = 0usize;
                    for entry in fs::read_dir(link)
                        .map_err(|error| format!("self-loop read_dir {link}: {error}"))?
                    {
                        entry?;
                        names += 1;
                    }
                    self.ledger.read += 1;
                    format!("expected-directory:{names}")
                }
            };
            symlink_receipts.push(format!("{link}->{target}\t{outcome}"));
        }
        self.rows(
            "stage4-symlink",
            symlink_receipts
                .iter()
                .map(|row| {
                    let (name, value) = row.split_once('\t').unwrap_or((row.as_str(), ""));
                    (name.to_owned(), value.to_owned())
                })
                .collect(),
        );
        self.record(
            "stage4",
            BTreeMap::from([
                ("cycle".to_owned(), cycle.to_string()),
                ("file_mode".to_owned(), format!("{file_mode:o}")),
                ("directory_mode".to_owned(), format!("{directory_mode:o}")),
                ("mtime_seconds".to_owned(), mtime.to_string()),
                (
                    "mtime_nanoseconds".to_owned(),
                    v016::ATTRIBUTE_MTIME_NS.to_string(),
                ),
                ("hardlinks".to_owned(), aliases.len().to_string()),
                ("symlinks".to_owned(), symlinks.len().to_string()),
                (
                    "hardlink_aliases".to_owned(),
                    aliases
                        .iter()
                        .map(|(alias, target)| format!("{alias}->{target}"))
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                (
                    "symlink_paths".to_owned(),
                    symlinks
                        .iter()
                        .map(|(link, target)| format!("{link}->{target}"))
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                ("attribute_files".to_owned(), attributes.join(",")),
                ("attribute_directories".to_owned(), directories.join(",")),
            ]),
        );
        Ok(())
    }

    // ----------------------------------------------------------- stage five

    fn stage_inode_subtree(&mut self) -> Result<()> {
        let cycle = self.cycle;
        let (link_parent, symlink_parent) = self.link_parents()?;
        // Before deleting the subtree, open one member and keep the descriptor.
        let probe_path = self
            .fixture
            .roles
            .deletion
            .first()
            .ok_or("v0.1.6 deletion subtree member")?
            .clone();
        let mut probe = OpenOptions::new().read(true).write(true).open(&probe_path)?;
        self.ledger.open += 1;
        let probe_before = probe.metadata()?;
        let root = self.fixture.roles.deletion_root.clone();
        for path in stages::subtree_paths(&self.fixture) {
            fs::remove_file(&path)?;
            self.ledger.unlink += 1;
        }
        fs::remove_dir(&root)?;
        self.ledger.rmdir += 1;
        let mut probe_range = [0u8; 64];
        probe.seek(SeekFrom::Start(0))?;
        probe.read_exact(&mut probe_range)?;
        self.ledger.read += 1;
        let write_bytes = self.recipe(
            &format!("unlinked-descriptor-{cycle}"),
            "v016-stage5",
            cycle,
            &format!("unlinked-descriptor-c{cycle}"),
            64,
        )?;
        probe.seek(SeekFrom::Start(0))?;
        probe.write_all(&write_bytes)?;
        self.ledger.pwrite += 1;
        self.ledger.unlinked_descriptor_write += 1;
        probe.sync_all()?;
        self.ledger.fsync += 1;
        let probe_after = probe.metadata()?;
        if probe_before.ino() != probe_after.ino() || probe_after.len() != probe_before.len() {
            return Err("v0.1.6 open-unlinked descriptor identity changed".into());
        }
        drop(probe);
        self.ledger.close += 1;
        self.record(
            "stage5-unlinked",
            BTreeMap::from([
                ("path".to_owned(), probe_path.clone()),
                ("ino".to_owned(), probe_before.ino().to_string()),
                ("len".to_owned(), probe_before.len().to_string()),
                ("range_sha256".to_owned(), sha(&probe_range)),
                ("write_sha256".to_owned(), sha(&write_bytes)),
            ]),
        );
        // Recreate the subtree with recurrent and generation-specific bytes.
        fs::create_dir(&root)?;
        self.ledger.mkdir += 1;
        // The recreated root's live mode is measured for the same reason as the
        // stage-3 scratch directories: the published mode must be compared with
        // what the runtime actually produced, not with an assumed default.
        let recreated_mode = fs::metadata(&root)
            .map_err(|error| format!("mkdir mode {root}: {error}"))?
            .permissions()
            .mode()
            & 0o7777;
        if recreated_mode & 0o7000 != 0 {
            return Err(format!("v0.1.6 recreated directory {root} carries {recreated_mode:o}").into());
        }
        self.rows(
            "mkdir-mode",
            vec![(root.clone(), format!("{recreated_mode:o}"))],
        );
        for index in 0..v016::DELETION_SUBTREE_FILES {
            let path = self.fixture.roles.deletion[index].clone();
            let content =
                stages::subtree_content(&self.fixture, cycle, index, &self.branch_salt)?;
            let bytes = self.render(&format!("subtree-c{cycle}-{index}"), &content)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&path)?;
            self.ledger.open += 1;
            self.ledger.create += 1;
            file.write_all(&bytes)?;
            self.ledger.pwrite += 1;
            file.sync_all()?;
            self.ledger.fsync += 1;
            drop(file);
            self.ledger.close += 1;
        }
        // Four atomic 4 KiB saves over four hardlinked destinations.
        let destinations = stages::save_destinations(&self.fixture);
        if destinations.len() != 4 {
            return Err("v0.1.6 atomic save destination cardinality".into());
        }
        let mut saves = Vec::new();
        for (index, destination) in destinations.iter().enumerate() {
            let alias = self.hardlink_alias(&link_parent, index + 4);
            let mut held = OpenOptions::new()
                .read(true)
                .write(true)
                .open(destination)?;
            self.ledger.open += 1;
            let old = held.metadata()?;
            let bytes = self.recipe(
                &format!("atomic-save-{cycle}-{index}"),
                "v016-stage5",
                index,
                &format!("atomic-save-c{cycle}-{index}"),
                v016::TINY,
            )?;
            // One temporary file at a time, carrying the destination's declared
            // mode and explicit mtime so the replacement does not silently
            // reset the attribute state the previous stage declared, then
            // rename over the destination.
            let temporary = format!("{destination}.tmp{index}");
            let destination_meta = fs::metadata(destination)
                .map_err(|error| format!("save destination {destination}: {error}"))?;
            let replacement_mode = destination_meta.permissions().mode() & 0o7777;
            let replacement_seconds = destination_meta.mtime();
            let replacement_nanos = destination_meta.mtime_nsec().max(0) as u32;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            self.ledger.open += 1;
            self.ledger.temporary_create += 1;
            set_mode(&temporary, replacement_mode)?;
            set_mtime(&temporary, replacement_seconds, replacement_nanos)?;
            file.write_all(&bytes)?;
            self.ledger.pwrite += 1;
            file.sync_all()?;
            self.ledger.fsync += 1;
            drop(file);
            self.ledger.close += 1;
            fs::rename(&temporary, destination)?;
            self.ledger.rename += 1;
            self.ledger.rename_overwrite += 1;
            let after = held.metadata()?;
            if after.ino() != old.ino() {
                return Err("v0.1.6 atomic save moved the held descriptor".into());
            }
            let alias_meta = fs::metadata(&alias)?;
            if alias_meta.ino() != old.ino() {
                return Err("v0.1.6 atomic save did not preserve the old alias inode".into());
            }
            let replacement = fs::metadata(destination)?;
            if replacement.ino() == old.ino() {
                return Err("v0.1.6 atomic save reused the replaced inode".into());
            }
            if replacement.len() != old.len() {
                return Err("v0.1.6 atomic save length changed".into());
            }
            let mut old_bytes = Vec::new();
            held.seek(SeekFrom::Start(0))?;
            held.read_to_end(&mut old_bytes)?;
            self.ledger.read += 1;
            if old_bytes.len() != old.len() as usize {
                return Err("v0.1.6 held descriptor short read".into());
            }
            drop(held);
            self.ledger.close += 1;
            saves.push((
                format!("save:{destination}"),
                format!(
                    "old_ino={}\tnew_ino={}\tnew_sha256={}\told_len={}",
                    old.ino(),
                    replacement.ino(),
                    sha(&bytes),
                    old.len()
                ),
            ));
        }
        self.rows("stage5-save", saves);
        // Remove the persisted links after their retained-state proof.
        for index in 0..v016::HARDLINK_TARGETS {
            let alias = self.hardlink_alias(&link_parent, index);
            fs::remove_file(&alias).map_err(|error| format!("unlink {alias}: {error}"))?;
            self.ledger.unlink += 1;
            self.ledger.hardlink_unlink += 1;
        }
        for index in 0..v016::SYMLINK_COUNT {
            let link = self.symlink_path(&symlink_parent, index);
            fs::remove_file(&link).map_err(|error| format!("unlink {link}: {error}"))?;
            self.ledger.unlink += 1;
            self.ledger.symlink_unlink += 1;
        }
        self.record(
            "stage5",
            BTreeMap::from([
                ("cycle".to_owned(), cycle.to_string()),
                (
                    "subtree_files".to_owned(),
                    v016::DELETION_SUBTREE_FILES.to_string(),
                ),
                ("recurrent".to_owned(), "16".to_owned()),
                ("generation_specific".to_owned(), "16".to_owned()),
                ("saves".to_owned(), saves_len(&self.evidence).to_string()),
                ("link_parent".to_owned(), link_parent),
                ("symlink_parent".to_owned(), symlink_parent),
            ]),
        );
        Ok(())
    }

    /// Add a bounded index report so the host can check declared path sets
    /// without re-deriving the fixture layout.
    fn run(&mut self, stage: Stage) -> Result<()> {
        let outcome = match stage {
            Stage::Refresh => self.stage_refresh(),
            Stage::Edits => self.stage_edits(),
            Stage::Directories => self.stage_directories(),
            Stage::AttrsLinks => self.stage_attrs_links(),
            Stage::InodeSubtree => self.stage_inode_subtree(),
        };
        match outcome {
            Ok(()) => Ok(()),
            Err(error) => Err(format!("stage {} cycle {}: {error}", stage as usize, self.cycle).into()),
        }
    }
}

fn saves_len(evidence: &[Evidence]) -> usize {
    evidence
        .iter()
        .filter(|row| row.key == "stage5-save")
        .count()
}

/// `v016-m1-witness PATH HEXBYTES` — the extra mutation the concurrent case
/// discards. The helper exits before the session is discarded, so the mutation
/// is exercised through the live FUSE mount and never committed.
pub(crate) fn witness_command(args: &[String]) -> Result<()> {
    let [path, hex] = args else {
        return Err("usage: v016-m1-witness PATH HEXBYTES".into());
    };
    if hex.len() % 2 != 0 {
        return Err("v0.1.6 witness bytes must be hex pairs".into());
    }
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16))
        .collect::<std::result::Result<Vec<u8>, _>>()?;
    let before = fs::metadata(path)?;
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    file.seek(SeekFrom::Start(0))?;
    let mut previous = vec![0u8; bytes.len()];
    file.read_exact(&mut previous)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    let after = fs::metadata(path)?;
    println!("witness_path={path}");
    println!("witness_len={}", after.len());
    println!("witness_ino={}", after.ino());
    println!("witness_previous_sha256={}", sha(&previous));
    println!("witness_new_sha256={}", sha(&bytes));
    println!("unauthorized_len_change={}", after.len() != before.len());
    Ok(())
}

/// `v016-m1-setmode PATH MODE` — one bounded chmod probe used to separate the
/// declared workload from a product behaviour question. It reports the live
/// readback so a difference between the live mode and the published mode is
/// observable rather than assumed.
pub(crate) fn setmode_command(args: &[String]) -> Result<()> {
    let [path, mode] = args else {
        return Err("usage: v016-m1-setmode PATH MODE".into());
    };
    let mode = u32::from_str_radix(mode, 8)?;
    set_mode(path, mode)?;
    let metadata = fs::metadata(path)?;
    println!("path={path}");
    println!("requested_mode={mode:o}");
    println!("live_mode={:o}", metadata.permissions().mode() & 0o7777);
    println!("live_kind={}", if metadata.is_dir() { "directory" } else { "file" });
    Ok(())
}

fn tier_named(name: &str) -> Result<&'static v016::LoadTier> {
    match name {
        "L100" => Ok(&v016::L100),
        "L500" => Ok(&v016::L500),
        other => Err(format!("v0.1.6 unknown load tier {other}").into()),
    }
}

/// `v016-m1-stage TIER SEED CYCLE BRANCH_TAG BRANCH_SALT STAGE`
pub(crate) fn run_command(args: &[String]) -> Result<()> {
    let [tier, seed, cycle, branch_tag, branch_salt, stage] = args else {
        return Err(format!(
            "usage: v016-m1-stage TIER SEED CYCLE BRANCH_TAG BRANCH_SALT STAGE (received {} arguments: {args:?})",
            args.len()
        )
        .into());
    };
    let tier = tier_named(tier)?;
    let seed: u8 = seed.parse()?;
    let cycle: usize = cycle.parse()?;
    if cycle == 0 {
        return Err("v0.1.6 cycles are 1-based".into());
    }
    let branch_tag: u64 = branch_tag.parse()?;
    let stage = Stage::from_ordinal(stage.parse()?)?;
    let fixture = v016::load_fixture(tier, seed)?;
    let mut helper = Helper::new(fixture, cycle, branch_tag, branch_salt);
    helper.run(stage)?;
    println!("m1_stage={}", stage as usize);
    println!("m1_cycle={cycle}");
    println!("m1_branch_tag={branch_tag}");
    println!("m1_posix_helper_executions=1");
    println!("m1_oracle_reads=0");
    println!("m1_content_generation_ns={}", helper.render_ns);
    for (name, value) in helper.ledger.rows() {
        println!("m1_syscall_{name}={value}");
    }
    for (name, expected) in stages::stage_counters(stage) {
        if stages::is_host_counter(stage, name) {
            continue;
        }
        println!("m1_declared_{name}={expected}");
    }
    let observed = BTreeMap::from([
        (
            "regular_unlinks".to_owned(),
            helper.ledger.unlink.to_string(),
        ),
        (
            "ordinary_regular_creates".to_owned(),
            helper.ledger.create.to_string(),
        ),
        (
            "temporary_regular_creates".to_owned(),
            helper.ledger.temporary_create.to_string(),
        ),
        (
            "rename_overwrites".to_owned(),
            helper.ledger.rename_overwrite.to_string(),
        ),
        ("hardlink_creates".to_owned(), helper.ledger.hardlink.to_string()),
        (
            "hardlink_unlinks".to_owned(),
            helper.ledger.hardlink_unlink.to_string(),
        ),
        ("symlink_creates".to_owned(), helper.ledger.symlink.to_string()),
        (
            "symlink_unlinks".to_owned(),
            helper.ledger.symlink_unlink.to_string(),
        ),
        (
            "alias_probe_writes".to_owned(),
            helper.ledger.alias_probe_write.to_string(),
        ),
        (
            "unlinked_descriptor_writes".to_owned(),
            helper.ledger.unlinked_descriptor_write.to_string(),
        ),
        (
            "populated_directory_moves".to_owned(),
            helper.ledger.moved_directory.to_string(),
        ),
        ("directory_creates".to_owned(), helper.ledger.mkdir.to_string()),
        ("directory_removes".to_owned(), helper.ledger.rmdir.to_string()),
        ("file_chmod".to_owned(), helper.ledger.chmod.to_string()),
        (
            "directory_chmod".to_owned(),
            helper.ledger.directory_chmod.to_string(),
        ),
        (
            "explicit_mtime_calls".to_owned(),
            helper.ledger.mtime.to_string(),
        ),
        (
            "main_edit_targets".to_owned(),
            helper.ledger.main_edit_target.to_string(),
        ),
        (
            "expected_eexist_probes".to_owned(),
            helper.ledger.create_excl_fail.to_string(),
        ),
    ]);
    for (name, value) in &observed {
        println!("m1_observed_{name}={value}");
    }
    // The live mode of every directory this stage created, as measured on the
    // live filesystem. The independent oracle uses these observations instead
    // of assuming a runtime default.
    let created: Vec<String> = helper
        .evidence
        .iter()
        .filter(|row| row.key == "mkdir-mode")
        .map(|row| {
            format!(
                "{}:{}",
                row.fields.get("name").map(String::as_str).unwrap_or(""),
                row.fields.get("value").map(String::as_str).unwrap_or("")
            )
        })
        .collect();
    println!("m1_created_directory_modes={}", created.join(","));
    let mut evidence = String::from("[");
    for (index, row) in helper.evidence.iter().enumerate() {
        if index > 0 {
            evidence.push(',');
        }
        evidence.push_str(&format!(
            "{{\"key\":\"{}\",\"fields\":{{",
            escape(&row.key)
        ));
        for (field_index, (name, value)) in row.fields.iter().enumerate() {
            if field_index > 0 {
                evidence.push(',');
            }
            evidence.push_str(&format!("\"{}\":\"{}\"", escape(name), escape(value)));
        }
        evidence.push_str("}}");
    }
    evidence.push(']');
    println!("m1_evidence={evidence}");
    Ok(())
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// `v016-m1-check TIER SEED` — product-free plan self-check.
pub(crate) fn check_command(args: &[String]) -> Result<()> {
    let [tier, seed] = args else {
        return Err("usage: v016-m1-check TIER SEED".into());
    };
    let tier = tier_named(tier)?;
    let seed: u8 = seed.parse()?;
    let fixture = v016::load_fixture(tier, seed)?;
    let (paths, bytes) = stages::live_envelope(&fixture)?;
    if paths > tier.max_paths() || bytes > tier.max_bytes() {
        return Err("v0.1.6 M1 live envelope exceeds the reserved capacity".into());
    }
    let declared = stages::declared_cycle_counters();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    for ordinal in 1..=stages::STAGES {
        for (name, value) in stages::stage_counters(Stage::from_ordinal(ordinal)?) {
            *totals.entry(name).or_default() += value;
        }
    }
    for (name, cycle_value) in &declared {
        if let Some(total) = totals.get(name) {
            if total != cycle_value {
                return Err(format!(
                    "v0.1.6 per-stage sum for {name}: {total} != {cycle_value}"
                )
                .into());
            }
        }
    }
    let plan = stages::stage2_plan(&fixture, 1)?;
    if plan.len() != stages::STAGE2_MAIN_TARGETS {
        return Err("v0.1.6 stage-2 plan cardinality".into());
    }
    if plan.iter().filter(|(_, op)| stages::is_sdk_row(op)).count() != stages::STAGE2_SDK_CALLS {
        return Err("v0.1.6 stage-2 SDK call cardinality".into());
    }
    let targets: std::collections::BTreeSet<&str> =
        plan.iter().map(|(path, _)| path.as_str()).collect();
    if targets.len() != stages::STAGE2_MAIN_TARGETS {
        return Err("v0.1.6 stage-2 targets must be distinct".into());
    }
    if stages::save_destinations(&fixture).len() != 4 {
        return Err("v0.1.6 atomic save destinations".into());
    }
    if stages::alias_probe_paths(&fixture).len() != stages::ALIAS_PROBE_WRITES {
        return Err("v0.1.6 alias probe cardinality".into());
    }
    // Every cycle's declared boundary pair shrinks before growth.
    for cycle in 1..=20 {
        let (below, above) = stages::boundary_lengths(cycle);
        if cycle % 2 == 1 && (below != v016::BOUNDARY_EXACT || above != v016::BOUNDARY_EXACT) {
            return Err("v0.1.6 odd-cycle boundary lengths".into());
        }
        if cycle % 2 == 0 && (below != v016::BOUNDARY_BELOW || above != v016::BOUNDARY_ABOVE) {
            return Err("v0.1.6 even-cycle boundary lengths".into());
        }
        let (insert, delete) = stages::sdk_pair(cycle);
        if insert == delete {
            return Err("v0.1.6 SDK pair must differ".into());
        }
    }
    let _ = h("v016-m1-check")?;
    println!("v016_m1_check=pass");
    println!("live_paths={paths}");
    println!("live_bytes={bytes}");
    println!("max_paths={}", tier.max_paths());
    println!("max_bytes={}", tier.max_bytes());
    println!("stage2_targets={}", targets.len());
    Ok(())
}
