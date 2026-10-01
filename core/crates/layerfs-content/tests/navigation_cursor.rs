//! Independent v1 range bytes and observed monotone navigation demands.
#[path = "support/cache_oracle.rs"]
mod oracle;
mod support;

use layerfs_content::file::mapping::{read_range, PageCache, RangeCursor};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::cell::RefCell;
use std::io::{self, Write};
use support::disabled_scope;

struct Provider<'a> {
    fixture: &'a oracle::Fixture,
    calls: RefCell<Vec<Vec<ObjectId>>>,
}
impl AuthenticatedObjects for Provider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.calls.borrow_mut().push(ids.to_vec());
        ids.iter()
            .map(|id| {
                let canonical = self
                    .fixture
                    .objects
                    .get(id)
                    .ok_or(ContentError::MissingObject)?;
                if ObjectId::for_bytes(canonical) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                Ok(canonical.clone())
            })
            .collect()
    }
}
impl Provider<'_> {
    fn acquisitions(&self, id: ObjectId) -> usize {
        self.calls
            .borrow()
            .iter()
            .flatten()
            .filter(|found| **found == id)
            .count()
    }
}

#[test]
fn wide_full_read_keeps_grouped_leaves_and_exact_independent_bytes() {
    let fixture = oracle::wide();
    let provider = Provider {
        fixture: &fixture,
        calls: RefCell::new(Vec::new()),
    };
    let mut output = Vec::new();
    let counters = disabled_scope(|scope| {
        read_range(
            &provider,
            fixture.state,
            0..fixture.state.logical_len,
            &mut output,
            scope,
        )
    })
    .unwrap();
    assert_eq!(output, fixture.expected);
    assert_eq!(counters.nodes_read, 66);
    assert_eq!(counters.node_batches_read, 4);
    assert_eq!(counters.max_node_batch, 32);
    assert_eq!(provider.acquisitions(fixture.state.mapping_root), 1);
    for id in &fixture.leaves {
        assert_eq!(provider.acquisitions(*id), 1);
    }
}

#[test]
fn cache_one_cursor_keeps_partial_leaf_and_path_across_gaps() {
    let fixture = oracle::wide();
    let provider = Provider {
        fixture: &fixture,
        calls: RefCell::new(Vec::new()),
    };
    let ranges = [
        3..17,
        31..4097,
        4103..8190,
        20_003..20_117,
        20_117..fixture.state.logical_len,
    ];
    let mut cache = PageCache::bounded(1);
    let mut output = Vec::new();
    let mut expected = Vec::new();
    let counters = disabled_scope(|scope| {
        let mut cursor = RangeCursor::new(&provider, fixture.state, &mut cache, scope)?;
        let mut counters = Default::default();
        for range in &ranges {
            expected.extend_from_slice(&fixture.expected[range.start as usize..range.end as usize]);
            counters = cursor.read_segment(range.clone(), &mut output, &mut cache)?;
        }
        Ok::<_, ContentError>(counters)
    })
    .unwrap();
    assert_eq!(output, expected);
    assert_eq!(provider.acquisitions(fixture.state.mapping_root), 1);
    for (index, id) in fixture.leaves.iter().enumerate() {
        assert_eq!(
            provider.acquisitions(*id),
            usize::from(index != 2 && index != 3),
            "leaf {index}"
        );
    }
    assert_eq!(counters.nodes_read, 64);
    assert_eq!(counters.max_node_batch, 32);
}

/// Independent envelope/header/branch framing; no candidate codec or builder.
fn branch(fixture: &mut oracle::Fixture, level: u8, children: &[(ObjectId, u64, u64)]) -> ObjectId {
    let bytes: u64 = children.iter().map(|child| child.1).sum();
    let extents: u64 = children.iter().map(|child| child.2).sum();
    let mut value = b"LFS4MAP\0".to_vec();
    value.extend_from_slice(&3u16.to_be_bytes());
    value.extend_from_slice(&[9, level, 0]);
    value.extend_from_slice(&(children.len() as u16).to_be_bytes());
    value.extend_from_slice(&bytes.to_be_bytes());
    value.extend_from_slice(&extents.to_be_bytes());
    let (mut logical, mut count) = (0u64, 0u64);
    for (id, size, items) in children {
        logical += size;
        count += items;
        value.extend_from_slice(&logical.to_be_bytes());
        value.extend_from_slice(&count.to_be_bytes());
        value.extend_from_slice(id.as_bytes());
    }
    let mut canonical = b"LFSO\x01".to_vec();
    canonical.extend_from_slice(&((value.len() + 4) as u32).to_be_bytes());
    canonical.extend_from_slice(&(value.len() as u32).to_be_bytes());
    canonical.extend_from_slice(&value);
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/object/v2\0");
    hash.update(&canonical);
    let id = ObjectId::from_bytes(hash.finalize().as_bytes()).unwrap();
    fixture.objects.insert(id, canonical);
    id
}

#[test]
fn deeper_cursor_rebases_nonzero_branch_origins_and_preserves_shared_subtrees() {
    let mut fixture = oracle::wide();
    let size = oracle::LEAF_LOGICAL_BYTES as u64;
    let children: Vec<_> = fixture
        .leaves
        .iter()
        .map(|id| (*id, size, oracle::LEAF_EXTENTS as u64))
        .collect();
    let left = branch(&mut fixture, 1, &children[..64]);
    let right = branch(&mut fixture, 1, &children[1..]);
    let root = branch(
        &mut fixture,
        2,
        &[(left, size * 64, 128 * 64), (right, size * 64, 128 * 64)],
    );
    let mut expected = fixture.expected[..size as usize * 64].to_vec();
    expected.extend_from_slice(&fixture.expected[size as usize..]);
    fixture.state.mapping_root = root;
    fixture.state.tree_level = 2;
    fixture.state.logical_len = size * 128;
    fixture.state.extent_count = 128 * 128;
    let provider = Provider {
        fixture: &fixture,
        calls: RefCell::new(Vec::new()),
    };
    let ranges = [
        17..4097,
        size * 63 + 7..size * 64 + 19,
        size * 65 + 3..size * 66 + 13,
        size * 127 + 9..size * 128,
    ];
    let mut cache = PageCache::bounded(1);
    let mut output = Vec::new();
    let mut selected = Vec::new();
    disabled_scope(|scope| {
        let mut cursor = RangeCursor::new(&provider, fixture.state, &mut cache, scope)?;
        for range in &ranges {
            selected.extend_from_slice(&expected[range.start as usize..range.end as usize]);
            cursor.read_segment(range.clone(), &mut output, &mut cache)?;
        }
        Ok::<_, ContentError>(())
    })
    .unwrap();
    assert_eq!(output, selected);
    for id in [root, left, right] {
        assert_eq!(provider.acquisitions(id), 1);
    }
}

struct FailingSink {
    accepted: usize,
}
impl Write for FailingSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.accepted >= 100 {
            return Err(io::Error::other("selected sink refusal"));
        }
        let count = bytes.len().min(100 - self.accepted);
        self.accepted += count;
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn failed_cursor_preserves_partial_sink_and_refuses_without_provider_replay() {
    let fixture = oracle::wide();
    let provider = Provider {
        fixture: &fixture,
        calls: RefCell::new(Vec::new()),
    };
    let mut cache = PageCache::bounded(1);
    let mut output = FailingSink { accepted: 0 };
    disabled_scope(|scope| {
        let mut cursor = RangeCursor::new(&provider, fixture.state, &mut cache, scope)?;
        assert!(matches!(
            cursor.read_segment(0..4096, &mut output, &mut cache),
            Err(ContentError::Io)
        ));
        assert_eq!(output.accepted, 100);
        let calls = provider.calls.borrow().len();
        assert!(matches!(
            cursor.read_segment(0..4096, &mut Vec::new(), &mut cache),
            Err(ContentError::InvalidRecord("mapping cursor terminal"))
        ));
        assert_eq!(provider.calls.borrow().len(), calls);
        Ok::<_, ContentError>(())
    })
    .unwrap();
}
