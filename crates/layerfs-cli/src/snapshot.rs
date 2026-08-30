use crate::fixture::{BranchRecord, CommitRecord, MockState};
use crate::model::{
    DeltaSummary, DiffChange, DiffEntryView, DiffSnapshot, FilePreview, FilesSnapshot, Page,
    PageRequest, RouteTarget, WorkspaceFileKind, WorkspaceFileView,
};
use crate::workspace::{canonical_tree, Tree, TreeEntry};
use crate::{BranchId, CliError, CliResult, CommitId, DiffRequest, ObjectId};
use std::collections::BTreeSet;

const PREVIEW_BYTES: usize = 4 * 1024;

struct Resolved<'a> {
    target: RouteTarget,
    root: ObjectId,
    tree: &'a Tree,
    generation: Option<u64>,
}

pub(crate) fn files_snapshot(
    state: &MockState,
    target: RouteTarget,
    page: &PageRequest,
) -> CliResult<FilesSnapshot> {
    let resolved = resolve(state, &target)?;
    let files = match &target {
        RouteTarget::Workspace(id) => state
            .workspaces
            .iter()
            .find(|workspace| &workspace.id == id)
            .ok_or_else(|| CliError::NotFound(id.to_string()))?
            .view
            .files
            .clone(),
        _ => resolved
            .tree
            .iter()
            .map(|(path, entry)| file_view(path, entry, None))
            .collect(),
    };
    Ok(FilesSnapshot {
        target,
        resolved: resolved.target,
        root: resolved.root,
        generation: resolved.generation,
        files: paginate(files, page, "files"),
    })
}

pub(crate) fn changes_snapshot(
    state: &MockState,
    target: RouteTarget,
    page: &PageRequest,
) -> CliResult<DiffSnapshot> {
    let to = resolve(state, &target)?;
    let from = baseline(state, &target)?;
    Ok(build_diff(target, from, to, page))
}

pub(crate) fn explicit_diff_snapshot(
    state: &MockState,
    request: &DiffRequest,
    page: &PageRequest,
) -> CliResult<DiffSnapshot> {
    let (target, from, to) = match request {
        DiffRequest::Layers { from, to } => {
            let from = state
                .find_layer(from)
                .ok_or_else(|| CliError::NotFound(from.clone()))?;
            let to = state
                .find_layer(to)
                .ok_or_else(|| CliError::NotFound(to.clone()))?;
            (
                RouteTarget::Layer(to.id.clone()),
                Some(resolve(state, &RouteTarget::Layer(from.id.clone()))?),
                resolve(state, &RouteTarget::Layer(to.id.clone()))?,
            )
        }
        DiffRequest::BranchCommits { branch, from, to } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let from = commit_target(branch, from)?;
            let to = commit_target(branch, to)?;
            (
                to.clone(),
                Some(resolve(state, &from)?),
                resolve(state, &to)?,
            )
        }
        DiffRequest::BranchLayer { branch, layer } => {
            let branch = state
                .find_branch(branch)
                .ok_or_else(|| CliError::NotFound(branch.clone()))?;
            let layer = state
                .find_layer(layer)
                .ok_or_else(|| CliError::NotFound(layer.clone()))?;
            let target = RouteTarget::Branch(branch.id.clone());
            (
                target.clone(),
                Some(resolve(state, &RouteTarget::Layer(layer.id.clone()))?),
                resolve(state, &target)?,
            )
        }
    };
    Ok(build_diff(target, from, to, page))
}

pub(crate) fn diff_trees(before: &Tree, after: &Tree) -> Vec<DiffEntryView> {
    before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
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

pub(crate) fn file_view(
    path: &str,
    entry: &TreeEntry,
    allocated_bytes: Option<u64>,
) -> WorkspaceFileView {
    let (kind, bytes, preview) = match entry {
        TreeEntry::Directory => (WorkspaceFileKind::Directory, 0, FilePreview::None),
        TreeEntry::File(bytes) => (WorkspaceFileKind::File, bytes.len() as u64, preview(bytes)),
    };
    WorkspaceFileView {
        path: path.into(),
        kind,
        bytes,
        allocated_bytes,
        preview,
    }
}

fn resolve<'a>(state: &'a MockState, target: &RouteTarget) -> CliResult<Resolved<'a>> {
    match target {
        RouteTarget::Layer(id) => {
            let layer = state
                .layers
                .iter()
                .find(|layer| &layer.id == id)
                .ok_or_else(|| CliError::NotFound(id.to_string()))?;
            Ok(Resolved {
                target: target.clone(),
                root: layer.root.clone(),
                tree: &layer.tree,
                generation: None,
            })
        }
        RouteTarget::Commit(branch, commit) => resolve_commit(state, branch, commit),
        RouteTarget::Branch(id) => {
            let branch = state
                .branch(id)
                .ok_or_else(|| CliError::NotFound(id.to_string()))?;
            if let Some(number) = branch.work_head.or(branch.authority_head) {
                let commit = branch
                    .commit(number)
                    .ok_or_else(|| CliError::NotFound(format!("{id}/C{number}")))?;
                resolved_commit(branch, commit)
            } else {
                resolve_origin(state, branch)
            }
        }
        RouteTarget::Workspace(id) => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| &workspace.id == id)
                .ok_or_else(|| CliError::NotFound(id.to_string()))?;
            let root = canonical_tree(&workspace.current).0;
            Ok(Resolved {
                target: target.clone(),
                root,
                tree: &workspace.current,
                generation: Some(workspace.generation),
            })
        }
        RouteTarget::Project(_) | RouteTarget::Operation(_) => Err(CliError::Parse(
            "files require Layer, Branch, Commit, or Workspace".into(),
        )),
    }
}

fn baseline<'a>(state: &'a MockState, target: &RouteTarget) -> CliResult<Option<Resolved<'a>>> {
    match target {
        RouteTarget::Layer(id) => {
            let layer = state
                .layers
                .iter()
                .find(|layer| &layer.id == id)
                .ok_or_else(|| CliError::NotFound(id.to_string()))?;
            let Some(number) = layer.number.checked_sub(1).filter(|number| *number > 0) else {
                return Ok(None);
            };
            let parent = state
                .layers
                .iter()
                .find(|candidate| {
                    candidate.project_id == layer.project_id && candidate.number == number
                })
                .ok_or_else(|| CliError::Integrity(format!("parent of {id}")))?;
            resolve(state, &RouteTarget::Layer(parent.id.clone())).map(Some)
        }
        RouteTarget::Branch(id) => state
            .branch(id)
            .ok_or_else(|| CliError::NotFound(id.to_string()))
            .and_then(|branch| resolve_origin(state, branch).map(Some)),
        RouteTarget::Commit(branch_id, commit_id) => {
            let branch = state
                .branch(branch_id)
                .ok_or_else(|| CliError::NotFound(branch_id.to_string()))?;
            let commit = branch
                .commits
                .iter()
                .find(|commit| &commit.id == commit_id)
                .ok_or_else(|| CliError::NotFound(commit_id.to_string()))?;
            if commit.number > 1 {
                let parent = branch
                    .commit(commit.number - 1)
                    .ok_or_else(|| CliError::Integrity(format!("parent of {commit_id}")))?;
                Ok(Some(resolved_commit(branch, parent)?))
            } else {
                resolve_origin(state, branch).map(Some)
            }
        }
        RouteTarget::Workspace(id) => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| &workspace.id == id)
                .ok_or_else(|| CliError::NotFound(id.to_string()))?;
            let target = match (&workspace.anchor_commit, &workspace.anchor_layer) {
                (Some(commit), None) => {
                    RouteTarget::Commit(workspace.branch_id.clone(), commit.clone())
                }
                (None, Some(layer)) => RouteTarget::Layer(layer.clone()),
                _ => return Err(CliError::Integrity(format!("Workspace {id} anchor"))),
            };
            resolve(state, &target).map(Some)
        }
        RouteTarget::Project(_) | RouteTarget::Operation(_) => Err(CliError::Parse(
            "changes require Layer, Branch, Commit, or Workspace".into(),
        )),
    }
}

fn resolve_origin<'a>(state: &'a MockState, branch: &BranchRecord) -> CliResult<Resolved<'a>> {
    match &branch.origin {
        crate::BranchOrigin::Layer(layer) => resolve(state, &RouteTarget::Layer(layer.clone())),
        crate::BranchOrigin::Commit(parent, commit) => resolve_commit(state, parent, commit),
    }
}

fn resolve_commit<'a>(
    state: &'a MockState,
    branch_id: &BranchId,
    commit_id: &CommitId,
) -> CliResult<Resolved<'a>> {
    let branch = state
        .branch(branch_id)
        .ok_or_else(|| CliError::NotFound(branch_id.to_string()))?;
    if let Some(commit) = branch.commits.iter().find(|commit| &commit.id == commit_id) {
        return resolved_commit(branch, commit);
    }
    if let crate::BranchOrigin::Commit(parent, boundary) = &branch.origin {
        if boundary == commit_id {
            return resolve_commit(state, parent, boundary);
        }
    }
    Err(CliError::NotFound(format!("{branch_id}/{commit_id}")))
}

fn resolved_commit<'a>(branch: &BranchRecord, commit: &'a CommitRecord) -> CliResult<Resolved<'a>> {
    Ok(Resolved {
        target: RouteTarget::Commit(branch.id.clone(), commit.id.clone()),
        root: commit.root.clone(),
        tree: &commit.tree,
        generation: None,
    })
}

fn commit_target(branch: &BranchRecord, value: &str) -> CliResult<RouteTarget> {
    branch
        .commits
        .iter()
        .find(|commit| commit.id.as_str() == value || format!("C{}", commit.number) == value)
        .map(|commit| RouteTarget::Commit(branch.id.clone(), commit.id.clone()))
        .ok_or_else(|| CliError::NotFound(format!("{}/{}", branch.id, value)))
}

fn build_diff(
    target: RouteTarget,
    from: Option<Resolved<'_>>,
    to: Resolved<'_>,
    page: &PageRequest,
) -> DiffSnapshot {
    let empty = Tree::new();
    let before = from.as_ref().map_or(&empty, |value| value.tree);
    let entries = diff_trees(before, to.tree);
    let summary = entries.iter().fold(
        DeltaSummary {
            before_bytes: logical_bytes(before),
            after_bytes: logical_bytes(to.tree),
            ..DeltaSummary::default()
        },
        |mut summary, entry| {
            match entry.change {
                DiffChange::Add => summary.added += 1,
                DiffChange::Modify => summary.modified += 1,
                DiffChange::Remove => summary.removed += 1,
            }
            summary
        },
    );
    let from_target = from.as_ref().map(|value| value.target.clone());
    let from_root = from.as_ref().map(|value| value.root.clone());
    let from_label = from_target
        .as_ref()
        .map_or_else(|| "empty filesystem".into(), target_label);
    let to_label = target_label(&to.target);
    DiffSnapshot {
        target,
        from_target,
        to_target: to.target,
        from_root,
        to_root: to.root,
        summary,
        title: format!("Changes {from_label} -> {to_label}"),
        from: from_label,
        to: to_label,
        entries: paginate(entries, page, "changes"),
    }
}

fn target_label(target: &RouteTarget) -> String {
    match target {
        RouteTarget::Project(id) => id.to_string(),
        RouteTarget::Layer(id) => id.to_string(),
        RouteTarget::Branch(id) => id.to_string(),
        RouteTarget::Commit(branch, commit) => format!("{branch}/{commit}"),
        RouteTarget::Workspace(id) => id.to_string(),
        RouteTarget::Operation(id) => id.to_string(),
    }
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

fn logical_bytes(tree: &Tree) -> u64 {
    tree.values()
        .map(|entry| match entry {
            TreeEntry::Directory => 0,
            TreeEntry::File(bytes) => bytes.len() as u64,
        })
        .sum()
}

fn paginate<T>(items: Vec<T>, request: &PageRequest, prefix: &str) -> Page<T> {
    let start = request
        .after
        .as_deref()
        .and_then(|cursor| cursor.strip_prefix(prefix))
        .and_then(|cursor| cursor.strip_prefix(':'))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
        .min(items.len());
    let limit = usize::from(request.limit.clamp(1, 128));
    let end = start.saturating_add(limit).min(items.len());
    let next = (end < items.len()).then(|| format!("{prefix}:{end}"));
    Page {
        items: items.into_iter().skip(start).take(limit).collect(),
        next,
    }
}
