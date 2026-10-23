//! The backend Store call router (#428: `backend.store.*`).
use super::{files, snapshot, store};
use crate::StoreService;
use layerfs_bridge::control::{SdkCall, SdkRefusal, SdkValue};

/// Runs one backend Store call once. The read model's history half
/// (`backend.store.project.get_history`) is the snapshot with no Workspaces,
/// read from the Store alone; `sandbox.list_workspaces` adds a daemon's
/// Workspaces and is the daemon's call.
pub fn dispatch(service: &StoreService, call: &SdkCall) -> Result<SdkValue, SdkRefusal> {
    match call.method.as_str() {
        "backend.store.project.get_history" => snapshot(&service.store, &call.args, &[]),
        "backend.store.project.get_storage" => store::stats(&service.store),
        "backend.store.branch.list_commit_storage" => store::commits(&service.store, &call.args),
        "backend.store.project.list_overwritten" => store::overwritten(&service.store, &call.args),
        "backend.store.fs.get_storage" => files::stats(&service.store, &call.args),
        "backend.store.fs.list_revisions" => files::history(&service.store, &call.args),
        method => Err(SdkRefusal {
            code: "UnknownMethod".into(),
            message: format!("no backend Store method {method}"),
            detail: SdkValue::Null,
        }),
    }
}
