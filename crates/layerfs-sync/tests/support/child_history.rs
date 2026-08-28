use super::{no_change, Scenario};
use layerfs_storage::{
    BranchId, BranchPushOutcome, BranchRollbackOutcome, ChildMergeOutcome, OperationVersionId,
};
use layerfs_sync::LocalDurable;
use layerfs_sync::ResumeToken;
use layerfs_sync::{push_branch, push_branch_rollback, push_child_branch_merge, push_objects};
use layerfs_working_store::{BranchRollbackResult, ChildMergeResult, CommitResult};

impl Scenario {
    pub(crate) fn qualify_child_merge_and_rollback(&mut self) {
        let continued = self.continued.expect("finalization qualification");
        let continued_record = self.continued_record.expect("continued operation record");
        let alternate_root = self.alternate_root.expect("alternate root");
        let endpoint = LocalDurable::new(&self.durable);
        let working = self.working_b.as_mut().expect("checkout qualification");

        let child = working
            .create_child_branch(
                BranchId::from_bytes([0x50; 32]),
                Some("child"),
                continued_record,
            )
            .unwrap();
        let child_begin = working.begin_operation(child).unwrap();
        let child_head = match working
            .operation_commit(child_begin, no_change(&child_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("child operation conflicted"),
        };
        push_objects(
            working,
            &endpoint,
            [0x51; 32],
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x51; 32],
                child.branch_id,
                None,
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == child_head
        ));

        let parent_begin = working.begin_operation(continued).unwrap();
        let parent_next = match working
            .operation_commit(parent_begin, no_change(&parent_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("parent operation conflicted"),
        };
        push_objects(
            working,
            &endpoint,
            [0x52; 32],
            self.object_ids.iter().copied(),
            ResumeToken::default(),
        )
        .unwrap();
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x52; 32],
                self.branch_id,
                Some(continued),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == parent_next
        ));

        let (merged_parent, publication) =
            match working.child_branch_merge(child_head, parent_next).unwrap() {
                ChildMergeResult::WorkingRecorded {
                    parent_head,
                    publication,
                    ..
                } => (parent_head, publication),
                other => panic!("Working ChildBranchMerge failed: {other:?}"),
            };
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x59; 32],
                self.branch_id,
                Some(parent_next),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == merged_parent
        ));
        let mut malicious_merge = publication.clone();
        malicious_merge.candidate.result_root = alternate_root;
        malicious_merge.accepted_parent.root = alternate_root;
        malicious_merge.accepted_parent.operation_version_id =
            Some(OperationVersionId::from_bytes(layerfs_storage::derive_id(
                b"child-merge-operation-version",
                &[
                    parent_next.branch_id.as_bytes(),
                    malicious_merge.candidate.request_id.as_bytes(),
                    alternate_root.as_bytes(),
                ],
            )));
        assert!(push_child_branch_merge(&endpoint, malicious_merge).is_err());
        assert!(matches!(
            push_child_branch_merge(&endpoint, publication.clone()).unwrap(),
            ChildMergeOutcome::WorkingRecorded { parent_head, .. } if parent_head == merged_parent
        ));
        assert!(matches!(
            push_child_branch_merge(&endpoint, publication).unwrap(),
            ChildMergeOutcome::WorkingRecorded {
                parent_head,
                reconciled: true,
            } if parent_head == merged_parent
        ));

        let after_merge_begin = working.begin_operation(merged_parent).unwrap();
        let after_merge = match working
            .operation_commit(after_merge_begin, no_change(&after_merge_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("post-merge operation conflicted"),
        };
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x53; 32],
                self.branch_id,
                Some(merged_parent),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == after_merge
        ));

        let rollback_source_begin = working.begin_operation(after_merge).unwrap();
        let rollback_source = match working
            .operation_commit(
                rollback_source_begin,
                no_change(&rollback_source_begin, self.root),
            )
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("pre-rollback operation conflicted"),
        };
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x54; 32],
                self.branch_id,
                Some(after_merge),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == rollback_source
        ));
        let (rolled_back, rollback_publication) = match working
            .branch_rollback(rollback_source, after_merge.operation_version_id.unwrap())
            .unwrap()
        {
            BranchRollbackResult::WorkingRecorded {
                head, publication, ..
            } => (head, publication),
            other => panic!("Working BranchRollback failed: {other:?}"),
        };
        assert!(matches!(
            push_branch_rollback(&endpoint, rollback_publication).unwrap(),
            BranchRollbackOutcome::WorkingRecorded { head, .. } if head == rolled_back
        ));
        assert!(matches!(
            push_branch_rollback(&endpoint, rollback_publication).unwrap(),
            BranchRollbackOutcome::WorkingRecorded {
                head,
                reconciled: true,
            } if head == rolled_back
        ));

        let durable_stack = self
            .durable
            .layer_stack_head(self.stack.layer_stack_id)
            .unwrap()
            .unwrap();
        let resumed_begin = working.begin_operation(rolled_back).unwrap();
        let resumed_head = match working
            .operation_commit(resumed_begin, no_change(&resumed_begin, self.root))
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, .. } => head,
            CommitResult::Conflict { .. } => panic!("post-rollback operation conflicted"),
        };
        assert!(matches!(
            push_branch(
                working,
                &endpoint,
                [0x55; 32],
                self.branch_id,
                Some(rolled_back),
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted { head, .. } if head == resumed_head
        ));
        assert!(matches!(
            push_branch(
                &self.working,
                &endpoint,
                [0x44; 32],
                self.branch_id,
                None,
                ResumeToken::default(),
            )
            .unwrap()
            .outcome,
            BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: true,
            } if head == self.accepted
        ));
        assert_eq!(
            self.durable.branch_head(self.branch_id).unwrap(),
            Some(resumed_head)
        );

        self.child = Some(child);
        self.child_head = Some(child_head);
        self.resumed_head = Some(resumed_head);
        self.durable_stack = Some(durable_stack);
    }
}
