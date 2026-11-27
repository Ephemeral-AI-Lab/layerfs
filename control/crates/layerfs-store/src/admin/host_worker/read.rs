//! The native CLI worker's bounded, read-only Store protocol.
use layerfs_home::names::{ProjectName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;
use std::cell::Cell;
use std::path::Path;

use crate::admin::route::resolve::{self, Target};

/// One native worker call asks for one Store method.
pub const OP: &str = "store.call";

thread_local! {
    static CLI: Cell<bool> = const { Cell::new(false) };
}

/// Whether this thread runs a checked operation of the CLI executable.
pub(crate) fn cli() -> bool {
    CLI.get()
}

/// Enables planning for one checked operation of the CLI executable.
pub fn with_cli<T>(enabled: bool, call: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            CLI.set(self.0);
        }
    }
    let _restore = Restore(CLI.replace(enabled));
    call()
}

/// One allowlisted read method, shared with the PostgreSQL route
/// ([`crate::host_postgresql`]).
#[derive(Clone, Copy)]
pub(crate) enum Method {
    Read,
    Stat,
    List,
}

impl Method {
    pub(crate) fn of(name: &str) -> Option<Self> {
        match name {
            "fs.read" => Some(Self::Read),
            "fs.stat" => Some(Self::Stat),
            "fs.list" => Some(Self::List),
            _ => None,
        }
    }

    // Check only the bounded transport contract. The engine still validates
    // names, paths, snapshot pins and operands exactly as for direct calls.
    pub(crate) fn accepts(self, args: &SdkValue) -> bool {
        let SdkValue::Map(fields) = args else {
            return false;
        };
        if !fields.iter().all(|(name, value)| match name.as_str() {
            "branch" | "path" | "commit" => matches!(value, SdkValue::Text(_)),
            "binary" => matches!(self, Self::Read) && value == &SdkValue::Bool(true),
            "startLine" | "endLine" => {
                matches!(self, Self::Read) && matches!(value, SdkValue::Int(_))
            }
            "pageSize" | "pagePos" => {
                matches!(self, Self::List) && matches!(value, SdkValue::Int(_))
            }
            _ => false,
        }) || args.get_text("path").is_none()
            || args.get_text("branch").is_none()
        {
            return false;
        }
        match self {
            Self::Read => match (args.get_int("startLine"), args.get_int("endLine")) {
                (Some(start), Some(end)) => {
                    start >= 1
                        && start <= end
                        && end.saturating_sub(start) < 2000
                        && args.get_bool("binary")
                }
                _ => false,
            },
            Self::Stat => true,
            Self::List => {
                args.get_int("pageSize")
                    .is_some_and(|size| (1..=layerfs_home::cursor::MAX_LIMIT).contains(&size))
                    && args.get_int("pagePos").is_none_or(|pos| pos >= 0)
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn valid_answer(self, value: &SdkValue) -> bool {
        let Some(at) = value.get("at") else {
            return false;
        };
        let snapshot = value.get_text("kind") == Some("ok")
            && value.get_text("snapshot").is_some()
            && value.get_text("root").is_some()
            && value.get_text("path").is_some()
            && at.get_text("id").is_some()
            && matches!(at.get_text("kind"), Some("Commit" | "Layer"))
            && matches!(at.get("branch"), Some(SdkValue::Null | SdkValue::Text(_)))
            && at.get_text("root").is_some();
        snapshot
            && match self {
                Self::Read if value.get_bool("binary") => value.get_int("size").is_some(),
                Self::Read => {
                    matches!(value.get("binary"), None | Some(SdkValue::Bool(false)))
                        && value.get_text("text").is_some()
                        && value.get_int("size").is_some()
                        && value.get_int("startLine").is_some()
                        && nullable_int(value, "endLine")
                        && nullable_int(value, "nextLine")
                        && matches!(value.get("lineTruncated"), Some(SdkValue::Bool(_)))
                }
                Self::Stat => {
                    inode(value)
                        && value.get_int("size").is_some()
                        && value.get_int("mode").is_some()
                        && value.get_int("modifiedMs").is_some()
                }
                Self::List => {
                    nullable_int(value, "nextPagePos")
                        && value.get_int("removed_count").is_some()
                        && matches!(value.get("items"), Some(SdkValue::List(items)) if items.iter().all(|row| {
                            row.get_text("name").is_some() && row.get_text("path").is_some() && inode(row)
                        }))
                }
            }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn nullable_int(value: &SdkValue, field: &str) -> bool {
    matches!(value.get(field), Some(SdkValue::Null | SdkValue::Int(_)))
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn inode(value: &SdkValue) -> bool {
    matches!(value.get_text("type"), Some("file" | "dir" | "symlink"))
        && value.get_int("inode").is_some()
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn routed(
    home: &Path,
    target: &Target,
    method: &str,
    fields: &[(String, SdkValue)],
) -> Option<Outcome> {
    use crate::admin::record::store::StoreRef;
    use layerfs_machine::worker::{HostPaths, HOST_MAX_REPLY, HOST_MAX_REQUEST};

    if !cli() {
        return None;
    }
    let StoreRef::Sqlite { machine, .. } = &target.store_ref else {
        return None;
    };
    if layerfs_machine::in_process() != Some(machine.as_str()) {
        return None;
    }
    let checked = Method::of(method)?;
    let fields = SdkValue::Map(fields.to_vec());
    if !checked.accepts(&fields) {
        return None;
    }

    // The engine caps text and positional list responses at 48 KiB. List
    // rows copy the basename into both name and path. Allow six JSON bytes
    // per input byte, duplicated header fields, and fixed reply metadata.
    const _: () = assert!(12 * 48 * 1024 + 2 * HOST_MAX_REQUEST + 4096 <= HOST_MAX_REPLY);
    let args = SdkValue::map([
        ("method", SdkValue::text(method)),
        ("store", SdkValue::text(&target.store)),
        ("project", SdkValue::text(&target.project)),
        ("fields", fields),
    ]);
    let paths = match HostPaths::new(home) {
        Ok(paths) => paths,
        Err(error) => return Some(Err(error)),
    };
    paths
        .request_fits(OP, &args)
        .then(|| paths.call_validated(OP, args, |value| checked.valid_answer(value)))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(crate) fn routed(
    _home: &Path,
    _target: &Target,
    _method: &str,
    _fields: &[(String, SdkValue)],
) -> Option<Outcome> {
    None
}

/// Runs one authenticated request from the native worker's ordinary lane.
pub fn serve(home: &Path, request: &SdkValue) -> Outcome {
    let invalid = || refused("InvalidInput", "invalid native Store read request");
    let SdkValue::Map(envelope) = request else {
        return Err(invalid());
    };
    if envelope.len() != 4 {
        return Err(invalid());
    }
    let method = request.get_text("method").ok_or_else(invalid)?;
    let allowed = Method::of(method).ok_or_else(|| {
        refused(
            "Unsupported",
            "the host worker only serves file read, stat and list",
        )
    })?;
    let store = request
        .get_text("store")
        .ok_or_else(invalid)
        .and_then(StoreName::parse)?;
    let project = request
        .get_text("project")
        .ok_or_else(invalid)
        .and_then(ProjectName::parse)?;
    let fields = request.get("fields").ok_or_else(invalid)?;
    if !allowed.accepts(fields) {
        return Err(refused(
            "InvalidInput",
            "invalid or unbounded native Store read arguments",
        ));
    }
    let resolved = resolve::target(home, &format!("{store}/{project}"))?;
    crate::admin::open::cache::run_host(
        home,
        &resolved.store,
        &resolved.project,
        method,
        fields.clone(),
    )
}
