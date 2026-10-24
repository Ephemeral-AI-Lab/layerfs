//! Admission to the daemon's backend method table (#428): its Workspaces
//! (`backend.daemon.workspace.*`) and the read model that lists them
//! (`sandbox.list_workspaces`). Backend Store calls are unknown here.
use crate::control::Service;
use layerfs_bridge::control::{Reply, SdkCall, SdkRefusal, SdkValue};

/// Whether `method` is on this table; any other SDK name is a Store call.
pub(crate) fn admits(method: &str) -> bool {
    method.starts_with("backend.") || method == "sandbox.list_workspaces"
}

pub(crate) fn dispatch(service: &Service, call: &SdkCall) -> Reply {
    let answer = match call.method.as_str() {
        "backend.daemon.workspace.get_storage" => super::workspace::overlay(service, &call.args),
        "backend.daemon.workspace.list_changes" => super::workspace::changes(service, &call.args),
        "sandbox.list_workspaces" => return service.inspect(call),
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
