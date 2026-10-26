//! Daemon backend method admission (#428).
use crate::{internal::Control, SdkError, SdkResult};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Calls one daemon backend method: its Workspaces and the read model that
/// lists them. Backend Store calls run in the caller's process.
pub fn call(control: &mut Control, method: &str, args: SdkValue) -> SdkResult {
    if !matches!(
        method,
        "sandbox.list_workspaces"
            | "backend.daemon.workspace.get_storage"
            | "backend.daemon.workspace.list_changes"
    ) {
        return Err(SdkError::Refused(SdkRefusal {
            code: "UnknownMethod".into(),
            message: format!("no daemon backend method {method}"),
            detail: SdkValue::Null,
        }));
    }
    crate::base::sdk::exchange_call(control, method, args)
}
