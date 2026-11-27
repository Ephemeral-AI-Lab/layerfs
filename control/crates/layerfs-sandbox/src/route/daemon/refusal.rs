//! Daemon control failures as the JavaScript SDK shows them: a daemon
//! control refusal, an SDK channel error, a mount or wire failure and an
//! uncertain outcome, each a refusal with a stable code. The Workspace
//! answer values (mounted, status, Commit, unmount) are the workspace
//! crate's (`machine/values.rs`).
use crate::{OperationCause, OperationFailure};
use layerfs_protocol::daemon::{AbortDisposition, DetachDisposition, TeardownStage};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

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
                crate::route::daemon::channel::internal::ControlCause::Protocol(_)
            ) =>
        {
            Some(unreadable(&failure.request))
        }
        // The request was sent but no reply arrived (the daemon reclaimed or
        // expired the worker, or it died mid-operation): its effect is
        // unknown, so the caller observes `Unavailable { outcome: unknown }`
        // rather than a silent failure (#547 slice 5).
        OperationCause::Exchange(exchange)
            if exchange.phase == crate::route::daemon::channel::internal::ControlPhase::Receive =>
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
fn unreadable(request: &layerfs_protocol::daemon::Request) -> SdkRefusal {
    use layerfs_protocol::daemon::Request;
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
