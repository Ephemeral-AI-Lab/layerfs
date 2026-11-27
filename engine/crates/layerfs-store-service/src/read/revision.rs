//! Bounded matching-result pages of literal saved-path changes.
use super::{
    file_facts, query,
    revision_pages::{self, Boundary, Key},
    snapshot,
    versions::{Batching, Versions},
    visibility::Visibility,
};
use crate::history::{lookup as args, rows::Rows};
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::EntryFacts;
use layerfs_history::{LayerStackId, NodeId};
use std::sync::Arc;

fn parent(
    rows: &mut Rows<'_>,
    node: NodeId,
    provenance: bool,
) -> Result<Option<NodeId>, SdkRefusal> {
    if provenance {
        return Ok(rows.revision(node)?.1);
    }
    Ok(match rows.record(node)? {
        args::NodeRecord::Commit(r) => {
            Some(r.parent.map_or(NodeId::Layer(r.base_layer), NodeId::Commit))
        }
        args::NodeRecord::Layer(r) => r.parent.map(NodeId::Layer),
    })
}

/// The bytes of one page's rows: the reply budget less room for the
/// header and the continuation. A page that reaches it ends early with a
/// continuation; it is never refused for its size.
const ROW_BYTES: usize = super::window::BYTES - 12 * 1024;

/// The matching-result query and its optional stateless position.
pub struct Request<'a> {
    pub path: &'a str,
    pub traversal: &'a str,
    pub page: query::ResultPage,
    pub branch: &'a str,
    pub continuation: Option<&'a str>,
    pub stack: LayerStackId,
    /// Whether the walk looks its versions up in windows (#818 H5).
    pub batching: Batching,
}

/// A complete page or Capacity: never advance past unreturned matches.
///
/// The walk reads each node's row once, as the previous node's parent, and
/// builds each item's `ref`, `comparedTo` and record from the rows it holds,
/// each Branch name read once per call (#818 H1). Each node's path entry and
/// serial are read once too: a node's parent side is the next node's own
/// (#818 H2).
pub fn list(
    store: &Arc<Store>,
    reader: &StorePorts,
    start: NodeId,
    request: Request<'_>,
) -> args::Answer {
    let Request {
        path,
        traversal,
        mut page,
        branch,
        continuation,
        stack,
        batching,
    } = request;
    let provenance = match traversal {
        "provenance" => true,
        "branch" => false,
        _ => return Err(args::invalid("traversal must be provenance or branch")),
    };
    // §4.3: a hidden anchor refuses; a hidden older version ends the walk
    // instead of being skipped over.
    let visibility = Visibility::for_stack(store, stack, Visibility::node_serial(store, start)?)?;
    let mut rows = Rows::new(store);
    // Every node's objects go through the call's caches and directory memo
    // (#818 H3).
    let reader = &super::cached::Cached::new(reader)?;
    let mut lookups = Versions::new(store, reader, path, batching);
    let anchor = rows.record(start)?.clone();
    // The walk's Commits in windows where the Store batches reads (#818 H4).
    rows.walk(query::MAXIMUM_QUERY_WORK as usize + 1);
    if reader.entry(anchor.root(), path)?.is_some() {
        if let Some(frontier) = visibility.hidden_at(path, visibility.serial()) {
            return Err(Visibility::refusal(path, frontier));
        }
    }
    let key = Key {
        start,
        path,
        traversal,
        branch,
        size: page.limit,
    };
    let query = SdkValue::map([
        ("kind", SdkValue::text("revision")),
        ("start", SdkValue::text(args::node_text(start))),
        ("path", SdkValue::text(path)),
        ("branch", SdkValue::text(branch)),
        ("traversal", SdkValue::text(traversal)),
        ("size", SdkValue::Int(page.limit as i64)),
    ]);
    let resumed = continuation
        .map(|token| super::continuation::decode(store, token, &query))
        .transpose()?;
    let cached = if let Some(state) = &resumed {
        page.number = super::continuation::page(state, page.number)?;
        page.skip = (page.number - 1)
            .checked_mul(page.limit as u64)
            .ok_or_else(|| args::invalid("continuation page overflow"))?;
        Some(Boundary {
            next: state.get_text("next").map(args::node).transpose()?,
            candidate: state
                .get_int("candidate")
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| args::invalid("invalid continuation candidate"))?,
        })
    } else {
        revision_pages::find(store, &key, page.skip)
    };
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
    let (mut spent, mut early) = (0usize, false);
    let mut boundary = Boundary {
        next: None,
        candidate,
    };
    let mut content_budget = file_facts::CONTENT_QUERY_BYTES;
    // The previous node's parent side, which is this node's own (#818 H2):
    // its entry before the hidden filter, and its serial when it exists.
    let mut carried: Option<(NodeId, Option<EntryFacts>, Option<u64>)> = None;
    while let Some(node) = next {
        if examined as u64 == query::MAXIMUM_QUERY_WORK {
            return Err(args::refuse(
                "Capacity",
                "revision query work bound reached",
                SdkValue::Null,
            ));
        }
        examined += 1;
        let previous = parent(&mut rows, node, provenance)?;
        let (now, serial) = match carried.take() {
            Some((held, found, serial)) if held == node => (found, serial),
            _ => {
                let root = rows.record(node)?.root();
                (lookups.entry(&rows, node, root)?, None)
            }
        };
        // A node the marker hides is never a returned version: the walk stops
        // at the first one rather than listing it or walking past it.
        if now.is_some() {
            let serial = match serial {
                Some(serial) => serial,
                None => rows.serial(node)?,
            };
            if visibility.hidden_at(path, serial).is_some() {
                break;
            }
        }
        let mut hidden_before = false;
        let before = previous
            .map(|p| {
                let root = rows.walked(p)?.root();
                let found = lookups.entry(&rows, p, root)?;
                let serial = found.map(|_| rows.serial(p)).transpose()?;
                carried = Some((p, found, serial));
                if serial.is_some_and(|serial| visibility.hidden_at(path, serial).is_some()) {
                    hidden_before = true;
                    Ok(None)
                } else {
                    Ok(found)
                }
            })
            .transpose()?
            .flatten();
        next = if hidden_before
            || (!provenance && branch != "main" && matches!(previous, Some(NodeId::Layer(_))))
        {
            None
        } else {
            previous
        };
        let (content, metadata, identity) =
            file_facts::changes_bounded(reader, before, now, &mut content_budget)?;
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
        let record = rows.details(node)?;
        let item = SdkValue::map([
            (
                "candidateIndex",
                SdkValue::Int((candidate + examined as u64 - 1) as i64),
            ),
            ("ref", rows.reference(node)?),
            (
                "comparedTo",
                previous
                    .map(|p| rows.reference(p))
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
        ]);
        let cost = file_facts::value_bytes(&item);
        if !items.is_empty() && spent + cost > ROW_BYTES {
            (more, early) = (true, true);
            boundary = Boundary {
                next: Some(node),
                candidate: candidate + examined as u64 - 1,
            };
            break;
        }
        spent += cost;
        items.push(item);
    }
    let count = items.len();
    let continuation = if more {
        super::continuation::encode(
            store,
            query,
            SdkValue::map([
                (
                    "next",
                    SdkValue::opt_text(boundary.next.map(args::node_text)),
                ),
                (
                    "candidate",
                    SdkValue::Int(
                        i64::try_from(boundary.candidate)
                            .map_err(|_| query::capacity("continuation candidate overflow"))?,
                    ),
                ),
                (
                    "page",
                    SdkValue::Int(
                        i64::try_from(page.number + 1)
                            .map_err(|_| query::capacity("continuation page overflow"))?,
                    ),
                ),
            ]),
        )?
    } else {
        SdkValue::Null
    };
    let answer = snapshot::answer(vec![
        ("continuation", continuation),
        ("kind", SdkValue::text("ok")),
        ("at", {
            rows.hold(start, anchor, None);
            rows.reference(start)?
        }),
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
    // a page cut short by its bytes does not hold `page.limit` rows: its
    // boundary is no numbered page's
    if !early {
        revision_pages::keep(store, &key, page.skip + count as u64, boundary);
    }
    Ok(answer)
}
