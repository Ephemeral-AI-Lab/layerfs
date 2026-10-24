//! A public method's arguments: name-keyed values in, typed values out.
//!
//! A method reads each argument just before the engine step that first uses
//! it, so a call with several faults is refused by the first, as it always
//! was. An argument the engine uses only after a lookup travels as an
//! [`Arg`], read but not yet refused.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::LayerStackRecord;
use layerfs_store_service::read::compare::{Request, Selector};
use layerfs_store_service::read::snapshot::Snap;
use layerfs_store_service::store::StorePorts;
use layerfs_store_service::write::files::{self, Change};
use layerfs_store_service::StoreService;
use std::sync::Arc;

pub use layerfs_store_service::history::lookup::{invalid, Answer, Arg};
pub use layerfs_store_service::operator::args::{paging, text};

/// The project's stack, by its `project` argument.
pub fn project(service: &StoreService, args: &SdkValue) -> Result<LayerStackRecord, SdkRefusal> {
    layerfs_store_service::operator::args::project(&service.store, args)
}

/// The project's reader and the snapshot a read names (`commit`, else `branch`).
pub fn snapshot(
    service: &StoreService,
    args: &SdkValue,
) -> Result<(Arc<StorePorts>, Snap), SdkRefusal> {
    let (_, reader, snap) = layerfs_store_service::operator::args::snapshot(&service.store, args)?;
    Ok((reader, snap))
}

/// The required absolute path argument `name`, checked.
pub fn path(args: &SdkValue, name: &str) -> Result<String, SdkRefusal> {
    files::path(text(args, name)?)
}

/// The `path` argument, checked; `/` when absent.
pub fn optional_path(args: &SdkValue) -> Result<String, SdkRefusal> {
    files::path(args.get_text("path").unwrap_or("/"))
}

/// A positive line number argument; `default` when absent or null.
pub fn line(args: &SdkValue, name: &str, default: u64) -> Result<u64, SdkRefusal> {
    match args.get(name) {
        None | Some(SdkValue::Null) => Ok(default),
        Some(SdkValue::Int(n)) if *n > 0 && *n <= 9_007_199_254_740_991 => Ok(*n as u64),
        _ => Err(invalid(format!("{name} must be a positive safe integer"))),
    }
}

/// A tree difference's page: offset or `cursor` path, and whether `hunks`.
pub fn diff_page(args: &SdkValue) -> Result<Request, SdkRefusal> {
    let (limit, pos) = paging(args)?;
    let cursor = match args.get("cursor") {
        None | Some(SdkValue::Null) => None,
        Some(SdkValue::Text(path)) => Some(files::path(path)?),
        Some(_) => return Err(invalid("cursor must be a path or null")),
    };
    let hunks = match args.get("hunks") {
        None | Some(SdkValue::Null) => true,
        Some(SdkValue::Bool(value)) => *value,
        Some(_) => return Err(invalid("hunks must be a boolean")),
    };
    Ok(Request {
        limit,
        pos,
        cursor,
        hunks,
    })
}

/// One side of a two-file difference: the `name` selector's `path`, at its
/// `commit`, else at `branch`.
pub fn selector<'a>(args: &'a SdkValue, name: &str, branch: &'a str) -> Arg<Selector<'a>> {
    let selector = args
        .get(name)
        .ok_or_else(|| invalid(format!("missing `{name}` selector")))?;
    Ok(Selector {
        path: path(selector, "path")?,
        at: selector.get_text("commit").unwrap_or(branch),
    })
}

/// A change's optional `mode`: permission bits, an integer up to 0o7777.
/// Whether the entry can hold them is checked where the entry's kind is known.
fn mode(change: &SdkValue) -> Result<Option<u32>, SdkRefusal> {
    match change.get("mode") {
        None | Some(SdkValue::Null) => Ok(None),
        Some(SdkValue::Int(n)) if (0..=0o7777).contains(n) => Ok(Some(*n as u32)),
        Some(_) => Err(invalid(
            "mode must be an integer permission mode from 0 to 0o7777",
        )),
    }
}

/// One change of a file call, from its `op` and that op's fields. A write
/// whose bytes were to follow the call (`streamed`) is refused: no bytes
/// follow a call on the in-process route.
pub fn change(change: &SdkValue) -> Arg<Change<'_>> {
    let kind = text(change, "op")?;
    let p = |name| path(change, name);
    Ok(match kind {
        "write" => match (
            change.get("content"),
            change.get("bytes"),
            change.get("streamed"),
        ) {
            (Some(SdkValue::Text(s)), None, None) => Change::Write {
                path: p("path")?,
                bytes: s.as_bytes(),
                mode: mode(change)?,
            },
            (None, Some(SdkValue::Bytes(b)), None) => Change::Write {
                path: p("path")?,
                bytes: b,
                mode: mode(change)?,
            },
            (None, None, Some(SdkValue::Int(n))) if *n > 0 => {
                p("path")?;
                return Err(invalid("streamed body beyond the declared bytes"));
            }
            _ => return Err(invalid("write content must be text or bytes")),
        },
        "edit" => Change::Edit {
            path: p("path")?,
            old: text(change, "oldText")?,
            new: text(change, "newText")?,
        },
        "mkdir" => Change::Mkdir {
            path: p("path")?,
            mode: mode(change)?,
        },
        "chmod" => Change::Chmod {
            path: p("path")?,
            mode: mode(change)?.ok_or_else(|| invalid("chmod needs a mode"))?,
        },
        "rm" => Change::Rm { path: p("path")? },
        "cp" => Change::Cp {
            from: p("from")?,
            to: p("to")?,
        },
        "mv" => Change::Mv {
            from: p("from")?,
            to: p("to")?,
        },
        _ => return Err(invalid(format!("unknown change op {kind}"))),
    })
}

/// A one-change method's arguments as its change: the call's own fields with
/// `op` set to `kind` after them.
pub fn single(args: &SdkValue, kind: &str) -> SdkValue {
    let mut fields = match args.clone() {
        SdkValue::Map(fields) => fields,
        _ => Vec::new(),
    };
    fields.push(("op".into(), SdkValue::text(kind)));
    SdkValue::Map(fields)
}

/// Commits `changes` once to the call's `branch` of its `project`.
pub fn commit(service: &StoreService, args: &SdkValue, changes: &[SdkValue]) -> Answer {
    let project = text(args, "project")?;
    let changes: Vec<_> = changes.iter().map(change).collect();
    files::commit(
        service,
        project,
        text(args, "branch"),
        &changes,
        args.get_text("message"),
    )
}
