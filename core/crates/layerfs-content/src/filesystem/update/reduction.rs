//! Prospectively selected narrow reference reduction; legacy and supplied roles stay explicit.
use super::{lookup_base, zero_count_serials};
use crate::filesystem::inode::read::{lookup_many, InodeReadWork, InodeTable};
use crate::filesystem::references::backing::OrderingBacking;
use crate::filesystem::references::count_reduce::{BaseAnswers, CountFinalRows, CountReducer};
use crate::filesystem::references::reduce::{
    FinalChange, FinalRows, ReferenceReducer, ReferenceWork,
};
use crate::filesystem::references::release::{release_zero_count, ReleaseWork};
use crate::filesystem::rows::PreparedBindingRows;
use crate::filesystem::state::{
    CanonicalConstructionState, CanonicalScope, EligibilityAuthority, GraphMemoryLease,
    ParentCalls, ZeroSeal,
};
use crate::object::{inode_leaf::InodeValue, AuthenticatedObjects};
use crate::{ContentError, ContentResult};
pub(super) trait FinalSource<S: ?Sized> {
    fn next_change(&mut self, state: &mut S) -> ContentResult<Option<FinalChange>>;
    fn work(&self) -> ReferenceWork;
}
impl<S: ?Sized> FinalSource<S> for FinalRows<'_> {
    fn next_change(&mut self, _state: &mut S) -> ContentResult<Option<FinalChange>> {
        self.next_change()
    }
    fn work(&self) -> ReferenceWork {
        self.work()
    }
}
impl<S: CanonicalConstructionState + ?Sized> FinalSource<S> for CountFinalRows<'_> {
    fn next_change(&mut self, state: &mut S) -> ContentResult<Option<FinalChange>> {
        self.next_change(state)
    }
    fn work(&self) -> ReferenceWork {
        self.work()
    }
}
pub(super) trait Reduction<S: ?Sized> {
    type Seeds;
    type Final<'a>: FinalSource<S>;
    fn initial_counts(
        &mut self,
        input: &dyn PreparedBindingRows,
    ) -> ContentResult<Option<Vec<u64>>>;
    fn check_capacity(&self) -> ContentResult<()>;
    fn working(&mut self, state: &mut S, bytes: usize) -> ContentResult<Option<GraphMemoryLease>>;
    fn base_values(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
    ) -> ContentResult<BaseAnswers>;
    fn declare_new(&mut self, state: &mut S, serial: u64) -> ContentResult<()>;
    fn retained(&mut self, state: &mut S, serial: u64) -> ContentResult<()>;
    fn removed(&mut self, state: &mut S, serial: u64) -> ContentResult<()>;
    fn value(&mut self, state: &mut S, serial: u64, value: InodeValue) -> ContentResult<()>;
    #[allow(clippy::too_many_arguments)]
    fn zeros(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
        unreachable: &EligibilityAuthority,
        maximum: usize,
        parents: ParentCalls<S>,
    ) -> ContentResult<(Self::Seeds, u64)>;
    fn release_descendants(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        seeds: &Self::Seeds,
        batch: usize,
    ) -> ContentResult<ReleaseWork>;
    fn finish<'a>(
        &mut self,
        state: &mut S,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<Self::Final<'a>>;
    fn release(&mut self, state: &mut S) -> ContentResult<()>;
}
impl<S: ?Sized> Reduction<S> for ReferenceReducer<'_, '_> {
    type Seeds = Vec<u64>;
    type Final<'a> = FinalRows<'a>;
    fn initial_counts(
        &mut self,
        input: &dyn PreparedBindingRows,
    ) -> ContentResult<Option<Vec<u64>>> {
        if input.base().is_some() {
            return Ok(None);
        }
        if input.new_rows() > input.resources().maximum_touched_serials() {
            return Err(ContentError::ObjectLimitExceeded {
                limit: input.resources().maximum_touched_serials(),
                actual: input.new_rows(),
            });
        }
        Ok(Some(vec![0; input.new_rows()]))
    }
    fn check_capacity(&self) -> ContentResult<()> {
        self.check_backing_capacity()
    }
    fn working(
        &mut self,
        _state: &mut S,
        _bytes: usize,
    ) -> ContentResult<Option<GraphMemoryLease>> {
        Ok(None)
    }
    fn base_values(
        &mut self,
        _state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
    ) -> ContentResult<BaseAnswers> {
        Ok(BaseAnswers::compatibility(lookup_many(
            reader,
            table,
            serials,
            &mut InodeReadWork::default(),
        )?))
    }
    fn declare_new(&mut self, _state: &mut S, serial: u64) -> ContentResult<()> {
        self.declare_new(serial)
    }
    fn retained(&mut self, _state: &mut S, serial: u64) -> ContentResult<()> {
        self.note_retained_binding(serial)
    }
    fn removed(&mut self, _state: &mut S, serial: u64) -> ContentResult<()> {
        self.note_removed_binding(serial)
    }
    fn value(&mut self, _state: &mut S, serial: u64, value: InodeValue) -> ContentResult<()> {
        self.note_value(serial, value)
    }
    fn zeros(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
        unreachable: &EligibilityAuthority,
        maximum: usize,
        parents: ParentCalls<S>,
    ) -> ContentResult<(Vec<u64>, u64)> {
        zero_count_serials(
            reader,
            table,
            self,
            batch,
            root,
            unreachable,
            maximum,
            state,
            parents,
        )
    }
    fn release_descendants(
        &mut self,
        _state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        seeds: &Vec<u64>,
        batch: usize,
    ) -> ContentResult<ReleaseWork> {
        release_zero_count(
            reader,
            table,
            self,
            seeds,
            batch,
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
            |reader, serial| {
                lookup_base(reader, table, serial)?
                    .ok_or(ContentError::InvalidRecord("released inode record"))
            },
        )
    }
    fn finish<'a>(
        &mut self,
        _state: &mut S,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<FinalRows<'a>> {
        self.finish(reader, table, batch, root)
    }
    fn release(&mut self, _state: &mut S) -> ContentResult<()> {
        self.release()
    }
}
pub(super) struct SuppliedReduction<'r, 'b> {
    counts: CountReducer,
    backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
}
impl<'r, 'b> SuppliedReduction<'r, 'b> {
    pub(super) fn new<S: CanonicalConstructionState + ?Sized>(
        state: &mut S,
        scope: CanonicalScope,
        backing: Option<&'r mut (dyn OrderingBacking + 'b)>,
    ) -> ContentResult<Self> {
        Ok(Self {
            counts: CountReducer::new(
                state,
                scope,
                std::mem::size_of::<Self>() - std::mem::size_of::<CountReducer>(),
            )?,
            backing,
        })
    }
}
impl<S: CanonicalConstructionState + ?Sized> Reduction<S> for SuppliedReduction<'_, '_> {
    type Seeds = ZeroSeal;
    type Final<'a> = CountFinalRows<'a>;
    fn initial_counts(
        &mut self,
        _input: &dyn PreparedBindingRows,
    ) -> ContentResult<Option<Vec<u64>>> {
        Ok(None)
    }
    fn check_capacity(&self) -> ContentResult<()> {
        Ok(())
    }
    fn working(&mut self, state: &mut S, bytes: usize) -> ContentResult<Option<GraphMemoryLease>> {
        state
            .count_memory(&self.counts.scope)?
            .reserve(bytes)
            .map(Some)
    }
    fn base_values(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        serials: &[u64],
    ) -> ContentResult<BaseAnswers> {
        self.counts.base_values(state, reader, table, serials)
    }
    fn declare_new(&mut self, state: &mut S, serial: u64) -> ContentResult<()> {
        self.counts.declare_new(state, serial)
    }
    fn retained(&mut self, state: &mut S, serial: u64) -> ContentResult<()> {
        self.counts.retained(state, serial)
    }
    fn removed(&mut self, state: &mut S, serial: u64) -> ContentResult<()> {
        self.counts.removed(state, serial)
    }
    fn value(&mut self, state: &mut S, serial: u64, value: InodeValue) -> ContentResult<()> {
        self.counts.value(state, serial, value)
    }
    fn zeros(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
        unreachable: &EligibilityAuthority,
        maximum: usize,
        parents: ParentCalls<S>,
    ) -> ContentResult<(ZeroSeal, u64)> {
        self.counts.collect_zero(
            state,
            reader,
            table,
            batch,
            root,
            unreachable,
            parents,
            maximum,
        )
    }
    fn release_descendants(
        &mut self,
        state: &mut S,
        reader: &dyn AuthenticatedObjects,
        table: InodeTable,
        seeds: &ZeroSeal,
        batch: usize,
    ) -> ContentResult<ReleaseWork> {
        crate::filesystem::references::release_state::release_with_state(
            reader,
            table,
            &mut self.counts,
            state,
            seeds,
            batch,
            64,
            crate::filesystem::limits::MAXIMUM_PAGE_BYTES,
        )
    }
    fn finish<'a>(
        &mut self,
        state: &mut S,
        reader: &'a dyn AuthenticatedObjects,
        table: InodeTable,
        batch: usize,
        root: u64,
    ) -> ContentResult<CountFinalRows<'a>> {
        self.counts.finish(state, reader, table, batch, root)
    }
    fn release(&mut self, state: &mut S) -> ContentResult<()> {
        let counts = self.counts.retire(state);
        let backing = match self.backing.as_deref_mut() {
            Some(backing) => backing.release(),
            None => Ok(()),
        };
        counts?;
        backing
    }
}

pub(super) fn register_reduced_values<S: ?Sized, R: Reduction<S>>(
    reducer: &mut R,
    input: &dyn PreparedBindingRows,
    unreachable: &EligibilityAuthority,
    state: &mut S,
    parents: ParentCalls<S>,
) -> ContentResult<()> {
    let mut serials = input.new_inodes()?;
    while let Some(serial) = serials.next_row()? {
        if unreachable.contains(state, serial, parents)? {
            continue;
        }
        reducer.declare_new(state, serial)?;
    }
    drop(serials);
    let mut values = input.inodes()?;
    while let Some(update) = values.next_row()? {
        if unreachable.contains(state, update.serial, parents)? {
            continue;
        }
        reducer.value(state, update.serial, update.value)?;
    }
    Ok(())
}
pub(super) fn note_reduced_binding<S: ?Sized, R: Reduction<S>>(
    reducer: &mut R,
    initial_counts: Option<&mut [u64]>,
    input: &dyn PreparedBindingRows,
    serial: u64,
    state: &mut S,
) -> ContentResult<()> {
    if let Some(counts) = initial_counts {
        let index = input
            .new_position(serial)?
            .ok_or(ContentError::InvalidRecord("effect inode record"))?;
        counts[index] = counts[index]
            .checked_add(1)
            .ok_or(ContentError::LengthOverflow)?;
        Ok(())
    } else {
        reducer.retained(state, serial)
    }
}

/// Actual supplied reduction coordinator/control and its one shared canonical budget.
pub const fn canonical_reduction_working_bytes() -> usize {
    crate::filesystem::references::canonical_count_control_working_bytes()
        + std::mem::size_of::<SuppliedReduction<'static, 'static>>()
        - std::mem::size_of::<CountReducer>()
}
