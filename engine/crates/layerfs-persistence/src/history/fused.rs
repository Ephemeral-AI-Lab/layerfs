//! One Workspace Commit as one write transaction (#768 stage 2): a Save's
//! held last publication, the binding hints of the new tree and the Commit
//! over an unmoved head, all or nothing.
//!
//! On PostgreSQL the transaction is one pipelined message, one round trip
//! (`backend/postgresql/history/fused.rs`). On SQLite it is one
//! `BEGIN IMMEDIATE` transaction under the Store's writer lock: the
//! publication, the hints and `commit_branch`, exactly as they run apart.
//!
//! A definite race refusal ([`Fused::Race`]: the Branch is no longer at the
//! head the request expects, or another writer stored one of the
//! publication's objects meanwhile, or the new Commit's identity exists)
//! writes nothing; its caller continues on the separate path, once. A
//! request the message cannot carry ([`Fused::Overflow`]) sends nothing.
//! Every other refusal is the Commit's own outcome: a fence, lane, epoch or
//! integrity refusal, or an unknown outcome, which quarantines the session
//! and keeps custody.
use super::{commit, hints, provider::HistoryProvider, transaction::Transaction};
use crate::backend::sqlite::session::transaction::Completion;
use crate::backend::Engine;
use layerfs_history::error::{HistoryError, HistoryResult};
use layerfs_history::identity::{LayerStackId, NodeId};
use layerfs_history::records::{BindingHint, CommitOutcome, CommitRequest};
use layerfs_storage::{policy, port::Publication};

/// Rows of a fused Commit besides the publication and the hint blocks: the
/// serial, the Commit, its lineage, the advance, the epoch and the lane.
pub const FIXED_ROWS: usize = 6;

/// Why a fused Commit was not made, when that is not the Commit's outcome.
#[derive(Debug)]
pub enum Fused {
    /// A definite race refusal; nothing was written. Continue on the
    /// separate path, once.
    Race,
    /// The request does not fit one message; nothing was sent. Publish the
    /// tail and the hints on their own first.
    Overflow,
    /// The Commit's own outcome: nothing written, or unknown.
    History(HistoryError),
}

impl From<HistoryError> for Fused {
    fn from(error: HistoryError) -> Self {
        Self::History(error)
    }
}

/// Makes a Workspace Commit in one write transaction on the writer session.
pub struct Committer {
    engine: Engine,
}

impl HistoryProvider {
    /// The fused committer of this catalog's session (#768).
    pub fn committer(&self) -> Committer {
        Committer {
            engine: match &self.engine {
                Engine::Sqlite(session) => Engine::Sqlite(session.clone()),
                Engine::Postgres(session) => Engine::Postgres(session.clone()),
            },
        }
    }
}

impl Committer {
    /// Publishes `tail` (a Save's held last publication, if any), records
    /// `hints` and makes the Commit `request` describes over its unmoved
    /// expected head, in one transaction. `stack` is the Branch's
    /// LayerStack. Only a plain Commit is fused: no second parent, and a
    /// required head equal to the expected one; anything else is
    /// [`Fused::Race`] before anything is sent.
    pub fn commit(
        &self,
        tail: Option<&Publication>,
        stack: LayerStackId,
        request: &CommitRequest,
        hints: &[BindingHint],
    ) -> Result<CommitOutcome, Fused> {
        self.commit_at(tail, stack, request, hints, None)
    }

    /// [`Self::commit`] with the PostgreSQL Commit made at `time_ms` when
    /// given (a test makes the same identity twice); SQLite's
    /// `commit_branch` always takes the time now.
    pub(crate) fn commit_at(
        &self,
        tail: Option<&Publication>,
        stack: LayerStackId,
        request: &CommitRequest,
        hints: &[BindingHint],
        time_ms: Option<i64>,
    ) -> Result<CommitOutcome, Fused> {
        let expected = request
            .expected_head
            .map_or(NodeId::Layer(request.expected_base), NodeId::Commit);
        if request.second_parent.is_some()
            || request.candidate_root == request.expected_root
            || request.required_head.is_some_and(|node| node != expected)
        {
            return Err(Fused::Race);
        }
        let blocks = crate::blocks::hints::encode(hints)
            .map_err(|_| HistoryError::Integrity("binding hint block encoding"))?;
        let rows = tail.map_or(0, rows_of) + blocks.len() + FIXED_ROWS;
        if blocks.len() > hints::ARRAY_ROWS || rows > policy::TRANSACTION_ROW_LIMIT as usize {
            return Err(Fused::Overflow);
        }
        match &self.engine {
            Engine::Postgres(session) => {
                if let Some(batch) = tail {
                    crate::publication::validate(batch, policy::PUBLICATION_BYTES_MAX)
                        .map_err(|_| Fused::Overflow)?;
                }
                let record = commit::derive_record_at(
                    stack,
                    request.branch,
                    request.candidate_root,
                    request.expected_head,
                    request.expected_base,
                    None,
                    &layerfs_history::records::CommitMeta {
                        displaced: None,
                        ..request.meta.clone()
                    },
                    time_ms,
                )?;
                crate::backend::postgresql::history::fused::commit(
                    session, tail, request, record, &blocks,
                )
            }
            Engine::Sqlite(session) => sqlite(session, tail, request, hints),
        }
    }
}

/// Every row `batch` writes, as the publication bounds count them.
fn rows_of(batch: &Publication) -> usize {
    batch.packs.len()
        + batch.objects.len()
        + batch.inode_values.len()
        + batch.signatures.len()
        + usize::from(batch.release_ordinals.is_some())
}

/// SQLite: the publication, the hints and `commit_branch` in one
/// `BEGIN IMMEDIATE` transaction; a race rolls it back.
fn sqlite(
    session: &std::sync::Arc<crate::backend::Session>,
    tail: Option<&Publication>,
    request: &CommitRequest,
    hints: &[BindingHint],
) -> Result<CommitOutcome, Fused> {
    if let Some(batch) = tail {
        // SQLite's publication bound: the port's default (4 MiB).
        if crate::publication::validate(batch, policy::TRANSACTION_CANONICAL_BYTES_LIMIT).is_err()
            || (session.layout.uses_units()
                && crate::backend::sqlite::objects::unit_layout::validate_batch(batch).is_err())
        {
            return Err(Fused::Overflow);
        }
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        if !batch.packs.is_empty() {
            let bytes = batch.packs.iter().map(|p| p.body().len() as u64).sum();
            crate::backend::sqlite::lifecycle::headroom::admit(&session.path, bytes)
                .map_err(|error| Fused::History(error.into()))?;
        }
    }
    let decided: HistoryResult<Option<CommitOutcome>> = session.run_decided(true, |tx| {
        if let Some(batch) = tail {
            let published = crate::backend::publish::run(tx, batch)?;
            if !published.lost.is_empty() {
                return Ok((None, Completion::Rollback));
            }
        }
        let history = Transaction::new(tx);
        hints::record(&history, hints)?;
        match commit::commit_branch(&history, request) {
            Ok(outcome @ CommitOutcome::Committed { .. }) => {
                Ok((Some(outcome), Completion::Commit))
            }
            Ok(CommitOutcome::UpToDate { .. }) | Err(HistoryError::HeadMoved(_)) => {
                Ok((None, Completion::Rollback))
            }
            Err(error) => Err(error),
        }
    });
    decided?.ok_or(Fused::Race)
}
