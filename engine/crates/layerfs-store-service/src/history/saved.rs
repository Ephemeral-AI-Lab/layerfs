//! Typed saved identities and bounded, branch-qualified history lookups.
use super::{lookup as args, MAIN};
use crate::store::Store;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::{LayerStackRecord, NodeId};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;
/// Per-query history lookup bound, never a limit on stored history.
pub const LOOKUP_WORK: usize = 10_000;

/// A full immutable reference, with its recorded originating branch when known.
pub fn reference(store: &Arc<Store>, node: NodeId) -> Result<SdkValue> {
    let (root, branch) = match node {
        NodeId::Layer(id) => (args::layer(store, id)?.root, Some(MAIN.to_owned())),
        NodeId::Commit(id) => {
            let record = args::commit(store, id)?;
            let branch = record
                .meta
                .branch
                .map(|id| store.published(|history| history.branch(id)))
                .transpose()
                .map_err(args::published)?
                .flatten()
                .map(|b| b.name.as_str().to_owned());
            (record.root, branch)
        }
    };
    Ok(SdkValue::map([
        ("id", SdkValue::text(args::node_text(node))),
        (
            "kind",
            SdkValue::text(match node {
                NodeId::Commit(_) => "Commit",
                NodeId::Layer(_) => "Layer",
            }),
        ),
        ("branch", SdkValue::opt_text(branch)),
        ("root", SdkValue::text(args::hex(root.as_bytes()))),
    ]))
}

fn optional(store: &Arc<Store>, node: Option<NodeId>) -> Result<SdkValue> {
    node.map(|node| reference(store, node))
        .transpose()
        .map(|v| v.unwrap_or_default())
}

/// Saved details: publication parent/source and their times remain distinct.
pub fn record(store: &Arc<Store>, node: NodeId) -> Result<SdkValue> {
    let mut fields = match node {
        NodeId::Commit(id) => {
            let r = args::commit(store, id)?;
            let mut value = args::commit_value(&r, None);
            let SdkValue::Map(ref mut fields) = value else {
                unreachable!()
            };
            fields.extend([
                (
                    "parentRef".into(),
                    optional(store, r.parent.map(NodeId::Commit))?,
                ),
                ("secondParentRef".into(), optional(store, r.second_parent)?),
                (
                    "baseLayer".into(),
                    reference(store, NodeId::Layer(r.base_layer))?,
                ),
                ("sourceCommitRef".into(), SdkValue::Null),
                ("sourceTime".into(), SdkValue::Null),
                ("timeStatus".into(), SdkValue::text("recorded")),
            ]);
            if let SdkValue::Map(fields) = value {
                fields
            } else {
                unreachable!()
            }
        }
        NodeId::Layer(id) => {
            let r = args::layer(store, id)?;
            let source = r
                .source_commit
                .map(|id| args::commit(store, id))
                .transpose()?;
            let source_branch = r
                .source_branch
                .map(|id| store.published(|history| history.branch(id)))
                .transpose()
                .map_err(args::published)?
                .flatten();
            let value = args::layer_value(&r, source_branch.as_ref().map(|b| b.name.as_str()));
            let SdkValue::Map(mut fields) = value else {
                unreachable!()
            };
            fields.extend([
                (
                    "parentRef".into(),
                    optional(store, r.parent.map(NodeId::Layer))?,
                ),
                ("secondParentRef".into(), SdkValue::Null),
                ("baseLayer".into(), SdkValue::Null),
                (
                    "sourceCommitRef".into(),
                    optional(store, r.source_commit.map(NodeId::Commit))?,
                ),
                (
                    "sourceTime".into(),
                    source
                        .as_ref()
                        .map_or(SdkValue::Null, |s| SdkValue::Int(s.meta.time_ms)),
                ),
                ("time".into(), SdkValue::Null),
                ("timeStatus".into(), SdkValue::text("unavailable")),
                (
                    "via".into(),
                    SdkValue::text(if r.source_commit.is_some() {
                        "Publish"
                    } else {
                        "Init"
                    }),
                ),
                ("message".into(), SdkValue::Null),
            ]);
            fields
        }
    };
    fields.push(("ref".into(), reference(store, node)?));
    Ok(SdkValue::Map(fields))
}

/// Resolve a saved start through its branch context. First-parent branch history
/// admits inherited Commits; second parents remain named links, not listing rows.
/// Main admits publication Layers only. No global-id bypass of project ownership.
pub fn resolve_context(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    branch: &str,
    at: Option<&str>,
) -> Result<NodeId> {
    let head = if branch == MAIN {
        NodeId::Layer(stack.head_layer)
    } else {
        args::branch_node(&args::branch(store, stack, branch)?)
    };
    let Some(at) = at else { return Ok(head) };
    let wanted = args::node(at)?;
    if branch == MAIN {
        if let NodeId::Layer(id) = wanted {
            let record = args::layer(store, id)?;
            if record.stack != stack.id {
                return Err(args::invalid("Layer belongs to another project"));
            }
            return Ok(wanted);
        }
    }

    if let NodeId::Commit(id) = wanted {
        let record = args::commit(store, id)?;
        if record.stack != stack.id {
            return Err(args::invalid("Commit belongs to another project"));
        }
        if branch != MAIN && record.meta.branch == Some(args::branch(store, stack, branch)?.id) {
            return Ok(wanted);
        }
    }

    let mut current = Some(head);
    for _ in 0..LOOKUP_WORK {
        let Some(node) = current else {
            return Err(args::refuse(
                "NoSuchCommit",
                "saved version is not in this branch history",
                SdkValue::Null,
            ));
        };
        if node == wanted {
            return Ok(node);
        }
        current = match node {
            NodeId::Commit(id) => {
                let r = args::commit(store, id)?;
                if r.stack != stack.id {
                    return Err(args::invalid("Commit belongs to another project"));
                }
                Some(r.parent.map_or(NodeId::Layer(r.base_layer), NodeId::Commit))
            }
            NodeId::Layer(id) => {
                let r = args::layer(store, id)?;
                if r.stack != stack.id {
                    return Err(args::invalid("Layer belongs to another project"));
                }
                r.parent.map(NodeId::Layer)
            }
        };
    }
    Err(args::refuse(
        "Capacity",
        "saved history lookup work bound reached; narrow the starting branch context",
        SdkValue::Null,
    ))
}

/// Conservative encoded reply cost including field names and string escaping.
pub fn reply_cost(value: &SdkValue) -> usize {
    match value {
        SdkValue::Null | SdkValue::Bool(_) => 8,
        SdkValue::Int(_) => 24,
        SdkValue::Text(s) => {
            8 + s
                .bytes()
                .map(|b| match b {
                    b'"' | b'\\' => 2,
                    0..=31 => 6,
                    _ => 1,
                })
                .sum::<usize>()
        }
        SdkValue::Bytes(b) => 8 + b.len().saturating_mul(2),
        SdkValue::List(v) => 8 + v.iter().map(reply_cost).sum::<usize>(),
        SdkValue::Map(v) => {
            8 + v
                .iter()
                .map(|(k, v)| 8 + k.len() + reply_cost(v))
                .sum::<usize>()
        }
    }
}

/// Compact listing metadata; full relationships and optional reports use get_commit.
pub fn summary(store: &Arc<Store>, node: NodeId, include_storage: bool) -> Result<SdkValue> {
    let full = record(store, node)?;
    let keys = [
        "id",
        "ref",
        "parent",
        "secondParent",
        "base",
        "sourceCommit",
        "sourceBranch",
        "time",
        "sourceTime",
        "timeStatus",
        "via",
        "logical",
    ];
    let mut fields: Vec<(String, SdkValue)> = keys
        .iter()
        .filter_map(|key| {
            full.get(key)
                .map(|value| ((*key).to_owned(), value.clone()))
        })
        .collect();
    // Layers do not perform a Save: absent allocation facts stay unavailable.
    if include_storage {
        for key in ["newCanonical", "newStored", "newMetadata"] {
            fields.push((key.to_owned(), full.get(key).cloned().unwrap_or_default()));
        }
    }
    let message = full.get_text("message");
    let clipped = message.is_some_and(|s| s.len() > 128);
    let message = message
        .map(|s| {
            let mut n = s.len().min(128);
            while !s.is_char_boundary(n) {
                n -= 1
            }
            SdkValue::text(&s[..n])
        })
        .unwrap_or_default();
    fields.extend([
        ("message".into(), message),
        ("messageTruncated".into(), SdkValue::Bool(clipped)),
        ("detailsOmitted".into(), SdkValue::Bool(true)),
    ]);
    Ok(SdkValue::Map(fields))
}
