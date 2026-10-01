//! Independent private framing and exact resident compatibility lifecycle.
use layerfs_content::file::edit::{DraftRecord, DraftState, ResidentDrafts};
use layerfs_content::file::mapping::{ChildDescriptor, ExtentNode, ExtentSlice};
use layerfs_content::{ContentError, ObjectId};
fn id(value: u8) -> ObjectId {
    ObjectId::from_bytes(&[value; 32]).unwrap()
}
fn leaf() -> DraftRecord {
    DraftRecord::Node(ExtentNode::Leaf {
        subtree_logical_bytes: 64,
        extents: (0..64)
            .map(|ordinal| ExtentSlice::new(id(90), ordinal * 2, 1).unwrap())
            .collect(),
    })
}
fn parent(child: ObjectId) -> DraftRecord {
    DraftRecord::Node(ExtentNode::Branch {
        level: 1,
        subtree_logical_bytes: 4096,
        subtree_extent_count: 4096,
        children: (1..=64)
            .map(|ordinal| ChildDescriptor {
                cumulative_logical_end: ordinal * 64,
                cumulative_extent_end: ordinal * 64,
                child_object_id: child,
            })
            .collect(),
    })
}
#[test]
fn private_decoded_body_has_independent_framing_and_exact_reference_refusal() {
    let record = leaf();
    let (form, role, body) = record.private_body().unwrap();
    let mut expected = vec![1, 0, 0, 64];
    expected.extend_from_slice(&64u64.to_be_bytes());
    expected.extend_from_slice(&64u64.to_be_bytes());
    for ordinal in 0..64u32 {
        expected.extend_from_slice(&[90; 32]);
        expected.extend_from_slice(&(ordinal * 2).to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
    }
    assert_eq!((form, role), (1, 3));
    assert_eq!(body, expected);
    assert_eq!(
        DraftRecord::from_private(form, role, body.clone(), vec![id(90); 64], Vec::new()).unwrap(),
        record
    );
    assert!(DraftRecord::from_private(form, role, body, vec![id(91); 64], Vec::new()).is_err());
}
#[test]
fn repeated_links_stale_jobs_and_selected_root_retire_exactly_once() {
    let mut state = ResidentDrafts::new();
    state.hold(id(1), leaf()).unwrap();
    state.hold(id(2), parent(id(1))).unwrap();
    state.select_root(None, id(2)).unwrap();
    let first = state.next_job().unwrap().unwrap();
    assert_eq!(first.id, id(1));
    assert_eq!(first.links, 64);
    state.retire_job(first).unwrap();
    let second = state.next_job().unwrap().unwrap();
    assert_eq!(second.links, 1);
    state.retire_job(second).unwrap();
    assert_eq!(state.next_job().unwrap(), None);
    state.hold(id(3), leaf()).unwrap();
    state.select_root(Some(id(2)), id(3)).unwrap();
    while let Some(job) = state.next_job().unwrap() {
        state.retire_job(job).unwrap();
    }
    assert!(state.get(id(1)).unwrap().is_none());
    assert!(state.get(id(2)).unwrap().is_none());
    assert!(state.get(id(3)).unwrap().is_some());
    let stats = state.stats();
    assert_eq!(stats.retired, 2);
    assert_eq!(stats.links_retired, 64);
    assert_eq!(stats.jobs_consumed, 5);
    state.finish().unwrap();
    assert_eq!(state.stats().bytes, 0);
}
#[test]
fn pending_acceptance_cannot_be_reserved_or_replayed_twice() {
    let mut state = ResidentDrafts::new();
    state.hold(id(1), leaf()).unwrap();
    state.select_root(None, id(1)).unwrap();
    assert!(state.begin_emission(id(1), id(10)).unwrap());
    assert!(matches!(
        state.begin_emission(id(1), id(10)),
        Err(ContentError::InvalidRecord("draft emission pending"))
    ));
    assert!(state.accepted(id(1), id(10)).is_err());
    assert!(state.stats().bytes > 0);
}

fn key(sequence: u64) -> ObjectId {
    let mut bytes = [0; 32];
    bytes[..24].copy_from_slice(b"layerfs-edit-draft-key\0\0");
    bytes[24..].copy_from_slice(&sequence.to_be_bytes());
    ObjectId::from_bytes(&bytes).unwrap()
}
#[test]
fn complete_record_quota_is_refused_before_the_offending_resident_creation() {
    let mut state = ResidentDrafts::new();
    // An empty decoded body is20 bytes. Header/body/count/job are four records;
    // body20+framing55+header151+count63+job71+container128 =488 bytes.
    const ALLOWED: u64 = 65_536 / 4;
    for sequence in 0..ALLOWED {
        state
            .hold(
                key(sequence),
                DraftRecord::Node(ExtentNode::Leaf {
                    subtree_logical_bytes: 0,
                    extents: Vec::new(),
                }),
            )
            .unwrap();
    }
    let before = state.stats();
    assert_eq!(before.created, ALLOWED);
    assert_eq!(before.bytes, ALLOWED as usize * 488);
    assert!(matches!(
        state.hold(
            key(ALLOWED),
            DraftRecord::Node(ExtentNode::Leaf {
                subtree_logical_bytes: 0,
                extents: Vec::new()
            })
        ),
        Err(ContentError::BoundedCapacityExceeded {
            what: "draft resident records",
            limit: 65_536,
            actual: 65_540
        })
    ));
    assert_eq!(state.stats().created, before.created);
    assert_eq!(state.stats().bytes, before.bytes);
}
#[test]
fn complete_byte_quota_is_refused_before_the_offending_resident_creation() {
    let mut state = ResidentDrafts::new();
    // Independent complete maximum decoded-branch framing plus associated facts:
    // 20+128*48+128*89+55+151+63+71+128 =18024 bytes,132 records.
    const WIDTH: usize = 18_024;
    let allowed = layerfs_content::file::edit::EDIT_DEFERRED_LIMIT / WIDTH;
    for sequence in 0..allowed {
        state
            .hold(
                key(sequence as u64),
                DraftRecord::Node(ExtentNode::Branch {
                    level: 1,
                    subtree_logical_bytes: 128,
                    subtree_extent_count: 128,
                    children: (1..=128)
                        .map(|ordinal| ChildDescriptor {
                            cumulative_logical_end: ordinal,
                            cumulative_extent_end: ordinal,
                            child_object_id: id(250),
                        })
                        .collect(),
                }),
            )
            .unwrap();
    }
    let before = state.stats();
    assert_eq!(before.bytes, allowed * WIDTH);
    let failure = state.hold(
        key(allowed as u64),
        DraftRecord::Node(ExtentNode::Branch {
            level: 1,
            subtree_logical_bytes: 128,
            subtree_extent_count: 128,
            children: (1..=128)
                .map(|ordinal| ChildDescriptor {
                    cumulative_logical_end: ordinal,
                    cumulative_extent_end: ordinal,
                    child_object_id: id(250),
                })
                .collect(),
        }),
    );
    assert!(matches!(
        failure,
        Err(ContentError::BoundedCapacityExceeded {
            what: "edit.deferred_nodes",
            ..
        })
    ));
    assert_eq!(state.stats().created, before.created);
    assert_eq!(state.stats().bytes, before.bytes);
}
