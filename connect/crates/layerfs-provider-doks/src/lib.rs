//! The DigitalOcean Kubernetes (DOKS) provider adapter (#554, D6): a
//! machine provider that produces the built-in ssh machine's [`Spec`] and
//! its typed capability, with no core branch.
//!
//! The adapter lives outside the machine layer: it declares `provider:
//! "doks"` as data, keeps `kind` an [`layerfs_sandbox::IsolationClass`]
//! (`remote`) and lets the one existing [`layerfs_machine::Platform::Ssh`]
//! constructor do the work.  No `Platform`, `Backend` or `IsolationClass`
//! variant is added; `Platform::parse("doks")` is `None`.
//!
//! [`Spec`]: layerfs_machine::Spec
#![cfg(unix)]
#![forbid(unsafe_code)]
mod capability;
mod env;
mod error;
mod record;
mod route;

pub use capability::{capability, capability_for, declaration, Declaration};
pub use env::{pod_ip, preflight, DoksEnv, Preflight};
pub use error::Error;
pub use record::{record_fields, RecordFields, PROVIDER};
pub use route::{connect, spec, spec_from_env};
