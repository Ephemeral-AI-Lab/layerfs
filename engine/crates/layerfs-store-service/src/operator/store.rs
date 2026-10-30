//! Publication counters and Init's recorded totals; diagnostics never scan the object catalog.
use super::args::{paging, project, text};
use crate::{
    history::{
        branch,
        lookup::{self as args, Answer},
    },
    store::Store,
};
use layerfs_bridge::control::SdkValue;
use layerfs_history::{CommitKindStatistics, CommitVia, NodeId};
use std::sync::Arc;

/// One Commit kind's totals as SDK values; absent kinds answer zero-filled.
fn kind_totals(kinds: &[CommitKindStatistics], via: CommitVia) -> SdkValue {
    let found = kinds.iter().find(|kind| kind.via == via);
    let (count, canonical, stored, metadata) = found.map_or((0, 0, 0, 0), |kind| {
        (
            kind.count,
            kind.new_canonical,
            kind.new_stored,
            kind.new_metadata,
        )
    });
    SdkValue::map([
        ("count", SdkValue::Int(count as i64)),
        ("newCanonical", SdkValue::Int(canonical as i64)),
        ("newStored", SdkValue::Int(stored as i64)),
        ("newMetadata", SdkValue::Int(metadata as i64)),
    ])
}

pub fn summary(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let project = match args {
        SdkValue::Map(fields) => {
            for (key, _) in fields {
                match key.as_str() {
                    "project" | "home" | "deploy" | "storePath" | "store_path" | "subscription" => {
                    }
                    _ => return Err(args::invalid(format!("unknown option `{key}`"))),
                }
            }
            text(args, "project")?
        }
        _ => return Err(args::invalid("options must be a map")),
    };
    let project = args::name(project)?;
    let value = store.history().summary(&project).map_err(args::history)?;
    Ok(SdkValue::map([
        ("logical", SdkValue::Int(value.logical as i64)),
        ("branches", SdkValue::Int(value.branches as i64)),
        ("commits", SdkValue::Int(value.commits as i64)),
        (
            "mainLayer",
            SdkValue::opt_text(value.main_layer.map(|id| args::hex(id.as_slice()))),
        ),
        (
            "lastCommit",
            value.last_commit.map_or(SdkValue::Null, |commit| {
                SdkValue::map([
                    ("id", SdkValue::text(args::hex(commit.id.as_slice()))),
                    ("createdMs", SdkValue::Int(commit.created_ms)),
                ])
            }),
        ),
    ]))
}

pub fn stats(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let commits = match args {
        SdkValue::Map(fields) => {
            let mut commits = false;
            for (key, val) in fields {
                match key.as_str() {
                    "commits" => match val {
                        SdkValue::Bool(b) => commits = *b,
                        _ => return Err(args::invalid("commits must be a boolean")),
                    },
                    "project" | "home" | "deploy" | "storePath" | "store_path" | "subscription" => {
                    }
                    _ => return Err(args::invalid(format!("unknown option `{key}`"))),
                }
            }
            commits
        }
        _ => return Err(args::invalid("options must be a map")),
    };
    let value = store.history().statistics().map_err(args::history)?;
    let disk = store.history().disk_statistics().map_err(args::history)?;
    let ratio = |n: u64, d: u64| {
        if d == 0 {
            SdkValue::Null
        } else {
            SdkValue::text(format!("{:.6}", n as f64 / d as f64))
        }
    };
    let mut out = vec![
        ("logical", SdkValue::Int(value.logical as i64)),
        ("canonical", SdkValue::Int(value.canonical as i64)),
        ("stored", SdkValue::Int(value.stored as i64)),
        ("metadata", SdkValue::Int(value.metadata as i64)),
        ("file", SdkValue::Int(value.file as i64)),
        ("free", SdkValue::Int(value.free as i64)),
        ("dedupe", ratio(value.logical, value.canonical)),
        ("compression", ratio(value.canonical, value.stored)),
        (
            "init",
            value.init.map_or(SdkValue::Null, |init| {
                SdkValue::map([
                    ("entries", SdkValue::Int(init.entries as i64)),
                    ("files", SdkValue::Int(init.files as i64)),
                    ("bytes", SdkValue::Int(init.bytes as i64)),
                    ("canonical", SdkValue::Int(init.canonical as i64)),
                    ("stored", SdkValue::Int(init.stored as i64)),
                    ("metadata", SdkValue::Int(init.metadata as i64)),
                    ("millis", SdkValue::Int(init.millis as i64)),
                ])
            }),
        ),
        (
            "disk",
            SdkValue::map([
                ("main", SdkValue::Int(disk.main as i64)),
                ("wal", SdkValue::Int(disk.wal as i64)),
                ("total", SdkValue::Int((disk.main + disk.wal) as i64)),
            ]),
        ),
    ];
    if commits {
        // One full scan of the commit catalog, opt-in; never a cheap total.
        let kinds = store
            .history()
            .commit_kind_statistics()
            .map_err(args::history)?;
        out.push((
            "commits",
            SdkValue::map([
                ("Workspace", kind_totals(&kinds, CommitVia::Workspace)),
                ("Files", kind_totals(&kinds, CommitVia::Files)),
                ("Merge", kind_totals(&kinds, CommitVia::Merge)),
            ]),
        ));
    }
    Ok(SdkValue::Map(
        out.into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}

pub fn commits(store: &Arc<Store>, args: &SdkValue) -> Answer {
    let stack = project(store, args)?;
    let named = args::branch(store, &stack, text(args, "branch")?)?;
    let (limit, pos) = paging(args)?;
    let value = branch::list_commits(store, &named, limit, pos)?;
    let items = match value.get("items") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|record| {
                SdkValue::map([
                    ("commit", record.get("id").cloned().unwrap_or_default()),
                    (
                        "logical",
                        record.get("logical").cloned().unwrap_or_default(),
                    ),
                    (
                        "newCanonical",
                        record.get("newCanonical").cloned().unwrap_or_default(),
                    ),
                    (
                        "newStored",
                        record.get("newStored").cloned().unwrap_or_default(),
                    ),
                    (
                        "newMetadata",
                        record.get("newMetadata").cloned().unwrap_or_default(),
                    ),
                ])
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(value.get_bool("hasMore"))),
        (
            "nextPagePos",
            value.get("nextPagePos").cloned().unwrap_or_default(),
        ),
    ]))
}

/// `backend.store.project.list_overwritten({ layer, pageSize?, pagePos? })`: the Layers a
/// publication overwrote, newest first, one page.
pub fn overwritten(store: &Arc<Store>, args: &SdkValue) -> Answer {
    project(store, args)?;
    let layer = match args::node(text(args, "layer")?)? {
        NodeId::Layer(layer) => layer,
        NodeId::Commit(_) => return Err(args::invalid("overwritten takes a Layer")),
    };
    let (limit, pos) = paging(args)?;
    let (items, more) = branch::overwritten(store, layer, pos, limit)?;
    let next = args::next_page(pos, items.len(), more);
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(more)),
        ("nextPagePos", next),
    ]))
}
