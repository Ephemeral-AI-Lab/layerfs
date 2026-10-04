//! The same byte/count allowance bounds complete bodies and selected units.
use layerfs_content::ObjectId;
use layerfs_storage::{
    encoding::{PackAcquisition, PackCache, PackUnit},
    location::{ObjectLocation, PackDomain, PackInfo, SignatureRow, ValueGroupRow},
    pack::{
        assemble, build_group,
        layout::{self, PackLane},
    },
    policy,
    port::*,
    source::Source,
    StorageError, StorageResult,
};
use std::cell::Cell;
struct Plan<'a>(&'a [usize]);
impl PackReadPlan for Plan<'_> {
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
                .map(|number| {
                    let view = views[*number];
                    PackRange {
                        offset: view.start,
                        length: view.end - view.start,
                    }
                })
                .collect(),
        ))
    }
}
struct Input {
    body: Vec<u8>,
    key: ObjectKey,
    calls: Cell<usize>,
    whole: Cell<bool>,
    validations: Cell<usize>,
    directory_passes: Cell<usize>,
    unshared_walks: Cell<usize>,
}
impl Input {
    fn new(record_bytes: usize) -> Self {
        let groups: Vec<_> = (0..8)
            .map(|_| build_group(PackLane::Ordinary, &[vec![0; record_bytes]], None).unwrap())
            .collect();
        let body = assemble(PackLane::Ordinary, &groups).unwrap();
        let key = ObjectKey::for_bytes(&body);
        Self {
            body,
            key,
            calls: Cell::new(0),
            whole: Cell::new(false),
            validations: Cell::new(0),
            directory_passes: Cell::new(0),
            unshared_walks: Cell::new(0),
        }
    }
    fn info(&self, id: i64) -> PackInfo {
        PackInfo {
            pack_id: id,
            domain: PackDomain::Metadata,
            key: self.key,
            length: self.body.len(),
        }
    }
}
impl Source for Input {
    fn location(&self, _: ObjectId, _: i64) -> StorageResult<Option<ObjectLocation>> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn pack_bytes(&self, _: i64) -> StorageResult<Vec<u8>> {
        Err(StorageError::Integrity("unexpected whole route"))
    }
    fn acquire_groups(
        &self,
        id: i64,
        groups: &[usize],
        whole_for_reuse: bool,
    ) -> StorageResult<PackAcquisition> {
        self.calls.set(self.calls.get() + 1);
        if whole_for_reuse || self.whole.get() {
            let row = PersistedPack::authenticate(self.info(id), self.body.clone())?;
            let (info, body) = row.into_parts();
            return Ok(PackAcquisition::Whole {
                info: Some(info),
                body,
            });
        }
        let acquired =
            PersistedPackRead::acquire(self.info(id), &mut Plan(groups), |offset, out| {
                out.copy_from_slice(&self.body[offset..offset + out.len()]);
                Ok(())
            })?;
        let PersistedPackRead::Ranges(row) = acquired else {
            panic!("strategy changed")
        };
        Ok(PackAcquisition::Units(PackUnit::from_ranges(row)?))
    }
    fn validate_cached_pack(&self, info: PackInfo) -> StorageResult<()> {
        self.validations.set(self.validations.get() + 1);
        if info != self.info(info.pack_id) {
            return Err(StorageError::Integrity("fixture descriptor"));
        }
        Ok(())
    }
    fn note_pack_directory_validation(&self, groups: usize, entries: usize) {
        assert_eq!(entries, 8);
        self.directory_passes.set(self.directory_passes.get() + 1);
        self.unshared_walks.set(self.unshared_walks.get() + groups);
    }
    fn value_group(&self, _: u32) -> StorageResult<Option<ValueGroupRow>> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn value_groups(
        &self,
        _: Option<u32>,
        _: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn window_start(&self) -> StorageResult<u32> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
    fn write_signatures(&self, _: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::Integrity("unexpected fixture query"))
    }
}
#[test]
fn selected_units_evict_selectively_under_the_same_two_mib_body_limit() {
    let source = Input::new(32_000);
    let mut cache = PackCache::new();
    for id in 1..=80 {
        cache.acquire_groups(&source, id, &[0]).unwrap();
        assert!(cache.retained_bytes() <= policy::DEPENDENCY_PACK_CACHE_BYTES);
    }
    assert!(cache.work().evictions > 0);
    assert!(cache.work().peak_bytes <= policy::DEPENDENCY_PACK_CACHE_BYTES);
    assert!(cache.group(&source, 31, 0).unwrap().is_some());
    assert_eq!(cache.acquire_groups(&source, 31, &[0]).unwrap(), (false, 0));
    cache.acquire_groups(&source, 81, &[0]).unwrap();
    assert!(cache.group(&source, 31, 0).unwrap().is_some());
    assert_eq!(source.calls.get(), 81);
}
#[test]
fn tiny_selected_units_still_obey_the_existing_entry_count_limit() {
    let source = Input::new(32);
    let mut cache = PackCache::new();
    for id in 1..=policy::READ_OBJECT_LIMIT as i64 + 1 {
        cache.acquire_groups(&source, id, &[0]).unwrap();
    }
    assert!(cache.retained_bytes() < policy::DEPENDENCY_PACK_CACHE_BYTES);
    assert_eq!(cache.work().evictions, 1);
    assert!(cache.group(&source, 1, 0).unwrap().is_none());
    assert!(cache
        .group(&source, policy::READ_OBJECT_LIMIT as i64 + 1, 0)
        .unwrap()
        .is_some());
}

#[test]
fn whole_promotion_replaces_units_within_same_byte_allowance() {
    let source = Input::new(32_000);
    let mut cache = PackCache::new();
    cache.acquire_groups(&source, 1, &[0]).unwrap();
    assert!(!cache.contains_key(&1));
    cache.acquire_groups(&source, 1, &[7]).unwrap();
    assert!(cache.contains_key(&1));
    assert_eq!(cache.retained_bytes(), source.body.len());
    assert!(cache.work().peak_bytes <= policy::DEPENDENCY_PACK_CACHE_BYTES);
    for number in 0..8 {
        assert_eq!(
            cache.acquire_groups(&source, 1, &[number]).unwrap(),
            (false, 0)
        );
    }
    assert_eq!(source.calls.get(), 2);
}

#[test]
fn whole_cohort_shares_full_directory_walks_and_preserves_descriptor_checks() {
    let source = Input::new(32);
    source.whole.set(true);
    let mut cache = PackCache::new();
    let wanted: Vec<_> = (0..8).collect();
    assert_eq!(
        cache.acquire_groups(&source, 1, &wanted).unwrap(),
        (true, source.body.len())
    );
    assert_eq!(source.directory_passes.get(), 2);
    assert_eq!(source.unshared_walks.get(), 16);
    assert_eq!(source.validations.get(), 8);
    assert_eq!(
        cache.acquire_groups(&source, 1, &wanted).unwrap(),
        (false, 0)
    );
    assert_eq!(source.directory_passes.get(), 3);
    assert_eq!(source.unshared_walks.get(), 24);
    assert_eq!(source.validations.get(), 16);
    assert_eq!(source.calls.get(), 1);
    assert_eq!(cache.retained_bytes(), source.body.len());
    assert!(cache.acquire_groups(&source, 1, &[0, 8]).is_err());
    assert!(cache.acquire_groups(&source, 1, &[8]).is_err());
}

#[test]
fn cohort_still_refuses_corruption_in_an_unrequested_directory_entry() {
    let source = Input::new(32);
    for field in [0, 4, 8, 12] {
        let mut body = source.body.clone();
        let at = layout::HEADER_LEN + 7 * layout::DIRECTORY_ENTRY_LEN + field;
        body[at] ^= 0x80;
        let mut cache = PackCache::new();
        cache.insert(1, body).unwrap();
        assert!(
            cache.acquire_groups(&source, 1, &[0, 1]).is_err(),
            "field {field}"
        );
    }
    let groups: Vec<_> = (0..8)
        .map(|_| build_group(PackLane::WholeFile, &[vec![0; 32]], None).unwrap())
        .collect();
    let mut body = assemble(PackLane::WholeFile, &groups).unwrap();
    body[layout::HEADER_LEN + 7 * layout::WHOLE_FILE_ENTRY_LEN] ^= 0x80;
    let mut cache = PackCache::new();
    cache.insert(1, body).unwrap();
    assert!(cache.acquire_groups(&source, 1, &[0, 1]).is_err());
}
