//! Bounded matching-result pages of literal saved-path changes.
use super::{
    file_facts, query,
    revision_pages::{self, Boundary, Key},
    snapshot,
};
use crate::history::{lookup as args, saved};
use crate::operator::files::{entry, revision};
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::NodeId;
use std::sync::Arc;

fn parent(
    store: &Arc<Store>,
    node: NodeId,
    provenance: bool,
) -> Result<Option<NodeId>, SdkRefusal> {
    if provenance {
        return Ok(revision(store, node)?.1);
    }
    Ok(match node {
        NodeId::Commit(id) => {
            let r = args::commit(store, id)?;
            Some(r.parent.map_or(NodeId::Layer(r.base_layer), NodeId::Commit))
        }
        NodeId::Layer(id) => args::layer(store, id)?.parent.map(NodeId::Layer),
    })
}

/// A complete page or Capacity: never advance past unreturned matches.
pub fn list(
    store: &Arc<Store>,
    reader: &StorePorts,
    start: NodeId,
    path: &str,
    traversal: &str,
    page: query::ResultPage,
    branch: &str,
) -> args::Answer {
    let provenance = match traversal {
        "provenance" => true,
        "branch" => false,
        _ => return Err(args::invalid("traversal must be provenance or branch")),
    };
    let key = Key {
        start,
        path,
        traversal,
        branch,
        size: page.limit,
    };
    let cached = revision_pages::find(store, &key, page.skip);
    let initial = if !provenance && branch != "main" && matches!(start, NodeId::Layer(_)) {
        None
    } else {
        Some(start)
    };
    let Boundary {
        next: initial,
        candidate,
    } = cached.unwrap_or(Boundary {
        next: initial,
        candidate: 0,
    });
    let replay_skip = if cached.is_some() { 0 } else { page.skip };
    let (mut next, mut examined, mut matches, mut items, mut more) =
        (initial, 0usize, 0u64, Vec::new(), false);
    let mut boundary = Boundary {
        next: None,
        candidate,
    };
    while let Some(node) = next {
        if examined as u64 == query::MAXIMUM_QUERY_WORK {
            return Err(args::refuse(
                "Capacity",
                "revision query work bound reached",
                SdkValue::Null,
            ));
        }
        examined += 1;
        let previous = parent(store, node, provenance)?;
        let now = entry(reader, args::root(store, node)?, path)?;
        let before = previous
            .map(|p| entry(reader, args::root(store, p)?, path))
            .transpose()?
            .flatten();
        next = if !provenance && branch != "main" && matches!(previous, Some(NodeId::Layer(_))) {
            None
        } else {
            previous
        };
        let (content, metadata, identity) = file_facts::changes(reader, before, now)?;
        if before.is_none() && now.is_none()
            || before.is_some() && now.is_some() && !content && !metadata && !identity
        {
            continue;
        }
        if matches < replay_skip {
            matches += 1;
            continue;
        }
        if items.len() == page.limit {
            more = true;
            boundary = Boundary {
                next: Some(node),
                candidate: candidate + examined as u64 - 1,
            };
            break;
        }
        let record = saved::record(store, node)?;
        items.push(SdkValue::map([
            (
                "candidateIndex",
                SdkValue::Int((candidate + examined as u64 - 1) as i64),
            ),
            ("ref", saved::reference(store, node)?),
            (
                "comparedTo",
                previous
                    .map(|p| saved::reference(store, p))
                    .transpose()?
                    .unwrap_or_default(),
            ),
            (
                "kind",
                SdkValue::text(match (before, now) {
                    (None, _) => "Added",
                    (_, None) => "Removed",
                    _ => "Modified",
                }),
            ),
            ("before", file_facts::facts(reader, before)?),
            ("after", file_facts::facts(reader, now)?),
            (
                "changes",
                file_facts::flags(content, metadata, identity, None),
            ),
            (
                "message",
                record.get("message").cloned().unwrap_or_default(),
            ),
            (
                "recordedAt",
                record.get("time").cloned().unwrap_or_default(),
            ),
            (
                "timeStatus",
                record.get("timeStatus").cloned().unwrap_or_default(),
            ),
        ]));
    }
    let count = items.len();
    let answer = snapshot::answer(vec![
        ("kind", SdkValue::text("ok")),
        ("at", saved::reference(store, start)?),
        ("path", SdkValue::text(path)),
        ("traversal", SdkValue::text(traversal)),
        ("items", SdkValue::List(items)),
        ("page", SdkValue::Int(page.number as i64)),
        ("pageSize", SdkValue::Int(page.limit as i64)),
        ("hasMore", SdkValue::Bool(more)),
        ("endOfQuery", SdkValue::Bool(!more)),
        (
            "scan",
            SdkValue::map([
                ("complete", SdkValue::Bool(true)),
                ("candidatesExamined", SdkValue::Int(examined as i64)),
                ("reasons", SdkValue::List(Vec::new())),
            ]),
        ),
    ])?;
    if file_facts::value_bytes(&answer) > super::window::BYTES {
        return Err(query::capacity(
            "revision page exceeds reply budget; reduce page_size",
        ));
    }
    revision_pages::keep(store, &key, page.skip + count as u64, boundary);
    Ok(answer)
}
