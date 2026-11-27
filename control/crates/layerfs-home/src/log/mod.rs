//! The home's log files: the call log (`calls`), one-line JSON logs
//! (`lines`), rotation at a size bound (`rotate`), reading a log's tail
//! (`tail`) and the call trace (`trace`).
pub mod calls;
pub mod lines;
pub mod rotate;
pub mod tail;
#[doc(hidden)]
pub mod trace;
