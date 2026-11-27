//! The daemon channel itself: one exchange (`exchange`), the fence
//! (`fence`), the internal calls (`internal`), the operator calls
//! (`operator`) and the connection pool (`pool`).
#[doc(hidden)]
pub mod exchange;
#[doc(hidden)]
pub mod fence;
#[doc(hidden)]
pub mod internal;
#[doc(hidden)]
pub mod operator;
#[doc(hidden)]
pub mod pool;
