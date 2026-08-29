use super::*;
use crate::{LayerHistoryId, StoreRole};
use layerfs_content::ObjectId;

#[test]
fn final_fact_folds_branch_visibility_and_up_to_date_writes_nothing() {
    let root = run_dir("folded-transfer");
    let db = StoreDb::create(root.join("store.sqlite"), StoreRole::Branch).unwrap();
    let mut facts = Vec::new();
    let mut parent = None;
    for index in 0..=FACT_BATCH_COUNT {
        let root_id = ObjectId::for_bytes(&(index as u64).to_be_bytes());
        let commit = CommitRecord {
            id: CommitId::derive(root_id, parent, None),
            root_id,
            parent_id: parent,
            merge_parent_id: None,
        };
        parent = Some(commit.id);
        facts.push(Fact::Commit(commit));
    }
    let Fact::Commit(commit) = facts[facts.len() - 1] else {
        unreachable!()
    };
    let branch = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Layer(LayerId::derive(LayerHistoryId::new(), None, commit.root_id)),
    };
    let intent = TransferIntent::Branch {
        branch,
        expected: None,
    };
    let (exchange, outcome) = db.finish_transfer(&[], &facts, intent.clone()).unwrap();
    let admission = exchange.into_parts().0;
    assert_eq!(admission.database.write_transactions, 2);
    assert_eq!(admission.database.fact_admission_transactions, 2);
    assert_eq!(admission.database.visibility_transactions, 1);
    assert_eq!(
        outcome,
        TransferOutcome::Commit(crate::RefOutcome::Created(commit.id))
    );
    let (exchange, outcome) = db.finish_transfer(&[], &[], intent).unwrap();
    assert_eq!(exchange.into_parts().0.database.write_transactions, 0);
    assert_eq!(
        outcome,
        TransferOutcome::Commit(crate::RefOutcome::UpToDate(commit.id))
    );
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn receipt_separates_local_candidate_reuse_facts_and_transactions() {
    let root = run_dir("local-receipt");
    let db = StoreDb::create(root.join("branch.sqlite"), StoreRole::Branch).unwrap();
    let base_root = ObjectId::for_bytes(b"base");
    let anchor = CommitRecord {
        id: CommitId::derive(base_root, None, None),
        root_id: base_root,
        parent_id: None,
        merge_parent_id: None,
    };
    let layer = LayerId::derive(LayerHistoryId::new(), None, base_root);
    let branches = [BranchId::new(), BranchId::new()];
    for id in branches {
        db.create_branch(
            BranchRecord {
                id,
                head_commit_id: anchor.id,
                base_id: BaseId::Layer(layer),
            },
            anchor,
        )
        .unwrap();
    }
    let built = crate::empty_root([9; 32]).unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(built.root_id, Some(anchor.id), None),
        root_id: built.root_id,
        parent_id: Some(anchor.id),
        merge_parent_id: None,
    };
    for branch in branches {
        db.commit_branch(branch, anchor.id, commit, Some(&built.objects))
            .unwrap();
    }
    let receipts = crate::take_storage_receipts();
    assert_eq!(receipts.len(), 2);
    let local = receipts
        .iter()
        .map(|receipt| match receipt {
            crate::StorageReceipt::Local(receipt) => receipt,
            _ => panic!("local receipt"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        local[0].objects.candidate_bytes,
        local[0].objects.inserted_bytes + local[0].objects.reused_bytes
    );
    assert_eq!(local[1].objects.inserted_bytes, 0);
    assert_eq!(
        local[1].objects.reused_bytes,
        local[1].objects.candidate_bytes
    );
    assert_eq!(local[0].facts[&crate::FactKind::Commit].inserted_ids, 1);
    assert_eq!(
        local[1].facts[&crate::FactKind::Commit].raced_existing_ids,
        1
    );
    assert_eq!(local[0].database.visibility_transactions, 1);
    assert_eq!(local[1].database.visibility_transactions, 1);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}

fn run_dir(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "layerfs-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}
