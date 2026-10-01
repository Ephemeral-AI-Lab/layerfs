//! Prospectively selected compatibility or supplied fact calls through one producer.
use super::{ValidationState, ValidationWork};
use crate::filesystem::inode::read::InodeTable;
use crate::filesystem::state::{FactScope, FactState};
use crate::object::{inode_leaf::InodeValue, AuthenticatedObjects};
use crate::{ContentError, ContentResult};

type FactLookup<S> = fn(
    &mut ValidationState,
    &mut S,
    &dyn AuthenticatedObjects,
    InodeTable,
    u64,
    &mut ValidationWork,
) -> ContentResult<Option<InodeValue>>;

type FactPrefetch<S> = fn(
    &mut ValidationState,
    &mut S,
    &dyn AuthenticatedObjects,
    InodeTable,
    &[u64],
    &mut ValidationWork,
) -> ContentResult<()>;

pub(super) struct FactAccess<S: ?Sized> {
    new: fn(&mut S, &FactScope) -> ContentResult<ValidationState>,
    working: fn(
        &mut ValidationState,
        &mut S,
        usize,
    ) -> ContentResult<Option<crate::filesystem::state::GraphMemoryLease>>,
    lookup: FactLookup<S>,
    known: fn(&mut ValidationState, &mut S, InodeTable, u64) -> ContentResult<bool>,
    prefetch: FactPrefetch<S>,
}
impl<S: ?Sized> Copy for FactAccess<S> {}
impl<S: ?Sized> Clone for FactAccess<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: ?Sized> FactAccess<S> {
    pub(super) fn compatibility() -> Self {
        Self {
            new: |_, _| {
                Err(ContentError::InvalidOrderingRecord(
                    "unexpected supplied fact scope",
                ))
            },
            working: |_, _, _| Ok(None),
            lookup: |facts, _, reader, table, serial, work| {
                facts.lookup_optional(reader, table, serial, work)
            },
            known: |facts, _, _, serial| Ok(facts.known(serial)),
            prefetch: |facts, _, reader, table, serials, work| {
                facts.prefetch_wave(reader, table, serials, work)
            },
        }
    }
    pub(super) fn initialize(
        self,
        state: &mut S,
        scope: &FactScope,
    ) -> ContentResult<ValidationState> {
        (self.new)(state, scope)
    }
    pub(super) fn working(
        self,
        facts: &mut ValidationState,
        state: &mut S,
        bytes: usize,
    ) -> ContentResult<Option<crate::filesystem::state::GraphMemoryLease>> {
        (self.working)(facts, state, bytes)
    }
    pub(super) fn lookup(
        self,
        facts: &mut ValidationState,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serial: u64,
        work: &mut ValidationWork,
    ) -> ContentResult<Option<InodeValue>> {
        (self.lookup)(facts, state, reader, table, serial, work)
    }
    pub(super) fn known(
        self,
        facts: &mut ValidationState,
        state: &mut S,
        table: InodeTable,
        serial: u64,
    ) -> ContentResult<bool> {
        (self.known)(facts, state, table, serial)
    }
    pub(super) fn prefetch(
        self,
        facts: &mut ValidationState,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
        work: &mut ValidationWork,
    ) -> ContentResult<()> {
        (self.prefetch)(facts, state, reader, table, serials, work)
    }
}
impl<S: FactState + ?Sized> FactAccess<S> {
    pub(super) fn supplied() -> Self {
        Self {
            new: |state, scope| ValidationState::supplied(scope.clone(), state.fact_memory(scope)?),
            working: |facts, state, bytes| facts.reserve_working(state, bytes).map(Some),
            lookup: |facts, state, reader, table, serial, work| {
                facts.lookup_supplied(state, reader, table, serial, work)
            },
            known: |facts, state, table, serial| facts.known_supplied(state, table, serial),
            prefetch: |facts, state, reader, table, serials, work| {
                facts.prefetch_supplied(state, reader, table, serials, work)
            },
        }
    }
}

impl<S: FactState + crate::filesystem::state::BindingSiteState + ?Sized>
    FactAccess<crate::filesystem::state::BindingSites<'_, S>>
{
    pub(super) fn supplied_sites() -> Self {
        Self {
            new: |state, scope| {
                ValidationState::supplied(scope.clone(), state.state().fact_memory(scope)?)
            },
            working: |facts, state, bytes| facts.reserve_working(state.state(), bytes).map(Some),
            lookup: |facts, state, reader, table, serial, work| {
                facts.lookup_supplied(state.state(), reader, table, serial, work)
            },
            known: |facts, state, table, serial| facts.known_supplied(state.state(), table, serial),
            prefetch: |facts, state, reader, table, serials, work| {
                facts.prefetch_supplied(state.state(), reader, table, serials, work)
            },
        }
    }
}
