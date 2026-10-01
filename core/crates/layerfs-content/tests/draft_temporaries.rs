//! Independent shared temporary-name and selected-root ownership oracles.
mod support;
use layerfs_content::{
    file::{
        edit::{apply_edits_with_state, DraftRecord, DraftState, ResidentDrafts},
        mapping::{ChildDescriptor, ExtentNode, ExtentSlice},
    },
    ConstructionPolicy, ContentError, ContentResult, Edit, EditRequest, FinalizedConsumer,
    FinalizedObject, ObjectId, ObjectRole,
};
fn id(value: u8) -> ObjectId {
    ObjectId::from_bytes(&[value; 32]).unwrap()
}
fn leaf() -> DraftRecord {
    DraftRecord::Node(ExtentNode::Leaf {
        subtree_logical_bytes: 64,
        extents: (0..64)
            .map(|i| ExtentSlice::new(id(90), i * 2, 1).unwrap())
            .collect(),
    })
}
fn parent(child: ObjectId) -> DraftRecord {
    DraftRecord::Node(ExtentNode::Branch {
        level: 1,
        subtree_logical_bytes: 128,
        subtree_extent_count: 128,
        children: (1..=2)
            .map(|i| ChildDescriptor {
                cumulative_logical_end: i * 64,
                cumulative_extent_end: i * 64,
                child_object_id: child,
            })
            .collect(),
    })
}
#[test]
fn shared_temporary_occurrences_survive_selected_parent_supersession_and_leave_unrelated_zero() {
    let mut state = ResidentDrafts::new();
    // The unrelated FIRST zero job is deliberately never a temporary input.
    state.hold(id(1), leaf()).unwrap();
    state.hold(id(2), leaf()).unwrap();
    state.hold(id(3), parent(id(2))).unwrap();
    state.retain_temporaries(&[id(3)]).unwrap();
    state.select_root(None, id(3)).unwrap();
    // Independently materialized branch descriptors each own one child name.
    state.retain_temporaries(&[id(2), id(2)]).unwrap();
    state.supersede(id(3), Some(id(3))).unwrap();
    assert!(state.get(id(3)).unwrap().is_none());
    assert_eq!(state.stats().links_retired, 2);
    assert_eq!(state.next_job().unwrap().unwrap().id, id(1));
    state.supersede(id(2), None).unwrap();
    assert!(
        state.get(id(2)).unwrap().is_some(),
        "second temporary still consumes this body"
    );
    state.supersede(id(2), None).unwrap();
    assert!(state.get(id(2)).unwrap().is_none());
    assert!(state.get(id(1)).unwrap().is_some(), "no global zero drain");
    let expected =
        std::mem::size_of::<ExtentNode>() + 64 * std::mem::size_of::<ExtentSlice>() + 128;
    assert_eq!(state.stats().deferred_body_bytes, expected);
    assert!(state.stats().bytes > expected);
    state.finish().unwrap();
    assert_eq!(
        (state.stats().bytes, state.stats().deferred_body_bytes),
        (0, 0)
    );
}
#[test]
fn rehosted_parent_links_outlive_consumed_child_pins_and_stale_selected_refuses_before_change() {
    let mut state = ResidentDrafts::new();
    state.hold(id(2), leaf()).unwrap();
    state.retain_temporaries(&[id(2), id(2)]).unwrap();
    state.hold(id(3), parent(id(2))).unwrap();
    state.retain_temporaries(&[id(3)]).unwrap();
    state.release_temporaries(&[id(2), id(2)]).unwrap();
    state.select_root(None, id(3)).unwrap();
    while let Some(job) = state.next_job().unwrap() {
        state.retire_job(job).unwrap();
    }
    assert!(state.get(id(2)).unwrap().is_some());
    let before = state.stats();
    assert!(matches!(
        state.supersede(id(3), Some(id(4))),
        Err(ContentError::InvalidRecord("draft selected root expected"))
    ));
    assert_eq!(state.stats(), before);
    assert!(state.get(id(3)).is_err(), "failed operation cannot resume");
}
struct LateFailure {
    mapping_attempts: usize,
    file_attempts: usize,
}
#[test]
fn escaped_temporary_pin_refuses_finish_without_erasing_its_body() {
    let mut state = ResidentDrafts::new();
    state.hold(id(1), leaf()).unwrap();
    state.retain_temporaries(&[id(1)]).unwrap();
    assert!(matches!(
        state.finish(),
        Err(ContentError::InvalidRecord("draft final exact EOF"))
    ));
    assert!(state.stats().bytes > 0);
    assert!(state.stats().deferred_body_bytes > 0);
}
impl FinalizedConsumer for LateFailure {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        if matches!(
            object.role(),
            ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch
        ) {
            self.mapping_attempts += 1;
            return Err(ContentError::Io);
        }
        if object.role() == ObjectRole::FileState {
            self.file_attempts += 1;
        }
        Ok(())
    }
}
#[test]
fn actual_edit_late_consumer_failure_keeps_authority_terminal_without_file_root_or_replay() {
    let bytes: Vec<u8> = (0..400_000u32)
        .map(|i| ((i.wrapping_mul(7) ^ (i >> 8)) & 255) as u8)
        .collect();
    let (store, base) = support::build_file(&bytes);
    let edits = support::edits::Edits::new(
        bytes.len() as u64,
        vec![Edit::overwrite(1000, 1004), Edit::overwrite(2000, 2004)],
    )
    .unwrap();
    let mut source = support::edits::Parts::new();
    source.push(vec![201; 4]);
    source.push(vec![202; 4]);
    let mut state = ResidentDrafts::new();
    let mut consumer = LateFailure {
        mapping_attempts: 0,
        file_attempts: 0,
    };
    let policy = ConstructionPolicy::frozen_default();
    let result = support::disabled_scope(|scope| {
        apply_edits_with_state(
            policy,
            &policy.capacities(),
            &store,
            EditRequest {
                root: base.root,
                edits: &edits,
                source: &source,
            },
            &mut consumer,
            &mut state,
            scope.child("temporary-late-failure"),
        )
    });
    assert!(matches!(result, Err(ContentError::Io)));
    assert_eq!((consumer.mapping_attempts, consumer.file_attempts), (1, 0));
    assert!(
        state.stats().bytes > 0,
        "failed accepted metadata is still owned"
    );
    let before = state.stats();
    assert!(state.retain_temporaries(&[id(2)]).is_err());
    assert_eq!(state.stats(), before);
}
