//! One synchronous Content/Save/history producer and exact paired local install.
use super::{
    bind::checked_base, BoundWorkspace, CommitError, CommitFailure, CommitPhase, CommitSuccess,
};
use crate::{Command, Response};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::{BranchSnapshot, CommitStagedOutcome, StageRequest};
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use layerfs_workspace::WorkspaceError;
use std::sync::{atomic::Ordering, Arc};

impl BoundWorkspace {
    /// Captures once and drives one caller-owned Content producer over that
    /// exact context. The producer must represent the capture faithfully; live
    /// namespace normalization is a separate constructor, not an implicit Init.
    /// No ownership guard spans construction or provider I/O. On definite
    /// nonpublication one local resolution preserves the accepted view; unknown
    /// and known-publication/install failure retain original custody.
    pub fn commit(
        &self,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
    ) -> Result<CommitSuccess, Box<CommitFailure>> {
        if self
            .committing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Box::new(CommitFailure::new(
                CommitError::InFlight,
                self.identity,
                self.route(),
            )));
        }
        let mut retained = CommitFailure::new(
            CommitError::Context("incomplete Commit"),
            self.identity,
            self.route(),
        );
        let result = (|| {
            let snapshot = self.snapshot().map_err(CommitError::Workspace)?;
            retained.binding = Some(snapshot.clone());
            let original_base = self.workspace.base().map_err(CommitError::Workspace)?;
            retained.phase = CommitPhase::Capture;
            let captured = self.job(Command::Capture).map_err(CommitError::Owner)?;
            let capture = match captured.result() {
                Ok(Response::Captured(capture)) => *capture,
                _ => return Err(CommitError::Completion(Box::new(captured))),
            };
            retained.capture = Some(capture);
            retained.captured = Some(captured);
            if capture.base_root != snapshot.effective_root.to_bytes()
                || original_base.identity().0 != snapshot.effective_root
            {
                return Err(CommitError::Context(
                    "capture differs from bound history root",
                ));
            }
            retained.phase = CommitPhase::Begin;
            let storage = self.store.producer().map_err(CommitError::Storage)?;
            let built = (|| {
                let save = storage.begin_save().map_err(CommitError::Storage)?;
                retained.phase = CommitPhase::Construct;
                let candidate = match construct(&save, capture, &snapshot) {
                    Ok(root) => root,
                    Err(original) => {
                        return Err(match save.take_failure() {
                            Some(storage) => CommitError::Construction {
                                original: Box::new(original),
                                storage,
                            },
                            None => original,
                        })
                    }
                };
                retained.phase = CommitPhase::Finish;
                retained.saved = Some(save.finish().map_err(CommitError::Storage)?);
                Ok(candidate)
            })();
            retained.storage = Some(storage.diagnostics());
            let candidate = built?;
            retained.intent = Some(StageRequest {
                workspace: self.identity,
                branch: snapshot.branch.id,
                expected_head: snapshot.branch.head_commit,
                expected_base: snapshot.branch.base_layer,
                expected_root: snapshot.effective_root,
                construction_base_root: snapshot.effective_root,
                intended_commit_base: snapshot.branch.base_layer,
                candidate_root: candidate.0,
                profile: snapshot.profile,
                scope: snapshot.scope,
                generation: capture.generation.number() as u64,
            });
            retained.phase = CommitPhase::Validate;
            let ports = self.store.ports(snapshot.scope);
            let client = ports.client();
            let mut next = snapshot.clone();
            next.effective_root = candidate.0;
            let checked =
                checked_base(client.clone(), &next).map_err(|error| CommitError::Content {
                    error,
                    provider: ports
                        .failure()
                        .unwrap_or_else(|error| Some(Arc::new(error))),
                })?;
            if checked.root().root_inode() != original_base.root().root_inode() {
                return Err(CommitError::Context(
                    "candidate changes root inode identity",
                ));
            }
            let scoped = self
                .workspace
                .scoped(client)
                .map_err(CommitError::Workspace)?;
            retained.prepared = Some(
                scoped
                    .prepare_base_install(capture, candidate)
                    .map_err(CommitError::Workspace)?,
            );
            retained.phase = CommitPhase::Publish;
            let outcome = self
                .store
                .history
                .stage_and_commit(retained.intent.as_ref().expect("checked intent"))
                .map_err(CommitError::History)?;
            let (head, root) = match &outcome {
                CommitStagedOutcome::Committed(record) => (Some(record.id), record.root),
                CommitStagedOutcome::UpToDate { head, root } => (*head, *root),
            };
            retained.published = Some(outcome);
            if root != candidate.0 {
                return Err(CommitError::Context(
                    "publication differs from saved candidate",
                ));
            }
            next.branch.head_commit = head;
            next.head_root = head.map(|_| root);
            retained.phase = CommitPhase::Install;
            let installed = self
                .job(Command::InstallPrepared {
                    workspace: self.workspace.clone(),
                    input: retained.prepared.take().expect("prepared install"),
                })
                .map_err(CommitError::Owner)?;
            let installed_ok = matches!(installed.result(), Ok(Response::Done));
            if !installed_ok {
                return Err(CommitError::Completion(Box::new(installed)));
            }
            retained.local = Some(installed);
            *self
                .snapshot
                .lock()
                .map_err(|_| CommitError::Workspace(WorkspaceError::BindingPoisoned))? = next;
            Ok(())
        })();
        if let Err(error) = result {
            retained.error = error;
            return Err(self.settle_failure(retained));
        }
        self.committing.store(false, Ordering::Release);
        Ok(CommitSuccess {
            history: retained.published.expect("known publication"),
            saved: retained.saved.expect("known Save"),
            storage: retained.storage.expect("producer observations"),
            capture: retained.capture.expect("known capture"),
            captured: retained.captured.expect("original capture completion"),
            installed: retained.local.expect("known local install"),
        })
    }
}
