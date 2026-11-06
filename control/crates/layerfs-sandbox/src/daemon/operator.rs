//! Daemon backend method admission (#428).
use crate::{internal::Control, SdkError, SdkResult};
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// Calls the daemon's read model that lists its Workspaces
/// (`sandbox.list_workspaces`), the one daemon backend method the host
/// Sandbox code sends; any other is refused before it is sent.
pub fn call(control: &mut Control, method: &str, args: SdkValue) -> SdkResult {
    if method != "sandbox.list_workspaces" {
        return Err(SdkError::Refused(SdkRefusal {
            code: "UnknownMethod".into(),
            message: format!("no daemon backend method {method}"),
            detail: SdkValue::Null,
        }));
    }
    crate::daemon::call::exchange_call(control, method, args)
}
