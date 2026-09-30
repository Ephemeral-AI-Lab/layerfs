//! Current navigation owners and admitted retention, through public C1 APIs.
//!
//! Expected mapping bytes/identities come from the independent finite fixture.
//! This target uses an authenticated in-memory provider; the separately owned
//! Storage target establishes real-provider/requested-allocation scope.
use std::{cell::RefCell, collections::BTreeMap};

use layerfs_content::{
    apply_edits,
    file::{
        edit::EditObjects,
        mapping::{
            encode_node, ExtentNode, ExtentSlice, NodeSummary, PageCache, RangeCursor,
            MAX_NODE_OBJECT_BYTES, READ_NAVIGATION_CACHE_PAGES,
        },
    },
    AuthenticatedObjects, ConstructionPolicy, ContentError, ContentResult, Edit, EditRequest,
    FileView, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_telemetry::timer::Timing;

#[allow(dead_code)]
#[path = "support/edits.rs"]
mod edits;
#[path = "support/cache_oracle.rs"]
mod oracle;

#[derive(Clone, Copy)]
enum ResponseShape {
    Exact,
    ShortNavigation,
    ExcessNavigationCapacity,
}

/// An ordinary supplied provider: authenticates each returned canonical object.
struct Reader<'a> {
    objects: &'a BTreeMap<ObjectId, Vec<u8>>,
    calls: RefCell<Vec<Vec<ObjectId>>>,
    shape: ResponseShape,
}
impl<'a> Reader<'a> {
    fn new(fixture: &'a oracle::Fixture) -> Self {
        Self {
            objects: &fixture.objects,
            calls: RefCell::new(Vec::new()),
            shape: ResponseShape::Exact,
        }
    }
}
impl AuthenticatedObjects for Reader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.calls.borrow_mut().push(ids.to_vec());
        let mut values: Vec<_> = ids
            .iter()
            .map(|id| {
                let canonical = self.objects.get(id).ok_or(ContentError::MissingObject)?;
                if ObjectId::for_bytes(canonical) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                Ok(canonical.to_vec())
            })
            .collect::<ContentResult<_>>()?;
        if ids.len() == 32 {
            match self.shape {
                ResponseShape::Exact => {}
                ResponseShape::ShortNavigation => {
                    values.pop();
                }
                ResponseShape::ExcessNavigationCapacity => {
                    let mut excessive = Vec::with_capacity(MAX_NODE_OBJECT_BYTES + 1);
                    excessive.extend_from_slice(&values[0]);
                    values[0] = excessive;
                }
            }
        }
        Ok(values)
    }
}

fn retained(f: &oracle::Fixture, cache: &PageCache) -> usize {
    f.mapping_keys()
        .map(|(id, _)| {
            usize::from(cache.get(id, false).is_some()) + usize::from(cache.get(id, true).is_some())
        })
        .sum()
}

fn prefill(f: &oracle::Fixture, cache: &mut PageCache) {
    for (id, root) in f.mixed_prefill() {
        cache.insert(id, root, f.canonical(id).to_vec()).unwrap();
    }
    assert_eq!(retained(f, cache), 64);
}

#[test]
fn mixed_hit_wave_keeps_its_evicted_hit_and_fetches_only_the_31_missing_pages() {
    let f = oracle::wide();
    let reader = Reader::new(&f);
    let mut cache = PageCache::new();
    prefill(&f, &mut cache);
    let mut output = Vec::new();
    let counters = Timing::disabled("mixed", |scope| {
        let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
        cursor.read_segment(0..oracle::FIRST_WAVE_BYTES as u64, &mut output, &mut cache)
    })
    .0
    .unwrap();
    assert_eq!(output, f.expected[..oracle::FIRST_WAVE_BYTES]);
    assert_eq!(counters.nodes_read, 31);
    assert_eq!(counters.node_batches_read, 1);
    assert_eq!(counters.max_node_batch, 31);
    let calls = reader.calls.borrow();
    assert_eq!(calls[0], f.leaves[1..32]);
    assert!(calls.iter().all(|ids| !ids.contains(&f.leaves[0])));
    assert!(calls.iter().all(|ids| !ids.contains(&f.state.mapping_root)));
    assert!(retained(&f, &cache) <= 64);
}

#[test]
fn requested_limits_smaller_than_a_wave_preserve_grouped_and_exact_ascending_reads() {
    for limit in [1, 31, 32, 64] {
        let f = oracle::wide();
        let reader = Reader::new(&f);
        let mut cache = PageCache::bounded(limit);
        let mut output = Vec::new();
        let counters = Timing::disabled("limits", |scope| {
            let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
            let mut counters = Default::default();
            for range in [
                0..oracle::FIRST_WAVE_BYTES as u64,
                oracle::FIRST_WAVE_BYTES as u64..f.state.logical_len,
            ] {
                counters = cursor.read_segment(range, &mut output, &mut cache)?;
                assert!(retained(&f, &cache) <= limit, "limit {limit}");
            }
            Ok::<_, ContentError>(counters)
        })
        .0
        .unwrap();
        assert_eq!(counters.max_node_batch, 32);
        assert_eq!(output, f.expected, "limit {limit}");
        let calls = reader.calls.borrow();
        assert_eq!(calls[0], vec![f.state.mapping_root]);
        assert_eq!(calls[1], f.leaves[..32]);
        assert!(calls.iter().all(|ids| ids.len() <= 32));
        assert!(calls.iter().any(|ids| ids.len() == 32));
    }
}

#[test]
fn default_matches_new_and_zero_still_means_one_page() {
    let f = oracle::wide();
    assert_eq!(READ_NAVIGATION_CACHE_PAGES, 64);
    for mut cache in [PageCache::new(), PageCache::default()] {
        for (index, id) in f.leaves.iter().copied().enumerate() {
            cache.insert(id, false, f.canonical(id).to_vec()).unwrap();
            assert_eq!(retained(&f, &cache), if index < 64 { index + 1 } else { 1 });
        }
    }
    let mut cache = PageCache::bounded(0);
    for id in f.leaves.iter().copied().take(2) {
        cache.insert(id, false, f.canonical(id).to_vec()).unwrap();
        assert_eq!(retained(&f, &cache), 1);
    }
}

#[test]
fn public_insert_checks_actual_owner_width_context_and_replacement_before_retention() {
    let f = oracle::wide();
    let id = f.leaves[0];
    let mut cache = PageCache::bounded(1);
    let canonical = f.canonical(id).to_vec();
    cache.insert(id, false, canonical.clone()).unwrap();
    let mut excessive = Vec::with_capacity(MAX_NODE_OBJECT_BYTES + 1);
    excessive.extend_from_slice(&canonical);
    let actual = excessive.capacity() as u64;
    assert_eq!(
        cache.insert(id, false, excessive),
        Err(ContentError::BoundedCapacityExceeded {
            what: "mapping.page_capacity",
            limit: MAX_NODE_OBJECT_BYTES as u64,
            actual,
        })
    );
    assert_eq!(cache.get(id, false), Some(canonical.as_slice()));
    let oversized = vec![0; MAX_NODE_OBJECT_BYTES + 1];
    let oversized_id = ObjectId::for_bytes(&oversized);
    assert_eq!(
        cache.insert(oversized_id, true, oversized),
        Err(ContentError::ObjectLimitExceeded {
            limit: MAX_NODE_OBJECT_BYTES,
            actual: MAX_NODE_OBJECT_BYTES + 1,
        })
    );
    assert!(cache.get(oversized_id, true).is_none());
    let one = encode_node(
        &ExtentNode::Leaf {
            subtree_logical_bytes: 32,
            extents: vec![ExtentSlice::new(f.payload_id, 0, 32).unwrap()],
        },
        true,
    )
    .unwrap();
    let one_id = ObjectId::for_bytes(&one);
    assert_eq!(
        cache.insert(one_id, false, one.clone()),
        Err(ContentError::NonCanonicalPagePartition)
    );
    assert!(cache.get(one_id, false).is_none());
    cache.insert(one_id, true, one).unwrap();
    assert!(cache.get(id, false).is_none());
    assert!(cache.get(one_id, true).is_some());
}

/// Replaces one root/child record while preserving honest hash authentication.
fn replace_child(f: &mut oracle::Fixture, index: usize, canonical: Vec<u8>) {
    let child = ObjectId::for_bytes(&canonical);
    f.objects.insert(child, canonical);
    f.leaves[index] = child;
    let mut root = f.canonical(f.state.mapping_root).to_vec();
    let offset = 44 + index * 48 + 16;
    root[offset..offset + 32].copy_from_slice(child.as_bytes());
    f.state.mapping_root = ObjectId::for_bytes(&root);
    f.objects.insert(f.state.mapping_root, root);
}

fn assert_no_leaf_retention(f: &oracle::Fixture, cache: &PageCache) {
    for id in &f.leaves {
        assert!(cache.get(*id, false).is_none());
        assert!(cache.get(*id, true).is_none());
    }
}

#[test]
fn entire_navigation_batch_is_checked_before_any_leaf_retention_or_payload_output() {
    for bad in [
        ResponseShape::ShortNavigation,
        ResponseShape::ExcessNavigationCapacity,
    ] {
        let f = oracle::wide();
        let reader = Reader {
            shape: bad,
            ..Reader::new(&f)
        };
        let mut cache = PageCache::new();
        cache
            .insert(
                f.state.mapping_root,
                true,
                f.canonical(f.state.mapping_root).to_vec(),
            )
            .unwrap();
        let mut output = Vec::new();
        let failure = Timing::disabled("refusal", |scope| {
            let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
            cursor.read_segment(0..oracle::FIRST_WAVE_BYTES as u64, &mut output, &mut cache)
        })
        .0
        .unwrap_err();
        match bad {
            ResponseShape::ShortNavigation => assert_eq!(
                failure,
                ContentError::BatchCardinality {
                    requested: 32,
                    returned: 31
                }
            ),
            ResponseShape::ExcessNavigationCapacity => assert_eq!(
                failure,
                ContentError::BoundedCapacityExceeded {
                    what: "mapping.page_capacity",
                    limit: MAX_NODE_OBJECT_BYTES as u64,
                    actual: MAX_NODE_OBJECT_BYTES as u64 + 1,
                }
            ),
            ResponseShape::Exact => unreachable!(),
        }
        assert!(output.is_empty());
        assert_no_leaf_retention(&f, &cache);
        assert_eq!(reader.calls.borrow().as_slice(), &[f.leaves[..32].to_vec()]);
    }
    for oversized in [false, true] {
        let mut f = oracle::wide();
        let mut bad = f.canonical(f.leaves[31]).to_vec();
        if oversized {
            bad.resize(MAX_NODE_OBJECT_BYTES + 1, 0);
        } else {
            bad[25] = 1;
        }
        replace_child(&mut f, 31, bad);
        let reader = Reader::new(&f);
        let mut cache = PageCache::new();
        cache
            .insert(
                f.state.mapping_root,
                true,
                f.canonical(f.state.mapping_root).to_vec(),
            )
            .unwrap();
        let mut output = Vec::new();
        let failure = Timing::disabled("codec", |scope| {
            let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
            cursor.read_segment(0..oracle::FIRST_WAVE_BYTES as u64, &mut output, &mut cache)
        })
        .0
        .unwrap_err();
        assert_eq!(
            failure,
            if oversized {
                ContentError::ObjectLimitExceeded {
                    limit: MAX_NODE_OBJECT_BYTES,
                    actual: MAX_NODE_OBJECT_BYTES + 1,
                }
            } else {
                ContentError::InvalidRecord("extent flags")
            }
        );
        assert!(output.is_empty());
        assert_no_leaf_retention(&f, &cache);
    }
}

#[test]
fn parent_child_summary_mismatch_is_refused_before_current_batch_output_or_retention() {
    let mut f = oracle::wide();
    let mut root = f.canonical(f.state.mapping_root).to_vec();
    root[52..60].copy_from_slice(&129u64.to_be_bytes());
    f.state.mapping_root = ObjectId::for_bytes(&root);
    f.objects.insert(f.state.mapping_root, root);
    let reader = Reader::new(&f);
    let mut cache = PageCache::new();
    let mut output = Vec::new();
    let failure = Timing::disabled("summary", |scope| {
        let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
        cursor.read_segment(0..oracle::FIRST_WAVE_BYTES as u64, &mut output, &mut cache)
    })
    .0
    .unwrap_err();
    assert_eq!(failure, ContentError::InvalidRecord("mapping coverage"));
    assert!(output.is_empty());
    assert_no_leaf_retention(&f, &cache);
}

#[test]
fn a_later_bad_batch_leaves_only_the_previously_validated_output_prefix() {
    let mut f = oracle::wide();
    let mut bad = f.canonical(f.leaves[63]).to_vec();
    bad[25] = 1;
    replace_child(&mut f, 63, bad);
    let reader = Reader::new(&f);
    let mut cache = PageCache::new();
    let mut output = Vec::new();
    let failure = Timing::disabled("later-batch", |scope| {
        let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
        cursor.read_segment(0..f.state.logical_len, &mut output, &mut cache)
    })
    .0
    .unwrap_err();
    assert_eq!(failure, ContentError::InvalidRecord("extent flags"));
    assert_eq!(output, f.expected[..oracle::FIRST_WAVE_BYTES]);
    for id in &f.leaves[32..] {
        assert!(cache.get(*id, false).is_none());
    }
}

#[test]
fn repeated_leaf_demands_keep_logical_order_but_acquire_one_immutable_id_once() {
    let mut f = oracle::wide();
    let first = f.canonical(f.leaves[0]).to_vec();
    replace_child(&mut f, 1, first);
    let first_bytes = f.expected[..oracle::LEAF_LOGICAL_BYTES].to_vec();
    f.expected[oracle::LEAF_LOGICAL_BYTES..2 * oracle::LEAF_LOGICAL_BYTES]
        .copy_from_slice(&first_bytes);
    let reader = Reader::new(&f);
    let mut cache = PageCache::bounded(1);
    let mut output = Vec::new();
    let counters = Timing::disabled("duplicates", |scope| {
        let mut cursor = RangeCursor::new(&reader, f.state, &mut cache, scope)?;
        cursor.read_segment(0..oracle::FIRST_WAVE_BYTES as u64, &mut output, &mut cache)
    })
    .0
    .unwrap();
    assert_eq!(output, f.expected[..oracle::FIRST_WAVE_BYTES]);
    assert_eq!(counters.nodes_read, 32); // one root +31 unique leaves
    let calls = reader.calls.borrow();
    assert_eq!(calls[1].len(), 31);
    assert_eq!(calls[1].iter().filter(|id| **id == f.leaves[0]).count(), 1);
    assert_eq!(
        calls[1],
        std::iter::once(f.leaves[0])
            .chain(f.leaves[2..32].iter().copied())
            .collect::<Vec<_>>()
    );
}

#[derive(Default)]
struct Output(BTreeMap<ObjectId, Vec<u8>>);
impl FinalizedConsumer for Output {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let object = object.into_parts();
        self.0.insert(object.id, object.canonical);
        Ok(())
    }
}

#[test]
fn edit_node_loading_retains_at_most_the_requested_count_and_checks_summary_first() {
    let f = oracle::wide();
    let reader = Reader::new(&f);
    for limit in [1, 31, 32, 64] {
        let mut cache = PageCache::bounded(limit);
        let mut output = Output::default();
        for (index, id) in f.leaves.iter().copied().enumerate() {
            let summary = NodeSummary {
                id,
                bytes: oracle::LEAF_LOGICAL_BYTES as u64,
                extents: 128,
                level: 0,
            };
            let node = EditObjects::new(&reader, &mut output, &mut cache)
                .load_node(summary, false)
                .unwrap();
            let ExtentNode::Leaf { extents, .. } = node else {
                panic!("leaf")
            };
            assert_eq!(extents.len(), 128);
            for (extent_index, extent) in extents.iter().enumerate() {
                assert_eq!(extent.payload_object_id(), f.payload_id);
                assert_eq!(extent.source_offset() as usize, extent_index * 63 + index);
                assert_eq!(extent.logical_length(), 32);
            }
            assert!(retained(&f, &cache) <= limit);
        }
        assert!(output.0.is_empty());
    }
    let mut cache = PageCache::new();
    let mut output = Output::default();
    let summary = NodeSummary {
        id: f.leaves[0],
        bytes: 4097,
        extents: 128,
        level: 0,
    };
    assert_eq!(
        EditObjects::new(&reader, &mut output, &mut cache).load_node(summary, false),
        Err(ContentError::InvalidRecord("extent summary"))
    );
    assert!(cache.is_empty());
}

#[test]
fn ordinary_65_page_edit_preserves_independent_new_bytes_and_selected_old_file() {
    let mut f = oracle::wide();
    let mut expected = f.expected.clone();
    let mut changes = Vec::new();
    let mut source = edits::Parts::new();
    for leaf in 0..oracle::LEAVES {
        let at = leaf * oracle::LEAF_LOGICAL_BYTES + 17;
        expected[at] ^= 0xff;
        changes.push(Edit::overwrite(at as u64, at as u64 + 1));
        source.push(vec![expected[at]]);
    }
    let changes = edits::Edits::new(f.state.logical_len, changes).unwrap();
    let mut output = Output::default();
    let edited = {
        let reader = Reader::new(&f);
        let policy = ConstructionPolicy::frozen_default();
        let edited = Timing::disabled("edit", |scope| {
            apply_edits(
                policy,
                &policy.capacities(),
                &reader,
                EditRequest {
                    root: f.file_root,
                    edits: &changes,
                    source: &source,
                },
                &mut output,
                scope.child("edit"),
            )
        })
        .0
        .unwrap();
        let calls = reader.calls.borrow();
        for id in &f.leaves {
            assert!(
                calls.iter().any(|call| call.contains(id)),
                "every changed old page is actually visited"
            );
        }
        edited
    };
    assert_eq!(edited.logical_len, f.state.logical_len);
    assert_ne!(edited.root, f.file_root);
    f.objects.extend(output.0);
    let reader = Reader::new(&f);
    for (root, expected) in [
        (edited.root, expected.as_slice()),
        (f.file_root, f.expected.as_slice()),
    ] {
        let mut bytes = Vec::new();
        Timing::disabled("readback", |scope| {
            let view = FileView::open(&reader, root, scope.child("open"))?;
            view.read_range(&reader, 0..view.logical_len(), &mut bytes, scope)
        })
        .0
        .unwrap();
        assert_eq!(bytes, expected);
    }
}
