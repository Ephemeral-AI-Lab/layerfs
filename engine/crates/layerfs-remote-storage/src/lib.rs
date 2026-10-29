//! Whole-file remote storage: a remote holds files by name, and a driver
//! uploads, downloads (whole or a byte range), describes and deletes them.
//!
//! This crate knows nothing about Stores. Every request is attempted once: a
//! request that was sent and whose outcome is unknown is reported
//! [`RemoteError::Uncertain`] and never resent. A file larger than a
//! provider holds as one object goes up as fixed-size pieces plus a sha256
//! manifest ([`pieces`]), reassembled and verified on download. Each
//! provider is behind its own Cargo feature; R2 needs `tls` to connect.
#![forbid(unsafe_code)]

pub mod credentials;
pub mod driver;
pub mod error;
pub mod limits;
pub mod pieces;

#[cfg(feature = "s3_api")]
pub mod cloudflare_r2;
#[cfg(feature = "local_folder")]
pub mod local_folder;
#[cfg(feature = "minio")]
pub mod minio;
#[cfg(feature = "s3_api")]
pub mod s3_api;

pub use credentials::{CredentialRef, Credentials};
pub use driver::{Driver, Metadata, Span, Stat};
pub use error::RemoteError;
pub use limits::Limits;
