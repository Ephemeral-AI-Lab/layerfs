//! A Sandbox's start, connection and mount failures read as words, never
//! Rust `Debug` text (UI pass 1 FAIL 1: a native start answered
//! "LaunchFailure { phase: Listener, … Protocol(…) }"), and so does
//! `layerfs_home::outcome::failed`, which most refusals of an unexpected
//! failure go through.
use layerfs_bridge::control::{
    Call, ControlCode, ControlError, ControlRefusal, Request, WorkspaceToken,
};
use layerfs_history::WorkspaceId;
use layerfs_sandbox::internal::{ControlCause, ControlFailure, ControlPhase};
use layerfs_sandbox::{MountFailure, OperationCause, OperationFailure, SandboxCause};
use layerfs_sandbox_backend::{LaunchFailure, LaunchPhase, RuntimeError, WireFailure};

fn no_debug(text: &str) {
    for debug in ["{", "}", "Some(", "Protocol(", "Failure", "Os ", "kind:"] {
        assert!(!text.contains(debug), "Debug text {debug:?} in {text:?}");
    }
}

fn status() -> Request {
    Request::Status(WorkspaceToken {
        workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
        namespace: 1,
    })
}

#[test]
fn a_launch_that_stopped_before_its_listener_reads_as_words() {
    let cause = SandboxCause::Provider(Box::new(LaunchFailure {
        phase: LaunchPhase::Listener,
        instance: None,
        cause: RuntimeError::Protocol("local daemon exited before its listener line"),
    }));
    let said = cause.to_string();
    assert_eq!(said, "local daemon exited before its listener line");
    let phase = layerfs_sandbox::SandboxPhase::Listener.words();
    assert_eq!(phase, "before its daemon said it was listening");
    let io = SandboxCause::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "no access",
    ));
    assert_eq!(io.to_string(), "no access");
    no_debug(&io.to_string());
}

#[test]
fn a_runtime_failure_says_what_it_knows_in_words() {
    let local = WireFailure::local(RuntimeError::Unsupported("image copy"));
    assert_eq!(
        local.to_string(),
        "unsupported runtime capability: image copy; nothing was sent"
    );
    no_debug(&local.to_string());
}

#[test]
fn a_connection_failure_and_a_daemon_refusal_read_as_words() {
    let exchange = OperationFailure {
        request: status(),
        cause: OperationCause::Exchange(Box::new(ControlFailure {
            call: Call {
                id: 2,
                request: status(),
            },
            phase: ControlPhase::Receive,
            attempted: true,
            received: None,
            cause: ControlCause::Protocol(ControlError("control refusal code")),
            fence_error: None,
        })),
    };
    assert_eq!(
        exchange.to_string(),
        "the connection to the Sandbox's daemon failed while its answer was awaited: \
         control refusal code"
    );
    let refused = OperationFailure {
        request: status(),
        cause: OperationCause::Remote(ControlRefusal {
            code: ControlCode::Busy,
            phase: "status".to_owned(),
            moved: None,
            published: None,
            detail: "the Workspace is busy".to_owned(),
        }),
    };
    assert_eq!(refused.to_string(), "the Workspace is busy");
    let mount = MountFailure {
        bound: None,
        failure: Box::new(refused),
    };
    assert_eq!(mount.to_string(), "the Workspace is busy");
    no_debug(&exchange.to_string());
}

#[test]
fn an_unexpected_failure_refusal_is_its_display() {
    let error = std::io::Error::new(std::io::ErrorKind::NotFound, "the file is gone");
    let refusal = layerfs_home::outcome::failed(error);
    assert_eq!(refusal.code, "Failed");
    assert_eq!(refusal.message, "the file is gone");
    let quoted = layerfs_home::outcome::failed("a \"quoted\" word".to_owned());
    assert_eq!(quoted.message, "a \"quoted\" word");
}
