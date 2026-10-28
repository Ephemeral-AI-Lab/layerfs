//! Explicit control operations with short local admission and original receipts.
use super::{
    registry::{bound_mut, Binding, Entry},
    types::KernelFailure,
    Failure, Service, Success,
};
use crate::store::{
    BindPhase, BindRequest, BoundWorkspace, CapturedConstruction, CommitError, CommitFailure,
    CommitOptions, CommitSuccess,
};
use layerfs_bridge::control::{
    Activity, ConflictReport, ControlCode, MergedCommit, Reply, Request, WorkspaceToken,
};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::merge::{Conflict, ConflictKind};
use layerfs_history::{BranchId, BranchSnapshot, WorkspaceId};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use std::sync::Arc;
/// The reply of a Commit with a known result.
fn committed(commit: &CommitSuccess) -> Reply {
    match (&commit.history, &commit.merge) {
        (Some(outcome), None) => Reply::Committed(outcome.clone()),
        (Some(outcome), Some(merge)) => Reply::MergeCommitted(Box::new(MergedCommit {
            outcome: outcome.clone(),
            from: merge.from,
            head: merge.head,
            lww: merge.lww.clone(),
        })),
        (None, merge) => Reply::Conflict(Box::new(ConflictReport {
            head: merge.as_ref().and_then(|merge| merge.head),
            base: merge.as_ref().and_then(|merge| merge.from),
            markers: false,
            conflicts: merge
                .as_ref()
                .map(|m| m.conflicts.clone())
                .unwrap_or_default(),
            lww: merge.as_ref().map(|m| m.lww.clone()).unwrap_or_default(),
            written: merge
                .as_ref()
                .map(|m| m.written.clone())
                .unwrap_or_default(),
            truncated: merge.as_ref().is_some_and(|m| m.written_truncated),
        })),
    }
}
impl Service {
    /// Runs one explicit command with a caller-owned Content producer for
    /// Commit, over the original capture. The product route is
    /// `execute_control`, whose Commit uses the captured namespace producer.
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
            Request::Commit(control) => {
                self.commit(control.token, |workspace| workspace.commit(construct))
            }
            other => self.execute_control(other),
        }
    }
    /// Runs one control operation. Commit captures the shared published
    /// frontier and constructs it with the captured namespace producer on
    /// this caller's thread; retained original custody refuses it first.
    pub fn execute_control(&self, request: &Request) -> Result<Success, Failure> {
        match request {
            Request::Observed { .. } => Err(Failure::Rejected(
                ControlCode::Invalid,
                "observed control requires application diagnostic owner",
            )),
            Request::EndSession => Ok(Success::reply(Reply::SessionEnded)),
            Request::Sdk(call) if crate::operator::admits(&call.method) => {
                Ok(Success::reply(crate::operator::dispatch(self, call)))
            }
            // Store calls run in the SDK's process (#352 F-020): one route.
            // Bodies a refused call declared are drained by the connection.
            Request::Sdk(call) => Ok(Success::reply(Reply::SdkRefused(
                layerfs_bridge::control::SdkRefusal {
                    code: "UnknownMethod".into(),
                    message: format!(
                        "{}: Store calls are not served by the daemon; they run in the SDK's process",
                        call.method
                    ),
                    detail: layerfs_bridge::control::SdkValue::Null,
                },
            ))),
            Request::Hello(_) => Err(Failure::Rejected(
                ControlCode::Invalid,
                "daemon Hello requires application startup owner",
            )),
            Request::Mount { workspace, branch } => {
                crate::telemetry::timed("sandbox.mount.bind", |timing| {
                    self.mount(*workspace, *branch, timing)
                })
            }
            Request::Status(token) => self.status(*token),
            Request::Cleanup(token) => self.cleanup(*token),
            Request::Commit(control) => self.commit(control.token, |workspace| {
                workspace.commit_captured(&CommitOptions {
                    force: control.force,
                    message: control.message.clone(),
                })
            }),
            Request::Unmount(control) => self.unmount(*control),
            Request::ForceUnmount {
                token,
                relinquish_unknown,
            } => self.force_unmount(*token, *relinquish_unknown),
            Request::Attach(token) => crate::telemetry::timed("sandbox.mount.attach", |timing| {
                self.attach(*token, timing)
            }),
            Request::Locate(workspace) => self.locate(*workspace),
            // History calls are Store calls: they run in the SDK's process.
            Request::Fork(_) | Request::Branches { .. } | Request::History(_) => Err(
                Failure::Rejected(ControlCode::Invalid, "Store calls are not served by the daemon"),
            ),
        }
    }
    pub(crate) fn mount(
        &self,
        workspace: WorkspaceId,
        branch: BranchId,
        timing: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<Success, Failure> {
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
            // Stopped maintenance reclaims nothing further, so no new
            // Workspace is admitted. The retained first failure is read
            // here; no owner job is submitted and no entry is created.
            if let Some(stopped) = self.owner.maintenance_failure().map_err(Failure::Owner)? {
                return Err(Failure::Kernel(Box::new(KernelFailure {
                    code: ControlCode::Capacity,
                    phase: "mount:debt",
                    detail: stopped.to_string(),
                    evidence: Some(Box::new(stopped)),
                })));
            }
            entries.insert(workspace, Entry::Binding);
        }
        let bound = match crate::telemetry::phase(timing, "store.bind", || {
            self.store
                .bind(self.owner.clone(), BindRequest { workspace, branch })
        }) {
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
            commit: None,
            observation_failure: None,
            kernel: None,
        };
        let installed = self.entries.lock().map(|mut entries| {
            entries.insert(
                workspace,
                Entry::Bound(Box::new(Binding {
                    kernel: super::kernel::Kernel::Unattached,
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
        run: impl FnOnce(&BoundWorkspace) -> Result<CommitSuccess, Box<CommitFailure>>,
    ) -> Result<Success, Failure> {
        let workspace = self.admit(token, Activity::Committing)?;
        let result = run(&workspace);
        // An install that replaced what the Workspace showed is invalidated
        // in the kernel before the answer.
        let invalidated = match &result {
            Ok(CommitSuccess {
                replaced: Some((shown, installed)),
                ..
            }) => self.invalidate(token, &workspace, *shown, *installed),
            _ => Ok(()),
        };
        // A Commit whose reader or operation owner could not be released
        // keeps that custody, published or settled: nothing later is admitted
        // past it.
        let unreleased = match &result {
            Ok(commit) => &commit.namespace,
            Err(failed) => &failed.namespace,
        }
        .as_ref()
        .is_some_and(CapturedConstruction::retained);
        let update = (|| {
            let mut entries = self.entries.lock().map_err(|_| Failure::Poisoned)?;
            let binding = bound_mut(&mut entries, token)?;
            binding.epoch = binding.epoch.saturating_add(1);
            match &result {
                Ok(commit) => {
                    binding.snapshot = workspace.snapshot().map_err(Failure::Workspace)?;
                    binding.activity = if unreleased {
                        Activity::LocalFailure
                    } else {
                        Activity::Idle
                    };
                    binding.published = if unreleased {
                        commit.history.clone()
                    } else {
                        None
                    };
                }
                Err(failed) => {
                    binding.published = failed.published.clone();
                    binding.activity = if failed.locally_settled && !unreleased {
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
                let mut success = Success::reply(committed(&commit));
                success.commit = Some(commit);
                if let Err(cause) = update.and(invalidated) {
                    return Err(Failure::After {
                        cause: Box::new(cause),
                        original: Box::new(success),
                    });
                }
                if unreleased {
                    return Err(Failure::After {
                        cause: Box::new(Failure::Rejected(
                            ControlCode::Unknown,
                            "captured namespace custody retained after Commit",
                        )),
                        original: Box::new(success),
                    });
                }
                Ok(success)
            }
            Err(failed) => match (&failed.error, failed.locally_settled, update) {
                // A definite refusal for unresolved markers is this Commit's
                // answer, not a failure: nothing was published.
                (CommitError::Markers(paths), true, Ok(())) => {
                    Ok(Success::reply(Reply::Conflict(Box::new(ConflictReport {
                        head: failed.binding.as_ref().and_then(|b| b.branch.head_commit),
                        base: failed.binding.as_ref().and_then(|b| b.branch.head_commit),
                        markers: true,
                        conflicts: paths
                            .iter()
                            .map(|path| Conflict {
                                path: path.clone(),
                                kind: ConflictKind::Markers,
                                hunks: Vec::new(),
                            })
                            .collect(),
                        lww: Vec::new(),
                        written: Vec::new(),
                        truncated: false,
                    }))))
                }
                _ => Err(Failure::Commit(failed)),
            },
        }
    }
    pub(super) fn job<T>(
        &self,
        route: layerfs_workspace::Route,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<T, crate::OwnerError>
            + Send,
    ) -> Result<T, Failure> {
        command(&self.owner, route).map_err(Failure::Owner)
    }
}
