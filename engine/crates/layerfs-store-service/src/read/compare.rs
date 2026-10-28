//! Differences between saved snapshots: two files, two Commits, two Branches.
//!
//! Tree differences use Content's bounded directory listings and grouped inode
//! demands; file differences use its bounded line difference. A directory whose
//! only change is its children is not listed, as in the simulator. Every answer
//! fits one control record: tree differences resume after `nextPagePos`, and a file's
//! lines that do not fit the remaining budget are left out (`hunksOmitted`)
//! for a two-file difference to show.
use super::reads::REPLY_BUDGET;
use super::snapshot::{self, answer, existing, kind_name, Kind, Snap};
use crate::history::lookup::{self as args, invalid, refuse, Answer, Arg};
use crate::store::{CatalogHints, Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::tree::{same_inode, same_metadata};
use layerfs_content::compare::{
    line_diff, same_file_bytes, ChangeKind, ChangeWalk, DiffLine, EntryFacts, LineOp, Memo,
    PathChange, Plan,
};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::LayerStackRecord;
use std::sync::Arc;

/// Budget one listed change costs besides its path and lines.
const ITEM_COST: usize = 160;

/// Lines as SDK values, or `None` when they do not fit `budget`.
fn hunks(lines: &[DiffLine], budget: &mut usize) -> Option<SdkValue> {
    let cost = lines.iter().map(|line| line.text.len() + 24).sum::<usize>();
    if cost > *budget {
        return None;
    }
    *budget -= cost;
    Some(SdkValue::List(
        lines
            .iter()
            .map(|line| {
                SdkValue::map([
                    (
                        "op",
                        SdkValue::text(match line.op {
                            LineOp::Same => " ",
                            LineOp::Removed => "-",
                            LineOp::Added => "+",
                        }),
                    ),
                    ("text", SdkValue::text(line.text.as_str())),
                ])
            })
            .collect(),
    ))
}

/// Whether two entries hold the same content: the same kind and root, or two
/// regular files whose different roots hold the same bytes. A root depends on
/// how the file was built, so a file edited away and back can keep its bytes
/// under a new root (#352 F-073).
fn same_content(
    reader: &StorePorts,
    left: &EntryFacts,
    right: &EntryFacts,
) -> Result<bool, SdkRefusal> {
    if left.value.kind != right.value.kind {
        return Ok(false);
    }
    if left.value.content_root == right.value.content_root {
        return Ok(true);
    }
    if left.value.kind != InodeKind::RegularFile {
        return Ok(false);
    }
    crate::telemetry::timed("sdk.compare", |timing| {
        same_file_bytes(
            reader,
            left.value.content_root,
            right.value.content_root,
            timing.child("content.compare"),
        )
    })
    .map_err(args::content)
}

/// `fs.diff_files({ left: { path, commit? }, right: { path, commit? } },
/// { branch })`.
/// One side of a two-file difference: a checked path at a snapshot name.
pub struct Selector<'a> {
    pub path: String,
    pub at: &'a str,
}

/// The line difference of two regular files, each at its own snapshot; the
/// left side is resolved before the right one is used.
pub fn diff_files(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    left: Arg<Selector<'_>>,
    right: Arg<Selector<'_>>,
) -> Answer {
    let reader = snapshot::reader(store, stack);
    let side = |selector: Arg<Selector<'_>>| -> Result<(String, Snap, EntryFacts, Option<String>), SdkRefusal> {
        let Selector { path, at } = selector?;
        let snap = snapshot::resolve(store, stack, &reader, at)?;
        let entry = existing(reader.as_ref(), &snap, &path)?;
        if entry.value.kind == InodeKind::Directory {
            return Err(refuse(
                "IsDirectory",
                format!("{path} is a directory"),
                SdkValue::map([("path", SdkValue::text(path.as_str()))]),
            ));
        }
        let content = snapshot::text(&reader, Some(entry))?;
        Ok((path, snap, entry, content))
    };
    let (left, right) = (side(left)?, side(right)?);
    let show = |(path, snap, entry, _): &(String, Snap, EntryFacts, Option<String>)| {
        SdkValue::map([
            ("path", SdkValue::text(path.as_str())),
            ("snapshot", SdkValue::text(snap.label.as_str())),
            ("inode", SdkValue::Int(entry.serial as i64)),
        ])
    };
    let same_inode = left.2.serial == right.2.serial;
    let (kind, equal, lines) = match (&left.3, &right.3) {
        (Some(l), Some(r)) if l == r => ("Equal", true, Some(SdkValue::List(Vec::new()))),
        (Some(l), Some(r)) => {
            let mut budget = REPLY_BUDGET;
            ("Text", false, hunks(&line_diff(l, r), &mut budget))
        }
        _ => {
            let equal = same_content(&reader, &left.2, &right.2)?;
            (
                if equal { "Equal" } else { "Binary" },
                equal,
                Some(SdkValue::List(Vec::new())),
            )
        }
    };
    answer(vec![
        ("kind", SdkValue::text(kind)),
        ("left", show(&left)),
        ("right", show(&right)),
        ("contentEqual", SdkValue::Bool(equal)),
        ("sameInode", SdkValue::Bool(same_inode)),
        ("hunksOmitted", SdkValue::Bool(lines.is_none())),
        ("hunks", lines.unwrap_or_default()),
    ])
}

fn facts(entry: Option<EntryFacts>) -> SdkValue {
    entry.map_or(SdkValue::Null, |entry| {
        SdkValue::map([
            ("type", SdkValue::text(kind_name(entry.value.kind))),
            ("inode", SdkValue::Int(entry.serial as i64)),
        ])
    })
}

/// One listed change, or `None` for a directory whose children changed.
fn change(
    reader: &StorePorts,
    change: &PathChange,
    budget: &mut usize,
    with_hunks: bool,
) -> Result<Option<SdkValue>, SdkRefusal> {
    let kind = match change.kind {
        ChangeKind::Added => "Added",
        ChangeKind::Removed => "Removed",
        ChangeKind::Modified => "Modified",
    };
    let shown = change.right.or(change.left).map(|entry| entry.value.kind);
    let (mut content, mut identity, mut metadata, mut lines) =
        (false, false, false, SdkValue::Null);
    let mut work = layerfs_content::filesystem::attributes::AttributeReadWork::default();
    let mut omitted = false;
    if let (Some(l), Some(r)) = (change.left, change.right) {
        let directories = l.value.kind == InodeKind::Directory && l.value.kind == r.value.kind;
        identity = l.serial != r.serial;
        metadata = !same_metadata(
            reader,
            l.value.kind,
            l.value.metadata_root,
            r.value.metadata_root,
            &mut work,
        )
        .map_err(args::content)?;
        if directories && !identity && !metadata {
            return Ok(None);
        }
        content = !directories && !same_content(reader, &l, &r)?;
        if with_hunks
            && content
            && l.value.kind == InodeKind::RegularFile
            && r.value.kind == l.value.kind
        {
            if let (Some(a), Some(b)) = (
                snapshot::text(reader, Some(l))?,
                snapshot::text(reader, Some(r))?,
            ) {
                match hunks(&line_diff(&a, &b), budget) {
                    Some(value) => lines = value,
                    None => omitted = true,
                }
            }
        }
    }
    Ok(Some(SdkValue::map([
        ("path", SdkValue::text(change.path.as_str())),
        ("kind", SdkValue::text(kind)),
        ("type", SdkValue::opt_text(shown.map(kind_name))),
        ("before", facts(change.left)),
        ("after", facts(change.right)),
        ("content", SdkValue::Bool(content)),
        ("identity", SdkValue::Bool(identity)),
        ("metadata", SdkValue::Bool(metadata)),
        ("hunks", lines),
        ("hunksOmitted", SdkValue::Bool(omitted)),
    ])))
}

/// Where a page starts and what it lists: the caller's offset or its cursor
/// (the last path an earlier page listed), and whether line hunks are wanted.
pub struct Request {
    pub limit: usize,
    pub pos: u64,
    pub cursor: Option<String>,
    pub hunks: bool,
}

/// The page after the listed one: offset and cursor, `Null` when complete.
struct Next {
    pos: SdkValue,
    cursor: SdkValue,
}

/// Component order of the walk: each directory before its children, names in
/// byte order.
fn order(a: &str, b: &str) -> std::cmp::Ordering {
    a.split('/').cmp(b.split('/'))
}

/// The listed changes from `left` to `right` after the offset or cursor, within
/// the reply budget, and where the next page starts. Content's change walk reads
/// what changed; without recorded bindings it is the full walk of both trees.
fn changes(
    store: &Arc<Store>,
    reader: &StorePorts,
    left: &Snap,
    right: &Snap,
    request: Arg<Request>,
) -> Result<(Vec<SdkValue>, Next), SdkRefusal> {
    let request = request?;
    if left.root == right.root {
        let done = Next {
            pos: SdkValue::Null,
            cursor: SdkValue::Null,
        };
        return Ok((Vec::new(), done));
    }
    let hints = CatalogHints::new(store.history());
    // One memo for the page: lookups share the B-trees' upper pages.
    let memo = Memo::new(reader);
    let key = (left.root, right.root);
    let plan = match super::plans::find(key) {
        Some(kept) => kept,
        None => {
            let made = Plan::make(&memo, &hints, &left.tree, &right.tree);
            if let Some(error) = hints.failure.take() {
                return Err(args::history(error));
            }
            let plan = made.map_err(args::content)?.map(Arc::new);
            super::plans::keep(key, plan.clone());
            plan
        }
    };
    let first = match &request.cursor {
        Some(cursor) => component(cursor, "/")?,
        None => None,
    };
    let mut walk = match &plan {
        Some(plan) => ChangeWalk::planned(&memo, &left.tree, &right.tree, plan, first.as_ref()),
        None => ChangeWalk::full(&memo, &left.tree, &right.tree, first.as_ref()),
    }
    .map_err(args::content)?;
    emit(reader, &mut walk, &request)
}

/// The component of `cursor` right below directory `path`, when `cursor`
/// lies below it.
fn component(cursor: &str, path: &str) -> Result<Option<PathName>, SdkRefusal> {
    let rest = if path == "/" {
        cursor.strip_prefix('/')
    } else {
        cursor
            .strip_prefix(path)
            .and_then(|rest| rest.strip_prefix('/'))
    };
    match rest.and_then(|rest| rest.split('/').next()) {
        Some(name) if !name.is_empty() => PathName::new(name)
            .map(Some)
            .map_err(|_| invalid("cursor must be a path or null")),
        _ => Ok(None),
    }
}

/// Lists the differing pairs after the offset or cursor, at most `limit` and
/// within budget. A cursor resumes the walk: pairs at or before it are passed
/// over and only the directories on its path are entered, so a later page
/// does not walk what earlier pages listed.
fn emit(
    reader: &StorePorts,
    walk: &mut ChangeWalk<'_>,
    request: &Request,
) -> Result<(Vec<SdkValue>, Next), SdkRefusal> {
    let (limit, pos) = (request.limit, request.pos);
    let skip = if request.cursor.is_some() { 0 } else { pos };
    let (mut items, mut seen, mut budget, mut more) = (Vec::new(), 0u64, REPLY_BUDGET, false);
    let mut work = layerfs_content::filesystem::attributes::AttributeReadWork::default();
    let mut last = None;
    while let Some(pair) = walk.next_pair().map_err(args::content)? {
        let (path, before, after) = &pair;
        if let Some(cursor) = &request.cursor {
            if order(path, cursor) != std::cmp::Ordering::Greater {
                let on_path = cursor == path
                    || cursor
                        .strip_prefix(path.as_str())
                        .is_some_and(|rest| rest.starts_with('/'));
                if on_path {
                    let from = component(cursor, path)?;
                    walk.enter_from(&pair, from.as_ref())
                        .map_err(args::content)?;
                }
                continue;
            }
        }
        let children_only = match (before, after) {
            (Some(x), Some(y))
                if x.serial == y.serial
                    && x.value.kind == InodeKind::Directory
                    && y.value.kind == InodeKind::Directory =>
            {
                same_metadata(
                    reader,
                    x.value.kind,
                    x.value.metadata_root,
                    y.value.metadata_root,
                    &mut work,
                )
                .map_err(args::content)?
            }
            _ => false,
        };
        let pair_differs = match (before, after) {
            (Some(x), Some(y)) if x.serial == y.serial => {
                // Bytes are compared only when the roots differ and nothing else did.
                let same = same_inode(reader, &x.value, &y.value, &mut work)
                    .map_err(args::content)?
                    || (same_metadata(
                        reader,
                        x.value.kind,
                        x.value.metadata_root,
                        y.value.metadata_root,
                        &mut work,
                    )
                    .map_err(args::content)?
                        && same_content(reader, x, y)?);
                !same
            }
            _ => before != after,
        };
        if pair_differs && !children_only {
            if seen < skip {
                seen += 1;
            } else {
                let cost = path.len() + ITEM_COST;
                if items.len() == limit || cost > budget {
                    more = true;
                    break;
                }
                budget -= cost;
                let kind = match (before, after) {
                    (None, _) => ChangeKind::Added,
                    (_, None) => ChangeKind::Removed,
                    _ => ChangeKind::Modified,
                };
                let listed = PathChange {
                    path: path.clone(),
                    kind,
                    left: *before,
                    right: *after,
                };
                if let Some(item) = change(reader, &listed, &mut budget, request.hunks)? {
                    items.push(item);
                    last = Some(path.clone());
                }
            }
        }
        walk.enter(&pair).map_err(args::content)?;
    }
    let next = Next {
        pos: args::next_page(pos, items.len(), more),
        cursor: match (more, last) {
            (true, Some(path)) => SdkValue::Text(path),
            (true, None) => request
                .cursor
                .clone()
                .map_or(SdkValue::Null, SdkValue::Text),
            (false, _) => SdkValue::Null,
        },
    };
    Ok((items, next))
}

fn listed(
    store: &Arc<Store>,
    reader: &StorePorts,
    from: &Snap,
    to: &Snap,
    request: Arg<Request>,
    sides: [SdkValue; 2],
) -> Answer {
    let (items, next) = changes(store, reader, from, to, request)?;
    let [from, to] = sides;
    answer(vec![
        ("kind", SdkValue::text("ok")),
        ("from", from),
        ("to", to),
        ("items", SdkValue::List(items)),
        (
            "hasMore",
            SdkValue::Bool(!matches!(next.pos, SdkValue::Null)),
        ),
        ("nextPagePos", next.pos),
        ("nextCursor", next.cursor),
    ])
}

/// The changes from Commit `from` to Commit `to`, one page.
pub fn diff_commits(
    store: &Arc<Store>,
    reader: &StorePorts,
    from: &Snap,
    to: &Snap,
    request: Arg<Request>,
) -> Answer {
    if from.kind != Kind::Commit || to.kind != Kind::Commit {
        return Err(invalid("diff_commits takes two saved Commits"));
    }
    let side = |snap: &Snap| {
        SdkValue::map([
            ("commit", SdkValue::text(args::node_text(snap.node))),
            ("root", SdkValue::text(args::hex(snap.root.as_bytes()))),
        ])
    };
    listed(store, reader, from, to, request, [side(from), side(to)])
}

/// The changes from Branch (or main) `from` to `to`, one page; the names
/// label the sides.
pub fn diff_branches(
    store: &Arc<Store>,
    reader: &StorePorts,
    (from, from_name): (&Snap, &str),
    (to, to_name): (&Snap, &str),
    request: Arg<Request>,
) -> Answer {
    let named = |snap: &Snap| matches!(snap.kind, Kind::Main | Kind::Branch);
    if !named(from) || !named(to) {
        return Err(invalid("diff_branches takes Branches or main"));
    }
    let side = |snap: &Snap, name: &str| {
        SdkValue::map([
            ("ref", SdkValue::text(name)),
            ("label", SdkValue::text(snap.label.as_str())),
            ("root", SdkValue::text(args::hex(snap.root.as_bytes()))),
        ])
    };
    let sides = [side(from, from_name), side(to, to_name)];
    listed(store, reader, from, to, request, sides)
}
