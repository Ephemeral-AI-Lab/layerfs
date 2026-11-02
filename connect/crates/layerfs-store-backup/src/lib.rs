//! Store backup: bucket drivers that know nothing about Stores, under
//! `bucket/`, and whole-Store backup to a bucket (push, restore, link) under
//! `backup/`. Each request is attempted once.
#![forbid(unsafe_code)]

pub mod backup;
pub mod bucket;
