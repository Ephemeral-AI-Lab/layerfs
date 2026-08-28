use super::{no_change, LoseFirstPushAcknowledgement, Scenario};
use layerfs_storage::{BranchPushOutcome, RequestId, SyncTransferCounters};
use layerfs_sync::{abort_push_transfer, push_branch, push_objects, LocalDurable};
use layerfs_sync::{DurableControlEndpoint, ResumeToken, SyncError};
use layerfs_working_store::CommitResult;
use std::cell::Cell;

impl Scenario {
    pub(crate) fn qualify_publication(&mut self) {
        let endpoint = LocalDurable::new(&self.durable);
        let first_push = [0x44; 32];
        push_objects(
            &self.working,
            &endpoint,
            first_push,
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert_eq!(self.durable.branch_head(self.branch_id).unwrap(), None);
        self.durable
            .bootstrap_layer_stack(
                self.stack.layer_stack_id,
                self.stack.layer_id,
                "main",
                self.root,
            )
            .unwrap();

        let abandoned = RequestId::from_bytes([0x46; 32]);
        endpoint
            .stage_branch_push_page(
                abandoned,
                0,
                RequestId::from_bytes([0x60; 32]),
                &self
                    .working
                    .export_branch_push(self.branch_id, None)
                    .unwrap(),
                SyncTransferCounters::default(),
            )
            .unwrap();
        assert_eq!(
            self.durable.sync_custody_rows(abandoned, "push").unwrap(),
            1
        );
        assert_eq!(
            abort_push_transfer(&endpoint, *abandoned.as_bytes()).unwrap(),
            1
        );

        let misreported = RequestId::from_bytes([0x63; 32]);
        let misreported_data = RequestId::from_bytes([0x64; 32]);
        let transferred_id = self.object_ids[0];
        self.durable
            .sync_accept_objects(
                misreported,
                misreported_data,
                "push",
                &[(
                    transferred_id,
                    self.working
                        .sync_read_object(transferred_id, 1024 * 1024)
                        .unwrap(),
                )],
            )
            .unwrap();
        assert!(endpoint
            .stage_branch_push_page(
                misreported,
                0,
                misreported_data,
                &self
                    .working
                    .export_branch_push(self.branch_id, None)
                    .unwrap(),
                SyncTransferCounters::default(),
            )
            .is_err());
        assert_eq!(
            self.durable
                .abort_sync_transfer(misreported, "push")
                .unwrap(),
            2
        );

        let fabricated = RequestId::from_bytes([0x61; 32]);
        let fabricated_data = RequestId::from_bytes([0x62; 32]);
        let fabricated_bundle = self
            .working
            .export_branch_push(self.branch_id, None)
            .unwrap();
        let fabricated_counters = SyncTransferCounters::default();
        endpoint
            .stage_branch_push_page(
                fabricated,
                0,
                fabricated_data,
                &fabricated_bundle,
                fabricated_counters,
            )
            .unwrap();
        let page_digest = layerfs_storage::branch_push_bundle_page_digest(
            fabricated,
            0,
            fabricated_data,
            &fabricated_bundle,
            fabricated_counters,
        )
        .unwrap();
        let mut identity = layerfs_storage::BranchPushIdentityBuilder::new(fabricated);
        identity.absorb_page(0, page_digest).unwrap();
        assert!(self
            .durable
            .commit_staged_branch_push(
                layerfs_storage::BranchPushRequest {
                    request_id: fabricated,
                    transfer_id: fabricated,
                    candidate_digest: identity.finish(fabricated_bundle.head),
                    expected: None,
                    counters: SyncTransferCounters {
                        unique_bytes: 1,
                        ..SyncTransferCounters::default()
                    },
                },
                self.branch_id,
            )
            .is_err());
        assert_eq!(self.durable.branch_head(self.branch_id).unwrap(), None);
        assert_eq!(
            self.durable
                .abort_sync_transfer(fabricated, "push")
                .unwrap(),
            1
        );

        let lossy = LoseFirstPushAcknowledgement {
            durable: &self.durable,
            lose: Cell::new(true),
        };
        assert!(matches!(
            push_branch(
                &self.working,
                &lossy,
                first_push,
                self.branch_id,
                None,
                ResumeToken::default(),
            ),
            Err(SyncError::Destination(_))
        ));
        assert_eq!(
            self.working
                .push_outbox_state(RequestId::from_bytes(first_push))
                .unwrap()
                .as_deref(),
            Some("indeterminate")
        );
        assert_eq!(
            self.durable.branch_head(self.branch_id).unwrap(),
            Some(self.accepted)
        );
        assert_eq!(
            push_branch(
                &self.working,
                &endpoint,
                first_push,
                self.branch_id,
                None,
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted {
                head: self.accepted,
                reconciled: true,
            }
        );

        let next_begin = self.working.begin_operation(self.accepted).unwrap();
        let next = match self
            .working
            .operation_commit(next_begin, no_change(&next_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
        };
        let second_push = [0x47; 32];
        push_objects(
            &self.working,
            &endpoint,
            second_push,
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert!(matches!(
            push_branch(
                &self.working,
                &endpoint,
                second_push,
                self.branch_id,
                Some(self.accepted),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == next
        ));

        let stale_push = [0x48; 32];
        push_objects(
            &self.working,
            &endpoint,
            stale_push,
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert_eq!(
            push_branch(
                &self.working,
                &endpoint,
                stale_push,
                self.branch_id,
                Some(self.accepted),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::Conflict { actual: Some(next) }
        );
        assert_eq!(
            self.working
                .push_outbox_state(RequestId::from_bytes(stale_push))
                .unwrap()
                .as_deref(),
            Some("conflict")
        );
        self.next = Some(next);
    }
}
