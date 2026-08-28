//! Compatibility direct-presentation facade. P0 still records commits in Working storage.

use crate::working::portable_metadata;
use crate::{BranchHead, CommitResult, LayerFs, OperationId, Result};
use layerfs_core::inode::InodeKind;
use layerfs_core::logical;
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_working_store::BeginOperation;
use layerfs_workspace::{
    DirectDriver, EndOperationReceipt, FinalizedCandidate, OperationWorkspace, Presentation,
    WorkspaceTicket,
};
use std::io::Read;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationCommitTimers {
    pub quiescence_ns: u128,
    pub candidate_ns: u128,
    pub working_commit_ns: u128,
    pub working_recorded_ns: u128,
    pub cleanup_ns: u128,
    pub complete_wall_ns: u128,
    pub equation_closed: bool,
}

impl LayerFs {
    pub fn begin_direct(&self, expected: BranchHead) -> Result<DirectOperation<'_>> {
        let (admission, ticket) =
            layerfs_workspace::begin_operation(&self.working, expected, Presentation::Direct)?;
        let (workspace, _) = OperationWorkspace::start(ticket, DirectDriver::default(), None)?;
        Ok(DirectOperation {
            fs: self,
            admission,
            workspace,
            candidate_root: expected.root,
            terminal: false,
        })
    }
}

pub struct DirectOperation<'a> {
    fs: &'a LayerFs,
    admission: BeginOperation,
    workspace: OperationWorkspace<DirectDriver>,
    candidate_root: ObjectId,
    terminal: bool,
}

pub struct DirectCommitReceipt {
    pub operation_id: OperationId,
    pub candidate_root: ObjectId,
    pub outcome: CommitResult,
    pub cleanup: layerfs_workspace::Result<EndOperationReceipt>,
    pub acknowledgement: Option<layerfs_working_store::Result<bool>>,
    pub timers: OperationCommitTimers,
}

impl DirectOperation<'_> {
    fn apply_candidate(
        &mut self,
        candidate: layerfs_working_store::WorkingTrustedCandidate,
        writer: layerfs_working_store::WorkingCandidateWrite<'_>,
    ) -> Result<logical::LogicalCounters> {
        let root = candidate.root();
        let counters = candidate.counters();
        writer.commit_trusted_operation_candidate(self.admission.operation_id, candidate)?;
        self.candidate_root = root;
        Ok(counters)
    }

    pub fn replace_file(
        &mut self,
        path: &str,
        input: impl Read,
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::RegularFile)?;
        let candidate = writer.trusted_replace_file(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            input,
            (inode, metadata),
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn replace_range(
        &mut self,
        path: &str,
        start: u64,
        delete_len: u64,
        replacement: impl Read,
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate = writer.trusted_replace_range(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            replacement,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn create_directory(&mut self, path: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::Directory)?;
        let candidate = writer.trusted_create_directory(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            inode,
            metadata,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn create_symlink(
        &mut self,
        path: &str,
        target: &[u8],
    ) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let inode = writer.allocate_inode_id()?;
        let metadata = portable_metadata(&mut writer, InodeKind::Symlink)?;
        let candidate = writer.trusted_create_symlink(
            self.candidate_root,
            &CanonicalPath::new(path)?,
            inode,
            target.to_vec(),
            metadata,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn hard_link(&mut self, source: &str, target: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate = writer.trusted_hard_link(
            self.candidate_root,
            &CanonicalPath::new(source)?,
            &CanonicalPath::new(target)?,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let from = CanonicalPath::new(from)?;
        let to = CanonicalPath::new(to)?;
        let mut counters = logical::LogicalCounters::default();
        let (source_parent, _) =
            logical::resolve_parent(&writer, self.candidate_root, &from, &mut counters)?;
        let (target_parent, _) =
            logical::resolve_parent(&writer, self.candidate_root, &to, &mut counters)?;
        let candidate = writer.trusted_rename(
            self.candidate_root,
            &from,
            &to,
            source_parent.record.metadata_root,
            target_parent.record.metadata_root,
        )?;
        self.apply_candidate(candidate, writer)
    }

    pub fn remove(&mut self, path: &str) -> Result<logical::LogicalCounters> {
        let mut writer = self.fs.working.begin_candidate_write()?;
        let candidate =
            writer.trusted_remove_path(self.candidate_root, &CanonicalPath::new(path)?)?;
        self.apply_candidate(candidate, writer)
    }

    pub fn commit(mut self) -> Result<DirectCommitReceipt> {
        let complete = Instant::now();
        let freeze = self.workspace.freeze_observed(Duration::from_secs(30))?;
        let candidate_started = Instant::now();
        let finalized: FinalizedCandidate = self.workspace.finalize_candidate(
            self.admission.base.root(),
            self.candidate_root,
            Vec::new(),
        )?;
        let candidate_ns = freeze.driver_freeze_ns + candidate_started.elapsed().as_nanos();
        let working_commit = Instant::now();
        let result = self
            .fs
            .working
            .operation_commit(self.admission, finalized.into_working());
        let working_commit_ns = working_commit.elapsed().as_nanos();
        match result {
            Ok(outcome) => {
                self.terminal = true;
                let cleanup_started = Instant::now();
                let cleanup = self.workspace.cleanup();
                let cleanup_ns = cleanup_started.elapsed().as_nanos();
                let acknowledgement = if cleanup.is_ok() {
                    match outcome {
                        CommitResult::WorkingRecorded { record, .. } => {
                            Some(self.fs.working.acknowledge_operation(record))
                        }
                        CommitResult::Conflict { candidate, .. } => {
                            Some(self.fs.working.acknowledge_conflict(candidate))
                        }
                    }
                } else {
                    None
                };
                Ok(DirectCommitReceipt {
                    operation_id: self.admission.operation_id,
                    candidate_root: self.candidate_root,
                    outcome,
                    cleanup,
                    acknowledgement,
                    timers: commit_timers(
                        complete,
                        freeze.quiescence_ns,
                        candidate_ns,
                        working_commit_ns,
                        cleanup_ns,
                    ),
                })
            }
            Err(error) => {
                if self.workspace.discard().is_ok() {
                    self.terminal = self
                        .fs
                        .working
                        .discard_operation(self.admission.operation_id)
                        .is_ok();
                }
                Err(error.into())
            }
        }
    }

    pub fn discard(mut self) -> Result<()> {
        self.workspace.discard()?;
        self.fs
            .working
            .discard_operation(self.admission.operation_id)?;
        self.terminal = true;
        Ok(())
    }

    pub fn candidate_root(&self) -> ObjectId {
        self.candidate_root
    }

    pub fn operation_id(&self) -> OperationId {
        self.admission.operation_id
    }

    pub fn ticket(&self) -> WorkspaceTicket {
        WorkspaceTicket::from_admission(&self.admission, Presentation::Direct)
    }
}

impl Drop for DirectOperation<'_> {
    fn drop(&mut self) {
        if !self.terminal
            && self.workspace.discard().is_ok()
            && self
                .fs
                .working
                .discard_operation(self.admission.operation_id)
                .is_ok()
        {
            self.terminal = true;
        }
    }
}

pub(crate) fn commit_timers(
    complete: Instant,
    quiescence_ns: u128,
    candidate_ns: u128,
    working_commit_ns: u128,
    cleanup_ns: u128,
) -> OperationCommitTimers {
    let working_recorded_ns = quiescence_ns + candidate_ns + working_commit_ns;
    OperationCommitTimers {
        quiescence_ns,
        candidate_ns,
        working_commit_ns,
        working_recorded_ns,
        cleanup_ns,
        complete_wall_ns: complete.elapsed().as_nanos(),
        equation_closed: working_recorded_ns == quiescence_ns + candidate_ns + working_commit_ns,
    }
}
