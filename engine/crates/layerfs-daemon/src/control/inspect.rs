//! The operator read model's Workspace half: this daemon's bound Workspaces,
//! copied from the registry, with one engine observation each.
use super::{registry::Entry, Service};
use crate::operator::{self, WorkspaceView};
use layerfs_bridge::control::{Activity, MountPhase, Reply, SdkCall, SdkRefusal, SdkValue};

fn activity(value: Activity) -> &'static str {
    match value {
        Activity::Idle => "idle",
        Activity::Committing => "committing",
        Activity::Closing => "closing",
        Activity::Uncertain => "uncertain",
        Activity::LocalFailure => "failed",
        Activity::Attaching => "attaching",
    }
}

impl Service {
    /// `inspect.snapshot`: history from the Store, Workspaces from here.
    pub(crate) fn inspect(&self, call: &SdkCall) -> Reply {
        let copies = match self.entries.lock() {
            Ok(entries) => entries
                .values()
                .filter_map(|entry| match entry {
                    Entry::Bound(value) => Some((
                        value.workspace.clone(),
                        value.snapshot.clone(),
                        value.activity,
                        value.kernel.block(),
                    )),
                    Entry::Binding => None,
                })
                .collect::<Vec<_>>(),
            Err(_) => {
                return Reply::SdkRefused(SdkRefusal {
                    code: "Failed".to_owned(),
                    message: "control registry poisoned".to_owned(),
                    detail: SdkValue::Null,
                })
            }
        };
        let views = copies
            .into_iter()
            .map(|(workspace, binding, state, kernel)| {
                let route = workspace.route();
                let observed = self.job(route, |client, route| client.state(route)).ok();
                let pending = matches!(workspace.pending_conflicts(), Ok(Some(_)));
                WorkspaceView {
                    workspace: layerfs_store_service::history::lookup::hex(
                        &workspace.identity().to_bytes(),
                    ),
                    namespace: route.namespace(),
                    activity: activity(state),
                    mounted: kernel.phase == MountPhase::Ready,
                    directory: kernel.ready.map(|ready| ready.directory),
                    revision: observed.map(|s| s.revision),
                    dirty: observed.map(|s| (s.dirty_inodes, s.dirty_directory_entries)),
                    pending_merge: pending
                        || observed
                            .is_some_and(|s| s.base_root != *binding.effective_root.as_bytes()),
                    binding,
                }
            })
            .collect::<Vec<_>>();
        match operator::snapshot(&self.store, &call.args, &views) {
            Ok(value) => Reply::Sdk(value),
            Err(refusal) => Reply::SdkRefused(refusal),
        }
    }
}
