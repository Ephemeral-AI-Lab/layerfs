//! `store.storage_report({ snapshots, maxObjects?, maxCanonicalBytes?,
//! includeEncoding? })`: bounded canonical sharing among explicit saved
//! snapshots of this Store's projects.
//!
//! Every snapshot is `{ project, branch, commit }` with a full saved id that
//! its branch context admits, exactly as `branch.get_commit` admits it. No
//! head, history page or Workspace is read here.
use crate::history::{lookup, saved};
use crate::methods::args::{invalid, text, Answer};
use crate::store::storage_report::{report as walk, Limits, Pin};
use crate::StoreService;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::collections::BTreeSet;

/// Most snapshots, objects and canonical bytes one report takes.
const MAXIMUM_SNAPSHOTS: usize = 64;
const MAXIMUM_OBJECTS: i64 = 10_000;
const MAXIMUM_CANONICAL_BYTES: i64 = 64 * 1024 * 1024;

pub fn report(service: &StoreService, args: &SdkValue) -> Answer {
    let limits = Limits {
        objects: budget(args, "maxObjects", MAXIMUM_OBJECTS)?,
        bytes: budget(args, "maxCanonicalBytes", MAXIMUM_CANONICAL_BYTES)?,
        encoding: match args.get("includeEncoding") {
            None | Some(SdkValue::Null) => false,
            Some(SdkValue::Bool(value)) => *value,
            Some(_) => return Err(invalid("includeEncoding must be a boolean")),
        },
    };
    let Some(SdkValue::List(given)) = args.get("snapshots") else {
        return Err(invalid("snapshots must be [{project,branch,commit}]"));
    };
    if given.is_empty() || given.len() > MAXIMUM_SNAPSHOTS {
        return Err(invalid(
            "snapshots must contain 1..=64 explicit immutable pins",
        ));
    }
    let store = &service.store;
    let mut pins = Vec::with_capacity(given.len());
    let mut keys = BTreeSet::new();
    let mut stacks = Vec::new();
    for snapshot in given {
        let project = text(snapshot, "project")?;
        let branch = text(snapshot, "branch")?;
        let stack = lookup::project(store, project)?;
        let node = saved::resolve_context(store, &stack, branch, Some(text(snapshot, "commit")?))?;
        let saved_id = lookup::node_text(node);
        if !keys.insert((project.to_owned(), saved_id.clone())) {
            return Err(invalid("duplicate project/saved-id snapshot"));
        }
        pins.push(Pin {
            project: project.to_owned(),
            branch: branch.to_owned(),
            saved: saved_id,
            root: lookup::root(store, node)?,
            at: saved::reference(store, node)?,
        });
        stacks.push(stack);
    }
    // Objects are Store-wide; one read session serves every snapshot.
    let reader = crate::read::snapshot::reader(store, &stacks[0]);
    walk(store, &reader, &pins, &limits)
}

/// A positive budget no larger than `maximum`; `maximum` when absent.
fn budget(args: &SdkValue, name: &str, maximum: i64) -> Result<u64, SdkRefusal> {
    match args.get(name) {
        None | Some(SdkValue::Null) => Ok(maximum as u64),
        Some(SdkValue::Int(n)) if (1..=maximum).contains(n) => Ok(*n as u64),
        _ => Err(invalid(format!("{name} must be in 1..={maximum}"))),
    }
}
