//! Typed saved identities and bounded, branch-qualified history lookups.
use super::{lookup as args, MAIN};
use crate::store::Store;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::ObjectId;
use layerfs_history::{LayerStackRecord, NodeId};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;
/// Per-query history lookup bound, never a limit on stored history.
pub const LOOKUP_WORK: usize = 10_000;

/// A full immutable reference, with its recorded originating branch when known.
pub fn reference(store: &Arc<Store>, node: NodeId) -> Result<SdkValue> {
    super::rows::Rows::new(store).reference(node)
}

/// [`reference`] from the node's record the caller already read (#818 R2):
/// only a Commit's recorded Branch name is read, unless `recorded` holds it
/// already (the read call's context, #818 R5).
pub fn reference_of(
    store: &Arc<Store>,
    node: NodeId,
    record: &args::NodeRecord,
    recorded: Option<&Option<String>>,
) -> Result<SdkValue> {
    let mut rows = super::rows::Rows::new(store);
    rows.hold(node, record.clone(), recorded.cloned());
    rows.reference(node)
}

/// The reference value of `node`, its `root` and recorded `branch`.
pub fn reference_value(node: NodeId, root: ObjectId, branch: Option<String>) -> SdkValue {
    SdkValue::map([
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
    ])
}

/// Saved details: publication parent/source and their times remain distinct.
pub fn record(store: &Arc<Store>, node: NodeId) -> Result<SdkValue> {
    super::rows::Rows::new(store).details(node)
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
