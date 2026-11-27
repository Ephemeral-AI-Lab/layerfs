//! Captured construction adapters; declarations and reexports only.
mod driver;
pub(crate) mod file;
mod namespace;
mod outcome;
mod records;
pub use driver::CapturedNamespace;
pub use file::{CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork};
pub use outcome::{CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork};
pub use records::{EditBackingCustody, EditBackingWork, EditInputRefusal, IndexedEditRecords};
