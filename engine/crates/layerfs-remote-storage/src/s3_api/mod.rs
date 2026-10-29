//! The S3 API, shared by S3-compatible providers: header-signed SigV4 over
//! hand-written HTTP/1.1, one TCP connection and one attempt per request,
//! single PUT or multipart upload, ranged GET, HEAD and DELETE.
//!
//! Revived from the retired `layerfs-s3` client (`da0fa3a120^`) and extended
//! with streaming bodies and multipart. Plain `http://` only in this phase:
//! TLS, which R2 and AWS need, comes later.
mod client;
mod config;
mod http;
mod multipart;
mod sign;

pub use client::S3Client;
pub use config::S3Config;
pub use multipart::{part_size, MAX_PART, MAX_PARTS, MIN_PART};
