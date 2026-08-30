use crate::command::WorkspaceAnchor;
use crate::database::Databases;
use crate::fixture::{commit_id, BranchRecord, CommitRecord, MockState};
use crate::model::{
    BranchOrigin, BranchRelation, CliError, CliResult, CommandResult, DiffChange, DiffEntryView,
    FilePreview, WorkspaceCommitReceipt, WorkspaceFileKind, WorkspaceFileView, WorkspaceRunView,
    WorkspaceState, WorkspaceStorageView, WorkspaceTimingView, WorkspaceView,
};
use crate::{CommitId, ExecutionId, ObjectId, WorkspaceId};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::ops::{Deref, DerefMut};
use std::path::{Component, Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

const MAX_FILES: usize = 1_024;
const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TREE_BYTES: u64 = 64 * 1024 * 1024;
const PREVIEW_BYTES: usize = 4 * 1024;
const OUTPUT_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TreeEntry {
    Directory,
    File(Vec<u8>),
}

pub(crate) type Tree = BTreeMap<String, TreeEntry>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalObject {
    pub id: ObjectId,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub(crate) struct WorkspaceRecord {
    pub view: WorkspaceView,
    pub anchor: Tree,
    pub current: Tree,
    artifacts: Arc<WorkspaceArtifacts>,
}

struct WorkspaceArtifacts {
    root: PathBuf,
    mount: PathBuf,
}

impl Drop for WorkspaceArtifacts {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

impl Deref for WorkspaceRecord {
    type Target = WorkspaceView;

    fn deref(&self) -> &Self::Target {
        &self.view
    }
}

impl DerefMut for WorkspaceRecord {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.view
    }
}

impl WorkspaceRecord {
    pub(crate) fn create(
        mut view: WorkspaceView,
        anchor: Tree,
        requested: &Path,
    ) -> CliResult<Self> {
        if !requested.is_absolute() {
            return Err(CliError::Parse("Workspace path must be absolute".into()));
        }
        if requested.exists() {
            return Err(CliError::Integrity(format!(
                "Workspace path already exists: {}",
                requested.display()
            )));
        }
        let started = Instant::now();
        fs::create_dir_all(requested).map_err(io_error)?;
        if let Err(error) = write_tree(requested, &anchor) {
            let _ = fs::remove_dir_all(requested);
            return Err(error);
        }
        let root = requested.to_path_buf();
        let mount = requested.to_path_buf();
        view.mount = mount.display().to_string();
        view.timing.create_micros = elapsed_micros(started);
        let mut record = Self {
            view,
            anchor: anchor.clone(),
            current: anchor,
            artifacts: Arc::new(WorkspaceArtifacts { root, mount }),
        };
        record.refresh(false)?;
        Ok(record)
    }

    pub(crate) fn fixture(mut view: WorkspaceView, anchor: Tree) -> CliResult<Self> {
        let path = unique_path("fixture");
        view.mount = path.display().to_string();
        Self::create(view, anchor, &path)
    }

    pub(crate) fn fixture_dirty(
        view: WorkspaceView,
        anchor: Tree,
        current: Tree,
    ) -> CliResult<Self> {
        let mut record = Self::fixture(view, anchor)?;
        write_tree(&record.artifacts.mount, &current)?;
        record.refresh(true)?;
        Ok(record)
    }

    pub(crate) fn remove_artifacts(&self) -> CliResult<()> {
        if self.artifacts.mount != self.artifacts.root
            || !self.artifacts.root.is_absolute()
            || self.artifacts.root.parent().is_none()
        {
            return Err(CliError::Integrity("Workspace cleanup target".into()));
        }
        if self.artifacts.root.exists() {
            fs::remove_dir_all(&self.artifacts.root).map_err(io_error)?;
        }
        Ok(())
    }

    fn refresh(&mut self, advance_generation: bool) -> CliResult<u64> {
        let started = Instant::now();
        let next = read_tree(&self.artifacts.mount)?;
        let old_root = canonical_tree(&self.current).0;
        let new_root = canonical_tree(&next).0;
        if advance_generation && old_root != new_root {
            self.view.generation = self.view.generation.saturating_add(1);
        }
        self.current = next;
        self.view.files = file_views(&self.artifacts.mount, &self.current)?;
        self.view.changes = diff_trees(&self.anchor, &self.current);
        self.view.changed_paths = self.view.changes.len().min(u16::MAX as usize) as u16;
        self.view.state = if self.view.changes.is_empty() {
            WorkspaceState::Clean
        } else {
            WorkspaceState::Dirty
        };
        self.view.storage.logical_bytes = logical_bytes(&self.current);
        self.view.storage.materialized_allocated_bytes = self
            .view
            .files
            .iter()
            .map(|file| file.allocated_bytes)
            .sum();
        self.view.storage.cow_delta_bytes = self
            .view
            .changes
            .iter()
            .filter(|change| matches!(change.change, DiffChange::Add | DiffChange::Modify))
            .map(|change| tree_entry_bytes(self.current.get(&change.path)))
            .sum();
        self.view.storage.base_reused_bytes = self
            .view
            .storage
            .logical_bytes
            .saturating_sub(self.view.storage.cow_delta_bytes);
        Ok(elapsed_micros(started))
    }
}

pub(crate) fn seed_tree(label: &str) -> Tree {
    let mut tree = Tree::new();
    tree.insert("src".into(), TreeEntry::Directory);
    tree.insert("test".into(), TreeEntry::Directory);
    tree.insert(
        "package.json".into(),
        TreeEntry::File(
            br#"{"name":"layerfs-demo","private":true,"scripts":{"test":"node test/index.test.js"}}
"#
            .to_vec(),
        ),
    );
    tree.insert(
        "src/index.js".into(),
        TreeEntry::File(format!("export const revision = {label:?};\n").into_bytes()),
    );
    tree.insert(
        "test/index.test.js".into(),
        TreeEntry::File(b"import assert from 'node:assert';\nassert.ok(true);\n".to_vec()),
    );
    tree
}

pub(crate) fn canonical_tree(tree: &Tree) -> (ObjectId, Vec<CanonicalObject>) {
    let mut objects = Vec::new();
    let mut seen = HashSet::new();
    let mut manifest = Vec::new();
    for (path, entry) in tree {
        let (tag, payload) = match entry {
            TreeEntry::Directory => (b'd', b"directory".to_vec()),
            TreeEntry::File(bytes) => (b'f', bytes.clone()),
        };
        let mut identity = vec![tag];
        identity.extend_from_slice(&payload);
        let id = ObjectId::new(format!("O-{:016x}", fnv1a(&identity)));
        manifest.extend_from_slice(path.as_bytes());
        manifest.push(0);
        manifest.push(tag);
        manifest.extend_from_slice(id.as_str().as_bytes());
        manifest.push(b'\n');
        if seen.insert(id.clone()) {
            objects.push(CanonicalObject { id, bytes: payload });
        }
    }
    let root = ObjectId::new(format!("R-{:016x}", fnv1a(&manifest)));
    objects.push(CanonicalObject {
        id: root.clone(),
        bytes: manifest,
    });
    (root, objects)
}

pub(crate) fn create_workspace(
    state: &mut MockState,
    anchor: WorkspaceAnchor,
    requested: String,
    container: Option<String>,
    projection: String,
) -> CliResult<CommandResult> {
    let (branch, anchor_commit, anchor_layer, tree, anchor_root) = match anchor {
        WorkspaceAnchor::Commit { branch, commit } => {
            let record = state
                .find_branch(&branch)
                .cloned()
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let selected = state.resolve_commit(&record, &commit)?;
            if branch_head(&record).as_ref() != Some(&selected.id) {
                return Err(CliError::HeadMoved(
                    "selected Commit is not target head".into(),
                ));
            }
            (
                record,
                Some(selected.id),
                None,
                selected.tree,
                selected.root,
            )
        }
        WorkspaceAnchor::InitialLayer { branch, layer } => {
            let record = state
                .find_branch(&branch)
                .cloned()
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            if branch_head(&record).is_some() {
                return Err(CliError::HeadMoved(
                    "Branch already has a Commit head".into(),
                ));
            }
            let source = state
                .find_layer(&layer)
                .cloned()
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            if source.project_id != record.project_id {
                return Err(CliError::Integrity("Workspace LayerStack mismatch".into()));
            }
            if record.origin != BranchOrigin::Layer(source.id.clone()) {
                return Err(CliError::Integrity(
                    "initial Workspace must use the Branch origin Layer".into(),
                ));
            }
            let root = source.root.clone();
            (record, None, Some(source.id), source.tree, root)
        }
    };
    if !matches!(
        branch.relation,
        BranchRelation::LocalOnly
            | BranchRelation::LocalCurrent
            | BranchRelation::LocalPushAhead { .. }
    ) {
        return Err(CliError::ReadOnly(branch.name.to_string()));
    }
    if state
        .workspaces
        .iter()
        .any(|workspace| workspace.branch_id == branch.id)
    {
        return Err(CliError::WorkspaceBusy(branch.name.to_string()));
    }
    let project = state
        .projects
        .iter()
        .find(|project| project.id == branch.project_id)
        .expect("fixture project");
    let id = WorkspaceId::new(format!("W{}", state.next_workspace));
    state.next_workspace += 1;
    let view = WorkspaceView {
        id: id.clone(),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        branch_id: branch.id.clone(),
        branch_name: branch.name.clone(),
        branch_relation: branch.relation.clone(),
        anchor_commit: anchor_commit.clone(),
        anchor_layer,
        anchor_root,
        expected_branch_head: branch_head(&branch),
        published_commit: None,
        published_root: None,
        state: WorkspaceState::Clean,
        generation: 0,
        projection,
        placement: container.map_or_else(|| "host".into(), |id| format!("container {id}")),
        mount: String::new(),
        changed_paths: 0,
        output_bytes: 0,
        execution: None,
        output: vec!["Workspace ready".into()],
        conflicts: Vec::new(),
        files: Vec::new(),
        changes: Vec::new(),
        runs: Vec::new(),
        storage: WorkspaceStorageView::default(),
        timing: WorkspaceTimingView::default(),
        commit_receipt: None,
    };
    let record = WorkspaceRecord::create(view, tree, Path::new(&requested))?;
    state.last_operation_elapsed_micros = Some(record.timing.create_micros);
    state.workspaces.push(record);
    Ok(CommandResult::Workspace(format!("Created {id}")))
}

pub(crate) fn workspace_action(
    state: &mut MockState,
    databases: &Databases,
    action: &str,
    target: &str,
    arguments: &[String],
) -> CliResult<CommandResult> {
    let index = state
        .workspaces
        .iter()
        .position(|workspace| {
            if matches!(action, "output" | "stop") {
                workspace
                    .execution
                    .as_ref()
                    .is_some_and(|execution| execution.as_str() == target)
            } else {
                workspace.id.as_str() == target
            }
        })
        .ok_or_else(|| CliError::NotFound(target.into()))?;
    match action {
        "exec" => run_bash(state, index, arguments),
        "commit" => commit_workspace(state, databases, index),
        "end" => end_workspace(state, index, arguments),
        "output" => Ok(CommandResult::Workspace(format!("output {target}"))),
        "stop" => Err(CliError::WorkspaceBusy(
            "Bash runs synchronously in this demo".into(),
        )),
        "conflicts" => Ok(CommandResult::Workspace(format!("conflicts {target}"))),
        "resolve" => resolve_conflict(state, index, arguments),
        _ => Err(CliError::Parse(format!(
            "unsupported Workspace action {action}"
        ))),
    }
}

fn run_bash(state: &mut MockState, index: usize, arguments: &[String]) -> CliResult<CommandResult> {
    let script = match arguments {
        [separator, executable, login, script]
            if separator == "--" && executable == "/bin/bash" && login == "-lc" =>
        {
            script
        }
        _ => {
            return Err(CliError::Parse(
                "Run Bash as: workspace exec <id> -- /bin/bash -lc '<script>'".into(),
            ))
        }
    };
    match state.workspaces[index].state {
        WorkspaceState::Running | WorkspaceState::Busy => {
            return Err(CliError::WorkspaceBusy(
                state.workspaces[index].id.to_string(),
            ))
        }
        WorkspaceState::HeadMoved => {
            return Err(CliError::HeadMoved(state.workspaces[index].id.to_string()))
        }
        WorkspaceState::ReadOnly => {
            return Err(CliError::ReadOnly(state.workspaces[index].id.to_string()))
        }
        WorkspaceState::Clean | WorkspaceState::Dirty => {}
    }
    let execution = ExecutionId::new(format!(
        "E-{}-{}",
        state.workspaces[index].id,
        state.workspaces[index].runs.len() + 1
    ));
    let started = Instant::now();
    state.workspaces[index].state = WorkspaceState::Running;
    state.workspaces[index].execution = Some(execution.clone());
    let result = run_process(&state.workspaces[index].artifacts.mount, script);
    let elapsed = elapsed_micros(started);
    let (exit_code, stdout, stderr) = match result {
        Ok(value) => value,
        Err(error) => {
            state.workspaces[index].state = WorkspaceState::Dirty;
            return Err(error);
        }
    };
    let capture = state.workspaces[index].refresh(true)?;
    let output_bytes = (stdout.len() + stderr.len()) as u64;
    let record = &mut state.workspaces[index];
    record.output_bytes = output_bytes;
    record.output = stdout
        .lines()
        .chain(stderr.lines())
        .take(64)
        .map(str::to_owned)
        .collect();
    record.runs.push(WorkspaceRunView {
        execution_id: execution,
        script: script.clone(),
        exit_code,
        stdout,
        stderr,
        output_bytes,
        elapsed_micros: elapsed,
    });
    if record.runs.len() > 32 {
        record.runs.remove(0);
    }
    record.timing.bash_last_micros = elapsed;
    record.timing.bash_total_micros = record.timing.bash_total_micros.saturating_add(elapsed);
    record.timing.capture_micros = capture;
    state.last_operation_elapsed_micros = Some(elapsed.saturating_add(capture));
    Ok(CommandResult::Workspace(format!(
        "Bash exited {exit_code}; generation {}",
        record.generation
    )))
}

fn commit_workspace(
    state: &mut MockState,
    databases: &Databases,
    index: usize,
) -> CliResult<CommandResult> {
    let total_started = Instant::now();
    let workspace = &state.workspaces[index];
    if matches!(
        workspace.state,
        WorkspaceState::Running | WorkspaceState::Busy
    ) {
        return Err(CliError::WorkspaceBusy(workspace.id.to_string()));
    }
    if workspace.state == WorkspaceState::HeadMoved {
        return Err(CliError::HeadMoved(workspace.id.to_string()));
    }
    if workspace.state == WorkspaceState::ReadOnly {
        return Err(CliError::ReadOnly(workspace.id.to_string()));
    }
    if !workspace.conflicts.is_empty() {
        return Err(CliError::Integrity(
            "resolve every reconciliation conflict before Commit".into(),
        ));
    }
    let capture_micros = state.workspaces[index].refresh(true)?;
    if state.workspaces[index].changes.is_empty() {
        state.workspaces[index].state = WorkspaceState::ReadOnly;
        state.last_operation_elapsed_micros = Some(elapsed_micros(total_started));
        return Ok(CommandResult::Workspace("NoChanges".into()));
    }
    let branch_id = state.workspaces[index].branch_id.clone();
    let expected = state.workspaces[index].expected_branch_head.clone();
    let current = state
        .branch(&branch_id)
        .ok_or_else(|| CliError::Integrity("Workspace Branch".into()))?;
    if branch_head(current) != expected {
        state.workspaces[index].state = WorkspaceState::HeadMoved;
        return Err(CliError::HeadMoved(branch_id.to_string()));
    }

    let admission_started = Instant::now();
    let (root, all_objects) = canonical_tree(&state.workspaces[index].current);
    let candidate = candidate_objects(
        &state.workspaces[index].changes,
        &state.workspaces[index].current,
        &root,
        &all_objects,
    );
    let present = databases.branch_objects(&candidate)?;
    let candidate_bytes = object_bytes(&candidate);
    let (inserted, reused): (Vec<_>, Vec<_>) = candidate
        .iter()
        .cloned()
        .partition(|object| !present.contains(&object.id));
    let admission_micros = elapsed_micros(admission_started);

    let publish_started = Instant::now();
    let committed_tree = state.workspaces[index].current.clone();
    let anchor_layer = state.workspaces[index].anchor_layer.clone();
    let branch = state
        .branch_mut(&branch_id)
        .ok_or_else(|| CliError::Integrity("Workspace Branch".into()))?;
    let number = branch.work_head.unwrap_or(0) + 1;
    let id = commit_id(branch.id.as_str(), number);
    branch.commits.push(CommitRecord {
        id: id.clone(),
        number,
        authority: false,
        work: true,
        inherited: false,
        owned: true,
        accepted_layer: None,
        root: root.clone(),
        tree: committed_tree,
        objects: candidate.clone(),
    });
    branch.work_head = Some(number);
    branch.relation = match branch.authority_head {
        None => BranchRelation::LocalOnly,
        Some(head) if head == number => BranchRelation::LocalCurrent,
        Some(head) => BranchRelation::LocalPushAhead {
            commits: number.saturating_sub(head),
        },
    };
    if let Some(base) = anchor_layer {
        branch.effective_base = Some(base);
    }
    let relation = branch.relation.clone();
    let publish_micros = elapsed_micros(publish_started);
    let total_micros = elapsed_micros(total_started);
    let changed_paths = state.workspaces[index].changed_paths;
    let inserted_bytes = object_bytes(&inserted);
    let reused_bytes = object_bytes(&reused);
    let receipt = WorkspaceCommitReceipt {
        commit_id: id.clone(),
        root: root.clone(),
        generation: state.workspaces[index].generation,
        changed_paths,
        candidate_objects: candidate.len() as u64,
        candidate_bytes,
        inserted_objects: inserted.len() as u64,
        inserted_bytes,
        reused_objects: reused.len() as u64,
        reused_bytes,
        sqlite_growth_bytes: 0,
        capture_micros,
        admission_micros,
        publish_micros,
        total_micros,
    };
    let record = &mut state.workspaces[index];
    record.state = WorkspaceState::ReadOnly;
    record.branch_relation = relation;
    record.published_commit = Some(id.clone());
    record.published_root = Some(root);
    record.storage.candidate_objects = receipt.candidate_objects;
    record.storage.candidate_bytes = receipt.candidate_bytes;
    record.storage.inserted_objects = receipt.inserted_objects;
    record.storage.inserted_bytes = receipt.inserted_bytes;
    record.storage.reused_objects = receipt.reused_objects;
    record.storage.reused_bytes = receipt.reused_bytes;
    record.timing.capture_micros = capture_micros;
    record.timing.admission_micros = admission_micros;
    record.timing.publish_micros = publish_micros;
    record.timing.commit_total_micros = total_micros;
    record.commit_receipt = Some(receipt);
    state.last_operation_elapsed_micros = Some(total_micros);
    Ok(CommandResult::Workspace(format!("Created {id}")))
}

fn end_workspace(
    state: &mut MockState,
    index: usize,
    arguments: &[String],
) -> CliResult<CommandResult> {
    if matches!(
        state.workspaces[index].state,
        WorkspaceState::Dirty | WorkspaceState::HeadMoved
    ) && !arguments.iter().any(|argument| argument == "--discard")
    {
        return Err(CliError::WorkspaceDirty(
            state.workspaces[index].id.to_string(),
        ));
    }
    let started = Instant::now();
    let id = state.workspaces[index].id.clone();
    state.workspaces[index].remove_artifacts()?;
    state.workspaces.remove(index);
    state.last_operation_elapsed_micros = Some(elapsed_micros(started));
    Ok(CommandResult::Workspace(format!("Ended {id}")))
}

fn resolve_conflict(
    state: &mut MockState,
    index: usize,
    arguments: &[String],
) -> CliResult<CommandResult> {
    let conflict_id = arguments
        .first()
        .ok_or_else(|| CliError::Parse("conflict ID required".into()))?;
    let conflict_index = state.workspaces[index]
        .conflicts
        .iter()
        .position(|conflict| conflict.id.as_str() == conflict_id)
        .ok_or_else(|| CliError::NotFound(format!("Conflict {conflict_id}")))?;
    let conflict = state.workspaces[index].conflicts.remove(conflict_index);
    let choice = arguments
        .get(1)
        .ok_or_else(|| CliError::Parse("resolution choice required".into()))?
        .trim_start_matches("--");
    if matches!(choice, "branch" | "layer") {
        let path = checked_join(&state.workspaces[index].artifacts.mount, &conflict.path)?;
        match state.workspaces[index].anchor.get(&conflict.path) {
            Some(TreeEntry::Directory) => fs::create_dir_all(path).map_err(io_error)?,
            Some(TreeEntry::File(bytes)) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                fs::write(path, bytes).map_err(io_error)?;
            }
            None if path.is_dir() => fs::remove_dir_all(path).map_err(io_error)?,
            None if path.exists() => fs::remove_file(path).map_err(io_error)?,
            None => {}
        }
        state.workspaces[index].refresh(true)?;
    }
    state.workspaces[index]
        .output
        .push(format!("resolved {} with {choice}", conflict.path));
    Ok(CommandResult::Workspace(format!(
        "Resolved {} with {choice}",
        conflict.id
    )))
}

pub(crate) fn branch_head(branch: &BranchRecord) -> Option<CommitId> {
    branch
        .work_head
        .map(|number| commit_id(branch.id.as_str(), number))
        .or_else(|| branch.boundary_commit.clone())
}

fn run_process(path: &Path, script: &str) -> CliResult<(i32, String, String)> {
    let mut child = ProcessCommand::new("/bin/bash")
        .args(["-lc", script])
        .current_dir(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| CliError::Integrity("Bash stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| CliError::Integrity("Bash stderr".into()))?;
    let stdout = std::thread::spawn(move || read_bounded(stdout, OUTPUT_BYTES));
    let stderr = std::thread::spawn(move || read_bounded(stderr, OUTPUT_BYTES));
    let status = child.wait().map_err(io_error)?;
    let stdout = stdout
        .join()
        .map_err(|_| CliError::Integrity("Bash stdout reader".into()))??;
    let stderr = stderr
        .join()
        .map_err(|_| CliError::Integrity("Bash stderr reader".into()))??;
    Ok((status.code().unwrap_or(128), stdout, stderr))
}

fn read_bounded(mut reader: impl Read, limit: usize) -> CliResult<String> {
    let mut retained = Vec::new();
    let mut buffer = [0; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(retained.len());
        retained.extend_from_slice(&buffer[..read.min(remaining)]);
    }
    Ok(String::from_utf8_lossy(&retained).into_owned())
}

fn write_tree(root: &Path, tree: &Tree) -> CliResult<()> {
    for (relative, entry) in tree {
        let path = checked_join(root, relative)?;
        match entry {
            TreeEntry::Directory => fs::create_dir_all(&path).map_err(io_error)?,
            TreeEntry::File(bytes) => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(io_error)?;
                }
                File::create(path)
                    .and_then(|mut file| file.write_all(bytes))
                    .map_err(io_error)?;
            }
        }
    }
    Ok(())
}

fn read_tree(root: &Path) -> CliResult<Tree> {
    let mut tree = Tree::new();
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0u64;
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(io_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(io_error)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if tree.len() >= MAX_FILES {
                return Err(CliError::Integrity("Workspace file limit".into()));
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(io_error)?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| CliError::Integrity("Workspace path escape".into()))?;
            let relative = normalized_relative(relative)?;
            if metadata.file_type().is_symlink() {
                return Err(CliError::Integrity(format!(
                    "Workspace symlink unsupported: {relative}"
                )));
            }
            if metadata.is_dir() {
                tree.insert(relative, TreeEntry::Directory);
                pending.push(path);
            } else if metadata.is_file() {
                if metadata.len() > MAX_FILE_BYTES {
                    return Err(CliError::Integrity("Workspace file size limit".into()));
                }
                total = total.saturating_add(metadata.len());
                if total > MAX_TREE_BYTES {
                    return Err(CliError::Integrity("Workspace tree size limit".into()));
                }
                tree.insert(relative, TreeEntry::File(fs::read(path).map_err(io_error)?));
            } else {
                return Err(CliError::Integrity(
                    "Workspace special file unsupported".into(),
                ));
            }
        }
    }
    Ok(tree)
}

fn file_views(root: &Path, tree: &Tree) -> CliResult<Vec<WorkspaceFileView>> {
    tree.iter()
        .map(|(path, entry)| {
            let absolute = checked_join(root, path)?;
            let metadata = fs::symlink_metadata(absolute).map_err(io_error)?;
            let (kind, bytes, preview) = match entry {
                TreeEntry::Directory => (WorkspaceFileKind::Directory, 0, FilePreview::None),
                TreeEntry::File(bytes) => {
                    (WorkspaceFileKind::File, bytes.len() as u64, preview(bytes))
                }
            };
            Ok(WorkspaceFileView {
                path: path.clone(),
                kind,
                bytes,
                allocated_bytes: allocated_bytes(&metadata),
                preview,
            })
        })
        .collect()
}

fn diff_trees(before: &Tree, after: &Tree) -> Vec<DiffEntryView> {
    let paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    paths
        .into_iter()
        .filter_map(|path| {
            let left = before.get(&path);
            let right = after.get(&path);
            let change = match (left, right) {
                (None, Some(_)) => DiffChange::Add,
                (Some(_), None) => DiffChange::Remove,
                (Some(left), Some(right)) if left != right => DiffChange::Modify,
                _ => return None,
            };
            Some(DiffEntryView {
                path,
                change,
                aspects: vec!["final state".into()],
                before: left.and_then(entry_text),
                after: right.and_then(entry_text),
            })
        })
        .collect()
}

fn candidate_objects(
    changes: &[DiffEntryView],
    tree: &Tree,
    root: &ObjectId,
    all: &[CanonicalObject],
) -> Vec<CanonicalObject> {
    let mut wanted = HashSet::from([root.clone()]);
    for change in changes {
        if let Some(entry) = tree.get(&change.path) {
            let payload = match entry {
                TreeEntry::Directory => b"directory".as_slice(),
                TreeEntry::File(bytes) => bytes,
            };
            let tag = if matches!(entry, TreeEntry::Directory) {
                b'd'
            } else {
                b'f'
            };
            let mut identity = vec![tag];
            identity.extend_from_slice(payload);
            wanted.insert(ObjectId::new(format!("O-{:016x}", fnv1a(&identity))));
        }
    }
    all.iter()
        .filter(|object| wanted.contains(&object.id))
        .cloned()
        .collect()
}

fn preview(bytes: &[u8]) -> FilePreview {
    if bytes.iter().take(PREVIEW_BYTES).any(|byte| *byte == 0) {
        return FilePreview::Binary;
    }
    let shown = &bytes[..bytes.len().min(PREVIEW_BYTES)];
    let text = String::from_utf8_lossy(shown).into_owned();
    if bytes.len() > PREVIEW_BYTES {
        FilePreview::Truncated(text)
    } else {
        FilePreview::Text(text)
    }
}

fn entry_text(entry: &TreeEntry) -> Option<String> {
    match entry {
        TreeEntry::Directory => Some("directory".into()),
        TreeEntry::File(bytes) if bytes.len() <= PREVIEW_BYTES && !bytes.contains(&0) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        TreeEntry::File(bytes) => Some(format!("{} bytes", bytes.len())),
    }
}

fn checked_join(root: &Path, relative: &str) -> CliResult<PathBuf> {
    let relative = Path::new(relative);
    normalized_relative(relative)?;
    Ok(root.join(relative))
}

fn normalized_relative(path: &Path) -> CliResult<String> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(CliError::Integrity("Workspace path traversal".into()));
    }
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| CliError::Integrity("Workspace path encoding".into()))
}

fn logical_bytes(tree: &Tree) -> u64 {
    tree.values()
        .map(|entry| match entry {
            TreeEntry::Directory => 0,
            TreeEntry::File(bytes) => bytes.len() as u64,
        })
        .sum()
}

fn tree_entry_bytes(entry: Option<&TreeEntry>) -> u64 {
    match entry {
        Some(TreeEntry::File(bytes)) => bytes.len() as u64,
        _ => 0,
    }
}

fn object_bytes(objects: &[CanonicalObject]) -> u64 {
    objects.iter().map(|object| object.bytes.len() as u64).sum()
}

#[cfg(unix)]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &fs::Metadata) -> u64 {
    metadata.len().div_ceil(4096).saturating_mul(4096)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn unique_path(label: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    std::env::temp_dir().join(format!(
        "layerfs-workspace-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ))
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn io_error(error: std::io::Error) -> CliError {
    CliError::Integrity(format!("Workspace filesystem: {error}"))
}
