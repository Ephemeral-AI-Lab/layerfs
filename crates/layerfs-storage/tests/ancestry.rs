use layerfs_content::ObjectId;
use layerfs_storage::{
    commit_merge_base, commit_merge_base_plan, CommitId, CommitRecord, Fact, MergeBaseOutcome,
    StoreDb, StoreRole, FACT_BATCH_COUNT,
};

fn path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "layerfs-{label}-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn recursive_cte_selects_one_maximal_commit() {
    let path = path("merge-base");
    let db = StoreDb::create(&path, StoreRole::Branch).unwrap();
    let root = ObjectId::for_bytes(b"root");
    let a = CommitId::derive(root, None, None);
    let b = CommitId::derive(root, Some(a), None);
    let c = CommitId::derive(ObjectId::for_bytes(b"other"), Some(a), None);
    db.admit_facts(&[
        Fact::Commit(CommitRecord {
            id: a,
            root_id: root,
            parent_id: None,
            merge_parent_id: None,
        }),
        Fact::Commit(CommitRecord {
            id: b,
            root_id: root,
            parent_id: Some(a),
            merge_parent_id: None,
        }),
        Fact::Commit(CommitRecord {
            id: c,
            root_id: ObjectId::for_bytes(b"other"),
            parent_id: Some(a),
            merge_parent_id: None,
        }),
    ])
    .unwrap();
    assert_eq!(
        commit_merge_base(&db, b, c).unwrap(),
        MergeBaseOutcome::Commit(a)
    );
    let plan = commit_merge_base_plan(&db).unwrap().join("\n");
    assert!(
        plan.contains("commits_parent") || plan.contains("commits_merge_parent"),
        "{plan}"
    );
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn ancestry_cursor_steps_one_union_cte_in_fixed_pages() {
    let path = path("ancestry-page");
    let db = StoreDb::create(&path, StoreRole::Branch).unwrap();
    let root = ObjectId::for_bytes(b"root");
    let mut parent = None;
    let mut facts = Vec::new();
    for _ in 0..1025 {
        let id = CommitId::derive(root, parent, None);
        facts.push(Fact::Commit(CommitRecord {
            id,
            root_id: root,
            parent_id: parent,
            merge_parent_id: None,
        }));
        parent = Some(id);
    }
    for page in facts.chunks(FACT_BATCH_COUNT) {
        db.admit_facts(page).unwrap();
    }
    let head = parent.unwrap();
    let mut sizes = Vec::new();
    let mut last = None;
    db.visit_commit_ancestry(head, None, &mut |_, page| {
        sizes.push(page.len());
        last = page.last().map(|commit| commit.id);
        Ok(())
    })
    .unwrap();
    assert_eq!(sizes, [512, 512, 1]);
    assert_eq!(last, Some(head));
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn merge_base_scales_over_one_hundred_thousand_linear_commits() {
    let path = path("merge-base-100k");
    let db = StoreDb::create(&path, StoreRole::Branch).unwrap();
    let (checkpoint, head) = admit_linear(&db, 100_000, 9_999);
    let started = std::time::Instant::now();
    assert_eq!(
        commit_merge_base(&db, head, checkpoint).unwrap(),
        MergeBaseOutcome::Commit(checkpoint)
    );
    eprintln!("100k linear merge-base elapsed={:?}", started.elapsed());
    drop(db);
    let _ = std::fs::remove_file(path);
}

#[test]
fn merge_base_filters_a_large_unrelated_descendant_subgraph_by_immediate_edges() {
    let path = path("merge-base-diamond");
    let db = StoreDb::create(&path, StoreRole::Branch).unwrap();
    let root = ObjectId::for_bytes(b"diamond-root");
    let a = commit(root, None, None);
    let b = commit(ObjectId::for_bytes(b"diamond-left"), Some(a.id), None);
    let c = commit(ObjectId::for_bytes(b"diamond-right"), Some(a.id), None);
    let d = commit(root, Some(b.id), Some(c.id));
    let e = commit(root, Some(c.id), Some(b.id));
    db.admit_facts(&[a, b, c, d, e].map(Fact::Commit)).unwrap();
    let _ = admit_linear_from(&db, 10_000, Some(a.id), b"unrelated");
    assert!(matches!(
        commit_merge_base(&db, d.id, e.id),
        Err(layerfs_storage::StorageError::AmbiguousMergeBase)
    ));
    let plan = commit_merge_base_plan(&db).unwrap().join("\n");
    eprintln!("diamond merge-base plan:\n{plan}");
    assert!(!plan.contains("descendants"));
    assert!(plan.contains("commits_parent") || plan.contains("commits_merge_parent"));
    drop(db);
    let _ = std::fs::remove_file(path);
}

fn admit_linear(db: &StoreDb, count: usize, checkpoint: usize) -> (CommitId, CommitId) {
    let mut checkpoint_id = None;
    let head = admit_linear_with(db, count, None, b"linear", |index, id| {
        if index == checkpoint {
            checkpoint_id = Some(id);
        }
    });
    (checkpoint_id.unwrap(), head)
}

fn admit_linear_from(
    db: &StoreDb,
    count: usize,
    parent: Option<CommitId>,
    label: &[u8],
) -> CommitId {
    admit_linear_with(db, count, parent, label, |_, _| {})
}

fn admit_linear_with(
    db: &StoreDb,
    count: usize,
    mut parent: Option<CommitId>,
    label: &[u8],
    mut visitor: impl FnMut(usize, CommitId),
) -> CommitId {
    let root = ObjectId::for_bytes(label);
    let mut page = Vec::with_capacity(FACT_BATCH_COUNT);
    for index in 0..count {
        let value = commit(root, parent, None);
        parent = Some(value.id);
        visitor(index, value.id);
        page.push(Fact::Commit(value));
        if page.len() == FACT_BATCH_COUNT {
            db.admit_facts(&page).unwrap();
            page.clear();
        }
    }
    if !page.is_empty() {
        db.admit_facts(&page).unwrap();
    }
    parent.unwrap()
}

fn commit(
    root_id: ObjectId,
    parent_id: Option<CommitId>,
    merge_parent_id: Option<CommitId>,
) -> CommitRecord {
    CommitRecord {
        id: CommitId::derive(root_id, parent_id, merge_parent_id),
        root_id,
        parent_id,
        merge_parent_id,
    }
}
