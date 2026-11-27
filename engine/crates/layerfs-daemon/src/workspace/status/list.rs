//! The operator read model's Workspace half: this daemon's bound Workspaces,
//! copied from the registry, with one engine observation each.
use super::super::{registry::Entry, Service};
use crate::operator::{self, WorkspaceView};
use layerfs_protocol::daemon::{Activity, MountPhase, Reply};
use layerfs_protocol::sdk::{SdkCall, SdkRefusal, SdkValue};

pub(super) fn activity(value: Activity) -> &'static str {
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
    /// `inspect.snapshot`: history from the Store, Workspaces from here. A
    /// caller that sends `history: false` gets the Workspaces alone, with no
    /// history read (layerfs-project#181); a daemon from before that option
    /// ignores it and answers the whole model, which holds the same list.
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
                // An observation: it leaves queued maintenance queued (#679).
                let observed = self
                    .job(route, |client, route| client.observe_state(route))
                    .ok();
                let pending = matches!(workspace.pending_conflicts(), Ok(Some(_)));
                WorkspaceView {
                    workspace: layerfs_store_service::lookup::hex(&workspace.identity().to_bytes()),
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
        let answer = match call.args.get("history") {
            Some(SdkValue::Bool(false)) => operator::workspaces(&self.store, &call.args, &views),
            _ => operator::snapshot(&self.store, &call.args, &views),
        };
        match answer {
            Ok(value) => Reply::Sdk(value),
            Err(refusal) => Reply::SdkRefused(refusal),
        }
    }
}
