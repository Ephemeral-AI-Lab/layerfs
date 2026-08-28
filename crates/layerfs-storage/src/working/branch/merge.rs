//! Private child-Branch merge records.

use crate::full::branch::read::BranchHead;
use crate::full::record_id::RequestId;
use layerfs_core::ObjectId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChildMergeCandidate {
    pub source: BranchHead,
    pub expected_parent: BranchHead,
    pub result_root: ObjectId,
    pub source_transition: Vec<u8>,
    pub applied_transition: Vec<u8>,
    pub request_id: RequestId,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChildMergeOutcome {
    WorkingRecorded {
        parent_head: BranchHead,
        reconciled: bool,
    },
    Conflict {
        actual_parent: BranchHead,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChildMergePublication {
    pub candidate: ChildMergeCandidate,
    pub accepted_parent: BranchHead,
}

use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::branch::read::{
    branch_contains_exact_version, read_branch_ancestry, read_branch_head,
};
use crate::full::branch::transition::insert_transition;
use crate::full::closure::membership::authenticate_root;
use crate::full::legacy_store::{
    begin_product_transaction, commit_product_request, rollback_product_transaction, Engine,
};
use crate::full::record_id::{derive_id, object_id, transition_identity, OperationVersionId};
use crate::full::transfer::batch::MAX_TRANSITION_PAYLOAD_BYTES;
use crate::working::operation::record::next_operation_version_sequence;
use rusqlite::{params, OptionalExtension};

impl Engine {
    pub fn product_child_branch_merge(
        &self,
        candidate: ChildMergeCandidate,
    ) -> EngineResult<ChildMergeOutcome> {
        if candidate.source_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
            || candidate.applied_transition.len() > MAX_TRANSITION_PAYLOAD_BYTES
        {
            return Err(EngineError::InvalidRecord(
                "Merge transition resource bound",
            ));
        }
        let mut connection = begin_product_transaction(self)?;
        let result = (|| {
            if let Some(parent_head) = replay_child_branch_merge(&connection, &candidate)? {
                return Ok(ChildMergeOutcome::WorkingRecorded {
                    parent_head,
                    reconciled: true,
                });
            }
            let ancestry = read_branch_ancestry(&connection, candidate.source.branch_id)?
                .ok_or(EngineError::InvalidRecord("source Branch"))?;
            let parent_id =
                ancestry
                    .immediate_parent_branch_id
                    .ok_or(EngineError::InvalidRecord(
                        "top-level Branch cannot child-merge",
                    ))?;
            if parent_id != candidate.expected_parent.branch_id
                || ancestry.fork_operation_id.is_none()
                || ancestry.fork_operation_version_id.is_none()
            {
                return Err(EngineError::InvalidRecord("non-parent Branch merge"));
            }
            if read_branch_head(&connection, candidate.source.branch_id)? != Some(candidate.source)
            {
                return Err(EngineError::PublicationConflict);
            }
            let source_version = candidate
                .source
                .operation_version_id
                .ok_or(EngineError::InvalidRecord("Child merge source head"))?;
            let actual_parent = read_branch_head(&connection, parent_id)?
                .ok_or(EngineError::InvalidRecord("parent Branch"))?;
            if actual_parent != candidate.expected_parent {
                return Ok(ChildMergeOutcome::Conflict { actual_parent });
            }
            authenticate_root(self, &connection, candidate.result_root)?;
            let source_delta_id = transition_identity(
                ancestry.fork_root,
                candidate.source.root,
                &candidate.source_transition,
            );
            insert_transition(
                &connection,
                source_delta_id,
                ancestry.fork_root,
                candidate.source.root,
                &candidate.source_transition,
            )?;
            let applied_delta_id = transition_identity(
                candidate.expected_parent.root,
                candidate.result_root,
                &candidate.applied_transition,
            );
            insert_transition(
                &connection,
                applied_delta_id,
                candidate.expected_parent.root,
                candidate.result_root,
                &candidate.applied_transition,
            )?;
            let branch_delta_id = derive_id(
                b"child-branch-delta",
                &[
                    candidate.source.branch_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    &source_delta_id,
                    &applied_delta_id,
                ],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_deltas
                     (branch_delta_id, purpose, source_branch_id,
                      source_branch_generation, source_branch_operation_version_id, base_root,
                      source_root, destination_root, result_root,
                      source_delta_id, applied_delta_id)
                     VALUES (?1, 'child_merge', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        branch_delta_id.as_slice(),
                        candidate.source.branch_id.as_bytes(),
                        i64::try_from(candidate.source.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        source_version.as_bytes(),
                        ancestry.fork_root.as_bytes(),
                        candidate.source.root.as_bytes(),
                        candidate.expected_parent.root.as_bytes(),
                        candidate.result_root.as_bytes(),
                        source_delta_id.as_slice(),
                        applied_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let next_generation = actual_parent
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            let version_sequence = next_operation_version_sequence(&connection, parent_id)?;
            let version_id = OperationVersionId(derive_id(
                b"child-merge-operation-version",
                &[
                    parent_id.as_bytes(),
                    candidate.request_id.as_bytes(),
                    candidate.result_root.as_bytes(),
                ],
            ));
            connection
                .execute(
                    "INSERT INTO layerfs_operation_versions
                     (operation_version_id, branch_id, sequence,
                      parent_operation_version_id, root_id, created_by_kind,
                      created_by_child_branch_id, created_by_branch_delta_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, 'child_merge', ?6, ?7)",
                    params![
                        version_id.as_bytes(),
                        parent_id.as_bytes(),
                        version_sequence,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        candidate.result_root.as_bytes(),
                        candidate.source.branch_id.as_bytes(),
                        branch_delta_id.as_slice(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let receipt_id = derive_id(
                b"child-branch-merge-receipt",
                &[candidate.request_id.as_bytes(), version_id.as_bytes()],
            );
            connection
                .execute(
                    "INSERT INTO layerfs_branch_transitions
                     (transition_id, branch_id, before_generation, after_generation,
                      before_operation_version_id, after_operation_version_id,
                      action_kind, source_record_id, request_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6,
                             'child_branch_merge', ?7, ?8)",
                    params![
                        receipt_id.as_slice(),
                        parent_id.as_bytes(),
                        i64::try_from(actual_parent.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                        version_id.as_bytes(),
                        branch_delta_id.as_slice(),
                        candidate.request_id.as_bytes(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed = connection
                .execute(
                    "UPDATE layerfs_branches
                     SET generation = ?1, head_operation_version_id = ?2
                     WHERE branch_id = ?3 AND generation = ?4
                       AND head_operation_version_id IS ?5 AND state = 'active'",
                    params![
                        i64::try_from(next_generation).map_err(|_| EngineError::CounterOverflow)?,
                        version_id.as_bytes(),
                        parent_id.as_bytes(),
                        i64::try_from(actual_parent.generation)
                            .map_err(|_| EngineError::CounterOverflow)?,
                        actual_parent
                            .operation_version_id
                            .map(|id| id.as_bytes().as_slice().to_vec()),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed != 1 {
                return Err(EngineError::PublicationConflict);
            }
            let parent_head = BranchHead {
                branch_id: parent_id,
                generation: next_generation,
                operation_version_id: Some(version_id),
                root: candidate.result_root,
            };
            let reconciled = commit_product_request(
                self,
                &mut connection,
                "layerfs_branch_transitions",
                candidate.request_id,
            )?;
            Ok(ChildMergeOutcome::WorkingRecorded {
                parent_head,
                reconciled,
            })
        })();
        rollback_product_transaction(self, &mut connection, &result);
        result
    }
}

struct StoredChildMergeReplay {
    branch_id: Vec<u8>,
    before_generation: i64,
    before_version_id: Option<Vec<u8>>,
    before_root: Vec<u8>,
    after_generation: i64,
    after_version_id: Vec<u8>,
    action: String,
    source_record_id: Vec<u8>,
    result_root: Vec<u8>,
    result_parent_version_id: Option<Vec<u8>>,
    result_source_branch_id: Vec<u8>,
    result_branch_delta_id: Vec<u8>,
    delta_source_branch_id: Vec<u8>,
    delta_source_generation: i64,
    delta_source_version_id: Vec<u8>,
    delta_base_root: Vec<u8>,
    delta_source_root: Vec<u8>,
    delta_destination_root: Vec<u8>,
    delta_result_root: Vec<u8>,
    source_delta_id: Vec<u8>,
    applied_delta_id: Vec<u8>,
}

fn replay_child_branch_merge(
    connection: &rusqlite::Connection,
    candidate: &ChildMergeCandidate,
) -> EngineResult<Option<BranchHead>> {
    let stored = connection
        .query_row(
            "SELECT t.branch_id, t.before_generation,
                    t.before_operation_version_id,
                    COALESCE(before_version.root_id, parent.fork_root_id),
                    t.after_generation, t.after_operation_version_id,
                    t.action_kind, t.source_record_id, result.root_id,
                    result.parent_operation_version_id,
                    result.created_by_child_branch_id,
                    result.created_by_branch_delta_id,
                    d.source_branch_id, d.source_branch_generation,
                    d.source_branch_operation_version_id, d.base_root,
                    d.source_root, d.destination_root, d.result_root,
                    d.source_delta_id, d.applied_delta_id
             FROM layerfs_branch_transitions t
             JOIN layerfs_branches parent ON parent.branch_id = t.branch_id
             LEFT JOIN layerfs_operation_versions before_version
               ON before_version.branch_id = t.branch_id
              AND before_version.operation_version_id = t.before_operation_version_id
             JOIN layerfs_operation_versions result
               ON result.branch_id = t.branch_id
              AND result.operation_version_id = t.after_operation_version_id
             JOIN layerfs_branch_deltas d
               ON d.branch_delta_id = t.source_record_id
              AND d.purpose = 'child_merge'
             WHERE t.request_id = ?1",
            params![candidate.request_id.as_bytes()],
            |row| {
                Ok(StoredChildMergeReplay {
                    branch_id: row.get(0)?,
                    before_generation: row.get(1)?,
                    before_version_id: row.get(2)?,
                    before_root: row.get(3)?,
                    after_generation: row.get(4)?,
                    after_version_id: row.get(5)?,
                    action: row.get(6)?,
                    source_record_id: row.get(7)?,
                    result_root: row.get(8)?,
                    result_parent_version_id: row.get(9)?,
                    result_source_branch_id: row.get(10)?,
                    result_branch_delta_id: row.get(11)?,
                    delta_source_branch_id: row.get(12)?,
                    delta_source_generation: row.get(13)?,
                    delta_source_version_id: row.get(14)?,
                    delta_base_root: row.get(15)?,
                    delta_source_root: row.get(16)?,
                    delta_destination_root: row.get(17)?,
                    delta_result_root: row.get(18)?,
                    source_delta_id: row.get(19)?,
                    applied_delta_id: row.get(20)?,
                })
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let source_version = candidate
        .source
        .operation_version_id
        .ok_or(EngineError::InvalidRecord("Child merge source head"))?;
    let expected_version = OperationVersionId(derive_id(
        b"child-merge-operation-version",
        &[
            candidate.expected_parent.branch_id.as_bytes(),
            candidate.request_id.as_bytes(),
            candidate.result_root.as_bytes(),
        ],
    ));
    let expected_source_delta = transition_identity(
        object_id(&stored.delta_base_root)?,
        candidate.source.root,
        &candidate.source_transition,
    );
    let expected_applied_delta = transition_identity(
        candidate.expected_parent.root,
        candidate.result_root,
        &candidate.applied_transition,
    );
    let generation = u64::try_from(stored.after_generation)
        .map_err(|_| EngineError::InvalidRecord("Branch generation"))?;
    if stored.branch_id.as_slice() != candidate.expected_parent.branch_id.as_bytes()
        || u64::try_from(stored.before_generation).ok()
            != Some(candidate.expected_parent.generation)
        || stored.before_version_id
            != candidate
                .expected_parent
                .operation_version_id
                .map(|version| version.as_bytes().to_vec())
        || object_id(&stored.before_root)? != candidate.expected_parent.root
        || generation
            != candidate
                .expected_parent
                .generation
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?
        || stored.after_version_id.as_slice() != expected_version.as_bytes()
        || stored.action != "child_branch_merge"
        || stored.source_record_id != stored.result_branch_delta_id
        || object_id(&stored.result_root)? != candidate.result_root
        || stored.result_parent_version_id
            != candidate
                .expected_parent
                .operation_version_id
                .map(|version| version.as_bytes().to_vec())
        || stored.result_source_branch_id.as_slice() != candidate.source.branch_id.as_bytes()
        || stored.delta_source_branch_id.as_slice() != candidate.source.branch_id.as_bytes()
        || u64::try_from(stored.delta_source_generation).ok() != Some(candidate.source.generation)
        || stored.delta_source_version_id.as_slice() != source_version.as_bytes()
        || object_id(&stored.delta_source_root)? != candidate.source.root
        || object_id(&stored.delta_destination_root)? != candidate.expected_parent.root
        || object_id(&stored.delta_result_root)? != candidate.result_root
        || stored.source_delta_id.as_slice() != expected_source_delta
        || stored.applied_delta_id.as_slice() != expected_applied_delta
        || !branch_contains_exact_version(connection, candidate.source)?
    {
        return Err(EngineError::InvalidRecord(
            "ChildBranchMerge request identity conflict",
        ));
    }
    Ok(Some(BranchHead {
        branch_id: candidate.expected_parent.branch_id,
        generation,
        operation_version_id: Some(expected_version),
        root: candidate.result_root,
    }))
}
