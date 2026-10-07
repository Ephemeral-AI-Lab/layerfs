//! One typed completion owner per original job, from admission to release.
use crate::{
    commands::{Command, Response},
    credits::Credit,
    owner::OwnerError,
    queue::Job,
    service::job_sql::{FAMILIES, LIFECYCLE_FAMILIES},
    JobWork,
};
use layerfs_overlay::StatementWork;
use std::{
    fmt,
    mem::size_of,
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc, OnceLock,
    },
    thread::{self, Thread},
};

/// Allocator rounding and other unexplained bookkeeping retained per job.
const BOOKKEEPING: usize = 512;
const EMPTY: u8 = 0;
const WAITING: u8 = 1;
const DONE: u8 = 2;
const TAKEN: u8 = 3;
const LOST: u8 = 4;

/// The original result and receipt, allocated once when the job finishes.
pub(crate) struct Outcome {
    pub result: Result<Response, OwnerError>,
    pub work: JobWork,
}
/// Shared by the caller's Pending/Completion and the owner's Publisher. It
/// holds the job's credit for as long as any of them can reach the result.
pub(crate) struct Cell {
    state: AtomicU8,
    outcome: OnceLock<Box<Outcome>>,
    waiter: OnceLock<Thread>,
    credit: Credit,
}
/// The owner's single right to finish one job.
pub(crate) struct Publisher(Option<Arc<Cell>>);

/// Largest simultaneously owned storage of one job plus its fixed allowance.
/// The queued job, the held outcome and an unattempted outcome returning the
/// original command never coexist; a job is charged for the largest of them.
/// The reply bytes cover the inline result value, so a smaller declared reply
/// is raised to that size. Lifecycle jobs never park, so they reserve only
/// their bounded held receipt rows. Every other class reserves all families
/// in whichever stage owns the rows, including a parked or unattempted job.
pub(crate) fn charge(lifecycle: bool, input: usize, reply: usize) -> Option<usize> {
    let row = size_of::<StatementWork>();
    let result = size_of::<Result<Response, OwnerError>>();
    let receipt = size_of::<Outcome>() - result;
    let (rows, readiness) = if lifecycle {
        (LIFECYCLE_FAMILIES * row, 0)
    } else {
        (FAMILIES * row, FAMILIES * row)
    };
    let queued = size_of::<Job>()
        .checked_add(input)?
        .checked_add(readiness)?;
    let held = receipt + rows;
    let unattempted = (receipt + readiness + size_of::<Command>() + size_of::<OwnerError>())
        .checked_add(input)?;
    (2 * size_of::<usize>() + size_of::<Cell>() + BOOKKEEPING)
        .checked_add(reply.max(result))?
        .checked_add(queued.max(held).max(unattempted))
}

pub(crate) fn admit(credit: Credit) -> (Publisher, Pending) {
    let cell = Arc::new(Cell {
        state: AtomicU8::new(EMPTY),
        outcome: OnceLock::new(),
        waiter: OnceLock::new(),
        credit,
    });
    (Publisher(Some(cell.clone())), Pending { cell })
}
impl Cell {
    fn settle(&self, state: u8) {
        if self.state.swap(state, Ordering::SeqCst) == WAITING {
            if let Some(waiter) = self.waiter.get() {
                waiter.unpark();
            }
        }
    }
}
impl Publisher {
    /// Makes the original outcome visible exactly once. Owner aggregates for
    /// this job must already be published. Receipt rows beyond the admitted
    /// allowance are charged first, so the ledger covers what is retained.
    pub fn publish(mut self, outcome: Box<Outcome>) {
        let Some(cell) = self.0.take() else {
            return;
        };
        cell.credit.cover_receipt(outcome.work.sql.heap_bytes());
        // This publisher is the cell's only writer.
        let _ = cell.outcome.set(outcome);
        cell.settle(DONE);
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        // A job lost without an outcome disconnects its waiter.
        if let Some(cell) = self.0.take() {
            cell.settle(LOST);
        }
    }
}

/// A pending original operation. Waiting never occupies a native dispatch worker
/// unless its caller chooses to wait synchronously there.
pub struct Pending {
    cell: Arc<Cell>,
}
/// Result and its retained aggregate credit. Data is borrowed until this drops.
pub struct Completion {
    cell: Arc<Cell>,
}
impl Pending {
    pub fn wait(self) -> Result<Completion, OwnerError> {
        // Registered before the state transition the publisher observes.
        let _ = self.cell.waiter.set(thread::current());
        if self
            .cell
            .state
            .compare_exchange(EMPTY, WAITING, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            while self.cell.state.load(Ordering::SeqCst) == WAITING {
                thread::park();
            }
        }
        self.take()?.ok_or(OwnerError::Disconnected)
    }
    pub fn try_complete(&self) -> Result<Option<Completion>, OwnerError> {
        self.take()
    }
    /// The published outcome is handed out once; later calls are disconnected.
    fn take(&self) -> Result<Option<Completion>, OwnerError> {
        match self
            .cell
            .state
            .compare_exchange(DONE, TAKEN, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => Ok(Some(Completion {
                cell: self.cell.clone(),
            })),
            Err(EMPTY | WAITING) => Ok(None),
            Err(_) => Err(OwnerError::Disconnected),
        }
    }
}
impl Completion {
    fn outcome(&self) -> &Outcome {
        self.cell
            .outcome
            .get()
            .expect("completion follows its published outcome")
    }
    /// Complete exclusive SQL/allocation work, including readiness turns which
    /// parked this original job. Queue/service spans are diagnostic wall.
    pub fn work(&self) -> &JobWork {
        &self.outcome().work
    }
    pub fn result(&self) -> &Result<Response, OwnerError> {
        &self.outcome().result
    }
}
impl fmt::Debug for Completion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.result().fmt(f)
    }
}
impl fmt::Display for Completion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "owner completion: {:?}", self.result())
    }
}
impl std::error::Error for Completion {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.result()
            .as_ref()
            .err()
            .map(|error| error as &dyn std::error::Error)
    }
}
