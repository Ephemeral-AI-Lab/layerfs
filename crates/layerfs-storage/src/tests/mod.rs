//! Legacy engine unit tests split by responsibility.

use super::*;
use layerfs_core::inode::InodeId;
use layerfs_core::namespace::NamespaceRootV1;
use layerfs_core::namespace_codec::{encode_namespace_root, profile_id};
use layerfs_core::{CoreError, ObjectId};
use rusqlite::{params, Connection};
use std::cell::Cell;
use std::fs;
use std::process::Command;

mod admission;
mod compaction;
mod fixture;
mod full_branch;
mod full_layer_stack;
mod full_storage;
mod full_transfer;
mod generation_selector;
mod generation_switch;
mod migration;
mod object;
mod observation;
mod profile;
mod schema_contract;
mod scrub;

use fixture::*;
