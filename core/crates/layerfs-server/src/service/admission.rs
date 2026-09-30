//! Typed admission for content Saves, catalog mutations and ordinary reads.
//!
//! This is logical operation admission. Native channel protection, byte leases
//! and SQLite engine headroom have separate owners and qualification gates.
use super::{error::storage, handler::StoreAccess};
use layerfs_bridge::contract::{Code, Failure, Operation, MAX_READ_OPERATIONS};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/// One Store's persisted writer setting and process-local live request count.
struct SaveBudget {
    live: AtomicUsize,
    limit: usize,
}

/// One continuing catalog handle's live metadata mutation count.
struct CatalogBudget(AtomicUsize);

/// Admission state assembled once for the service's bounded Store list.
pub(crate) struct Admission {
    saves: Vec<SaveBudget>,
    catalogs: Vec<Option<Arc<CatalogBudget>>>,
    readers: AtomicUsize,
}

/// A counter owner returns exactly its one logical credit when it ends.
struct Permit<'a>(&'a AtomicUsize);
impl Drop for Permit<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Release);
    }
}

/// A request that may construct canonical objects through C2.
pub(crate) struct C2SavePermit<'a> {
    _permit: Permit<'a>,
}

/// A metadata-only request on one exact C5 authority.
pub(crate) struct C5CatalogPermit<'a> {
    _permit: Permit<'a>,
}

/// A read request's existing process-wide decode/connection allowance.
pub(crate) struct ReadPermit<'a> {
    _permit: Permit<'a>,
}

/// Exactly one of the exhaustive contract predicates selects this owner.
pub(crate) enum OperationPermit<'a> {
    Content { _permit: C2SavePermit<'a> },
    Catalog { _permit: C5CatalogPermit<'a> },
    Read { _permit: ReadPermit<'a> },
}

fn admit(counter: &AtomicUsize, limit: usize) -> Result<Permit<'_>, Failure> {
    counter
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
            (live < limit).then_some(live + 1)
        })
        .map_err(|_| Code::Capacity)?;
    Ok(Permit(counter))
}

impl Admission {
    pub(crate) fn new(stores: &[StoreAccess]) -> Result<Self, Failure> {
        let mut saves = Vec::with_capacity(stores.len());
        let mut catalogs: Vec<Option<Arc<CatalogBudget>>> = Vec::with_capacity(stores.len());
        for (index, access) in stores.iter().enumerate() {
            saves.push(SaveBudget {
                live: AtomicUsize::new(0),
                limit: usize::from(access.store.max_concurrent_writes().map_err(storage)?),
            });
            let budget = access.history.as_ref().map(|catalog| {
                // StoreAccess aliases of the same live trait object share one
                // allowance. Equal catalog IDs do not prove the same provider:
                // a separately opened read-only handle owns a different cell.
                let shared = stores[..index].iter().position(|prior| {
                    prior
                        .history
                        .as_ref()
                        .is_some_and(|value| Arc::ptr_eq(value, catalog))
                });
                match shared {
                    Some(prior) => Arc::clone(catalogs[prior].as_ref().unwrap()),
                    None => Arc::new(CatalogBudget(AtomicUsize::new(0))),
                }
            });
            catalogs.push(budget);
        }
        Ok(Self {
            saves,
            catalogs,
            readers: AtomicUsize::new(0),
        })
    }

    /// Refuses before body consumption or dependent effects, without waiting.
    pub(crate) fn enter(
        &self,
        index: usize,
        operation: &Operation,
    ) -> Result<OperationPermit<'_>, Failure> {
        if operation.content_mutation() {
            let budget = &self.saves[index];
            Ok(OperationPermit::Content {
                _permit: C2SavePermit {
                    _permit: admit(&budget.live, budget.limit)?,
                },
            })
        } else if operation.metadata_mutation() {
            let budget = self.catalogs[index].as_ref().ok_or(Code::Unsupported)?;
            Ok(OperationPermit::Catalog {
                _permit: C5CatalogPermit {
                    _permit: admit(&budget.0, 1)?,
                },
            })
        } else if operation.read_only() {
            Ok(OperationPermit::Read {
                _permit: ReadPermit {
                    _permit: admit(&self.readers, MAX_READ_OPERATIONS)?,
                },
            })
        } else {
            Err(Code::Unsupported.into())
        }
    }
}
