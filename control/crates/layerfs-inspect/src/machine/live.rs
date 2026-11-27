//! What the inspect kinds share (layerfs-project#164): the lists of names a
//! request holds, and [`checked`], whether each of a machine's Sandboxes
//! has a daemon process that still runs, for `machine_stats`. The side that
//! runs them (the home on `native`, the machine's worker otherwise) reads
//! each one's daemon record under its own root and checks the process it
//! names by id, start time and boot ([`super::process::live`]). No daemon
//! is asked.
use layerfs_home::disk::paths;
use layerfs_home::names::SandboxName;
use layerfs_home::reply::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::record::{DaemonRecord, SandboxRecord};
use layerfs_sandbox::route::inspect::Budget;
use layerfs_sandbox::status::stats as process;
use std::path::Path;

/// The most names one request holds in one list.
pub(super) const MOST: usize = 4096;
/// Why a value is null: the request's soft budget ended before its turn.
pub(super) const DEADLINE: &str = "deadline";
/// Why `live` is null: the Sandbox's record on its machine is not read.
const UNREADABLE: &str = "record_unreadable";

/// The texts of list `field` of `request`, at most [`MOST`]; a request
/// without the list names none.
pub(super) fn listed(request: &SdkValue, field: &str) -> Result<Vec<String>, SdkRefusal> {
    let names: Option<Vec<String>> = match request.get(field) {
        None | Some(SdkValue::Null) => Some(Vec::new()),
        Some(SdkValue::List(items)) if items.len() <= MOST => (items.iter())
            .map(|item| match item {
                SdkValue::Text(name) => Some(name.clone()),
                _ => None,
            })
            .collect(),
        _ => None,
    };
    names.ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("an inspect request's `{field}` is a list of at most {MOST} names"),
        )
    })
}

/// The Sandbox names `request` holds, each checked as a name.
pub(super) fn names(request: &SdkValue) -> Result<Vec<String>, SdkRefusal> {
    let names = listed(request, "sandboxes")?;
    for name in &names {
        SandboxName::parse(name)?;
    }
    Ok(names)
}

/// One Sandbox's check: whether its daemon runs, else why that is not known.
pub(super) type Checked = (Option<bool>, Option<&'static str>);

/// Whether the daemon of each of Sandboxes `names` under `root` runs, and
/// whether every one was checked: once `budget` is spent the rest are
/// `None` with [`DEADLINE`]. A Sandbox with no record or no daemon record
/// under `root` has no process: `Some(false)`. Beside each check, what
/// `read` makes of its daemon record when it has one.
pub(super) fn checked<T>(
    root: &Path,
    names: &[String],
    budget: &Budget,
    mut read: impl FnMut(&DaemonRecord) -> T,
) -> (Vec<(Checked, Option<T>)>, bool) {
    let mut complete = true;
    let one = |name: &String| {
        if budget.spent() {
            complete = false;
            return ((None, Some(DEADLINE)), None);
        }
        let record = SandboxRecord::read_in(&paths::sandbox_dir(root, name), name);
        match record.as_ref().map(|r| r.as_ref().and_then(|r| r.daemon())) {
            Err(_) => ((None, Some(UNREADABLE)), None),
            Ok(None) => ((Some(false), None), None),
            Ok(Some(daemon)) => {
                let live = match process::live(daemon) {
                    Ok(live) => (Some(live), None),
                    Err(reason) => (None, Some(reason)),
                };
                (live, Some(read(daemon)))
            }
        }
    };
    let checked = names.iter().map(one).collect();
    (checked, complete)
}
