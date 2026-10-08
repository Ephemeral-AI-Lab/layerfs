//! External raw backing fixture; bounded jobs and original refused vector custody.
use layerfs_content::{
    ConstructionRecordApply, ConstructionRecordChange, ConstructionRecordExpected,
    ConstructionRecordKey, ContentError, ContentResult, IndexedConstructionBacking,
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};
pub const CONTEXT: u32 = 0x4653_0000;
pub const FRESH: u32 = CONTEXT + 4;
pub const ROW: u32 = CONTEXT + 5;
pub const TOUCH: u32 = CONTEXT + 6;
pub const WORK: u32 = CONTEXT + 7;
pub const FRAME: u32 = CONTEXT + 8;
pub const NODE: u32 = CONTEXT + 9;
/// Validation topology evidence; the reserved range ends at `CONTEXT + 0x2F`.
pub const PLACED: u32 = CONTEXT + 0x20;
pub const TERRITORY: u32 = CONTEXT + 0x21;
pub const ROOTED: u32 = CONTEXT + 0x22;
pub const QUEUE: u32 = CONTEXT + 0x23;
pub const SCANNED: u32 = CONTEXT + 0x24;
pub fn topology(kind: u32) -> bool {
    (PLACED..=CONTEXT + 0x2F).contains(&kind)
}
pub fn key(kind: u32, serial: u64) -> ConstructionRecordKey {
    let mut key = [0; 32];
    key[24..].copy_from_slice(&serial.to_be_bytes());
    ConstructionRecordKey { kind, key }
}
#[derive(Default)]
pub struct Records {
    pub values: BTreeMap<ConstructionRecordKey, Vec<u8>>,
    pub calls: usize,
    pub after_failure: usize,
    pub terminal: bool,
    pub original: Option<Vec<ConstructionRecordChange>>,
    pub fail_kind: Option<u32>,
    pub fail_final_row_at: Option<u64>,
    pub scan_starts: usize,
    pub fault: Rc<Cell<bool>>,
    pub refuse_child_progress: bool,
    pub refuse_kind: Option<u32>,
    pub corrupt_kind: Option<u32>,
    pub missing_kind: Option<u32>,
    pub corrupt_row_tag: bool,
    pub missing_queued: bool,
    pub malformed_key: bool,
    pub duplicate_key: bool,
    pub maximum_job: usize,
    pub maximum_keys: usize,
    pub maximum_changes: usize,
    pub enumerations: usize,
    /// Point reads, guarded batches and enumerations of topology kinds.
    pub topology_reads: usize,
    pub topology_batches: usize,
    pub topology_enumerations: usize,
    pub row_versions: BTreeMap<u64, Vec<Vec<u8>>>,
}
impl Records {
    fn enter(&mut self) -> ContentResult<()> {
        if self.terminal {
            self.after_failure += 1;
            return Err(ContentError::ProviderFailure {
                what: "fixture terminal",
            });
        }
        self.calls += 1;
        Ok(())
    }
    pub fn failure(&mut self) -> ContentError {
        self.terminal = true;
        self.fault.set(true);
        ContentError::ProviderFailure {
            what: "original indexed reference failure",
        }
    }
}
impl IndexedConstructionBacking for Records {
    fn contains(&mut self, key: ConstructionRecordKey) -> ContentResult<bool> {
        self.enter()?;
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: ConstructionRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.enter()?;
        if topology(key.kind) {
            self.topology_reads += 1;
        }
        let serial = u64::from_be_bytes(key.key[24..].try_into().unwrap());
        if key.kind == ROW
            && self.scan_starts >= 2
            && self.fail_final_row_at.is_some_and(|at| serial >= at)
        {
            return Err(self.failure());
        }
        if self.missing_kind == Some(key.kind) && self.values.contains_key(&key) {
            return Ok(None);
        }
        let mut result = self.values.get(&key).cloned();
        if self.corrupt_kind == Some(key.kind) && result.is_some() {
            self.terminal = true;
            return Ok(Some(vec![255]));
        }
        if self.corrupt_row_tag && key.kind == ROW {
            if let Some(row) = &mut result {
                row[9] = 1;
                self.terminal = true;
            }
        }
        Ok(result)
    }
    fn apply(
        &mut self,
        changes: Vec<ConstructionRecordChange>,
    ) -> ContentResult<ConstructionRecordApply> {
        self.enter()?;
        assert!(changes.windows(2).all(|pair| pair[0].key < pair[1].key));
        let bytes = changes.capacity() * std::mem::size_of::<ConstructionRecordChange>()
            + changes
                .iter()
                .map(|change| {
                    change.value.as_ref().map_or(0, Vec::capacity)
                        + match &change.expected {
                            ConstructionRecordExpected::Missing => 0,
                            ConstructionRecordExpected::ExactBytes(value) => value.capacity(),
                        }
                })
                .sum::<usize>();
        assert!(bytes <= 65_536);
        self.maximum_job = self.maximum_job.max(bytes);
        self.maximum_changes = self.maximum_changes.max(changes.len());
        if changes.iter().any(|change| topology(change.key.kind)) {
            assert!(changes.iter().all(|change| topology(change.key.kind)));
            assert!(changes.len() <= 64, "one topology batch is one key window");
            self.topology_batches += 1;
        }
        if changes
            .iter()
            .any(|change| self.fail_kind == Some(change.key.kind))
        {
            self.original = Some(changes);
            return Err(self.failure());
        }
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let valid = match (&change.expected, actual) {
                (ConstructionRecordExpected::Missing, None) => true,
                (ConstructionRecordExpected::ExactBytes(expected), Some(actual)) => {
                    expected == actual
                }
                _ => false,
            };
            let child_progress = self.refuse_child_progress
                && changes.iter().any(|c| c.key.kind == ROW)
                && changes.iter().any(|c| {
                    c.key.kind == FRAME
                        && matches!(&c.expected, ConstructionRecordExpected::ExactBytes(_))
                });
            if !valid || self.refuse_kind == Some(change.key.kind) || child_progress {
                let result = ConstructionRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                };
                self.original = Some(changes);
                self.terminal = true;
                return Ok(result);
            }
        }
        for change in changes {
            if let Some(value) = change.value {
                if change.key.kind == ROW {
                    let serial = u64::from_be_bytes(change.key.key[24..].try_into().unwrap());
                    self.row_versions
                        .entry(serial)
                        .or_default()
                        .push(value.clone());
                }
                self.values.insert(change.key, value);
            } else {
                self.values.remove(&change.key);
            }
        }
        Ok(ConstructionRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.enter()?;
        assert_eq!(kind, WORK);
        if self.missing_queued && self.values.keys().any(|key| key.kind == WORK) {
            self.terminal = true;
            return Ok(Vec::new());
        }
        let keys = self
            .values
            .keys()
            .filter(|key| key.kind == kind && excluded != Some(key.key))
            .take(64)
            .map(|key| key.key)
            .collect::<Vec<_>>();
        self.maximum_keys = self.maximum_keys.max(keys.len());
        Ok(keys)
    }
    fn keys_after(&mut self, kind: u32, after: Option<[u8; 32]>) -> ContentResult<Vec<[u8; 32]>> {
        self.enter()?;
        if kind == PLACED {
            // The placement pass is a plain sealed enumeration: it shares the
            // window bound and none of the touched-membership fault machinery.
            self.topology_enumerations += 1;
            let keys = self
                .values
                .keys()
                .filter(|key| key.kind == kind && after.is_none_or(|after| key.key > after))
                .take(64)
                .map(|key| key.key)
                .collect::<Vec<_>>();
            self.maximum_keys = self.maximum_keys.max(keys.len());
            return Ok(keys);
        }
        assert_eq!(kind, TOUCH);
        if after.is_none() {
            self.scan_starts += 1;
        }
        self.enumerations += 1;
        let mut keys = self
            .values
            .keys()
            .filter(|key| key.kind == kind && after.is_none_or(|after| key.key > after))
            .take(64)
            .map(|key| key.key)
            .collect::<Vec<_>>();
        if !keys.is_empty() && self.malformed_key {
            keys.last_mut().unwrap()[0] = 1;
            self.terminal = true;
        }
        if !keys.is_empty() && self.duplicate_key {
            keys.insert(1, keys[0]);
            self.terminal = true;
        }
        self.maximum_keys = self.maximum_keys.max(keys.len());
        Ok(keys)
    }
}
