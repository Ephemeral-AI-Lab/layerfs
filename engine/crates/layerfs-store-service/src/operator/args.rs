//! Argument readers for calls that arrive as name-keyed value maps: the
//! operator tables here and in the daemon, and the public method files of
//! layerfs-api, which re-export them. The Store engine takes typed values.
use crate::history::lookup::{self, invalid};
use crate::read::snapshot::Snap;
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::LayerStackRecord;
use std::sync::Arc;

pub use crate::history::lookup::next_page;

/// A required text argument.
pub fn text<'a>(args: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    args.get_text(name)
        .ok_or_else(|| invalid(format!("missing text argument `{name}`")))
}

/// The project's stack, by its `project` argument.
pub fn project(store: &Arc<Store>, args: &SdkValue) -> Result<LayerStackRecord, SdkRefusal> {
    lookup::project(store, text(args, "project")?)
}

/// Safe numeric page arguments. Offset skipping is linear and uses bounded windows.
pub fn paging(value: &SdkValue) -> Result<(usize, u64), SdkRefusal> {
    let integer = |name: &str, default: i64| -> Result<i64, SdkRefusal> {
        match value.get(name) {
            None => Ok(default),
            Some(SdkValue::Int(n)) if *n >= 0 && *n <= 9_007_199_254_740_991 => Ok(*n),
            _ => Err(invalid(format!(
                "{name} must be a nonnegative safe integer"
            ))),
        }
    };
    let size = integer("pageSize", 50)?;
    if !(1..=1000).contains(&size) {
        return Err(invalid("pageSize must be in 1..=1000"));
    }
    Ok((size as usize, integer("pagePos", 0)? as u64))
}

/// The snapshot a read names: `commit` when given, else `branch`.
pub fn snapshot_name(args: &SdkValue) -> Result<&str, SdkRefusal> {
    match args.get_text("commit") {
        Some(commit) => Ok(commit),
        None => text(args, "branch"),
    }
}

/// The project, its reader and the snapshot a read names, in that order.
pub fn snapshot(
    store: &Arc<Store>,
    args: &SdkValue,
) -> Result<(LayerStackRecord, Arc<StorePorts>, Snap), SdkRefusal> {
    let stack = project(store, args)?;
    let (reader, snap) = crate::read::snapshot::open(store, &stack, snapshot_name(args)?)?;
    Ok((stack, reader, snap))
}
