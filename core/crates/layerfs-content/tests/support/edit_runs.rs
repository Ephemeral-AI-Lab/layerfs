//! External stable run providers and guarded records; no product fault hooks.
use super::MemoryStore;
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, EditRecordApply, EditRecordChange,
    EditRecordExpected, EditRecordKey, EditSource, FileRun, IndexedEditBacking, ObjectId,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};

pub enum Piece {
    Data(Vec<u8>),
    Zero(u64),
}
impl Piece {
    fn len(&self) -> u64 {
        match self {
            Self::Data(bytes) => bytes.len() as u64,
            Self::Zero(length) => *length,
        }
    }
}
#[derive(Default)]
pub struct Sparse {
    pub parts: Vec<Vec<Piece>>,
    pub calls: Cell<usize>,
    pub data_bytes: Cell<u64>,
    pub zero_bytes: Cell<u64>,
}
impl Sparse {
    pub fn one(parts: Vec<Piece>) -> Self {
        Self {
            parts: vec![parts],
            ..Self::default()
        }
    }
    pub fn materialize(&self) -> super::edits::Parts {
        let mut source = super::edits::Parts::new();
        for parts in &self.parts {
            let mut output = Vec::new();
            for piece in parts {
                match piece {
                    Piece::Data(bytes) => output.extend_from_slice(bytes),
                    Piece::Zero(length) => output.resize(output.len() + *length as usize, 0),
                }
            }
            source.push(output);
        }
        source
    }
}
impl EditSource for Sparse {
    fn replacement_len(&self, index: usize) -> u64 {
        self.parts[index].iter().map(Piece::len).sum()
    }
    fn read_at(&self, _: usize, _: u64, _: &mut [u8]) -> ContentResult<usize> {
        panic!("the production edit must consume the run port")
    }
    fn read_run_at(&self, index: usize, offset: u64, output: &mut [u8]) -> ContentResult<FileRun> {
        self.calls.set(self.calls.get() + 1);
        let mut before = 0;
        for piece in &self.parts[index] {
            let end = before + piece.len();
            if offset < end {
                return match piece {
                    Piece::Zero(_) => {
                        self.zero_bytes.set(self.zero_bytes.get() + end - offset);
                        Ok(FileRun::Zero(end - offset))
                    }
                    Piece::Data(bytes) => {
                        let from = (offset - before) as usize;
                        let count = output.len().min(bytes.len() - from);
                        output[..count].copy_from_slice(&bytes[from..from + count]);
                        self.data_bytes.set(self.data_bytes.get() + count as u64);
                        Ok(FileRun::Data(count))
                    }
                };
            }
            before = end;
        }
        Ok(FileRun::End)
    }
}

pub struct Counted<'a> {
    pub store: &'a MemoryStore,
    pub requests: RefCell<Vec<ObjectId>>,
    pub deny: RefCell<Option<(ObjectId, ContentError)>>,
    pub denied: Cell<usize>,
    pub after_error: Cell<usize>,
}
impl<'a> Counted<'a> {
    pub fn new(store: &'a MemoryStore) -> Self {
        Self {
            store,
            requests: RefCell::new(Vec::new()),
            deny: RefCell::new(None),
            denied: Cell::new(0),
            after_error: Cell::new(0),
        }
    }
}
impl AuthenticatedObjects for Counted<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if self.denied.get() != 0 {
            self.after_error.set(self.after_error.get() + 1);
        }
        self.requests.borrow_mut().extend_from_slice(ids);
        let refuses = self
            .deny
            .borrow()
            .as_ref()
            .is_some_and(|(id, _)| ids.contains(id));
        if refuses {
            let (_, original) = self.deny.borrow_mut().take().unwrap();
            self.denied.set(self.denied.get() + 1);
            return Err(original);
        }
        self.store.read_canonical_batch(ids)
    }
}

#[derive(Default)]
pub struct Backing {
    pub values: BTreeMap<EditRecordKey, Vec<u8>>,
    pub calls: usize,
    pub max_bytes: usize,
    pub forbidden: bool,
}
impl Backing {
    fn enter(&mut self) {
        assert!(
            !self.forbidden,
            "an immutable/no-op path mutated or read edit backing"
        );
        self.calls += 1;
    }
}
impl IndexedEditBacking for Backing {
    fn contains(&mut self, key: EditRecordKey) -> ContentResult<bool> {
        self.enter();
        Ok(self.values.contains_key(&key))
    }
    fn get(&mut self, key: EditRecordKey) -> ContentResult<Option<Vec<u8>>> {
        self.enter();
        Ok(self.values.get(&key).cloned())
    }
    fn apply(&mut self, changes: Vec<EditRecordChange>) -> ContentResult<EditRecordApply> {
        self.enter();
        assert!(changes.windows(2).all(|pair| pair[0].key < pair[1].key));
        let bytes = changes.capacity() * std::mem::size_of::<EditRecordChange>()
            + changes
                .iter()
                .map(|change| {
                    let old = match &change.expected {
                        EditRecordExpected::Missing => 0,
                        EditRecordExpected::ExactBytes(value) => value.capacity(),
                    };
                    old + change.value.as_ref().map_or(0, Vec::capacity)
                })
                .sum::<usize>();
        assert!(bytes <= 65_536);
        self.max_bytes = self.max_bytes.max(bytes);
        for (index, change) in changes.iter().enumerate() {
            let actual = self.values.get(&change.key);
            let equal = match (&change.expected, actual) {
                (EditRecordExpected::Missing, None) => true,
                (EditRecordExpected::ExactBytes(expected), Some(value)) => expected == value,
                _ => false,
            };
            if !equal {
                return Ok(EditRecordApply::NotApplied {
                    index,
                    key: change.key,
                    actual: actual.cloned(),
                });
            }
        }
        for change in changes {
            match change.value {
                Some(value) => {
                    self.values.insert(change.key, value);
                }
                None => {
                    self.values.remove(&change.key);
                }
            }
        }
        Ok(EditRecordApply::Applied)
    }
    fn first_keys(
        &mut self,
        kind: u32,
        excluded: Option<[u8; 32]>,
    ) -> ContentResult<Vec<[u8; 32]>> {
        self.enter();
        Ok(self
            .values
            .range(
                EditRecordKey { kind, key: [0; 32] }..=EditRecordKey {
                    kind,
                    key: [255; 32],
                },
            )
            .filter_map(|(key, _)| (Some(key.key) != excluded).then_some(key.key))
            .take(64)
            .collect())
    }
}
