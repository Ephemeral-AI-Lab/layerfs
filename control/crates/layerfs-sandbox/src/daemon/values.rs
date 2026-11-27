//! Workspace control answers as the JavaScript SDK shows them.
//!
//! Commit, status and unmount keep their typed control records on the wire;
//! these functions give them the same value shapes the SDK channel uses, and
//! turn a Commit conflict, a Dirty unmount and a daemon control refusal into
//! refusals with stable codes.
use super::workspace::{CommitReply, MountedWorkspace, Unmounted};
use crate::{OperationCause, OperationFailure};
use layerfs_protocol::provision::install::{AbortDisposition, DetachDisposition, ForcedOutcome, SdkRefusal, SdkValue, TeardownStage, WorkspaceStatus};
use layerfs_protocol::sdk::{commit_value, conflicts, conflicts_truncated, hex, lww, lww_truncated};
use layerfs_history::{CommitId, CommitOutcome};

fn head(id: Option<CommitId>) -> SdkValue {
    SdkValue::opt_text(id.map(|id| hex(id.as_slice())))
}

/// A daemon control refusal with its stable code, when the failure is one.
pub fn refusal(failure: &OperationFailure) -> Option<SdkRefusal> {
    match &failure.cause {
        OperationCause::Remote(refused) => Some(SdkRefusal {
            code: format!("{:?}", refused.code),
            message: refused.detail.clone(),
            detail: SdkValue::map([("phase", SdkValue::text(refused.phase.as_str()))]),
        }),
        // A terminal step the daemon stopped after effects. Its stage is the
        // one fact a later `force` is judged by, so it travels in the detail
        // the machine writes into `workspace/<id>.unmount`; a failure without
        // such an answer (a send or connect failure, or a refusal before any
        // effect) has no stage.
        // A forced teardown's abort and detach dispositions travel too: a
        // detach the kernel refused `EBUSY` is the one custody a later
        // unmount or force may ask again (#538).
        OperationCause::Retained(custody) => {
            let mut detail = vec![
                ("stage", SdkValue::text(stage_text(custody.stage))),
                ("detached", SdkValue::Bool(custody.detached)),
            ];
            let text = |value: &dyn std::fmt::Debug| SdkValue::text(format!("{value:?}"));
            if let Some(facts) = &custody.forced {
                detail.push(("abort", text(&facts.abort)));
                detail.push(("detach", text(&facts.detach)));
            }
            let busy = custody.forced.as_ref().is_some_and(|facts| {
                facts.abort == AbortDisposition::Written && facts.detach == DetachDisposition::Busy
            });
            Some(SdkRefusal {
                code: "Failed".to_owned(),
                message: if busy && custody.stage == TeardownStage::Detach {
                    "aborted; a process still holds the mount, so the kernel refused its detach; unmount again once it exits".to_owned()
                } else {
                    format!(
                        "teardown custody retained at {}; do not resend",
                        stage_text(custody.stage)
                    )
                },
                detail: SdkValue::Map(
                    detail.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
                ),
            })
        }
        // The daemon answered something this build cannot read: a daemon of
        // another build whose record differs. It is never shown raw.
        OperationCause::Exchange(exchange)
            if matches!(
                exchange.cause,
                crate::daemon::internal::ControlCause::Protocol(_)
            ) =>
        {
            Some(unreadable(&failure.request))
        }
        // The request was sent but no reply arrived (the daemon reclaimed or
        // expired the worker, or it died mid-operation): its effect is
        // unknown, so the caller observes `Unavailable { outcome: unknown }`
        // rather than a silent failure (#547 slice 5).
        OperationCause::Exchange(exchange)
            if exchange.phase == crate::daemon::internal::ControlPhase::Receive =>
        {
            Some(SdkRefusal {
                code: "Unavailable".to_owned(),
                message: "the Sandbox's daemon ended this connection before it answered; the operation's outcome is unknown"
                    .to_owned(),
                detail: SdkValue::map([
                    ("outcome", SdkValue::text("unknown")),
                    ("retryable", SdkValue::Bool(false)),
                    (ATTEMPTED, SdkValue::Bool(true)),
                ]),
            })
            .map(|refusal| layerfs_home::outcome::with_next(refusal, ATTEMPTED, SdkValue::Bool(true)))
        }
        _ => None,
    }
}

/// The refusal of a daemon operation's `failure`: the daemon's own code when
/// [`refusal`] gives one, else `Failed` in the failure's words, uncertain
/// ([`uncertain`]) when its request was sent and no answer settled it.
pub fn failure(failure: Box<OperationFailure>) -> SdkRefusal {
    refusal(&failure).unwrap_or_else(|| uncertain(&failure, failure.attempted()))
}

/// The refusal of one SDK call's `error`: the daemon's refusal as it
/// answered it, else [`failure`] or [`mount_failure`].
pub fn sdk_error(error: crate::SdkError) -> SdkRefusal {
    match error {
        crate::SdkError::Refused(refusal) => refusal,
        crate::SdkError::Operation(failed) => failure(failed),
        crate::SdkError::Mount(failed) => mount_failure(failed),
    }
}

/// [`failure`] of a mount: the step that stopped, in words, and whether the
/// Workspace stayed bound.
pub fn mount_failure(failure: Box<crate::MountFailure>) -> SdkRefusal {
    uncertain(&failure, failure.failure.attempted())
}

/// The refusal of a runtime request's `failure` (a stop, delete or reopen
/// of a Sandbox's daemon): uncertain when it was sent and nothing observed
/// settles it.
pub fn wire(failure: Box<crate::WireFailure>) -> SdkRefusal {
    uncertain(&failure, failure.unsettled())
}

/// `Failed` in `error`'s words. When `attempted`, the request was sent and
/// its outcome is not known: the words say so, and the refusal carries
/// `attempted: true` in its detail and flat beside `error`, so the short
/// form shows it too. Nothing is resent.
pub fn uncertain(error: impl std::fmt::Display, attempted: bool) -> SdkRefusal {
    let refusal = layerfs_home::outcome::failed(error);
    match attempted {
        false => refusal,
        true => with_attempted(refusal),
    }
}

/// `refusal` marked uncertain: `attempted: true` in its detail and flat.
fn with_attempted(mut refusal: SdkRefusal) -> SdkRefusal {
    let sent = crate::OUTCOME_UNKNOWN;
    if !refusal.message.contains(sent) {
        refusal.message = format!("{}; {sent}", refusal.message);
    }
    refusal.detail =
        layerfs_home::home::with_field(refusal.detail, ATTEMPTED, SdkValue::Bool(true));
    layerfs_home::outcome::with_next(refusal, ATTEMPTED, SdkValue::Bool(true))
}

/// The key that marks an uncertain refusal: its request was sent.
pub const ATTEMPTED: &str = "attempted";

/// The refusal of `request` whose answer this build could not read (a
/// Sandbox daemon of another LayerFS build): `Failed` for a read, which
/// changed nothing; `Unknown` for any other request, whose effect is not
/// known. Its fix is the doctor that names the daemon's build.
fn unreadable(request: &layerfs_protocol::provision::install::Request) -> SdkRefusal {
    use layerfs_protocol::provision::install::Request;
    let read = matches!(
        request,
        Request::Status(_) | Request::Hello(_) | Request::Sdk(_)
    );
    let said = "the Sandbox's daemon answered in a form this LayerFS build cannot read: it runs another build, which `layerfs home doctor` names";
    let refusal = match read {
        true => layerfs_home::outcome::refused("Failed", said),
        false => SdkRefusal {
            code: "Unknown".to_owned(),
            message: format!(
                "{said}; the operation's outcome is unknown, so check it before sending it again"
            ),
            detail: SdkValue::map([("outcome", SdkValue::text("unknown"))]),
        },
    };
    layerfs_home::outcome::with_fix(refusal, "layerfs home doctor")
}

/// A retained teardown stage as the machine's unmount record writes it.
pub(crate) fn stage_text(stage: TeardownStage) -> &'static str {
    match stage {
        TeardownStage::Detach => "Detach",
        TeardownStage::Join => "Join",
        TeardownStage::Owner => "Owner",
        TeardownStage::Requests => "Requests",
        TeardownStage::Lane => "Lane",
        TeardownStage::Revoke => "Revoke",
        TeardownStage::Close => "Close",
        TeardownStage::Registry => "Registry",
        TeardownStage::Abort => "Abort",
    }
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
                    let (conflicts, count) = layerfs_home::helpers::shown(paths, total, false);
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
