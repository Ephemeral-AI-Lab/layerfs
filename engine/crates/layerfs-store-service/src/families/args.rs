//! A public method's arguments: name-keyed values in, typed values out.
//!
//! A method reads each argument just before the engine step that first uses
//! it, so a call with several faults is refused by the first, as it always
//! was. An argument the engine uses only after a lookup travels as an
//! [`Arg`], read but not yet refused.
//!
//! The operator tables here and in the daemon read the same name-keyed maps
//! with the plain readers below ([`text`], [`stack`], [`paging`],
//! [`snapshot`]). The Store engine takes typed values.
use crate::filesystem::change::run::{self, Change};
use crate::filesystem::root::{Named, Saved, Snap};
use crate::lookup;
use crate::store::{Store, StorePorts};
use crate::StoreService;
use layerfs_content::AuthenticatedObjects;
use layerfs_history::{ContextTarget, HistoryName, LayerStackRecord, NodeId};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

pub use crate::lookup::{invalid, next_page, Answer, Arg};

/// The project's stack, by its `project` argument.
pub fn project(service: &StoreService, args: &SdkValue) -> Result<LayerStackRecord, SdkRefusal> {
    stack(&service.store, args)
}

/// The project, its reader and the snapshot a read names, in that order:
/// its `commit` when given, else its `branch`. A `commit` shaped as an
/// id prefix (8 to 65 hex characters) is resolved to its Commit first, a
/// prefix more than one Commit starts with refused `InvalidInput` with
/// `matches`.
///
/// A read naming `main`, a Branch by a valid name or a node by its full
/// identity reads its history context in one call (#818 R5): the stack,
/// the Branch, the node's record and, on PostgreSQL, its markers and
/// recorded Branch, all in one statement. Any other read keeps the reads
/// one by one, so each refuses where it did.
///
/// The snapshot itself resolves in [`snapshot_in`], once the call has built
/// its caches over the reader (#818 R4).
pub fn snapshot_target(
    service: &StoreService,
    args: &SdkValue,
) -> Result<(LayerStackRecord, Arc<StorePorts>, Named), SdkRefusal> {
    if let Some((project, name, target)) = context_target(args) {
        let context = service
            .store
            .published(|history| history.read_context(&project, &target))
            .map_err(crate::lookup::published)?;
        let stack = context
            .stack
            .clone()
            .ok_or_else(|| crate::lookup::no_project(project.as_str()))?;
        let reader = crate::filesystem::root::reader(&service.store, &stack);
        let named = Named {
            name,
            context: Some(context),
        };
        return Ok((stack, reader, named));
    }
    let stack = project(service, args)?;
    let name = match pinned(service, &stack, args)? {
        Some(full) => full,
        None => crate::families::args::snapshot_name(args)?.to_owned(),
    };
    let reader = crate::filesystem::root::reader(&service.store, &stack);
    Ok((
        stack,
        reader,
        Named {
            name,
            context: None,
        },
    ))
}

/// The project and target of a read whose history context one call reads
/// (#818 R5): a valid project name with `main`, a valid Branch name or a
/// full node identity, named as [`snapshot_target`] reads them. `None` for
/// any other read: a missing or invalid argument, a `commit` prefix, an
/// invalid name or an identity that does not parse.
fn context_target(args: &SdkValue) -> Option<(HistoryName, String, ContextTarget)> {
    use crate::history::resolve::FULL;
    let project = HistoryName::new(args.get_text("project")?).ok()?;
    let name = match args.get_text("commit") {
        Some(text)
            if text.len() == FULL
                && text.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) =>
        {
            text
        }
        Some(_) => return None,
        None => args.get_text("branch")?,
    };
    let target = if name == crate::history::MAIN {
        ContextTarget::Main
    } else if name.len() == FULL && layerfs_protocol::sdk::unhex(name).is_some() {
        ContextTarget::Node(NodeId::from_slice(&layerfs_protocol::sdk::unhex(name)?).ok()?)
    } else {
        ContextTarget::Branch(HistoryName::new(name).ok()?)
    };
    Some((project, name.to_owned(), target))
}

/// The snapshot `named` resolves to, its root opened through `pages` (the
/// call's caches), with what the resolve read on the way ([`Saved`]): the
/// node's record for the answer's `at`, and its markers for the visibility
/// step.
pub fn snapshot_in(
    service: &StoreService,
    stack: &LayerStackRecord,
    pages: &dyn AuthenticatedObjects,
    named: Named,
) -> Result<(Snap, Saved), SdkRefusal> {
    match named.context {
        Some(context) => crate::filesystem::root::resolve_context(
            &service.store,
            stack,
            pages,
            &named.name,
            context,
        ),
        None => crate::filesystem::root::resolve_saved(&service.store, stack, pages, &named.name),
    }
}

/// The full identity of the read's `commit` when it is an id prefix. A
/// `commit` is a Commit only, never a Branch name: anything but a full id
/// or a prefix of 8 or more hex characters is refused `InvalidInput`, as
/// `file.history` refuses it.
pub fn pinned(
    service: &StoreService,
    stack: &LayerStackRecord,
    args: &SdkValue,
) -> Result<Option<String>, SdkRefusal> {
    use crate::history::resolve::{commit, FULL};
    match args.get_text("commit") {
        // a full id opens by itself, a Commit or a publication of main
        Some(text)
            if text.len() == FULL
                && text.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) =>
        {
            Ok(None)
        }
        Some(text) => Ok(Some(commit(&service.store, stack, text)?.text())),
        None => Ok(None),
    }
}

/// An optional boolean argument; `default` when absent or null.
pub fn flag(args: &SdkValue, name: &str, default: bool) -> Result<bool, SdkRefusal> {
    match args.get(name) {
        None | Some(SdkValue::Null) => Ok(default),
        Some(SdkValue::Bool(value)) => Ok(*value),
        Some(_) => Err(invalid(format!("{name} must be a boolean"))),
    }
}

/// The required absolute path argument `name`, checked.
pub fn path(args: &SdkValue, name: &str) -> Result<String, SdkRefusal> {
    run::path(text(args, name)?)
}

/// The `path` argument, checked; `/` when absent.
pub fn optional_path(args: &SdkValue) -> Result<String, SdkRefusal> {
    run::path(args.get_text("path").unwrap_or("/"))
}

/// A positive line number argument; `default` when absent or null.
pub fn line(args: &SdkValue, name: &str, default: u64) -> Result<u64, SdkRefusal> {
    match args.get(name) {
        None | Some(SdkValue::Null) => Ok(default),
        Some(SdkValue::Int(n)) if *n > 0 && *n <= 9_007_199_254_740_991 => Ok(*n as u64),
        _ => Err(invalid(format!("{name} must be a positive safe integer"))),
    }
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
/// whose bytes follow it (`streamed: true`) takes them from the call's
/// streamed source; without one it is refused where it is applied. A
/// declared byte length (`streamed: n`) is refused: no route sends one.
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
            // bytes that follow the change in a streamed call's source
            (None, None, Some(SdkValue::Bool(true))) => Change::Streamed {
                path: p("path")?,
                mode: mode(change)?,
            },
            _ => return Err(invalid("write content must be text or bytes")),
        },
        "edit" => Change::Edit {
            path: p("path")?,
            old: text(change, "oldText")?,
            new: text(change, "newText")?,
            all: flag(change, "replaceAll", false)?,
        },
        "mkdir" => Change::Mkdir {
            path: p("path")?,
            mode: mode(change)?,
        },
        "chmod" => Change::Chmod {
            path: p("path")?,
            mode: mode(change)?.ok_or_else(|| invalid("chmod needs a mode"))?,
        },
        // a folder with everything in it unless `recursive` is false
        "rm" => Change::Rm {
            path: p("path")?,
            recursive: flag(change, "recursive", true)?,
        },
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
    let changes: Vec<_> = changes.iter().map(change).collect();
    commit_from(service, args, run::Changes::Listed(&changes))
}

/// Commits the changes `source` streams once, as [`commit`] commits listed
/// ones: one private Workspace, one Commit after the source's last change.
pub fn commit_streamed(
    service: &StoreService,
    args: &SdkValue,
    source: &mut dyn run::Source,
) -> Answer {
    commit_from(
        service,
        args,
        run::Changes::Streamed {
            source,
            parse: change,
        },
    )
}

fn commit_from(service: &StoreService, args: &SdkValue, changes: run::Changes<'_, '_>) -> Answer {
    let project = text(args, "project")?;
    run::commit(
        service,
        project,
        text(args, "branch"),
        changes,
        args.get_text("message"),
        args.get_text("expected"),
    )
}

/// A required text argument.
pub fn text<'a>(args: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    args.get_text(name)
        .ok_or_else(|| invalid(format!("missing text argument `{name}`")))
}

/// The project's stack in `store`, by its `project` argument.
pub fn stack(store: &Arc<Store>, args: &SdkValue) -> Result<LayerStackRecord, SdkRefusal> {
    lookup::project(store, text(args, "project")?)
}

/// Safe numeric page arguments. Offset skipping is linear and uses bounded windows.
pub fn paging(value: &SdkValue) -> Result<(usize, u64), SdkRefusal> {
    if value.get("page").is_some() {
        let page = crate::filesystem::paged::query::result_page(value)?;
        return Ok((page.limit, page.skip));
    }
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
    let stack = stack(store, args)?;
    let (reader, snap) = crate::filesystem::root::open(store, &stack, snapshot_name(args)?)?;
    Ok((stack, reader, snap))
}
