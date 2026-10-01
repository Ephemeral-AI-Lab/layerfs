//! Deferred actual-table binding and charged fixed fact/parent failure owners.
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::{
    BaseFact, FactCapacity, FactScope, FactSeal, GraphMemory, GraphMemoryLease, GraphSubject,
    ParentFact, ParentSeal,
};
use std::cell::Cell;
#[derive(Clone, Debug, Default)]
pub(crate) struct Section {
    pub(crate) scope: Option<FactScope>,
    pub(crate) stage: u8,
    pub(crate) records: u64,
    pub(crate) bound: u64,
    pub(crate) bytes: u64,
    pub(crate) remaining: u64,
    pub(crate) after: Option<u64>,
    pub(crate) maximum: Option<u64>,
    pub(crate) digest: Option<[u8; 32]>,
}
pub(crate) struct FactAttemptData {
    pub(crate) kind: &'static str,
    pub(crate) table: u8,
    pub(crate) before: Section,
    pub(crate) after: Section,
    pub(crate) bases: Vec<BaseFact>,
    pub(crate) parents: Vec<(Option<ParentFact>, Option<ParentFact>)>,
}
pub(crate) struct FactAttempt {
    value: Box<FactAttemptData>,
    _memory: GraphMemoryLease,
}
impl FactAttempt {
    pub(crate) fn new(
        owner: &Facts,
        table: u8,
        kind: &'static str,
        count: usize,
    ) -> StorageResult<Self> {
        if count > 128 {
            return Err(StorageError::Integrity("fact attempt window"));
        }
        let width = if table == 17 {
            std::mem::size_of::<BaseFact>()
        } else {
            std::mem::size_of::<(Option<ParentFact>, Option<ParentFact>)>()
        };
        let memory = owner.memory.reserve(
            std::mem::size_of::<FactAttemptData>() + std::mem::size_of::<Self>() + count * width,
        )?;
        let mut bases = Vec::new();
        let mut parents = Vec::new();
        if table == 17 {
            bases.try_reserve_exact(count).map_err(|_| unavailable())?;
            if bases.capacity() > count {
                return Err(unavailable());
            }
        } else {
            parents
                .try_reserve_exact(count)
                .map_err(|_| unavailable())?;
            if parents.capacity() > count {
                return Err(unavailable());
            }
        }
        let before = owner.section(table).clone();
        Ok(Self {
            value: Box::new(FactAttemptData {
                kind,
                table,
                before: before.clone(),
                after: before,
                bases,
                parents,
            }),
            _memory: memory,
        })
    }
}
fn unavailable() -> StorageError {
    StorageError::Content(layerfs_content::ContentError::ResourceUnavailable {
        what: "fact.attempt",
    })
}
impl std::ops::Deref for FactAttempt {
    type Target = FactAttemptData;
    fn deref(&self) -> &FactAttemptData {
        &self.value
    }
}
impl std::ops::DerefMut for FactAttempt {
    fn deref_mut(&mut self) -> &mut FactAttemptData {
        &mut self.value
    }
}
pub(crate) struct Facts {
    pub(crate) subject: GraphSubject,
    pub(crate) capacity: FactCapacity,
    pub(crate) memory: GraphMemory,
    pub(crate) base: Section,
    pub(crate) parent: Section,
    pub(crate) failed: Cell<bool>,
    pub(crate) attempt: Option<FactAttempt>,
    pub(crate) base_seal: Option<FactSeal>,
    pub(crate) parent_seal: Option<ParentSeal>,
}
pub(crate) struct FactsOwner {
    value: Box<Facts>,
    _memory: GraphMemoryLease,
}
impl std::ops::Deref for FactsOwner {
    type Target = Facts;
    fn deref(&self) -> &Facts {
        &self.value
    }
}
impl std::ops::DerefMut for FactsOwner {
    fn deref_mut(&mut self) -> &mut Facts {
        &mut self.value
    }
}
impl Facts {
    // Construction returns the funded owner so its allocation and lease stay inseparable.
    #[allow(clippy::new_ret_no_self)]
    pub(crate) fn new(
        subject: GraphSubject,
        capacity: FactCapacity,
        memory: GraphMemory,
    ) -> StorageResult<FactsOwner> {
        let lease =
            memory.reserve(std::mem::size_of::<Self>() + std::mem::size_of::<FactsOwner>())?;
        Ok(FactsOwner {
            value: Box::new(Self {
                subject,
                capacity,
                memory,
                base: Section::default(),
                parent: Section::default(),
                failed: Cell::new(false),
                attempt: None,
                base_seal: None,
                parent_seal: None,
            }),
            _memory: lease,
        })
    }
    pub(crate) fn section(&self, table: u8) -> &Section {
        if table == 17 {
            &self.base
        } else {
            &self.parent
        }
    }
    pub(crate) fn section_mut(&mut self, table: u8) -> &mut Section {
        if table == 17 {
            &mut self.base
        } else {
            &mut self.parent
        }
    }
    pub(crate) fn select(&self, scope: &FactScope) -> StorageResult<u8> {
        let table = scope.state().table().code();
        if !matches!(table, 17 | 18)
            || scope.subject().selected() != &self.subject
            || self
                .section(table)
                .scope
                .as_ref()
                .is_some_and(|s| s != scope)
        {
            return Err(StorageError::Content(
                layerfs_content::ContentError::InvalidOrderingRecord("fact foreign scope"),
            ));
        }
        if self.failed.get() {
            return Err(StorageError::Integrity("fact terminal owner"));
        }
        Ok(table)
    }
    pub(crate) fn acknowledge(&mut self) {
        let FactAttempt { value, _memory } = self.attempt.take().unwrap();
        let FactAttemptData {
            table,
            after,
            bases,
            parents,
            ..
        } = *value;
        *self.section_mut(table) = after;
        drop(bases);
        drop(parents);
        drop(_memory);
    }
    pub(crate) fn description(&self) -> String {
        let mut text = format!(
            "fact custody: subject={:?}, class={:?}, base={:?}, parent={:?}",
            self.subject.encode(),
            self.capacity,
            self.base,
            self.parent
        );
        if let Some(a) = &self.attempt {
            text.push_str(&format!(
                "; fact attempt {}: table={}, before={:?}, proposed={:?}, bases={:?}, parents={:?}",
                a.kind, a.table, a.before, a.after, a.bases, a.parents
            ));
        }
        text
    }
}
