//! A daemon or runtime request that was sent and never settled keeps its
//! uncertainty in the answer (LF2 review FAIL 1): the message says so in
//! words, and the refusal carries `attempted: true` in its detail and flat
//! beside its code, so the short form shows it as well. A request never
//! sent, a daemon refusal and an observed stopped daemon are known.
use layerfs_history::WorkspaceId;
use layerfs_protocol::daemon::{Call, ControlCode, ControlRefusal, Reply, Request, WorkspaceToken};
use layerfs_protocol::duplex::ChannelError;
use layerfs_protocol::provision::install::ControlError;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::daemon::values::{failure, mount_failure, sdk_error, wire, ATTEMPTED};
use layerfs_sandbox::internal::{ControlCause, ControlFailure, ControlPhase};
use layerfs_sandbox::{ContainerId, RuntimeError, WireFailure, OUTCOME_UNKNOWN};
use layerfs_sandbox::{MountFailure, OperationCause, OperationFailure, SdkError};
use std::io;

fn unmount() -> Request {
    Request::Unmount(layerfs_protocol::daemon::UnmountControl {
        token: WorkspaceToken {
            workspace: WorkspaceId::from_authority([5; 32]).unwrap(),
            namespace: 2,
        },
        discard: false,
    })
}

fn exchange(
    phase: ControlPhase,
    attempted: bool,
    cause: ControlCause,
    fence_error: Option<ChannelError>,
) -> Box<OperationFailure> {
    Box::new(OperationFailure {
        request: unmount(),
        cause: OperationCause::Exchange(Box::new(ControlFailure {
            call: Call {
                id: 7,
                request: unmount(),
            },
            phase,
            attempted,
            received: None,
            cause,
            fence_error,
        })),
    })
}

fn reset() -> ControlCause {
    ControlCause::Channel(ChannelError::Io(io::Error::new(
        io::ErrorKind::ConnectionReset,
        "connection reset",
    )))
}

/// `attempted: true` in the detail and among the next call's fields.
fn uncertain(refusal: &SdkRefusal) -> bool {
    let flat = (refusal.detail.get("next")).and_then(|next| next.get(ATTEMPTED));
    let held = refusal.detail.get(ATTEMPTED);
    assert_eq!(flat, held, "{refusal:?}");
    held == Some(&SdkValue::Bool(true)) && refusal.message.contains(OUTCOME_UNKNOWN)
}

#[test]
fn a_request_lost_while_it_was_sent_is_uncertain() {
    let refusal = failure(exchange(ControlPhase::Send, true, reset(), None));
    assert_eq!(refusal.code, "Failed");
    assert_eq!(
        refusal.message,
        "the connection to the Sandbox's daemon failed while the request was sent: \
         connection reset; the request was sent; its outcome is not known"
    );
    assert!(uncertain(&refusal), "{refusal:?}");
}

#[test]
fn a_request_never_sent_is_known() {
    let quarantined = ControlCause::Channel(ChannelError::Quarantined);
    let refusal = failure(exchange(ControlPhase::Encode, false, quarantined, None));
    assert_eq!(refusal.code, "Failed");
    assert!(!refusal.message.contains(OUTCOME_UNKNOWN), "{refusal:?}");
    assert_eq!(refusal.detail.get(ATTEMPTED), None);
    assert_eq!(refusal.detail.get("next"), None);
}

#[test]
fn a_failed_fence_is_said_in_words() {
    let fence = ChannelError::Io(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"));
    let refusal = failure(exchange(ControlPhase::Fence, true, reset(), Some(fence)));
    assert!(
        refusal
            .message
            .ends_with("; closing the connection also failed: broken pipe"),
        "{refusal:?}"
    );
    assert!(uncertain(&refusal), "{refusal:?}");
}

#[test]
fn an_answer_lost_after_sending_is_unavailable_and_uncertain() {
    let refusal = failure(exchange(ControlPhase::Receive, true, reset(), None));
    assert_eq!(refusal.code, "Unavailable");
    assert_eq!(refusal.detail.get_text("outcome"), Some("unknown"));
    let flat = (refusal.detail.get("next")).and_then(|next| next.get(ATTEMPTED));
    assert_eq!(flat, Some(&SdkValue::Bool(true)));
    assert_eq!(refusal.detail.get(ATTEMPTED), Some(&SdkValue::Bool(true)));
}

#[test]
fn an_answer_this_call_does_not_expect_is_uncertain() {
    let refusal = failure(OperationFailure::unexpected(unmount(), Reply::SessionEnded));
    assert_eq!(refusal.code, "Failed");
    assert!(uncertain(&refusal), "{refusal:?}");
}

#[test]
fn a_daemon_refusal_is_known_and_keeps_its_code() {
    let refused = Box::new(OperationFailure {
        request: unmount(),
        cause: OperationCause::Remote(ControlRefusal {
            code: ControlCode::Busy,
            phase: "unmount".to_owned(),
            moved: None,
            published: None,
            detail: "the Workspace is busy".to_owned(),
        }),
    });
    let refusal = sdk_error(SdkError::Operation(refused));
    assert_eq!(refusal.code, "Busy");
    assert_eq!(refusal.detail.get(ATTEMPTED), None);
    let passed = sdk_error(SdkError::Refused(layerfs_home::outcome::refused(
        "NoSuchPath",
        "gone",
    )));
    assert_eq!(passed.code, "NoSuchPath");
}

#[test]
fn a_mount_whose_bind_was_sent_and_lost_is_uncertain() {
    let mount = Box::new(MountFailure {
        bound: None,
        failure: exchange(ControlPhase::Send, true, reset(), None),
    });
    let refusal = mount_failure(mount);
    assert_eq!(refusal.code, "Failed");
    assert!(uncertain(&refusal), "{refusal:?}");
    let protocol = ControlCause::Protocol(ControlError("record"));
    let unsent = Box::new(MountFailure {
        bound: None,
        failure: exchange(ControlPhase::Encode, false, protocol, None),
    });
    assert_eq!(mount_failure(unsent).detail.get(ATTEMPTED), None);
}

fn sent(cause: RuntimeError, observed: bool) -> Box<WireFailure> {
    let id = ContainerId::parse(&"ab".repeat(32)).unwrap();
    let mut failure = WireFailure::local(cause);
    failure.attempted = true;
    failure.requested_container = Some(id);
    failure.observed_container = observed.then_some(id);
    failure.fence_error = Some(io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe"));
    failure
}

#[test]
fn a_runtime_request_sent_and_lost_is_uncertain() {
    let lost = sent(RuntimeError::Io(io::Error::other("reset")), false);
    let refusal = wire(lost);
    assert_eq!(
        refusal.message,
        "runtime I/O: reset; the request was sent; its outcome is not known; \
         closing its connection also failed: broken pipe"
    );
    assert!(uncertain(&refusal), "{refusal:?}");
    // never sent: known
    let local = wire(WireFailure::local(RuntimeError::Unsupported("stop")));
    assert_eq!(local.detail.get(ATTEMPTED), None);
    assert!(local.message.ends_with("; nothing was sent"));
    // its daemon observed stopped: known
    let stopped = sent(
        RuntimeError::Protocol("exact stopped container observation"),
        true,
    );
    assert!(stopped.container_stopped());
    let refusal = wire(stopped);
    assert_eq!(refusal.message, "the Sandbox's daemon is not running");
    assert_eq!(refusal.detail.get(ATTEMPTED), None);
}
