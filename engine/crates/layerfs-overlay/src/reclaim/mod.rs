//! Responsibility-scoped implementation modules and reexports.
pub(crate) mod garbage;
pub(crate) mod orphan;
pub(crate) mod pieces;
pub(crate) mod ready;
pub(crate) mod turn;
pub(crate) use ready::{
    Item, Page, FOLD, OPERATION_RECORD, ORPHAN, PAGE, RETIRE, SERIAL_RETIRE, STALE, STALE_PIECES,
    STEPS,
};
pub use ready::{MaintenanceCursor, MaintenanceStep};

pub(crate) mod operation_record;
mod source_wait;
