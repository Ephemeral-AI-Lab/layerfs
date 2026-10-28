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
use layerfs_history::NodeId;
use std::sync::Arc;

pub fn stats(store: &Arc<Store>) -> Answer {
    let value = store.history().statistics().map_err(args::history)?;
    let ratio = |n: u64, d: u64| {
        if d == 0 {
            SdkValue::Null
        } else {
            SdkValue::text(format!("{:.6}", n as f64 / d as f64))
        }
    };
    Ok(SdkValue::map([
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
    ]))
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
