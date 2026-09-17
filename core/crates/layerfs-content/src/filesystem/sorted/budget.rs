//! Reserve-before-growth accounting for one sorted tree operation.
//!
//! Every page, decoded wire, buffer and canonical output this operation owns is
//! charged to one budget *before* it is allocated. A reservation that does not
//! fit fails the operation; it never silently degrades to a slower route or a
//! larger allocation. The budget is the operation's own scratch ceiling and is
//! deliberately separate from the caller's input, ordering and output costs.

use std::cell::Cell;
use std::rc::Rc;

use crate::error::{ContentError, ContentResult};

/// Shared reservation state of one operation.
pub struct Budget {
    used: Cell<usize>,
    peak: Cell<usize>,
    limit: usize,
}

impl Budget {
    /// A budget of `limit` bytes.
    pub fn new(limit: usize) -> Rc<Self> {
        Rc::new(Self {
            used: Cell::new(0),
            peak: Cell::new(0),
            limit,
        })
    }

    /// Reserves `bytes`, failing before any allocation happens.
    pub fn reserve(self: &Rc<Self>, bytes: usize) -> ContentResult<Lease> {
        let next = self
            .used
            .get()
            .checked_add(bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if next > self.limit {
            return Err(ContentError::ObjectLimitExceeded {
                limit: self.limit,
                actual: next,
            });
        }
        self.used.set(next);
        self.peak.set(self.peak.get().max(next));
        Ok(Lease {
            budget: self.clone(),
            bytes,
        })
    }

    /// Largest simultaneous reservation observed.
    pub fn peak(&self) -> usize {
        self.peak.get()
    }

    /// Bytes currently reserved.
    pub fn used(&self) -> usize {
        self.used.get()
    }
}

/// One live reservation; the bytes return to the budget when it drops.
pub struct Lease {
    budget: Rc<Budget>,
    bytes: usize,
}

impl Lease {
    /// Adds `bytes` to this reservation.
    pub fn grow(&mut self, bytes: usize) -> ContentResult<()> {
        let next = self
            .budget
            .used
            .get()
            .checked_add(bytes)
            .ok_or(ContentError::LengthOverflow)?;
        if next > self.budget.limit {
            return Err(ContentError::ObjectLimitExceeded {
                limit: self.budget.limit,
                actual: next,
            });
        }
        self.budget.used.set(next);
        self.budget.peak.set(self.budget.peak.get().max(next));
        self.bytes += bytes;
        Ok(())
    }

    /// Returns `bytes` from this reservation.
    pub fn shrink(&mut self, bytes: usize) {
        let bytes = bytes.min(self.bytes);
        self.bytes -= bytes;
        self.budget
            .used
            .set(self.budget.used.get().saturating_sub(bytes));
    }

    /// Bytes currently reserved by this lease.
    pub const fn bytes(&self) -> usize {
        self.bytes
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.budget
            .used
            .set(self.budget.used.get().saturating_sub(self.bytes));
    }
}
