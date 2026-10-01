//! Typed final shape, exact common output and pre-provider foreign/replay refusal.
mod support;
use layerfs_content::{
    file::edit::{apply_edits_without_drafts, NoDraft},
    ConstructionPolicy, Edit, EditRequest, ObjectId,
};
use support::{
    build_file, disabled_scope,
    edits::{Edits, Parts},
    read_back, Counted, CountingProvider, MemoryStore,
};
#[test]
fn opaque_whole_shape_uses_same_canonical_producer_and_refuses_replay_before_reads() {
    let policy = ConstructionPolicy::frozen_default();
    let old = b"original no draft bytes";
    let (store, base) = build_file(old);
    let expected = b"replacement known bytes";
    // Independent frozen whole value, before the candidate is constructed.
    let mut value = b"LFS5SML\0".to_vec();
    value.extend_from_slice(&1u16.to_be_bytes());
    value.extend_from_slice(expected);
    let canonical = layerfs_content::object::encode_bytes_object(&value).unwrap();
    let expected_root = ObjectId::for_bytes(&canonical);
    let edits = Edits::new(
        old.len() as u64,
        vec![Edit::new(0, old.len() as u64, expected.len() as u64)],
    )
    .unwrap();
    let mut parts = Parts::new();
    parts.push(expected.to_vec());
    let mut state =
        NoDraft::new(policy, base.root, old.len() as u64, expected.len() as u64).unwrap();
    let counts = CountingProvider::new();
    let reader = Counted {
        store: &store,
        counts: &counts,
    };
    let mut output = MemoryStore::new();
    let result = disabled_scope(|scope| {
        apply_edits_without_drafts(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root: base.root,
                edits: &edits,
                source: &parts,
            },
            &mut output,
            &mut state,
            scope.child("no-draft"),
        )
    })
    .unwrap();
    assert_eq!(result.root, expected_root);
    assert!(state.completed());
    assert_eq!(output.canonical(result.root), Some(canonical.as_slice()));
    assert_eq!(result.counters.nodes_created, 0);
    assert_eq!(result.counters.peak_deferred_bytes, 0);
    let read_count = counts.objects();
    let emitted = output.len();
    assert!(disabled_scope(|scope| apply_edits_without_drafts(
        policy,
        &policy.capacities(),
        &reader,
        EditRequest {
            root: base.root,
            edits: &edits,
            source: &parts
        },
        &mut output,
        &mut state,
        scope.child("replay")
    ))
    .is_err());
    assert_eq!((counts.objects(), output.len()), (read_count, emitted));
    let mut merged = store.merged_clone();
    merged.absorb(&output);
    assert_eq!(read_back(&merged, result.root).unwrap(), expected);
    assert_eq!(read_back(&store, base.root).unwrap(), old);
}
#[test]
fn excluded_chunked_shape_foreign_final_policy_and_consumer_failure_cannot_complete() {
    let policy = ConstructionPolicy::frozen_default();
    let (store, base) = build_file(b"old");
    assert!(NoDraft::new(policy, base.root, 3, policy.small_file_threshold_bytes()).is_err());
    let edits = Edits::new(3, vec![Edit::new(0, 3, 1)]).unwrap();
    let mut parts = Parts::new();
    parts.push(vec![9]);
    for (captured, declared) in [(policy, 2), (ConstructionPolicy::new(262144, 8, 4), 1)] {
        let mut state = NoDraft::new(captured, base.root, 3, declared).unwrap();
        let counts = CountingProvider::new();
        let reader = Counted {
            store: &store,
            counts: &counts,
        };
        let mut output = MemoryStore::new();
        assert!(disabled_scope(|scope| apply_edits_without_drafts(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root: base.root,
                edits: &edits,
                source: &parts
            },
            &mut output,
            &mut state,
            scope.child("foreign")
        ))
        .is_err());
        assert_eq!((counts.objects(), output.len()), (0, 0));
        assert!(!state.completed());
    }
    struct Refuse;
    impl layerfs_content::FinalizedConsumer for Refuse {
        fn accept(
            &mut self,
            _object: layerfs_content::FinalizedObject,
        ) -> layerfs_content::ContentResult<()> {
            Err(layerfs_content::ContentError::Io)
        }
    }
    let mut state = NoDraft::new(policy, base.root, 3, 1).unwrap();
    assert!(disabled_scope(|scope| apply_edits_without_drafts(
        policy,
        &policy.capacities(),
        &store,
        EditRequest {
            root: base.root,
            edits: &edits,
            source: &parts
        },
        &mut Refuse,
        &mut state,
        scope.child("consumer")
    ))
    .is_err());
    assert!(!state.completed());
}

#[test]
fn inconsistent_empty_sequence_refuses_captured_shape_before_provider_or_emission() {
    struct Malformed;
    impl layerfs_content::EditSequence for Malformed {
        fn base_len(&self) -> u64 {
            3
        }
        fn final_len(&self) -> u64 {
            1
        }
        fn len(&self) -> usize {
            0
        }
        fn edit_at(&self, _at: usize) -> layerfs_content::ContentResult<Edit> {
            panic!("invalid empty shape must not descend an edit");
        }
    }
    let policy = ConstructionPolicy::frozen_default();
    let (store, base) = build_file(b"old");
    let counts = CountingProvider::new();
    let reader = Counted {
        store: &store,
        counts: &counts,
    };
    let mut output = MemoryStore::new();
    let mut state = NoDraft::new(policy, base.root, 3, 1).unwrap();
    let error = disabled_scope(|scope| {
        apply_edits_without_drafts(
            policy,
            &policy.capacities(),
            &reader,
            EditRequest {
                root: base.root,
                edits: &Malformed,
                source: &Parts::new(),
            },
            &mut output,
            &mut state,
            scope.child("malformed-empty"),
        )
    })
    .unwrap_err();
    assert!(matches!(
        error,
        layerfs_content::ContentError::InvalidEdit {
            what: "no-draft empty sequence length"
        }
    ));
    assert_eq!((counts.objects(), output.len()), (0, 0));
    assert!(!state.completed());
}
