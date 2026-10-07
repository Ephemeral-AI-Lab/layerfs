//! Captured construction adapters; declarations and reexports only.
mod captured;
mod records;
pub use captured::{CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork};
pub use records::{EditBackingCustody, EditBackingWork, EditInputRefusal, IndexedEditRecords};
