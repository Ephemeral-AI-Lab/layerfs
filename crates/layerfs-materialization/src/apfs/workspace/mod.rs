use crate::driver::*;
use std::any::Any;
#[cfg(test)]
use std::cell::Cell;
use std::fs::{self, File, FileTimes};
use std::io::{Read, Seek, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod driver;
mod handles;
mod mutation_support;
mod projection;
mod recovery;
mod setup_cleanup;
mod state;
mod telemetry;
mod workspace_drop;

pub(super) use mutation_support::modified_time;
pub use state::AppleDriver;

use super::{ffi, metadata};
use mutation_support::*;
use recovery::{encode_recovery_record, recover_owned_workspaces};
use state::*;
use telemetry::*;
