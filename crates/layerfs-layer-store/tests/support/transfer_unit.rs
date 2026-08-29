use super::*;
use ed25519_dalek::{Signer, SigningKey};
use layerfs_storage::{AddResultRecord, CommitRecord, ResultId, SourceId, StackHistoryId};

#[test]
fn stack_attestation_and_relationship_fail_before_publication() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-stack-visibility-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStore::create(root.join("layer.sqlite")).unwrap();
    let (_, base) = store
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let key = SigningKey::from_bytes(&[7; 32]);
    let history_id = StackHistoryId::new(&key.verifying_key().to_bytes());
    let seed = StackRecord {
        id: StackId::derive(history_id, None, base.root_id),
        history_id,
        parent_id: None,
        root_id: base.root_id,
    };
    store
        .db
        .create_stack_history_record(
            StackHistoryRecord {
                id: history_id,
                base_layer_id: base.id,
                head_stack_id: seed.id,
            },
            seed,
        )
        .unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, None),
        root_id: base.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let incoming = StackRecord {
        id: StackId::derive(history_id, Some(seed.id), base.root_id),
        history_id,
        parent_id: Some(seed.id),
        root_id: base.root_id,
    };
    let branch = BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit.id,
        base_id: BaseId::Stack(seed.id),
    };
    let bad_result = AddResultRecord {
        source_id: SourceId::Branch(branch.id),
        result_id: ResultId::Layer(base.id),
    };
    let foundation = [Fact::Commit(commit), Fact::Stack(incoming)];
    let publication = [Fact::Branch(branch), Fact::AddResult(bad_result)];
    let (mut push, attestation) = signed_push(
        &key,
        history_id,
        base.id,
        seed.id,
        incoming.id,
        &foundation,
        &publication,
    );
    push.provenance_digest[0] ^= 1;
    push.signature = key.sign(&push.signing_bytes()).to_bytes();
    assert!(store
        .finish_received_transfer(
            &[],
            &foundation,
            TransferIntent::Stack(push),
            attestation,
            Some(publication_spool(&publication)),
        )
        .is_err());
    assert!(store.db.stack(incoming.id).unwrap().is_none());
    assert!(store.db.branch(branch.id).unwrap().is_none());

    let (push, attestation) = signed_push(
        &key,
        history_id,
        base.id,
        seed.id,
        incoming.id,
        &foundation,
        &publication,
    );
    assert!(store
        .finish_received_transfer(
            &[],
            &foundation,
            TransferIntent::Stack(push),
            attestation,
            Some(publication_spool(&publication)),
        )
        .is_err());
    assert_eq!(
        store
            .db
            .stack_history(history_id)
            .unwrap()
            .unwrap()
            .head_stack_id,
        seed.id
    );
    assert!(store.db.stack(incoming.id).unwrap().is_some());
    assert!(store.db.branch(branch.id).unwrap().is_none());
    assert!(store
        .db
        .add_result(SourceId::Branch(branch.id))
        .unwrap()
        .is_none());

    let after = StackRecord {
        id: StackId::derive(history_id, Some(incoming.id), base.root_id),
        history_id,
        parent_id: Some(incoming.id),
        root_id: base.root_id,
    };
    store.db.admit_facts(&[Fact::Stack(after)]).unwrap();
    let unattached = StackId::derive(history_id, Some(seed.id), ObjectId::for_bytes(b"divergent"));
    let wrong_history = StackHistoryId::new(&[31; 32]);
    let wrong = StackRecord {
        id: StackId::derive(wrong_history, None, base.root_id),
        history_id: wrong_history,
        parent_id: None,
        root_id: base.root_id,
    };
    store
        .db
        .create_stack_history_record(
            StackHistoryRecord {
                id: wrong_history,
                base_layer_id: base.id,
                head_stack_id: wrong.id,
            },
            wrong,
        )
        .unwrap();
    let good_result = |stack| AddResultRecord {
        source_id: SourceId::Branch(branch.id),
        result_id: ResultId::Stack(stack),
    };
    let (push, _) = signed_push(
        &key,
        history_id,
        base.id,
        seed.id,
        incoming.id,
        &foundation,
        &[
            Fact::Branch(branch),
            Fact::AddResult(good_result(incoming.id)),
        ],
    );
    let positions = store.db.stack_positions(history_id, incoming.id).unwrap();
    assert!(store
        .db
        .validate_stack_publication(&push, &[(branch, good_result(incoming.id))], &positions,)
        .is_ok());
    for result in [seed.id, after.id, unattached, wrong.id] {
        assert!(store
            .db
            .validate_stack_publication(&push, &[(branch, good_result(result))], &positions)
            .is_err());
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn large_publication_spools_in_bounded_pages_and_moves_head_last() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-large-publication-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStore::create(root.join("layer.sqlite")).unwrap();
    let (_, base) = store
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let key = SigningKey::from_bytes(&[9; 32]);
    let history_id = StackHistoryId::new(&key.verifying_key().to_bytes());
    let seed = StackRecord {
        id: StackId::derive(history_id, None, base.root_id),
        history_id,
        parent_id: None,
        root_id: base.root_id,
    };
    store
        .db
        .create_stack_history_record(
            StackHistoryRecord {
                id: history_id,
                base_layer_id: base.id,
                head_stack_id: seed.id,
            },
            seed,
        )
        .unwrap();
    let commit = CommitRecord {
        id: CommitId::derive(base.root_id, None, None),
        root_id: base.root_id,
        parent_id: None,
        merge_parent_id: None,
    };
    let incoming = StackRecord {
        id: StackId::derive(history_id, Some(seed.id), base.root_id),
        history_id,
        parent_id: Some(seed.id),
        root_id: base.root_id,
    };
    let mut branches = (0..700)
        .map(|_| BranchRecord {
            id: BranchId::new(),
            head_commit_id: commit.id,
            base_id: BaseId::Stack(seed.id),
        })
        .collect::<Vec<_>>();
    branches.sort_by_key(|branch| branch.id);
    let mut publication = branches
        .iter()
        .copied()
        .map(Fact::Branch)
        .collect::<Vec<_>>();
    publication.extend(branches.iter().map(|branch| {
        Fact::AddResult(AddResultRecord {
            source_id: SourceId::Branch(branch.id),
            result_id: ResultId::Stack(incoming.id),
        })
    }));
    assert!(
        publication
            .iter()
            .map(|fact| fact.encoded_size())
            .sum::<usize>()
            > 64 * 1024
    );
    let foundation = [Fact::Commit(commit), Fact::Stack(incoming)];
    let (push, attestation) = signed_push(
        &key,
        history_id,
        base.id,
        seed.id,
        incoming.id,
        &foundation,
        &publication,
    );
    for batch in layerfs_storage::fact_batches(&foundation).unwrap() {
        store.db.admit_facts(batch).unwrap();
    }
    for known in [
        branches
            .iter()
            .step_by(2)
            .copied()
            .map(Fact::Branch)
            .collect::<Vec<_>>(),
        publication[branches.len()..]
            .iter()
            .step_by(2)
            .copied()
            .collect::<Vec<_>>(),
    ] {
        for batch in layerfs_storage::fact_batches(&known).unwrap() {
            store.db.admit_facts(batch).unwrap();
        }
    }
    let mut publication_store = None;
    let mut sent = 0;
    for typed in [
        &publication[..branches.len()],
        &publication[branches.len()..],
    ] {
        for page in typed.chunks(layerfs_storage::ID_BATCH_COUNT) {
            let kind = page[0].kind();
            let ids = page.iter().copied().map(Fact::id).collect::<Vec<_>>();
            let missing = store.db.missing_facts(kind, &ids).unwrap();
            let selected = page
                .iter()
                .enumerate()
                .filter(|(index, _)| missing.is_missing(*index).unwrap())
                .map(|(_, fact)| *fact)
                .collect::<Vec<_>>();
            sent += selected.len();
            let mut pending =
                PublicationPage::begin(&store, kind, &ids, missing, &mut publication_store)
                    .unwrap();
            for batch in layerfs_storage::fact_batches(&selected).unwrap() {
                assert_eq!(
                    pending
                        .as_mut()
                        .unwrap()
                        .receive(&store, batch, &mut publication_store)
                        .unwrap(),
                    batch.as_ptr_range().end == selected.as_ptr_range().end
                );
            }
        }
    }
    assert_eq!(sent, publication.len() / 2);
    let mut spool = publication_store.unwrap();
    let mut pages = 0;
    let mut admitted = 0;
    spool
        .visit_batches(&mut |facts, _| {
            pages += 1;
            admitted += facts.len();
            Ok(())
        })
        .unwrap();
    assert!(pages > 2);
    assert_eq!(admitted, sent);
    assert!(spool.peak_batch_bytes <= FACT_BATCH_BYTES);
    assert!(!spool.spilled());
    store
        .finish_received_transfer(
            &[],
            &foundation,
            TransferIntent::Stack(push),
            attestation,
            Some(spool),
        )
        .unwrap();
    assert_eq!(
        store
            .db
            .stack_history(history_id)
            .unwrap()
            .unwrap()
            .head_stack_id,
        incoming.id
    );
    assert!(store.db.branch(branches[0].id).unwrap().is_some());
    assert!(store.db.branch(branches[699].id).unwrap().is_some());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn hundred_thousand_publication_relations_use_one_position_walk_and_fixed_pages() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-publication-scale-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let store = LayerStore::create(root.join("layer.sqlite")).unwrap();
    let (_, base) = store
        .initialize(layerfs_storage::LayerInitialization::Empty)
        .unwrap();
    let history_id = StackHistoryId::new(&[41; 32]);
    let seed = StackRecord {
        id: StackId::derive(history_id, None, base.root_id),
        history_id,
        parent_id: None,
        root_id: base.root_id,
    };
    store
        .db
        .create_stack_history_record(
            StackHistoryRecord {
                id: history_id,
                base_layer_id: base.id,
                head_stack_id: seed.id,
            },
            seed,
        )
        .unwrap();
    let commit = CommitId::derive(base.root_id, None, None);
    let mut parent = seed.id;
    let mut pairs = Vec::with_capacity(100_000);
    for start in (0..100_000).step_by(FACT_BATCH_COUNT) {
        let mut facts = Vec::with_capacity(FACT_BATCH_COUNT);
        for index in start..(start + FACT_BATCH_COUNT).min(100_000) {
            let stack = StackRecord {
                id: StackId::derive(history_id, Some(parent), base.root_id),
                history_id,
                parent_id: Some(parent),
                root_id: base.root_id,
            };
            let mut branch_id = [0; 17];
            branch_id[0] = 0x11;
            branch_id[1..].copy_from_slice(&(index as u128 + 1).to_be_bytes());
            let branch = BranchRecord {
                id: BranchId::from_bytes(branch_id).unwrap(),
                head_commit_id: commit,
                base_id: BaseId::Stack(parent),
            };
            pairs.push((
                branch,
                AddResultRecord {
                    source_id: SourceId::Branch(branch.id),
                    result_id: ResultId::Stack(stack.id),
                },
            ));
            facts.push(Fact::Stack(stack));
            parent = stack.id;
        }
        store.db.admit_facts(&facts).unwrap();
    }
    let push = StackPush {
        history_id,
        base_layer_id: base.id,
        expected_head: Some(seed.id),
        incoming_head: parent,
        fact_count: 0,
        root_count: 0,
        provenance_digest: [0; 32],
        publication_count: 0,
        publication_digest: [0; 32],
        public_key: [0; 32],
        signature: [0; 64],
    };
    let started = std::time::Instant::now();
    let positions = store.db.stack_positions(history_id, parent).unwrap();
    assert_eq!(positions.position(parent).unwrap(), Some(0));
    assert_eq!(positions.position(seed.id).unwrap(), Some(100_000));
    let mut pages = 0;
    for page in pairs.chunks(64) {
        store
            .db
            .validate_stack_publication(&push, page, &positions)
            .unwrap();
        pages += 1;
    }
    assert_eq!(pages, 1_563);
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn publication_larger_than_one_wire_frame_spills_and_replays_in_bounded_batches() {
    let root = ObjectId::for_bytes(b"publication-frame");
    let commit = CommitId::derive(root, None, None);
    let history = StackHistoryId::new(&[53; 32]);
    let base = BaseId::Stack(StackId::derive(history, None, root));
    let sample = Fact::Branch(BranchRecord {
        id: BranchId::new(),
        head_commit_id: commit,
        base_id: base,
    });
    let count = layerfs_storage::MAX_FRAME_BYTES / sample.encoded_size() + 1;
    let mut spool = PublicationSpool::new().unwrap();
    for start in (0..count).step_by(FACT_BATCH_COUNT) {
        let page = (start..(start + FACT_BATCH_COUNT).min(count))
            .map(|index| {
                let mut id = [0; 17];
                id[0] = 0x11;
                id[1..].copy_from_slice(&(index as u128 + 1).to_be_bytes());
                Fact::Branch(BranchRecord {
                    id: BranchId::from_bytes(id).unwrap(),
                    head_commit_id: commit,
                    base_id: base,
                })
            })
            .collect::<Vec<_>>();
        spool.push(&page, true).unwrap();
    }
    assert!(count * sample.encoded_size() > layerfs_storage::MAX_FRAME_BYTES);
    assert!(spool.spilled());
    let mut replayed = 0;
    let mut pages = 0;
    spool
        .visit_batches(&mut |facts, _| {
            assert!(facts.len() <= FACT_BATCH_COUNT);
            assert!(
                facts.iter().map(|fact| fact.encoded_size()).sum::<usize>() <= FACT_BATCH_BYTES
            );
            replayed += facts.len();
            pages += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(replayed, count);
    assert!(pages > layerfs_storage::MAX_FRAME_BYTES / FACT_BATCH_BYTES);
}

fn publication_spool(facts: &[Fact]) -> PublicationSpool {
    let mut spool = PublicationSpool::new().unwrap();
    for page in layerfs_storage::fact_batches(facts).unwrap() {
        spool.push(page, true).unwrap();
    }
    spool
}

fn signed_push(
    key: &SigningKey,
    history_id: StackHistoryId,
    base_layer_id: LayerId,
    expected: StackId,
    incoming: StackId,
    foundation: &[Fact],
    publication: &[Fact],
) -> (StackPush, StackAttestation) {
    let mut attestation = StackAttestation::default();
    let mut summary = StackAttestation::default();
    for kind in [FactKind::Commit, FactKind::Stack] {
        let mut ids = foundation
            .iter()
            .filter(|fact| fact.kind() == kind)
            .copied()
            .map(Fact::id)
            .collect::<Vec<_>>();
        ids.sort();
        attestation.observe(kind, &ids);
        summary.observe(kind, &ids);
    }
    let (fact_count, root_count, provenance_digest) = summary.finish();
    let mut publication_hasher = blake3::Hasher::new();
    publication_hasher.update(b"layerfs/stack-publication/v1\0");
    for fact in publication {
        let bytes = fact.signing_bytes();
        publication_hasher.update(&(bytes.len() as u64).to_be_bytes());
        publication_hasher.update(&bytes);
    }
    let mut push = StackPush {
        history_id,
        base_layer_id,
        expected_head: Some(expected),
        incoming_head: incoming,
        fact_count,
        root_count,
        provenance_digest,
        publication_count: publication.len() as u64,
        publication_digest: *publication_hasher.finalize().as_bytes(),
        public_key: key.verifying_key().to_bytes(),
        signature: [0; 64],
    };
    push.signature = key.sign(&push.signing_bytes()).to_bytes();
    (push, attestation)
}
