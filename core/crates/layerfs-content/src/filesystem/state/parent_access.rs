//! Prospectively selected parent lookup/retirement for the shared canonical body.
use crate::filesystem::state::{ParentEligibilityState, ParentSeal};
use crate::{ContentError, ContentResult};
use std::collections::BTreeMap;

// Keep the complete supplied seal inline in the charged coordinator layout.
// Boxing would introduce a separate allocation and owner not admitted here.
#[allow(clippy::large_enum_variant)]
pub(crate) enum EligibilityAuthority {
    Legacy(BTreeMap<u64, ()>),
    Supplied(ParentSeal),
}
pub(crate) struct ParentCalls<S: ?Sized> {
    get: fn(&mut S, &ParentSeal, u64) -> ContentResult<bool>,
    retire: fn(&mut S, &ParentSeal) -> ContentResult<()>,
}
impl<S: ?Sized> Copy for ParentCalls<S> {}
impl<S: ?Sized> Clone for ParentCalls<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: ?Sized> ParentCalls<S> {
    pub(crate) fn compatibility() -> Self {
        Self {
            get: |_, _, _| {
                Err(ContentError::InvalidOrderingRecord(
                    "unexpected supplied parent authority",
                ))
            },
            retire: |_, _| {
                Err(ContentError::InvalidOrderingRecord(
                    "unexpected supplied parent retirement",
                ))
            },
        }
    }
}
impl<S: ParentEligibilityState + ?Sized> ParentCalls<S> {
    pub(crate) fn supplied() -> Self {
        Self {
            get: |state, seal, serial| {
                Ok(state.parent_get(seal, serial)?.is_some_and(|p| !p.bound))
            },
            retire: |state, seal| state.parent_retire(seal),
        }
    }
}
impl EligibilityAuthority {
    pub(crate) fn contains<S: ?Sized>(
        &self,
        state: &mut S,
        serial: u64,
        calls: ParentCalls<S>,
    ) -> ContentResult<bool> {
        match self {
            Self::Legacy(map) => Ok(map.contains_key(&serial)),
            Self::Supplied(seal) => (calls.get)(state, seal, serial),
        }
    }
    pub(crate) fn len(&self) -> ContentResult<usize> {
        match self {
            Self::Legacy(map) => Ok(map.len()),
            Self::Supplied(seal) => {
                usize::try_from(seal.excluded()?).map_err(|_| ContentError::LengthOverflow)
            }
        }
    }
    pub(crate) fn retire<S: ?Sized>(
        &self,
        state: &mut S,
        calls: ParentCalls<S>,
    ) -> ContentResult<()> {
        match self {
            Self::Legacy(_) => Ok(()),
            Self::Supplied(seal) => (calls.retire)(state, seal),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum EligibilityView<'a> {
    Legacy(&'a BTreeMap<u64, ()>),
    Supplied(&'a ParentSeal),
}
impl EligibilityView<'_> {
    pub(crate) fn contains<S: ?Sized>(
        self,
        state: &mut S,
        serial: u64,
        calls: ParentCalls<S>,
    ) -> ContentResult<bool> {
        match self {
            Self::Legacy(map) => Ok(map.contains_key(&serial)),
            Self::Supplied(seal) => (calls.get)(state, seal, serial),
        }
    }
}
