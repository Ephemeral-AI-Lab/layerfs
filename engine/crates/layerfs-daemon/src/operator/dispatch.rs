//! Admission to the daemon's backend method table (#428): its Workspaces
//! (`backend.daemon.workspace.*`) and the read model that lists them
//! (`sandbox.list_workspaces`). Backend Store calls are unknown here.
use crate::workspace::Service;
use layerfs_protocol::daemon::Reply;
use layerfs_protocol::sdk::{SdkCall, SdkRefusal, SdkValue};

/// Whether `method` is on this table; any other SDK name is a Store call.
pub(crate) fn admits(method: &str) -> bool {
    method.starts_with("backend.")
        || matches!(
            method,
            "sandbox.list_workspaces" | "sandbox.doctor_workspaces"
        )
}

pub(crate) fn dispatch(service: &Service, call: &SdkCall) -> Reply {
    let answer = match call.method.as_str() {
        "backend.daemon.workspace.get_storage" => super::list_changes::overlay(service, &call.args),
        "backend.daemon.workspace.list_changes" => {
            super::list_changes::changes(service, &call.args)
        }
        "backend.daemon.workspace.list_changed" => {
            super::list_changes::changed(service, &call.args)
        }
        "sandbox.list_workspaces" => return service.inspect(call),
        "sandbox.doctor_workspaces" => return service.diagnostic_workspaces(call),
        method => Err(SdkRefusal {
            code: "UnknownMethod".into(),
            message: format!("no daemon backend method {method}"),
            detail: SdkValue::Null,
        }),
    };
    match answer {
        Ok(value) => Reply::Sdk(value),
        Err(error) => Reply::SdkRefused(error),
    }
}
