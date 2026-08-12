//! Concrete FsCas occupied-object adapter for canonical random-read decoding.
//!
//! This is the only read-layer owner that translates authenticated occupied
//! ranges into the neutral object decoder's exact-read port. FsCas variants
//! are retained on the occupied owner so the outer operation can promote the
//! first typed storage failure instead of flattening it.

use crate::cas::{FsCasControlV1, FsCasErrorV1, FsCasOccupiedV1};
use crate::limits::{CounterFieldV1, OperationCountersV1};
use crate::object::{PhysicalObjectReadPortV1, TypedPhysicalObjectIdV1};
use crate::{CoreError, CoreResult};

pub(super) struct OccupiedObjectReaderV1<'a, C: FsCasControlV1 + ?Sized> {
    occupied: &'a mut FsCasOccupiedV1,
    counters: &'a mut OperationCountersV1,
    control: &'a mut C,
    id: TypedPhysicalObjectIdV1,
    len: u64,
}

impl<'a, C: FsCasControlV1 + ?Sized> OccupiedObjectReaderV1<'a, C> {
    pub(super) const fn new(
        occupied: &'a mut FsCasOccupiedV1,
        counters: &'a mut OperationCountersV1,
        control: &'a mut C,
        id: TypedPhysicalObjectIdV1,
        len: u64,
    ) -> Self {
        Self {
            occupied,
            counters,
            control,
            id,
            len,
        }
    }

    pub(super) fn resolved_new(
        occupied: &'a mut FsCasOccupiedV1,
        counters: &'a mut OperationCountersV1,
        control: &'a mut C,
        id: TypedPhysicalObjectIdV1,
        len: u64,
    ) -> CoreResult<Self> {
        let _ = required_occupied_len_v1(occupied, control, id)?;
        Ok(Self::new(occupied, counters, control, id, len))
    }
}

impl<C: FsCasControlV1 + ?Sized> PhysicalObjectReadPortV1 for OccupiedObjectReaderV1<'_, C> {
    fn len(&mut self) -> CoreResult<u64> {
        Ok(self.len)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()> {
        let end = offset
            .checked_add(destination.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if end > self.len {
            return Err(CoreError::Truncated);
        }
        self.occupied
            .read_occupied_exact_at_typed_controlled_v1(self.id, offset, destination, self.control)
            .map_err(|error| {
                self.occupied.retain_first_error_typed_v1(error);
                CoreError::SourceFailure
            })?;
        self.counters
            .add(CounterFieldV1::BytesRead, destination.len() as u64)
    }
}

pub(super) fn required_occupied_len_v1<C>(
    occupied: &mut FsCasOccupiedV1,
    control: &mut C,
    id: TypedPhysicalObjectIdV1,
) -> CoreResult<u64>
where
    C: FsCasControlV1 + ?Sized,
{
    match occupied.occupied_len_typed_controlled_v1(id, control) {
        Ok(Some(len)) => Ok(len),
        Ok(None) => {
            occupied.retain_first_error_typed_v1(FsCasErrorV1::MissingOccupant);
            Err(CoreError::SourceFailure)
        }
        Err(error) => {
            occupied.retain_first_error_typed_v1(error);
            Err(CoreError::SourceFailure)
        }
    }
}
