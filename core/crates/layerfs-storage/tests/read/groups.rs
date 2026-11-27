//! Group reads (#758): needed groups of several packs come back in one call
//! as ranges or whole packs, in request order and within the cache bound; a
//! cached unit refuses its pack under another digest; ranged bytes are
//! authenticated by the objects rebuilt from them.
use crate::chains;

use chains::{copy, history, read, ChainProvider, Shape};
use layerfs_content::ObjectId;
use layerfs_storage::{
    encoding::{PackAcquisition, PackCache, PackUnit},
    location::{ObjectLocation, PackInfo, SignatureRow, Source},
    pack::layout,
    port::{
        AcquiredPackRead, ObjectKey, PackRange, PackReadChoice, PackReadPlan, PersistenceError,
    },
    StorageError, StorageResult,
};
use std::sync::Arc;

const EDITS: Shape = Shape {
    files: 4,
    versions: 6,
    size: 24_000,
    rewrite: false,
    reopen: None,
};

#[test]
fn ranges_where_a_directory_is_known_and_whole_packs_where_not() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let (expected, _) = read(copy(&memory), &roots[..1], true);
    let ranged = Arc::new(ChainProvider::new(copy(&memory)));
    let (got, work) = read(ranged.clone(), &roots[..1], true);
    assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
    let requests = ranged.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 1);
    let ranges: usize = requests[0].iter().map(|read| read.ranges.len()).sum();
    let whole = requests[0]
        .iter()
        .filter(|read| read.ranges.is_empty())
        .count();
    assert!(ranges > 0, "a sparse pack is read as ranges");
    assert_eq!(work.group_read_ranges, ranges as u64);
    assert_eq!(work.group_read_whole, whole as u64);
    assert_eq!(work.group_read_packs, requests[0].len() as u64);
    let returned: u64 = requests[0]
        .iter()
        .map(|read| match read.ranges.is_empty() {
            true => read.pack.length as u64,
            false => read.ranges.iter().map(|range| range.length as u64).sum(),
        })
        .sum();
    assert_eq!(work.group_read_bytes, returned);
    // Without directories every pack is asked for whole, and the port's
    // default answer (whole packs through `read_packs`) serves the read.
    let mut plain = ChainProvider::new(copy(&memory));
    plain.directories = false;
    plain.ranged = false;
    let plain = Arc::new(plain);
    let (got, work) = read(plain.clone(), &roots[..1], true);
    assert_eq!(got.unwrap(), expected.unwrap());
    let requests = plain.requests.lock().unwrap().clone();
    assert!(requests.iter().flatten().all(|read| read.ranges.is_empty()));
    assert_eq!(work.group_read_whole, work.group_read_packs);
    assert_eq!(work.chain_hint_stale, 0);
}

#[test]
fn each_call_stays_within_the_cache_bound() {
    let shape = Shape {
        files: 48,
        versions: 3,
        size: 60_000,
        ..EDITS
    };
    let (memory, roots, _) = history(shape, |m| m);
    let (expected, _) = read(copy(&memory), &roots, true);
    for directories in [true, false] {
        let mut provider = ChainProvider::new(copy(&memory));
        provider.directories = directories;
        let provider = Arc::new(provider);
        // The provider refuses any call over the bound.
        let (got, work) = read(provider.clone(), &roots, true);
        assert_eq!(got.unwrap(), *expected.as_ref().unwrap());
        assert!(provider.calls("read_groups") >= 2, "the demand spans calls");
        assert_eq!(work.chain_hint_stale, 0);
    }
}

#[test]
fn answers_out_of_request_order_are_refused() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let mut provider = ChainProvider::new(copy(&memory));
    provider.ranged = false;
    provider.reverse = true;
    let (got, _) = read(Arc::new(provider), &roots[..1], true);
    assert!(
        matches!(got, Err(StorageError::Integrity("group read reply"))),
        "{got:?}"
    );
}

#[test]
fn a_corrupted_range_fails_at_the_object_hash() {
    let (memory, roots, _) = history(EDITS, |m| m);
    let provider = Arc::new(ChainProvider::new(copy(&memory)));
    let (_, _) = read(provider.clone(), &roots[..1], true);
    let target = provider.requests.lock().unwrap()[0]
        .iter()
        .find(|read| !read.ranges.is_empty())
        .map(|read| (read.pack.pack_id, read.ranges[0]))
        .expect("a ranged read");
    // Every byte of that range, one at a time, is either unused by the read
    // or caught: never a silently wrong answer.
    let (pack, range) = target;
    let mut caught = 0;
    for at in (range.offset..range.offset + range.length).step_by(997) {
        let provider = ChainProvider::new(copy(&memory));
        *provider.corrupt_range.lock().unwrap() = Some((pack, at));
        match read(Arc::new(provider), &roots[..1], false).0 {
            Ok(bytes) => assert_eq!(bytes, read(copy(&memory), &roots[..1], true).0.unwrap()),
            Err(StorageError::Integrity(_)) => caught += 1,
            Err(other) => panic!("{other:?}"),
        }
    }
    assert!(caught > 0, "a used byte was corrupted");
}

/// Serves one real pack's groups under a chosen digest.
struct Keyed {
    info: PackInfo,
    body: Vec<u8>,
}
struct Groups<'a>(&'a [usize]);
impl PackReadPlan for Groups<'_> {
    fn select(
        &mut self,
        info: PackInfo,
        prefix: &[u8],
    ) -> Result<PackReadChoice, PersistenceError> {
        let header = layout::parse_directory_header(prefix, info.length).unwrap();
        let views = layout::directory_group_views(prefix, header).unwrap();
        Ok(PackReadChoice::Ranges(
            self.0
                .iter()
                .map(|group| PackRange {
                    offset: views[*group].start,
                    length: views[*group].end - views[*group].start,
                })
                .collect(),
        ))
    }
}
impl Source for Keyed {
    fn location(&self, _: ObjectId, _: i64) -> StorageResult<Option<ObjectLocation>> {
        Ok(None)
    }
    fn pack_bytes(&self, _: i64) -> StorageResult<Vec<u8>> {
        Ok(self.body.clone())
    }
    fn acquire_groups(&self, _: i64, groups: &[usize], _: bool) -> StorageResult<PackAcquisition> {
        let read = AcquiredPackRead::acquire(self.info, &mut Groups(groups), |at, out| {
            out.copy_from_slice(&self.body[at..at + out.len()]);
            Ok(())
        })?;
        let AcquiredPackRead::Units(units) = read else {
            panic!("ranges were planned");
        };
        Ok(PackAcquisition::Units(PackUnit::from_acquired(units)?))
    }
    fn inode_values(&self, _: &[u32]) -> StorageResult<Vec<[u8; 73]>> {
        unreachable!()
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        unreachable!()
    }
    fn write_signatures(&self, _: &[SignatureRow]) -> StorageResult<usize> {
        unreachable!()
    }
}

#[test]
fn a_cached_range_refuses_its_pack_under_another_digest() {
    let (memory, _, _) = history(EDITS, |m| m);
    let state = memory.state.lock().unwrap();
    let pack = state
        .packs
        .values()
        .find(|pack| layout::parse_header(&pack.body).unwrap().group_count >= 2)
        .expect("a pack of several groups");
    let first = Keyed {
        info: pack.info,
        body: pack.body.as_ref().clone(),
    };
    let mut info = pack.info;
    info.key = ObjectKey::for_bytes(b"the same pack id under another digest");
    let second = Keyed {
        info,
        body: pack.body.as_ref().clone(),
    };
    let id = pack.info.pack_id;
    drop(state);
    let mut cache = PackCache::new();
    cache.acquire_groups(&first, id, &[0]).unwrap();
    assert!(cache.group(&first, id, 0).unwrap().is_some());
    assert!(matches!(
        cache.acquire_groups(&second, id, &[1]),
        Err(StorageError::Integrity(
            "immutable cached descriptor changed"
        ))
    ));
    assert!(cache.group(&first, id, 0).unwrap().is_some(), "kept");
    assert_eq!(cache.work().evictions, 0);
}
