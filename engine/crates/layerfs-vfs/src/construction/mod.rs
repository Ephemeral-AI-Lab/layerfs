//! Captured construction adapters; declarations and reexports only.
pub(crate) mod file;
mod driver;
mod namespace;
mod outcome;
mod records;
pub use file::{CapturedFileAttempt, CapturedFileCustody, CapturedFileEdits, CapturedFileWork};
pub use driver::CapturedNamespace;
pub use outcome::{CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork};
pub use records::{EditBackingCustody, EditBackingWork, EditInputRefusal, IndexedEditRecords};
