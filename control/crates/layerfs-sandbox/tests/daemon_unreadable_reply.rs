//! A Sandbox daemon's answer this build cannot read (a daemon of another
//! LayerFS build) is a public refusal with a fix, never the raw failure's
//! debug text (RF re-check N-A): `Failed` for a read, `Unknown` for an
//! operation whose effect is then not known.
use layerfs_protocol::provision::install::{Call, CommitControl, ControlError, Request, WorkspaceToken};
use layerfs_history::WorkspaceId;
use layerfs_sandbox::daemon::values::refusal;
use layerfs_sandbox::internal::{ControlCause, ControlFailure, ControlPhase};
use layerfs_sandbox::{OperationCause, OperationFailure};

fn unreadable(request: Request) -> OperationFailure {
    OperationFailure {
        request: request.clone(),
        cause: OperationCause::Exchange(Box::new(ControlFailure {
            call: Call { id: 2, request },
            phase: ControlPhase::Receive,
            attempted: true,
            received: None,
            cause: ControlCause::Protocol(ControlError("control refusal code")),
            fence_error: None,
        })),
    }
}

fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
        namespace: 1,
    }
}

#[test]
fn an_unreadable_status_is_failed_with_the_doctor_as_fix() {
    let refused = refusal(&unreadable(Request::Status(token()))).unwrap();
    assert_eq!(refused.code, "Failed");
    assert!(refused.message.contains("another build"), "{refused:?}");
    assert!(!refused.message.contains("OperationFailure"), "{refused:?}");
    assert_eq!(refused.detail.get_text("fix"), Some("layerfs home doctor"));
}

#[test]
fn an_unreadable_commit_answer_leaves_its_outcome_unknown() {
    let commit = Request::Commit(CommitControl {
        token: token(),
        force: false,
        message: None,
        expected: None,
    });
    let refused = refusal(&unreadable(commit)).unwrap();
    assert_eq!(refused.code, "Unknown");
    assert_eq!(refused.detail.get_text("outcome"), Some("unknown"));
    assert_eq!(refused.detail.get_text("fix"), Some("layerfs home doctor"));
}
