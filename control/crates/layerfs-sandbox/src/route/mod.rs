//! How a Sandbox call reaches where it runs: the daemon channel
//! (`daemon`), Workspace calls straight to a forwarded daemon (`direct`),
//! the inspect route (`inspect`) and a host operation here or in the
//! machine's worker (`machine`).
pub mod daemon;
#[doc(hidden)]
pub mod direct;
pub mod inspect;
#[doc(hidden)]
pub mod machine;
