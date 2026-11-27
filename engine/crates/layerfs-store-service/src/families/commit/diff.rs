//! `commit.diff`: the paths that differ between two Commits or Branches.
//!
//! `ref` and `from` are each a Branch name first and, when no Branch has
//! that name, a Commit id or a prefix of one. Without `from` the other side
//! is the fork point of a Branch, the parent of a Commit, the `base` of a
//! Commit with no parent, and the empty tree for main's first publication;
//! main is compared as its head is. A later page names both sides it
//! compared as `pinTo` and `pinFrom` (`pinned`), so a moved Branch does not
//! change what the pages list.
use crate::families::args::{project, text};
use crate::filesystem::change::run;
use crate::filesystem::compare::diff::{self, within, Request, Within};
use crate::filesystem::root::{self, Snap};
use crate::filesystem::tree::visibility::Visibility;
use crate::filesystem::tree::walk::Walker;
use crate::history::resolve::{self, Named};
use crate::history::{listing, saved::reply_cost};
use crate::lookup::{self as args, node_text, Answer};
use crate::store::{Store, StorePorts};
use crate::StoreService;
use layerfs_content::compare::{ChangeKind, PathChange};
use layerfs_history::{LayerStackRecord, NodeId};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `diff({ ref, from?, path?, hunks?, after?, limit?, pinned?, pinTo?,
/// pinFrom? })`: one page of the changes from `from` to `ref`, paths only
/// unless `hunks`, after the path `after`.
pub fn diff(service: &StoreService, args: &SdkValue) -> Answer {
    let store = &service.store;
    let stack = project(service, args)?;
    let (to, from) = sides(store, &stack, args)?;
    let scope = args.get_text("path").map(run::path).transpose()?;
    let after = args.get_text("after").map(run::path).transpose()?;
    let request = Request {
        limit: listing::limit(args)?,
        pos: 0,
        cursor: after,
        hunks: args.get_bool("hunks"),
        scope,
    };
    let reader = root::reader(store, &stack);
    let to = root::resolve(store, &stack, &reader, &node_text(to))?;
    let (items, next, from) = match from {
        Some(from) => {
            let from = root::resolve(store, &stack, &reader, &node_text(from))?;
            let (items, next) = diff::changes(store, &reader, &from, &to, Ok(request))?;
            (items, next.cursor, Some(from))
        }
        None => {
            let (items, next) = added(store, &reader, &to, &request)?;
            (items, next, None)
        }
    };
    let rows = items.into_iter().map(row).collect();
    let root = |snap: &Snap| SdkValue::text(args::hex(snap.root.as_bytes()));
    root::answer(vec![
        ("items", SdkValue::List(rows)),
        ("next", next),
        (
            "from",
            SdkValue::opt_text(from.as_ref().map(|s| node_text(s.node))),
        ),
        ("to", SdkValue::text(node_text(to.node))),
        ("fromRoot", from.as_ref().map_or(SdkValue::Null, root)),
        ("toRoot", root(&to)),
    ])
}

/// The node `ref` stands at and the node it is compared from (none: the
/// empty tree): the pinned ones of a later page, else as named.
fn sides(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    args: &SdkValue,
) -> Result<(NodeId, Option<NodeId>)> {
    let node = |text: &str| resolve::commit(store, stack, text).map(|saved| saved.node());
    if args.get_bool("pinned") {
        let to = node(text(args, "pinTo")?)?;
        let from = args.get_text("pinFrom").map(node).transpose()?;
        return Ok((to, from));
    }
    let named = resolve::reference(store, stack, text(args, "ref")?)?;
    let to = named.node(stack);
    let from = match args.get_text("from") {
        Some(from) => Some(resolve::reference(store, stack, from)?.node(stack)),
        None => match &named {
            Named::Branch(branch) => Some(
                branch
                    .forked_from
                    .unwrap_or(NodeId::Layer(branch.base_layer)),
            ),
            Named::Main => resolve::saved(store, stack, to)?.before(),
            Named::Node(saved) => saved.before(),
        },
    };
    Ok((to, from))
}

/// The page's rows when the other side is the empty tree: every path of
/// `to` in the scope, added, after the cursor.
fn added(
    store: &Arc<Store>,
    reader: &StorePorts,
    to: &Snap,
    request: &Request,
) -> Result<(Vec<SdkValue>, SdkValue)> {
    let scope = request.scope.as_deref().unwrap_or("/");
    let seen = Visibility::for_snap(store, to)?;
    let exists = root::entry(reader, &to.tree, scope)?.is_some();
    if !exists || seen.hidden(scope).is_some() {
        return Ok((Vec::new(), SdkValue::Null));
    }
    let mut walk = Walker::after(reader, to, scope, request.cursor.as_deref())?;
    let (mut items, mut spent, mut budget) = (
        Vec::new(),
        0usize,
        crate::filesystem::tree::positional::REPLY_BUDGET,
    );
    let mut more = false;
    while let Some((path, entry)) = walk.next()? {
        if seen.hidden(&path).is_some() || within(&path, request.scope.as_deref()) != Within::In {
            continue;
        }
        let change = PathChange {
            path: path.clone(),
            kind: ChangeKind::Added,
            left: None,
            right: Some(entry),
        };
        let Some(item) = diff::change(reader, &change, &mut budget, false)? else {
            continue;
        };
        let cost = reply_cost(&item);
        if items.len() == request.limit || (!items.is_empty() && spent + cost > listing::PAGE_BYTES)
        {
            more = true;
            break;
        }
        spent += cost;
        items.push(item);
    }
    let next = match (more, items.last()) {
        (true, Some(last)) => last.get("path").cloned().unwrap_or_default(),
        _ => SdkValue::Null,
    };
    Ok((items, next))
}

/// A change as a row: its path, how it changed and what differs; `hunks`
/// and `hunksOmitted` answer only when the lines were asked for.
fn row(item: SdkValue) -> SdkValue {
    let take = |key: &str| item.get(key).cloned().unwrap_or_default();
    let change = item.get_text("kind").unwrap_or("").to_ascii_lowercase();
    SdkValue::map([
        ("path", take("path")),
        ("change", SdkValue::Text(change)),
        ("type", take("type")),
        ("content", take("content")),
        ("metadata", take("metadata")),
        ("identity", take("identity")),
        ("hunks", take("hunks")),
        ("hunksOmitted", take("hunksOmitted")),
    ])
}
