//! Explicit staging; file saves, filesystem construction and C5 acknowledgement stay distinct.
use super::{lower::Dirty, source::ReplacementSource};
use crate::{
    overlay::snapshot::{SavedInode, Submission},
    *,
};
use layerfs_bridge::contract::{
    HistoryCommand, HistoryResult, Operation, PreparedChanges, Response, StageWire,
    HISTORY_RESULT_BYTES, MAX_OPERATION_MS,
};
use std::time::Instant;
impl Workspace {
    pub fn stage(&self, deadline: Instant) -> Result<StageSelector, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(WorkspaceError::Deadline)?;
        if remaining.as_millis() == 0 || remaining.as_millis() > u128::from(MAX_OPERATION_MS) {
            return Err(WorkspaceError::InvalidInput);
        }
        let _operation = self.begin(false, deadline)?;
        let submission = self.capture_submission(false, deadline)?;
        match self.stage_captured(&submission, deadline) {
            Ok(selector) => Ok(selector),
            Err(error) => Err(submission.fail(error)),
        }
    }
    pub(crate) fn prepare_changes(
        &self,
        submission: &Submission,
        deadline: Instant,
        first_remote: &mut Option<crate::runtime::state::OperationGuard>,
    ) -> Result<PreparedChanges, WorkspaceError> {
        let captured = submission.capture()?;
        let mut after = 0;
        let mut count = 0;
        submission.phase(StagePhase::LocalBookkeeping, None)?;
        while let Some((serial, inode)) = self.next_dirty(submission, after, deadline)? {
            if count == captured.count {
                return Err(WorkspaceError::Io);
            }
            let Dirty::Inode(inode) = inode else {
                // A maintained directory needs no content save, and an identity
                // this generation created without a name needs no declaration.
                after = serial;
                count += 1;
                continue;
            };
            submission.phase(StagePhase::LocalBookkeeping, Some(serial))?;
            let kind = if inode.symlink { 3 } else { 1 };
            let content = if inode.symlink {
                submission.phase(StagePhase::FileSave, Some(serial))?;
                let remote = first_remote
                    .take()
                    .map_or_else(|| self.begin(true, deadline), Ok)?;
                let target = self
                    .symlink_target(&captured.root, inode, deadline)
                    .inspect_err(|failure| {
                        if let Ok(mut state) = submission.state.lock() {
                            state.source_failure = Some(failure.clone());
                        }
                    })?;
                let response = self.remote_call(
                    (self.inner.store, captured.generation),
                    Operation::ConstructSymlink { target },
                    &mut &[][..],
                    0,
                    &mut std::io::sink(),
                    deadline,
                );
                drop(remote);
                let Response::Saved { root, length, .. } = response? else {
                    return Err(WorkspaceError::InvalidInput);
                };
                if length != inode.length {
                    return Err(WorkspaceError::InvalidInput);
                }
                submission
                    .state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .status
                    .saved_files += 1;
                (root, None)
            } else {
                let plan = self.lower_file(submission, inode, deadline)?;
                if plan.edits.is_empty() && !inode.fresh {
                    (inode.base, None)
                } else {
                    submission.phase(StagePhase::FileSave, Some(serial))?;
                    let mut source =
                        ReplacementSource::new(self.clone(), captured.root.clone(), plan.inode);
                    let remote = first_remote
                        .take()
                        .map_or_else(|| self.begin(true, deadline), Ok)?;
                    // One dirty file is one publication: a published file
                    // carries its portable fields in the same request, so the
                    // route pays one crossing instead of two. A fresh file has
                    // no base root to patch, so its metadata stays separate.
                    let response = self.remote_call(
                        (self.inner.store, captured.generation),
                        if inode.fresh {
                            Operation::ConstructFile {
                                length: inode.length,
                            }
                        } else {
                            Operation::EditFileWithMetadata {
                                root: inode.base,
                                base_length: inode.base_length,
                                edits: plan.edits,
                                kind,
                                mode: inode.mode,
                                mtime_seconds: inode.seconds,
                                mtime_nanoseconds: inode.nanos,
                            }
                        },
                        &mut source,
                        0,
                        &mut std::io::sink(),
                        deadline,
                    );
                    drop(remote);
                    if let Some(failure) = source.failure.take() {
                        submission
                            .state
                            .lock()
                            .map_err(|_| WorkspaceError::Io)?
                            .source_failure = Some(failure);
                    }
                    let Response::Saved {
                        root,
                        length,
                        metadata,
                        ..
                    } = response?
                    else {
                        return Err(WorkspaceError::InvalidInput);
                    };
                    if length != inode.length || !source.complete() {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    if metadata.is_some() == inode.fresh {
                        return Err(WorkspaceError::InvalidInput);
                    }
                    submission
                        .state
                        .lock()
                        .map_err(|_| WorkspaceError::Io)?
                        .status
                        .saved_files += 1;
                    (root, metadata)
                }
            };
            let (content, merged) = content;
            {
                let mut state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.pending = Some(SavedInode {
                    serial,
                    revision: inode.revision,
                    length: inode.length,
                    content,
                    metadata: None,
                });
            }
            if let Some(metadata) = merged {
                // The merged save already stamped this inode's portable fields
                // under the same save owner, so no second request is needed.
                let mut state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.pending.as_mut().ok_or(WorkspaceError::Io)?.metadata = Some(metadata);
                state.status.saved_metadata += 1;
                submission.phase(StagePhase::LocalBookkeeping, Some(serial))?;
                self.persist_saved(submission, deadline)?;
                submission.phase(StagePhase::LocalBookkeeping, None)?;
                after = serial;
                count += 1;
                continue;
            }
            submission.phase(StagePhase::MetadataSave, Some(serial))?;
            let remote = first_remote
                .take()
                .map_or_else(|| self.begin(true, deadline), Ok)?;
            let response = self.remote_call(
                (self.inner.store, captured.generation),
                if inode.fresh {
                    Operation::ConstructPortableMetadata {
                        kind,
                        mode: inode.mode,
                        mtime_seconds: inode.seconds,
                        mtime_nanoseconds: inode.nanos,
                    }
                } else {
                    Operation::UpdatePortableMetadata {
                        base: inode.metadata,
                        kind,
                        mode: inode.mode,
                        mtime_seconds: inode.seconds,
                        mtime_nanoseconds: inode.nanos,
                    }
                },
                &mut &[][..],
                0,
                &mut std::io::sink(),
                deadline,
            );
            drop(remote);
            let response = response?;
            if inode.fresh {
                response.validate_metadata_constructed()?;
            } else {
                response.validate_metadata_saved()?;
            }
            let (actual_kind, mode, mtime_seconds, mtime_nanoseconds, metadata) = match response {
                Response::MetadataConstructed {
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    metadata,
                    ..
                } if inode.fresh => (kind, mode, mtime_seconds, mtime_nanoseconds, metadata),
                Response::MetadataSaved {
                    base,
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    metadata,
                    ..
                } if !inode.fresh && base == inode.metadata => {
                    (kind, mode, mtime_seconds, mtime_nanoseconds, metadata)
                }
                _ => return Err(WorkspaceError::InvalidInput),
            };
            if actual_kind != kind
                || mode != inode.mode
                || mtime_seconds != inode.seconds
                || mtime_nanoseconds != inode.nanos
            {
                return Err(WorkspaceError::InvalidInput);
            }
            {
                let mut state = submission.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.pending.as_mut().ok_or(WorkspaceError::Io)?.metadata = Some(metadata);
                state.status.saved_metadata += 1;
            }
            submission.phase(StagePhase::LocalBookkeeping, Some(serial))?;
            self.persist_saved(submission, deadline)?;
            submission.phase(StagePhase::LocalBookkeeping, None)?;
            after = serial;
            count += 1;
        }
        if count != captured.count {
            return Err(WorkspaceError::Io);
        }
        if first_remote.is_none() {
            *first_remote = Some(self.begin(true, deadline)?);
        }
        let (inodes, new_file_serials, new_symlink_serials) =
            self.prepared_inodes(submission, deadline)?;
        let (directories, new_directories, directory_metadata) =
            self.prepared_directories(submission, deadline)?;
        let context = &captured.context;
        let changes = PreparedChanges {
            new_file_serials,
            new_symlink_serials,
            directory_metadata,
            new_directories,
            workspace: self.inner.incarnation,
            branch: context.branch.branch,
            expected_head: context.branch.head_commit,
            expected_base: context.branch.base_layer,
            generation: captured.generation,
            base: context.effective_root,
            scope: context.scope,
            root_serial: context.root_serial.ok_or(WorkspaceError::Io)?,
            directories,
            inodes,
        };
        let header = if changes.expected_head.is_some() {
            228
        } else {
            195
        };
        let expected = header
            + 73 * changes.inodes.len()
            + 34 * captured.directories
            + captured.name_bytes
            + if captured.fresh_symlinks > 0 {
                9 + 8 * (captured.fresh_files + captured.fresh_symlinks)
            } else if captured.fresh_files > 0 {
                7 + 8 * captured.fresh_files
            } else {
                usize::from(captured.directories > 0) * 5
            };
        if expected > layerfs_bridge::contract::METADATA_BYTES {
            return Err(WorkspaceError::Io);
        }
        Ok(changes)
    }
    fn stage_captured(
        &self,
        submission: &Submission,
        deadline: Instant,
    ) -> Result<StageSelector, WorkspaceError> {
        let mut first_remote = None;
        let changes = self.prepare_changes(submission, deadline, &mut first_remote)?;
        let captured = submission.capture()?;
        submission.phase(StagePhase::StageChanges, None)?;
        let remote = first_remote.take().ok_or(WorkspaceError::Io)?;
        let response = self.remote_call(
            (self.inner.store, captured.generation),
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes)),
            &mut &[][..],
            HISTORY_RESULT_BYTES as u64,
            &mut std::io::sink(),
            deadline,
        );
        drop(remote);
        let response = response?;
        let Response::History(result) = response else {
            return Err(WorkspaceError::InvalidInput);
        };
        let HistoryResult::Stage(stage) = *result else {
            return Err(WorkspaceError::InvalidInput);
        };
        submission
            .observed_stage
            .set(stage.clone())
            .map_err(|_| WorkspaceError::Io)?;
        validate_stage(submission, self.inner.incarnation, &stage)?;
        submission.acknowledged(stage)
    }
}
pub(crate) fn validate_stage(
    submission: &Submission,
    incarnation: [u8; 32],
    stage: &StageWire,
) -> Result<(), WorkspaceError> {
    let captured = submission.capture()?;
    let context = &captured.context;
    if stage.workspace != incarnation
        || stage.token == 0
        || stage.stack != context.branch.stack
        || stage.branch != context.branch.branch
        || stage.expected_head != context.branch.head_commit
        || stage.expected_base != context.branch.base_layer
        || stage.expected_root != context.effective_root
        || stage.construction_base_root != context.effective_root
        || stage.intended_commit_base != context.branch.base_layer
        || stage.profile != context.profile
        || stage.scope != context.scope
        || stage.generation != captured.generation
    {
        return Err(WorkspaceError::InvalidInput);
    }
    Ok(())
}
