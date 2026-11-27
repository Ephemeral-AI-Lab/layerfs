//! Daemon backend method admission (#428).
use crate::{internal::Control, SdkError, SdkResult};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};

/// Calls one daemon backend method: the read model that lists its
/// Workspaces (`sandbox.list_workspaces`), the operator view of control
/// admission and retained connection failures
/// (`backend.daemon.connection_failures`, #540), or the operator reclaim
/// (`backend.daemon.reclaim`, #547 slice 5). Any other method is refused
/// before it is sent.
/// The daemon's bounded changed-path read of one Workspace.
pub const CHANGED: &str = "backend.daemon.workspace.list_changed";

pub fn call(control: &mut Control, method: &str, args: SdkValue) -> SdkResult {
    match method {
        // The daemon's control admission and bounded retained-failure store
        // (#540): an operator read model, never a product operation.
        layerfs_protocol::daemon::OPERATION => {
            crate::route::daemon::call::exchange_call(control, method, args)
        }
        // One generation-checked, one-shot reclaim of a named worker (#547
        // slice 5): never restarts the daemon and never touches Workspaces.
        layerfs_protocol::daemon::RECLAIM => {
            crate::route::daemon::call::exchange_call(control, method, args)
        }
        "sandbox.list_workspaces" => {
            crate::route::daemon::call::exchange_call(control, method, args)
        }
        // a Workspace's changes from one bounded overlay read (call surface
        // 2.0 `workspace.status`, and a Dirty unmount's paths)
        CHANGED => crate::route::daemon::call::exchange_call(control, method, args),
        _ => Err(SdkError::Refused(SdkRefusal {
            code: "UnknownMethod".into(),
            message: format!("no daemon backend method {method}"),
            detail: SdkValue::Null,
        })),
    }
}
