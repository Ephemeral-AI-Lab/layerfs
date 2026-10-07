//! Explicit control operations with short local admission and original receipts.
use super::{
    registry::{bound_mut, Binding, Entry},
    Failure, Service, Success,
};
use crate::{
    store::{BindPhase, BindRequest, CommitError},
    Command, OwnerError, Response,
};
use layerfs_bridge::control::{
    Activity, ControlCode, Reply, Request, WorkspaceToken, HISTORY_WINDOW,
};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{BranchId, BranchSnapshot, WorkspaceId};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use std::sync::Arc;
impl Service {
    /// Runs one explicit command. Commit forwards the normal caller's Content
    /// producer over the original capture; S10 supplies live normalization.
    pub fn execute(
        &self,
        request: &Request,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
    ) -> Result<Success, Failure> {
        match request {
            Request::Mount { workspace, branch } => self.mount(*workspace, *branch),
            Request::Status(token) => self.status(*token),
            Request::Commit(token) => self.commit(*token, construct),
            Request::Unmount(token) => self.unmount(*token),
            Request::Fork(request) => self
                .store
                .history()
                .fork(request)
                .map(|snapshot| Success::reply(Reply::Forked(snapshot)))
                .map_err(Failure::History),
            Request::History(request) => {
                if request.limit == 0 || request.limit > HISTORY_WINDOW {
                    return Err(Failure::Rejected(
                        ControlCode::Invalid,
                        "history processing window",
                    ));
                }
                self.store
                    .history()
                    .commit_history(request)
                    .map(|page| Success::reply(Reply::History(page)))
                    .map_err(Failure::History)
            }
        }
    }
    fn mount(&self, workspace: WorkspaceId, branch: BranchId) -> Result<Success, Failure> {
        {
            let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            if entries.contains_key(&workspace) {
                return Err(Failure::Rejected(
                    ControlCode::Busy,
                    "Workspace incarnation already admitted",
                ));
            }
            if entries.len() == self.capacity {
                return Err(Failure::Rejected(
                    ControlCode::Capacity,
                    "daemon Workspace capacity",
                ));
            }
            entries.insert(workspace, Entry::Binding);
        }
        let bound = match self
            .store
            .bind(self.owner.clone(), BindRequest { workspace, branch })
        {
            Ok(value) => value,
            Err(value) => {
                let phase = value.phase;
                let failure = Failure::Bind(Box::new(value));
                if phase != BindPhase::Open || !failure.uncertain() {
                    // The original bind proves no local ownership was created.
                    // If the registry is poisoned, its retained entry stays put.
                    if let Ok(mut entries) = self.entries.lock() {
                        entries.remove(&workspace);
                    }
                }
                return Err(failure);
            }
        };
        let token = WorkspaceToken {
            workspace,
            namespace: bound.workspace.route().namespace(),
        };
        // This newly returned binding has never been shared with another caller.
        let snapshot = bound
            .workspace
            .snapshot()
            .expect("new unshared binding metadata");
        let success = Success {
            reply: Reply::Bound {
                token,
                binding: snapshot.clone(),
            },
            completion: Some(bound.open),
            commit: None,
            observation_failure: None,
        };
        let installed = self.entries.lock().map(|mut entries| {
            entries.insert(
                workspace,
                Entry::Bound(Box::new(Binding {
                    workspace: Arc::new(bound.workspace),
                    snapshot,
                    activity: Activity::Idle,
                    epoch: 1,
                    published: None,
                })),
            )
        });
        if installed.is_err() {
            return Err(Failure::After {
                cause: Box::new(Failure::Poisoned),
                original: Box::new(success),
            });
        }
        Ok(success)
    }
    fn commit(
        &self,
        token: WorkspaceToken,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
    ) -> Result<Success, Failure> {
        let workspace = self.admit(token, Activity::Committing)?;
        let result = workspace.commit(construct);
        let update = (|| {
            let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let binding = bound_mut(&mut entries, token)?;
            binding.epoch = binding.epoch.saturating_add(1);
            match &result {
                Ok(_) => {
                    binding.snapshot = workspace.snapshot().map_err(Failure::Workspace)?;
                    binding.activity = Activity::Idle;
                    binding.published = None;
                }
                Err(failed) => {
                    binding.published = failed.published.clone();
                    binding.activity = if failed.locally_settled {
                        Activity::Idle
                    } else if failed.published.is_some() {
                        Activity::LocalFailure
                    } else {
                        Activity::Uncertain
                    };
                }
            }
            Ok(())
        })();
        match result {
            Ok(commit) => {
                let success = Success {
                    reply: Reply::Committed(commit.history.clone()),
                    completion: None,
                    commit: Some(commit),
                    observation_failure: None,
                };
                if let Err(cause) = update {
                    return Err(Failure::After {
                        cause: Box::new(cause),
                        original: Box::new(success),
                    });
                }
                Ok(success)
            }
            Err(failed) => Err(Failure::Commit(failed)),
        }
    }
    fn unmount(&self, token: WorkspaceToken) -> Result<Success, Failure> {
        let workspace = self.admit(token, Activity::Closing)?;
        let result = self.job(workspace.route(), Command::Close);
        let completion = match result {
            Ok(done) if matches!(done.result(), Ok(Response::Done)) => done,
            other => {
                let failure = match other {
                    Ok(done) => Failure::Completion(Box::new(done)),
                    Err(error) => error,
                };
                if let Ok(mut entries) = self.entries.lock() {
                    if let Ok(binding) = bound_mut(&mut entries, token) {
                        binding.activity = if failure.uncertain() {
                            Activity::Uncertain
                        } else {
                            Activity::Idle
                        };
                        binding.epoch = binding.epoch.saturating_add(1);
                    }
                }
                return Err(failure);
            }
        };
        let success = Success {
            reply: Reply::Unmounted(token),
            completion: Some(completion),
            commit: None,
            observation_failure: None,
        };
        if self
            .entries
            .lock()
            .map(|mut entries| entries.remove(&token.workspace))
            .is_err()
        {
            return Err(Failure::After {
                cause: Box::new(Failure::Poisoned),
                original: Box::new(success),
            });
        }
        Ok(success)
    }
    pub(super) fn job(
        &self,
        route: layerfs_overlay::Route,
        command: Command,
    ) -> Result<crate::Completion, Failure> {
        self.owner
            .try_submit(Some(route), command)
            .map_err(|(cause, command)| {
                Failure::Owner(OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                })
            })?
            .wait()
            .map_err(Failure::Owner)
    }
}
