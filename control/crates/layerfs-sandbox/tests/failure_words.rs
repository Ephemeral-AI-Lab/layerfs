//! A Sandbox's start, connection and mount failures read as words, never
//! Rust `Debug` text (UI pass 1 FAIL 1: a native start answered
//! "LaunchFailure { phase: Listener, … Protocol(…) }"), and so does
//! `layerfs_home::outcome::failed`, which most refusals of an unexpected
//! failure go through.
use layerfs_protocol::provision::install::{Call, ControlCode, ControlError, ControlRefusal, Request, WorkspaceToken};
use layerfs_history::WorkspaceId;
use layerfs_sandbox::internal::{ControlCause, ControlFailure, ControlPhase};
use layerfs_sandbox::{MountFailure, OperationCause, OperationFailure, SandboxCause};
use layerfs_sandbox::{LaunchFailure, LaunchPhase, RuntimeError, WireFailure};

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
         control refusal code; the request was sent; its outcome is not known"
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

/// A Sandbox whose daemon ended is named by its daemon, a process on
/// either backend, never a "container" (UI pass 1, minor).
#[test]
fn a_sandbox_whose_daemon_ended_names_its_daemon() {
    let stopped = layerfs_sandbox::reopen::gone("t1", "c0ffee", true);
    assert_eq!(stopped.code, "NoSuchSandbox");
    assert_eq!(
        stopped.message,
        "the daemon of Sandbox t1 has stopped, and every Workspace that was in it with it"
    );
    let gone = layerfs_sandbox::reopen::gone("t1", "c0ffee", false);
    assert!(gone.message.starts_with("the daemon of Sandbox t1 is gone"));
    assert!(!gone.message.contains("container"), "{gone:?}");
}

/// A close on a worker's side goes on when the Sandbox's Store is of an
/// older schema version this build does not open (#776): `sandbox remove`
/// of a Sandbox of such a Store (one its machine's forced stop ended, say)
/// would otherwise be refused by the Store it does not need, with the
/// removal of the whole Store as its fix. Any other call, and any other
/// open refusal, keeps the refusal.
#[test]
fn a_close_goes_on_without_an_old_format_store() {
    use layerfs_home::outcome::{refused, with_fix};
    use layerfs_sandbox::host::Op;
    use layerfs_sandbox::route::opened_for;
    let version = "SQLite Store schema version 2 (before each Commit recorded its attempt, \
#776): not supported by this release and not migrated; create a new Store";
    let old = || {
        with_fix(
            refused(layerfs_store::state::OLD_STORE, version),
            "layerfs store remove shop",
        )
    };
    assert_eq!(opened_for(Op::SandboxClose, Err(old())).unwrap(), None);
    let path = std::path::PathBuf::from("/s/shop.store.sqlite");
    assert_eq!(
        opened_for(Op::SandboxClose, Ok(path.clone())).unwrap(),
        Some(path)
    );
    let refused_for = |op| opened_for(op, Err(old())).unwrap_err().code;
    assert_eq!(refused_for(Op::SandboxCreate), "OldStore");
    assert_eq!(refused_for(Op::Status), "OldStore");
    let busy = opened_for(Op::SandboxClose, Err(refused("Busy", "held")));
    assert_eq!(busy.unwrap_err().code, "Busy");
}
