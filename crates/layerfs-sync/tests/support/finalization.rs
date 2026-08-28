use super::{no_change, object_ids, valid_empty_root, Scenario};
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::{BranchPushOutcome, LayerId, LayerStackMergeOutcome};
use layerfs_sync::{fetch_branch, push_branch, push_objects, LocalDurable};
use layerfs_sync::{ResumeToken, SyncError};
use layerfs_working_store::{CommitResult, LayerPreparationResult, WorkingStore};

impl Scenario {
    pub(crate) fn qualify_checkout_and_finalization(&mut self) {
        let next = self.next.expect("publication qualification");
        let endpoint = LocalDurable::new(&self.durable);
        let working_b_path = self.base.join("working-b");
        let mut working_b = WorkingStore::open(&working_b_path, IntegrityMode::Verified).unwrap();
        working_b.inject_fetch_boundary_failure_for_test();
        match fetch_branch(
            &endpoint,
            &working_b,
            [0x45; 32],
            self.branch_id,
            ResumeToken::default(),
        ) {
            Err(SyncError::Destination(_)) => {}
            other => panic!("expected injected Fetch publication failure, got {other:?}"),
        }
        assert_eq!(working_b.branch_head(self.branch_id).unwrap(), None);
        assert!(!working_b
            .has_verified_branch_tracking(self.durable.storage_id(), next)
            .unwrap());
        drop(working_b);

        let mut working_b = WorkingStore::open(&working_b_path, IntegrityMode::Verified).unwrap();
        let fetched = fetch_branch(
            &endpoint,
            &working_b,
            [0x45; 32],
            self.branch_id,
            ResumeToken::default(),
        )
        .unwrap();
        assert_eq!(fetched.head, next);
        assert_eq!(fetched.terminal_object_page_entries, 0);
        assert_eq!(fetched.transfer.terminal_buffer_bytes, 0);
        assert_eq!(fetched.transfer.terminal_queued_batches, 0);
        assert!(fetched.complete_wall_ns >= fetched.head_transaction_ns);
        assert!(working_b
            .has_verified_branch_tracking(self.durable.storage_id(), next)
            .unwrap());

        let continued_begin = working_b.begin_operation(next).unwrap();
        let (continued, continued_record) = match working_b
            .operation_commit(continued_begin, no_change(&continued_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, record, .. } => (head, record),
            CommitResult::Conflict { .. } => panic!("unexpected fetched Working conflict"),
        };
        push_objects(
            &working_b,
            &endpoint,
            [0x49; 32],
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert!(matches!(
            push_branch(
                &working_b,
                &endpoint,
                [0x49; 32],
                self.branch_id,
                Some(next),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == continued
        ));

        let candidate = match working_b
            .prepare_layer_stack_merge(continued, self.stack)
            .unwrap()
        {
            LayerPreparationResult::Prepared(candidate) => candidate,
            other => panic!("Layer candidate failed: {other:?}"),
        };
        let alternate_root = valid_empty_root(&mut working_b);
        let working_b_ids = object_ids(&working_b);
        push_objects(
            &working_b,
            &endpoint,
            [0x4a; 32],
            working_b_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        let mut malicious_layer = candidate;
        malicious_layer.root = alternate_root;
        malicious_layer.layer_id = LayerId::from_bytes(layerfs_storage::derive_id(
            b"candidate-layer",
            &[
                self.stack.layer_stack_id.as_bytes(),
                malicious_layer.request_id.as_bytes(),
                alternate_root.as_bytes(),
            ],
        ));
        assert!(self
            .durable
            .accept_layer_stack_merge(malicious_layer, self.stack)
            .is_err());
        let merged_stack = match self
            .durable
            .accept_layer_stack_merge(candidate, self.stack)
            .unwrap()
        {
            LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
            other => panic!("Durable LayerStack merge failed: {other:?}"),
        };
        assert_eq!(merged_stack.layer_id, candidate.layer_id);
        match self
            .durable
            .layer_stack_rollback(merged_stack, self.stack.layer_id)
            .unwrap()
        {
            layerfs_storage::LayerStackRollbackOutcome::DurablyAccepted { head, .. } => {
                assert_eq!(head.layer_id, self.stack.layer_id)
            }
            other => panic!("Durable LayerStack rollback failed: {other:?}"),
        }
        assert_eq!(
            self.durable.branch_head(self.branch_id).unwrap(),
            Some(continued)
        );

        self.working_b = Some(working_b);
        self.alternate_root = Some(alternate_root);
        self.continued = Some(continued);
        self.continued_record = Some(continued_record);
    }
}
