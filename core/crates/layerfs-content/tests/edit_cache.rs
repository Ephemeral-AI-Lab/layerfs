//! Stored-node edit reads honor the existing cache window and original failures.

use std::cell::RefCell;
use std::collections::BTreeMap;

use layerfs_content::file::mapping::{
    encode_node, ExtentNode, ExtentSlice, NodeSummary, PageCache, READ_NAVIGATION_CACHE_PAGES,
};
use layerfs_content::file::EditObjects;
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, DiscardingConsumer, ObjectId,
};

#[derive(Default)]
struct Pages {
    canonical: BTreeMap<ObjectId, Vec<u8>>,
    demands: RefCell<Vec<ObjectId>>,
    allocations: RefCell<BTreeMap<ObjectId, usize>>,
    refusal: Option<ContentError>,
}

impl Pages {
    fn insert(&mut self, canonical: Vec<u8>, bytes: u64, extents: u64) -> NodeSummary {
        let id = ObjectId::for_bytes(&canonical);
        self.canonical.insert(id, canonical);
        NodeSummary {
            id,
            bytes,
            extents,
            level: 0,
        }
    }

    fn leaf(&mut self, index: usize) -> (NodeSummary, ExtentNode) {
        let payload = ObjectId::for_bytes(&index.to_be_bytes());
        let node = ExtentNode::Leaf {
            subtree_logical_bytes: 64,
            extents: (0..64)
                .map(|entry| ExtentSlice::new(payload, (entry % 2) * 2, 1).unwrap())
                .collect(),
        };
        let summary = self.insert(encode_node(&node, false).unwrap(), 64, 64);
        (summary, node)
    }
}

impl AuthenticatedObjects for Pages {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demands.borrow_mut().extend_from_slice(ids);
        if let Some(error) = &self.refusal {
            return Err(error.clone());
        }
        ids.iter()
            .map(|id| {
                let bytes = self
                    .canonical
                    .get(id)
                    .ok_or(ContentError::MissingObject)?
                    .clone();
                if ObjectId::for_bytes(&bytes) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                self.allocations
                    .borrow_mut()
                    .insert(*id, bytes.as_ptr() as usize);
                Ok(bytes)
            })
            .collect()
    }
}

#[test]
fn more_than_two_cache_windows_evict_and_retained_pages_move_without_a_copy() {
    let mut reader = Pages::default();
    let nodes = (0..2 * READ_NAVIGATION_CACHE_PAGES + 3)
        .map(|index| reader.leaf(index))
        .collect::<Vec<_>>();
    let mut cache = PageCache::new();
    let mut consumer = DiscardingConsumer::new();
    {
        let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
        for (summary, expected) in &nodes {
            assert_eq!(objects.load_node(*summary, false).unwrap(), *expected);
        }
        let (last, expected) = nodes.last().unwrap();
        assert_eq!(objects.load_node(*last, false).unwrap(), *expected);
        assert_eq!(reader.demands.borrow().len(), nodes.len(), "retained hit");
        assert_eq!(objects.load_node(nodes[0].0, false).unwrap(), nodes[0].1);
        assert_eq!(
            reader.demands.borrow().len(),
            nodes.len() + 1,
            "evicted demand"
        );
        assert_eq!(objects.counters().nodes_read, (nodes.len() + 1) as u64);
    }
    let held = nodes
        .iter()
        .filter(|(summary, _)| cache.get(summary.id, false).is_some())
        .count();
    assert_eq!(held, 4);
    assert!(held <= READ_NAVIGATION_CACHE_PAGES);
    assert!(cache
        .get(nodes[READ_NAVIGATION_CACHE_PAGES].0.id, false)
        .is_none());
    for (summary, _) in nodes
        .iter()
        .filter(|(summary, _)| cache.get(summary.id, false).is_some())
    {
        assert_eq!(
            cache.get(summary.id, false).unwrap().as_ptr() as usize,
            reader.allocations.borrow()[&summary.id],
            "the canonical demand allocation transfers to the memo"
        );
    }
    assert_eq!(consumer.objects(), 0);
}

#[test]
fn a_full_cache_survives_a_rejected_new_summary_without_retaining_the_page() {
    let mut reader = Pages::default();
    let nodes = (0..=READ_NAVIGATION_CACHE_PAGES)
        .map(|index| reader.leaf(index))
        .collect::<Vec<_>>();
    let mut cache = PageCache::new();
    let mut consumer = DiscardingConsumer::new();
    let wrong = NodeSummary {
        bytes: 65,
        ..nodes.last().unwrap().0
    };
    {
        let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
        for (summary, _) in nodes.iter().take(READ_NAVIGATION_CACHE_PAGES) {
            objects.load_node(*summary, false).unwrap();
        }
        assert_eq!(
            objects.load_node(wrong, false),
            Err(ContentError::InvalidRecord("extent summary"))
        );
        assert_eq!(
            reader.demands.borrow().len(),
            READ_NAVIGATION_CACHE_PAGES + 1
        );
    }
    assert!(cache.get(wrong.id, false).is_none());
    assert_eq!(
        nodes
            .iter()
            .filter(|(summary, _)| cache.get(summary.id, false).is_some())
            .count(),
        READ_NAVIGATION_CACHE_PAGES
    );
}

#[test]
fn cached_pages_still_check_each_requested_summary_without_another_demand() {
    let mut reader = Pages::default();
    let (summary, expected) = reader.leaf(1);
    let mut cache = PageCache::new();
    let mut consumer = DiscardingConsumer::new();
    {
        let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
        assert_eq!(objects.load_node(summary, false).unwrap(), expected);
        assert_eq!(objects.load_node(summary, false).unwrap(), expected);
        assert_eq!(
            objects.load_node(
                NodeSummary {
                    extents: 65,
                    ..summary
                },
                false
            ),
            Err(ContentError::InvalidRecord("extent summary"))
        );
        assert_eq!(reader.demands.borrow().as_slice(), &[summary.id]);
        assert_eq!(objects.counters().nodes_read, 1);
    }
    assert!(cache.get(summary.id, false).is_some());
}

#[test]
fn rehashed_malformed_or_short_nonroot_pages_are_never_retained() {
    let mut reader = Pages::default();
    let (valid, _) = reader.leaf(1);
    let mut malformed = reader.canonical[&valid.id].clone();
    malformed[13 + 12] = 1;
    let malformed = reader.insert(malformed, 64, 64);
    let short = ExtentNode::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![ExtentSlice::new(ObjectId::for_bytes(b"payload"), 0, 1).unwrap()],
    };
    let short = reader.insert(encode_node(&short, true).unwrap(), 1, 1);
    for (summary, error) in [
        (malformed, ContentError::InvalidRecord("extent flags")),
        (short, ContentError::NonCanonicalPagePartition),
    ] {
        let mut cache = PageCache::new();
        let mut consumer = DiscardingConsumer::new();
        let calls = reader.demands.borrow().len();
        {
            let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
            assert_eq!(objects.load_node(summary, false), Err(error));
        }
        assert_eq!(reader.demands.borrow().len(), calls + 1);
        assert!(cache.get(summary.id, false).is_none());
    }
}

#[test]
fn provider_refusal_and_absence_end_the_original_demand_once() {
    let mut reader = Pages::default();
    let (summary, _) = reader.leaf(1);
    reader.refusal = Some(ContentError::ProviderFailure {
        what: "original page refusal",
    });
    let mut cache = PageCache::new();
    let mut consumer = DiscardingConsumer::new();
    {
        let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
        assert_eq!(
            objects.load_node(summary, false),
            Err(reader.refusal.clone().unwrap())
        );
    }
    assert_eq!(reader.demands.borrow().as_slice(), &[summary.id]);
    assert!(cache.get(summary.id, false).is_none());
    let reader = Pages::default();
    {
        let mut objects = EditObjects::new(&reader, &mut consumer, &mut cache);
        assert_eq!(
            objects.load_node(summary, false),
            Err(ContentError::MissingObject)
        );
    }
    assert_eq!(reader.demands.borrow().as_slice(), &[summary.id]);
    assert!(cache.get(summary.id, false).is_none());
}
