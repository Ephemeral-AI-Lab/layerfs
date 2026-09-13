use crate::records::decode_workspace_stage;
use crate::{BranchId, LayerStackStore, Result, StoreError, WorkspaceStage};
use layerfs_content::ObjectId;
use rusqlite::{OptionalExtension, TransactionBehavior};

impl LayerStackStore {
    pub fn workspace_stage(&self, workspace_id: [u8; 16]) -> Result<Option<WorkspaceStage>> {
        let connection = self.db.reader()?;
        workspace_stage_from_connection(&connection, workspace_id)
    }

    pub fn discard_workspace_stage(&self, workspace_id: [u8; 16]) -> Result<bool> {
        let _operation = self.db.enter_operation()?;
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(stage) = workspace_stage_from_connection(&transaction, workspace_id)? else {
            return Ok(false);
        };
        delete_workspace_stage(&transaction, stage)?;
        transaction.commit()?;
        Ok(true)
    }

    pub(crate) fn stage_workspace_root(
        &self,
        workspace_id: [u8; 16],
        branch_id: BranchId,
        root_id: ObjectId,
    ) -> Result<WorkspaceStage> {
        let requested = WorkspaceStage {
            workspace_id,
            branch_id,
            root_id,
        };
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if publication_table_exists(&transaction)?
            && transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_publications WHERE workspace_id=?1)",
                [workspace_id.as_slice()],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Err(StoreError::InvalidInput(
                "Workspace publication not acknowledged",
            ));
        }
        transaction.execute(
            crate::statements::workspace::INSERT_STAGE,
            rusqlite::params![
                requested.workspace_id.as_slice(),
                requested.branch_id.as_slice(),
                requested.root_id.as_bytes().as_slice(),
            ],
        )?;
        let actual = workspace_stage_from_connection(&transaction, workspace_id)?
            .ok_or(StoreError::Integrity("Workspace stage insertion"))?;
        if actual != requested {
            return Err(StoreError::InvalidInput("Workspace stage already retained"));
        }
        transaction.commit()?;
        Ok(actual)
    }
}

pub(crate) fn workspace_stage_from_connection(
    connection: &rusqlite::Connection,
    workspace_id: [u8; 16],
) -> Result<Option<WorkspaceStage>> {
    Ok(connection
        .query_row(
            crate::statements::workspace::GET_STAGE,
            [workspace_id.as_slice()],
            decode_workspace_stage,
        )
        .optional()?)
}

pub(crate) fn delete_workspace_stage(
    transaction: &rusqlite::Transaction<'_>,
    stage: WorkspaceStage,
) -> Result<()> {
    if transaction.execute(
        crate::statements::workspace::DELETE_STAGE,
        rusqlite::params![
            stage.workspace_id.as_slice(),
            stage.branch_id.as_slice(),
            stage.root_id.as_bytes().as_slice(),
        ],
    )? != 1
    {
        return Err(StoreError::Integrity("Workspace stage changed"));
    }
    Ok(())
}

/// Immutable publication context. Retry never substitutes a newer live snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspacePublicationAttempt {
    pub workspace_id: [u8; 16],
    pub branch_id: BranchId,
    pub layer_stack_id: crate::LayerStackId,
    pub expected_head: Option<crate::CommitId>,
    pub expected_root: ObjectId,
    pub expected_base: crate::LayerId,
    pub candidate_root: ObjectId,
    pub new_base: crate::LayerId,
    pub covered_sequence: u64,
}

impl WorkspacePublicationAttempt {
    pub fn new(
        workspace_id: [u8; 16],
        expected: &crate::BranchRecord,
        expected_root: ObjectId,
        candidate_root: ObjectId,
        new_base: crate::LayerId,
        covered_sequence: u64,
    ) -> Self {
        Self {
            workspace_id,
            branch_id: expected.id,
            layer_stack_id: expected.layer_stack_id,
            expected_head: expected.head_commit_id,
            expected_root,
            expected_base: expected.base_layer_id,
            candidate_root,
            new_base,
            covered_sequence,
        }
    }

    /// V4 identity is the exact deterministic tuple, independently of delivery.
    /// The full stored context, including sequence, is checked in addition to it.
    pub fn key(&self) -> [u8; 32] {
        let mut hash = blake3::Hasher::new();
        hash.update(b"layerfs/workspace-publication/v1\0");
        hash.update(&self.workspace_id);
        hash.update(self.candidate_root.as_bytes());
        match self.expected_head {
            Some(head) => {
                hash.update(&[1]);
                hash.update(head.as_slice());
            }
            None => {
                hash.update(&[0]);
            }
        }
        hash.update(self.new_base.as_slice());
        *hash.finalize().as_bytes()
    }

    pub(crate) fn up_to_date(&self) -> bool {
        self.candidate_root == self.expected_root && self.new_base == self.expected_base
    }

    pub(crate) fn head_after(&self) -> Option<crate::CommitId> {
        if self.up_to_date() {
            self.expected_head
        } else {
            Some(crate::CommitId::derive(
                self.candidate_root,
                self.expected_head,
                self.new_base,
            ))
        }
    }
}

/// Authoritative state only; reconstruction never fabricates performance counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkspacePublicationReceipt {
    pub attempt: WorkspacePublicationAttempt,
    pub head_after: Option<crate::CommitId>,
    pub up_to_date: bool,
    /// Absent for an independently verified branch-history witness.
    pub published_ns: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspacePublicationResolution {
    Published(WorkspacePublicationReceipt),
    NotPublished,
    Unknown,
}

// Fixed-width rows, one unresolved receipt per Workspace. A full Store rejects
// further publication before advancing a branch; it never evicts unresolved proof.
pub const WORKSPACE_PUBLICATION_LIMIT: usize = 1024;
pub const WORKSPACE_PUBLICATION_ANCESTRY_LIMIT: usize = 256;

impl LayerStackStore {
    /// Inspect one exact attempt under a consistent Store view. Missing stages
    /// and Commit objects outside this branch's history are never witnesses.
    pub fn resolve_workspace_publication(
        &self,
        attempt: &WorkspacePublicationAttempt,
    ) -> Result<WorkspacePublicationResolution> {
        let connection = self.db.reader()?;
        if let Some(receipt) = publication_from_connection(&connection, attempt)? {
            return Ok(WorkspacePublicationResolution::Published(receipt));
        }
        let (branch, root) =
            crate::workspace::workspace_snapshot_from_connection(&connection, attempt.branch_id)?;
        if branch.layer_stack_id != attempt.layer_stack_id {
            return Err(StoreError::Integrity("Workspace publication source"));
        }
        validate_publication_predecessor(&connection, attempt)?;
        if branch.head_commit_id == attempt.expected_head
            && branch.base_layer_id == attempt.expected_base
            && root == attempt.expected_root
        {
            return Ok(WorkspacePublicationResolution::NotPublished);
        }
        // UpToDate leaves no unique history identity. A moved branch without the
        // receipt cannot prove that this particular no-op transaction happened.
        if attempt.up_to_date() {
            return Ok(WorkspacePublicationResolution::Unknown);
        }
        let mut head = branch.head_commit_id;
        for _ in 0..WORKSPACE_PUBLICATION_ANCESTRY_LIMIT {
            let Some(id) = head else { break };
            let commit = connection
                .query_row(
                    crate::statements::branch::GET_COMMIT,
                    [id.as_slice()],
                    crate::records::decode_commit,
                )
                .optional()?
                .ok_or(StoreError::Integrity("publication history Commit"))?;
            if commit.id
                != crate::CommitId::derive(
                    commit.root_id,
                    commit.parent_commit_id,
                    commit.base_layer_id,
                )
            {
                return Err(StoreError::Integrity("publication history identity"));
            }
            if Some(id) == attempt.head_after() {
                if commit.root_id != attempt.candidate_root
                    || commit.parent_commit_id != attempt.expected_head
                    || commit.base_layer_id != attempt.new_base
                {
                    return Err(StoreError::Integrity("publication witness context"));
                }
                return Ok(WorkspacePublicationResolution::Published(
                    WorkspacePublicationReceipt {
                        attempt: *attempt,
                        head_after: Some(id),
                        up_to_date: false,
                        published_ns: None,
                    },
                ));
            }
            if Some(id) == attempt.expected_head {
                break;
            }
            head = commit.parent_commit_id;
        }
        Ok(WorkspacePublicationResolution::Unknown)
    }

    /// Call only after applying the exact receipt to runtime publication coverage.
    /// Failed cleanup leaves the row retained; repeated acknowledgement is safe.
    pub fn acknowledge_workspace_publication(
        &self,
        attempt: &WorkspacePublicationAttempt,
    ) -> Result<bool> {
        let _operation = self.db.enter_operation()?;
        let mut connection = self.db.writer()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if publication_from_connection(&transaction, attempt)?.is_none() {
            return Ok(false);
        }
        let deleted = transaction.execute(
            "DELETE FROM workspace_publications WHERE workspace_id=?1 AND attempt_key=?2 AND covered_sequence=?3",
            rusqlite::params![attempt.workspace_id.as_slice(), attempt.key().as_slice(), attempt.covered_sequence.to_be_bytes().as_slice()],
        )?;
        crate::schema::fail_transaction_statement(u64::MAX - 4)?;
        transaction.commit()?;
        Ok(deleted == 1)
    }
}

pub(crate) fn publication_table_exists(connection: &rusqlite::Connection) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name='workspace_publications')",
        [],
        |row| row.get(0),
    )?)
}

pub(crate) fn publication_from_connection(
    connection: &rusqlite::Connection,
    attempt: &WorkspacePublicationAttempt,
) -> Result<Option<WorkspacePublicationReceipt>> {
    if !publication_table_exists(connection)? {
        return Ok(None);
    }
    let row = connection.query_row(
        "SELECT attempt_key=?2 AND branch_id=?3 AND layer_stack_id=?4 AND expected_root=?5 AND expected_head IS ?6 AND expected_base=?7 AND root_id=?8 AND base_after=?9 AND covered_sequence=?10, outcome_kind, head_after, published_ns FROM workspace_publications WHERE workspace_id=?1",
        rusqlite::params![
            attempt.workspace_id.as_slice(), attempt.key().as_slice(), attempt.branch_id.as_slice(),
            attempt.layer_stack_id.as_slice(), attempt.expected_root.as_bytes().as_slice(),
            attempt.expected_head.map(|id| id.to_bytes().to_vec()), attempt.expected_base.as_slice(),
            attempt.candidate_root.as_bytes().as_slice(), attempt.new_base.as_slice(),
            attempt.covered_sequence.to_be_bytes().as_slice(),
        ],
        |row| Ok((row.get::<_, bool>(0)?, row.get::<_, i64>(1)?, row.get::<_, Option<Vec<u8>>>(2)?, row.get::<_, Vec<u8>>(3)?)),
    ).optional()?;
    let Some((matches, kind, head, time)) = row else {
        return Ok(None);
    };
    if !matches {
        return Err(StoreError::InvalidInput(
            "Workspace publication context mismatch",
        ));
    }
    let head_after = crate::records::optional_id::<crate::CommitId>(head)?;
    if kind != if attempt.up_to_date() { 2 } else { 1 } || head_after != attempt.head_after() {
        return Err(StoreError::Integrity("Workspace publication receipt"));
    }
    let published_ns = u64::from_be_bytes(
        time.try_into()
            .map_err(|_| StoreError::Integrity("publication timestamp"))?,
    );
    Ok(Some(WorkspacePublicationReceipt {
        attempt: *attempt,
        head_after,
        up_to_date: kind == 2,
        published_ns: Some(published_ns),
    }))
}

pub(crate) fn insert_workspace_publication(
    transaction: &rusqlite::Transaction<'_>,
    attempt: &WorkspacePublicationAttempt,
) -> Result<WorkspacePublicationReceipt> {
    if !publication_table_exists(transaction)? {
        transaction.execute_batch(crate::schema::WORKSPACE_PUBLICATIONS_SCHEMA)?;
    }
    let count: i64 =
        transaction.query_row("SELECT count(*) FROM workspace_publications", [], |row| {
            row.get(0)
        })?;
    if count >= WORKSPACE_PUBLICATION_LIMIT as i64 {
        return Err(StoreError::InvalidInput(
            "Workspace publication receipt capacity",
        ));
    }
    let published_ns: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| StoreError::Integrity("publication clock"))?
        .as_nanos()
        .try_into()
        .map_err(|_| StoreError::Integrity("publication clock"))?;
    transaction.execute(
        "INSERT INTO workspace_publications VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        rusqlite::params![
            attempt.workspace_id.as_slice(),
            attempt.key().as_slice(),
            attempt.branch_id.as_slice(),
            attempt.layer_stack_id.as_slice(),
            attempt.expected_root.as_bytes().as_slice(),
            attempt.expected_head.map(|id| id.to_bytes().to_vec()),
            attempt.expected_base.as_slice(),
            attempt.candidate_root.as_bytes().as_slice(),
            attempt.new_base.as_slice(),
            attempt.covered_sequence.to_be_bytes().as_slice(),
            if attempt.up_to_date() { 2 } else { 1 },
            attempt.head_after().map(|id| id.to_bytes().to_vec()),
            published_ns.to_be_bytes().as_slice(),
        ],
    )?;
    Ok(WorkspacePublicationReceipt {
        attempt: *attempt,
        head_after: attempt.head_after(),
        up_to_date: attempt.up_to_date(),
        published_ns: Some(published_ns),
    })
}

fn validate_publication_predecessor(
    connection: &rusqlite::Connection,
    attempt: &WorkspacePublicationAttempt,
) -> Result<()> {
    let root = if let Some(head) = attempt.expected_head {
        let commit = connection
            .query_row(
                crate::statements::branch::GET_COMMIT,
                [head.as_slice()],
                crate::records::decode_commit,
            )
            .optional()?
            .ok_or(StoreError::Integrity("publication expected Commit"))?;
        if commit.id
            != crate::CommitId::derive(
                commit.root_id,
                commit.parent_commit_id,
                commit.base_layer_id,
            )
            || commit.base_layer_id != attempt.expected_base
        {
            return Err(StoreError::Integrity("publication expected Commit"));
        }
        commit.root_id
    } else {
        let bytes: Vec<u8> = connection.query_row(
            "SELECT root_id FROM layers WHERE layer_id=?1 AND layer_stack_id=?2",
            rusqlite::params![
                attempt.expected_base.as_slice(),
                attempt.layer_stack_id.as_slice()
            ],
            |row| row.get(0),
        )?;
        crate::records::decode_object_id(bytes)?
    };
    if root != attempt.expected_root {
        return Err(StoreError::Integrity("publication expected root"));
    }
    for base in [attempt.expected_base, attempt.new_base] {
        let owned: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM layers WHERE layer_id=?1 AND layer_stack_id=?2)",
            rusqlite::params![base.as_slice(), attempt.layer_stack_id.as_slice()],
            |row| row.get(0),
        )?;
        if !owned {
            return Err(StoreError::Integrity("Branch LayerStack ownership"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_changes, EntityName, LayerStackInitialization, LocalForkSource};
    use layerfs_content::filesystem::ContentChange;

    fn fixture(label: &str) -> (std::path::PathBuf, LayerStackStore, BranchId) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-publication-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let initialized = store
            .initialize_layerstack(
                EntityName::new("publication").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap();
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer {
                    layer_id: initialized.genesis_layer_id,
                },
            )
            .unwrap();
        (root, store, branch)
    }

    fn candidate(
        store: &LayerStackStore,
        branch: BranchId,
        workspace_id: [u8; 16],
        sequence: u64,
        contents: Option<&str>,
    ) -> (WorkspacePublicationAttempt, crate::BuiltRoot) {
        let pinned = store.pin_branch(branch).unwrap();
        let changes = contents
            .map(|text| ContentChange::Write {
                path: "file".into(),
                bytes: text.as_bytes().to_vec(),
                mode: 0o644,
            })
            .into_iter()
            .collect::<Vec<_>>();
        let built =
            apply_changes(&pinned.reader, pinned.root, &changes, [sequence as u8; 32]).unwrap();
        (
            WorkspacePublicationAttempt::new(
                workspace_id,
                &pinned.branch,
                pinned.root,
                built.root_id,
                pinned.branch.base_layer_id,
                sequence,
            ),
            built,
        )
    }

    fn publish(
        store: &LayerStackStore,
        attempt: &WorkspacePublicationAttempt,
        built: crate::BuiltRoot,
    ) -> Result<WorkspacePublicationReceipt> {
        store.commit_workspace_candidate_retained(
            attempt,
            built,
            store.workspace_admission(attempt.workspace_id).unwrap(),
        )
    }

    #[test]
    fn publication_lost_ack_reopen_exact_context_and_bounded_history() {
        let (root, store, branch) = fixture("lost-ack");
        assert!(!publication_table_exists(&store.db.reader().unwrap()).unwrap());
        let (created, built) = candidate(&store, branch, [1; 16], 1, Some("first"));
        assert_eq!(
            store.resolve_workspace_publication(&created).unwrap(),
            WorkspacePublicationResolution::NotPublished
        );
        crate::schema::set_transaction_failure_at(Some(u64::MAX - 3));
        let lost = publish(&store, &created, built);
        crate::schema::set_transaction_failure_at(None);
        assert!(matches!(
            lost,
            Err(StoreError::Integrity("injected transaction failure"))
        ));
        assert!(store
            .workspace_stage(created.workspace_id)
            .unwrap()
            .is_none());
        let first_receipt = store.publish_workspace_stage(&created).unwrap();
        assert!(!first_receipt.up_to_date);
        assert_eq!(store.store_counts().unwrap().commits, 1);
        drop(store);

        // The optional extension is accepted exactly after a fresh connection.
        let store = LayerStackStore::connect(root.join("store.sqlite")).unwrap();
        assert_eq!(
            store.resolve_workspace_publication(&created).unwrap(),
            WorkspacePublicationResolution::Published(first_receipt)
        );
        let mut wrong = created;
        wrong.covered_sequence += 1;
        assert_eq!(wrong.key(), created.key());
        assert!(matches!(
            store.publish_workspace_stage(&wrong),
            Err(StoreError::InvalidInput(
                "Workspace publication context mismatch"
            ))
        ));
        wrong = created;
        wrong.expected_root = created.candidate_root;
        assert!(matches!(
            store.resolve_workspace_publication(&wrong),
            Err(StoreError::InvalidInput(
                "Workspace publication context mismatch"
            ))
        ));
        wrong = created;
        wrong.branch_id = BranchId::new();
        assert!(matches!(
            store.acknowledge_workspace_publication(&wrong),
            Err(StoreError::InvalidInput(
                "Workspace publication context mismatch"
            ))
        ));

        // An authorized later publication does not erase the exact earlier proof.
        let (later, built) = candidate(&store, branch, [2; 16], 2, Some("second"));
        publish(&store, &later, built).unwrap();
        assert_eq!(
            store.publish_workspace_stage(&created).unwrap(),
            first_receipt
        );
        crate::schema::set_transaction_failure_at(Some(u64::MAX - 4));
        let cleanup = store.acknowledge_workspace_publication(&created);
        crate::schema::set_transaction_failure_at(None);
        assert!(cleanup.is_err());
        assert_eq!(
            store.publish_workspace_stage(&created).unwrap(),
            first_receipt
        );
        assert!(store.acknowledge_workspace_publication(&created).unwrap());
        assert!(!store.acknowledge_workspace_publication(&created).unwrap());
        let WorkspacePublicationResolution::Published(history) =
            store.resolve_workspace_publication(&created).unwrap()
        else {
            panic!("exact descendant must witness publication")
        };
        assert_eq!(history.head_after, first_receipt.head_after);
        assert_eq!(history.published_ns, None);

        // An unacknowledged no-op must not silently acknowledge a later boundary.
        let (no_op, built) = candidate(&store, branch, [3; 16], 3, None);
        crate::schema::set_transaction_failure_at(Some(u64::MAX - 3));
        let lost = publish(&store, &no_op, built);
        crate::schema::set_transaction_failure_at(None);
        assert!(lost.is_err());
        assert_eq!(store.store_counts().unwrap().commits, 2);
        let receipt = store.publish_workspace_stage(&no_op).unwrap();
        assert!(receipt.up_to_date);
        wrong = no_op;
        wrong.covered_sequence += 1;
        assert_eq!(wrong.key(), no_op.key());
        assert!(matches!(
            store.resolve_workspace_publication(&wrong),
            Err(StoreError::InvalidInput(
                "Workspace publication context mismatch"
            ))
        ));
        drop(store);
        let store = LayerStackStore::connect(root.join("store.sqlite")).unwrap();
        assert_eq!(store.publish_workspace_stage(&no_op).unwrap(), receipt);
        let (advanced, built) = candidate(&store, branch, [4; 16], 4, Some("third"));
        publish(&store, &advanced, built).unwrap();
        assert_eq!(store.publish_workspace_stage(&no_op).unwrap(), receipt);
        assert!(store.acknowledge_workspace_publication(&no_op).unwrap());
        assert_eq!(
            store.resolve_workspace_publication(&no_op).unwrap(),
            WorkspacePublicationResolution::Unknown
        );

        // After acknowledgement the same tuple can cover a later no-op boundary.
        let (current_noop, built) = candidate(&store, branch, [5; 16], 5, None);
        publish(&store, &current_noop, built).unwrap();
        store
            .acknowledge_workspace_publication(&current_noop)
            .unwrap();
        let (next_noop, built) = candidate(&store, branch, [5; 16], 6, None);
        assert_eq!(current_noop.key(), next_noop.key());
        assert_eq!(
            publish(&store, &next_noop, built)
                .unwrap()
                .attempt
                .covered_sequence,
            6
        );

        // Limit ancestry observation; passing the bound remains explicitly unknown.
        let mut head = advanced.head_after();
        {
            let mut connection = store.db.writer().unwrap();
            let transaction = connection.transaction().unwrap();
            for _ in 0..WORKSPACE_PUBLICATION_ANCESTRY_LIMIT {
                let id = crate::CommitId::derive(advanced.candidate_root, head, advanced.new_base);
                transaction
                    .execute(
                        crate::statements::workspace::INSERT_COMMIT,
                        rusqlite::params![
                            id.as_slice(),
                            advanced.candidate_root.as_bytes().as_slice(),
                            head.map(|id| id.to_bytes().to_vec()),
                            advanced.new_base.as_slice()
                        ],
                    )
                    .unwrap();
                head = Some(id);
            }
            transaction
                .execute(
                    "UPDATE branches SET head_commit_id=?1 WHERE branch_id=?2",
                    rusqlite::params![head.unwrap().as_slice(), branch.as_slice()],
                )
                .unwrap();
            transaction.commit().unwrap();
        }
        assert_eq!(
            store.resolve_workspace_publication(&created).unwrap(),
            WorkspacePublicationResolution::Unknown
        );
        drop(store);

        // An unexpected extension column is not accepted as a loose schema match.
        let invalid = root.join("invalid.sqlite");
        std::fs::copy(root.join("store.sqlite"), &invalid).unwrap();
        rusqlite::Connection::open(&invalid)
            .unwrap()
            .execute_batch("ALTER TABLE workspace_publications ADD COLUMN extra INTEGER;")
            .unwrap();
        let before = std::fs::read(&invalid).unwrap();
        assert!(matches!(
            LayerStackStore::connect(&invalid),
            Err(StoreError::WrongStoreSchema)
        ));
        assert_eq!(std::fs::read(&invalid).unwrap(), before);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn publication_retains_failed_stage_retries_exactly_and_bounds_receipts() {
        let (root, store, branch) = fixture("stage-capacity");
        let (attempt, built) = candidate(&store, branch, [10; 16], 10, Some("candidate"));
        crate::schema::set_transaction_failure_at(Some(u64::MAX - 2));
        let failure = publish(&store, &attempt, built);
        crate::schema::set_transaction_failure_at(None);
        assert!(failure.is_err());
        assert_eq!(
            store.resolve_workspace_publication(&attempt).unwrap(),
            WorkspacePublicationResolution::NotPublished
        );
        assert_eq!(
            store
                .workspace_stage(attempt.workspace_id)
                .unwrap()
                .unwrap()
                .root_id,
            attempt.candidate_root
        );
        let receipt = store.publish_workspace_stage(&attempt).unwrap();
        assert_eq!(receipt.attempt, attempt);
        assert_eq!(store.store_counts().unwrap().commits, 1);
        let (blocked, built) = candidate(
            &store,
            branch,
            attempt.workspace_id,
            11,
            Some("cannot replace receipt"),
        );
        assert!(matches!(
            publish(&store, &blocked, built),
            Err(StoreError::InvalidInput(
                "Workspace publication context mismatch"
            ))
        ));

        let (stale, built) = candidate(&store, branch, [11; 16], 11, Some("stale"));
        let (winner, winner_built) = candidate(&store, branch, [12; 16], 12, Some("winner"));
        publish(&store, &winner, winner_built).unwrap();
        assert!(matches!(
            publish(&store, &stale, built),
            Err(StoreError::CommitHeadMoved { .. })
        ));
        let stage = store.workspace_stage(stale.workspace_id).unwrap().unwrap();
        assert_eq!(stage.root_id, stale.candidate_root);
        assert!(matches!(
            store.publish_workspace_stage(&stale),
            Err(StoreError::CommitHeadMoved { .. })
        ));
        assert_eq!(
            store.workspace_stage(stale.workspace_id).unwrap(),
            Some(stage)
        );
        assert_eq!(
            store.resolve_workspace_publication(&stale).unwrap(),
            WorkspacePublicationResolution::Unknown
        );
        assert!(store.discard_workspace_stage(stale.workspace_id).unwrap());
        assert_eq!(
            store.resolve_workspace_publication(&stale).unwrap(),
            WorkspacePublicationResolution::Unknown
        );

        // Even the exact deterministic Commit existing on another branch is
        // insufficient: publication must be witnessed on the requested branch.
        let other = store
            .fork_branch(
                EntityName::new("other").unwrap(),
                LocalForkSource::Branch {
                    branch_id: branch,
                    commit_id: stale.expected_head.unwrap(),
                },
            )
            .unwrap();
        let pinned = store.pin_branch(other).unwrap();
        let built = crate::ObjectBuffer::new(&pinned.reader)
            .unwrap()
            .finish(stale.candidate_root, 0)
            .unwrap();
        let elsewhere = store
            .commit_candidate(
                &pinned.branch,
                pinned.root,
                pinned.branch.base_layer_id,
                built,
            )
            .unwrap();
        assert!(
            matches!(elsewhere, crate::CommitOutcome::Committed { commit_id, .. } if Some(commit_id) == stale.head_after())
        );
        assert_eq!(
            store.resolve_workspace_publication(&stale).unwrap(),
            WorkspacePublicationResolution::Unknown
        );
        drop(pinned);

        // Capacity is a fixed maximum even if callers never acknowledge.
        let (mut slot, _) = candidate(&store, branch, [0; 16], 0, None);
        {
            let mut connection = store.db.writer().unwrap();
            let transaction = connection.transaction().unwrap();
            for index in 0..WORKSPACE_PUBLICATION_LIMIT - 2 {
                slot.workspace_id = (index as u128).to_be_bytes();
                insert_workspace_publication(&transaction, &slot).unwrap();
            }
            transaction.commit().unwrap();
        }
        let (full, built) = candidate(&store, branch, [20; 16], 20, Some("must retain stage"));
        assert!(matches!(
            publish(&store, &full, built),
            Err(StoreError::InvalidInput(
                "Workspace publication receipt capacity"
            ))
        ));
        assert_eq!(
            store.branch(branch).unwrap().unwrap().head_commit_id,
            winner.head_after()
        );
        assert_eq!(
            store
                .workspace_stage(full.workspace_id)
                .unwrap()
                .unwrap()
                .root_id,
            full.candidate_root
        );
        assert_eq!(
            store.resolve_workspace_publication(&full).unwrap(),
            WorkspacePublicationResolution::NotPublished
        );
        assert!(store.acknowledge_workspace_publication(&slot).unwrap());
        assert_eq!(store.publish_workspace_stage(&full).unwrap().attempt, full);
        assert_eq!(store.store_counts().unwrap().commits, 4);
        let count: i64 = store
            .db
            .reader()
            .unwrap()
            .query_row("SELECT count(*) FROM workspace_publications", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, WORKSPACE_PUBLICATION_LIMIT as i64);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
