//! Workspace control answers as the JavaScript SDK shows them.
//!
//! Commit, status and unmount keep their typed control records on the wire;
//! these functions give them the same value shapes the SDK channel uses, and
//! turn a Commit conflict and a Dirty unmount into refusals with stable
//! codes. A daemon control refusal is the Sandbox's
//! (`layerfs_sandbox::route::daemon::refusal`).
use layerfs_history::{CommitId, CommitOutcome};
use layerfs_protocol::daemon::{ForcedOutcome, WorkspaceStatus};
use layerfs_protocol::sdk::{
    commit_value, conflicts, conflicts_truncated, hex, lww, lww_truncated,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::route::daemon::workspace::{CommitReply, MountedWorkspace, Unmounted};

fn head(id: Option<CommitId>) -> SdkValue {
    SdkValue::opt_text(id.map(|id| hex(id.as_slice())))
}

/// A mounted Workspace: its path and binding.
pub fn mounted(workspace: &MountedWorkspace, branch: &str) -> SdkValue {
    let binding = &workspace.bound.binding;
    SdkValue::map([
        ("kind", SdkValue::text("Mounted")),
        ("path", SdkValue::text(workspace.ready.directory.as_str())),
        (
            "workspace",
            SdkValue::text(hex(&workspace.bound.token.workspace.to_bytes())),
        ),
        ("namespace", SdkValue::Int(workspace.bound.token.namespace)),
        ("branch", SdkValue::text(branch)),
        ("base", head(binding.branch.head_commit)),
        (
            "baseLayer",
            SdkValue::text(hex(binding.branch.base_layer.as_slice())),
        ),
        (
            "root",
            SdkValue::text(hex(binding.effective_root.as_bytes())),
        ),
    ])
}

/// `sandbox.status`: base, the Branch head now, whether it moved, and the
/// uncaptured change counts. Status itself reads no Store; `current_head` is
/// the head the caller read separately (`None` when that read failed).
pub fn status(status: &WorkspaceStatus, current_head: Option<Option<CommitId>>) -> SdkValue {
    let binding = &status.binding;
    SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("base", head(binding.branch.head_commit)),
        (
            "baseLayer",
            SdkValue::text(hex(binding.branch.base_layer.as_slice())),
        ),
        ("branchHead", current_head.map_or(SdkValue::Null, head)),
        (
            "headMovedSinceMount",
            current_head.map_or(SdkValue::Null, |current| {
                SdkValue::Bool(current != binding.branch.head_commit)
            }),
        ),
        ("activity", SdkValue::text(format!("{:?}", status.activity))),
        (
            "overlay",
            status.local.as_ref().map_or(SdkValue::Null, |local| {
                SdkValue::map([
                    ("inodes", SdkValue::Int(local.dirty_inodes as i64)),
                    (
                        "entries",
                        SdkValue::Int(local.dirty_directory_entries as i64),
                    ),
                    (
                        "pendingMerge",
                        SdkValue::Bool(local.base_root != *binding.effective_root.as_bytes()),
                    ),
                ])
            }),
        ),
    ])
}

fn outcome(
    outcome: &CommitOutcome,
    merged: SdkValue,
    lww_value: SdkValue,
    lww_omitted: bool,
) -> SdkValue {
    match outcome {
        CommitOutcome::Committed { record, displaced } => SdkValue::map([
            ("kind", SdkValue::text("Committed")),
            ("head", SdkValue::text(hex(record.id.as_slice()))),
            ("commit", commit_value(record, None)),
            ("displacedHead", head(*displaced)),
            ("merged", merged),
            ("lww", lww_value),
            ("lwwTruncated", SdkValue::Bool(lww_omitted)),
        ]),
        CommitOutcome::UpToDate { head: at, root } => SdkValue::map([
            ("kind", SdkValue::text("UpToDate")),
            ("head", head(*at)),
            ("root", SdkValue::text(hex(root.as_bytes()))),
            ("merged", merged),
            ("lww", lww_value),
            ("lwwTruncated", SdkValue::Bool(lww_omitted)),
        ]),
    }
}

/// `sandbox.commit`: Committed or UpToDate, with `merged` when the head had
/// moved; a conflict is the `Conflict` refusal.
pub fn commit(reply: &CommitReply) -> Result<SdkValue, SdkRefusal> {
    match reply {
        CommitReply::Committed(done) => Ok(outcome(
            done,
            SdkValue::Null,
            SdkValue::List(Vec::new()),
            false,
        )),
        CommitReply::Merged(done) => Ok(outcome(
            &done.outcome,
            SdkValue::map([("from", head(done.from)), ("head", head(done.head))]),
            lww(&done.lww),
            lww_truncated(&done.lww),
        )),
        CommitReply::Conflict(report) => Err(SdkRefusal {
            code: "Conflict".to_owned(),
            message: if report.markers {
                format!(
                    "{} file(s) still contain conflict markers; nothing committed",
                    report.conflicts.len()
                )
            } else {
                format!(
                    "the head moved and {} file(s) conflict with this Workspace; markers are written and nothing was committed",
                    report.conflicts.len()
                )
            },
            detail: SdkValue::map([
                ("head", head(report.head)),
                ("base", head(report.base)),
                ("markers", SdkValue::Bool(report.markers)),
                ("conflicts", conflicts(&report.conflicts)),
                ("lww", lww(&report.lww)),
                (
                    "conflictsTruncated",
                    SdkValue::Bool(report.truncated || conflicts_truncated(&report.conflicts)),
                ),
                (
                    "lwwTruncated",
                    SdkValue::Bool(report.truncated || lww_truncated(&report.lww)),
                ),
                (
                    "written",
                    SdkValue::List(report.written.iter().map(SdkValue::text).collect()),
                ),
                ("truncated", SdkValue::Bool(report.truncated)),
                // what the call answers flat beside the code (call surface
                // 2.0): the head, the first 20 conflicting paths, and their
                // count only when the list was cut
                ("next", {
                    let paths = (report.conflicts.iter())
                        .map(|conflict| SdkValue::text(&conflict.path))
                        .collect();
                    let total = report.conflicts.len();
                    let (conflicts, count) =
                        layerfs_home::reply::values::shown(paths, total, false);
                    let mut next = vec![("commit", head(report.head)), ("conflicts", conflicts)];
                    if count != SdkValue::Null {
                        next.push(("conflicts_count", count));
                    }
                    SdkValue::Map(next.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
                }),
            ]),
        }),
    }
}

/// `sandbox.unmount` with `force`: what the forced teardown established.
pub fn force_unmounted(outcome: &ForcedOutcome) -> SdkValue {
    let text = |value: &dyn std::fmt::Debug| SdkValue::text(format!("{value:?}"));
    SdkValue::map([
        ("kind", SdkValue::text("ForceUnmounted")),
        ("abort", text(&outcome.facts.abort)),
        ("detach", text(&outcome.facts.detach)),
        ("cleanup", text(&outcome.cleanup)),
    ])
}

/// `sandbox.unmount`: Unmounted, or the `Dirty` refusal naming the paths.
pub fn unmounted(answer: &Unmounted) -> Result<SdkValue, SdkRefusal> {
    match answer {
        Unmounted::Closed => Ok(SdkValue::map([("kind", SdkValue::text("Unmounted"))])),
        Unmounted::Dirty(report) => Err(SdkRefusal {
            code: "Dirty".to_owned(),
            message: format!(
                "{} uncommitted change(s); unmount with discard to drop them",
                report.paths.len()
            ),
            detail: SdkValue::map([
                (
                    "changed",
                    SdkValue::List(report.paths.iter().map(SdkValue::text).collect()),
                ),
                ("pendingMerge", SdkValue::Bool(report.pending_merge)),
                ("truncated", SdkValue::Bool(report.truncated)),
            ]),
        }),
    }
}
