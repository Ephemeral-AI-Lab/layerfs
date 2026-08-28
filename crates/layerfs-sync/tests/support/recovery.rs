use super::{no_change, Scenario};
use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_sync::ResumeToken;
use layerfs_sync::{fetch_branch, LocalDurable};
use layerfs_working_store::{CommitResult, WorkingStore};

impl Scenario {
    pub(crate) fn qualify_recovery(&self) {
        let child = self.child.expect("child history qualification");
        let child_head = self.child_head.expect("child head");
        let resumed_head = self.resumed_head.expect("resumed head");
        let durable_stack = self.durable_stack.expect("durable stack");
        let endpoint = LocalDurable::new(&self.durable);

        let working_c =
            WorkingStore::open(&self.base.join("working-c"), IntegrityMode::Verified).unwrap();
        let reconstructed = fetch_branch(
            &endpoint,
            &working_c,
            [0x56; 32],
            self.branch_id,
            ResumeToken::default(),
        )
        .unwrap();
        assert_eq!(reconstructed.head, resumed_head);
        assert_eq!(
            working_c.branch_head(child.branch_id).unwrap(),
            Some(child_head)
        );
        assert_eq!(
            working_c
                .layer_stack_head(self.stack.layer_stack_id)
                .unwrap(),
            Some(durable_stack)
        );

        let working_child = WorkingStore::open(
            &self.base.join("working-child-only"),
            IntegrityMode::Verified,
        )
        .unwrap();
        assert_eq!(
            fetch_branch(
                &endpoint,
                &working_child,
                [0x57; 32],
                child.branch_id,
                ResumeToken::default(),
            )
            .unwrap()
            .head,
            child_head
        );
        assert_eq!(
            working_child.branch_head(self.branch_id).unwrap(),
            Some(resumed_head)
        );
        drop(working_child);

        let reconstructed_begin = working_c.begin_operation(resumed_head).unwrap();
        assert!(matches!(
            working_c
                .operation_commit(
                    reconstructed_begin,
                    no_change(&reconstructed_begin, self.root),
                )
                .unwrap(),
            CommitResult::WorkingRecorded { .. }
        ));

        let backup = self.base.join("durable-backup.sqlite");
        self.durable.backup(&backup).unwrap();
        let restored = DurableStore::restore(&backup, &self.base.join("durable-restored")).unwrap();
        assert_eq!(restored.storage_id(), self.durable.storage_id());
        assert_eq!(
            restored.branch_head(self.branch_id).unwrap(),
            Some(resumed_head)
        );
        assert_eq!(
            restored.branch_head(child.branch_id).unwrap(),
            Some(child_head)
        );
        assert_eq!(
            restored
                .layer_stack_head(self.stack.layer_stack_id)
                .unwrap(),
            Some(durable_stack)
        );

        let restored_endpoint = LocalDurable::new(&restored);
        let working_d =
            WorkingStore::open(&self.base.join("working-d"), IntegrityMode::Verified).unwrap();
        assert_eq!(
            fetch_branch(
                &restored_endpoint,
                &working_d,
                [0x58; 32],
                self.branch_id,
                ResumeToken::default(),
            )
            .unwrap()
            .head,
            resumed_head
        );
        assert_eq!(
            working_d.branch_head(child.branch_id).unwrap(),
            Some(child_head)
        );
        drop(working_d);
        let restored_id = restored.storage_id();
        let restored = restored.compact().unwrap();
        assert_eq!(restored.storage_id(), restored_id);
        assert_eq!(
            restored.branch_head(self.branch_id).unwrap(),
            Some(resumed_head)
        );
        assert!(restored
            .database_path()
            .ends_with("generation-0000000000000001.sqlite"));
        drop(restored);
        drop(working_c);
    }
}
