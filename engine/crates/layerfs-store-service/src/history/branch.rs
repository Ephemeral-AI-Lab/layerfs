//! Branch history: list, fork, list Commits, merge and publish, typed.
//!
//! A merge of `a` and `b` into `name` creates Branch `name` and never changes `a` or
//! `b`. When one side contains the other it is a fast-forward to the newer
//! node; otherwise the sides are merged against their merge base in one Save,
//! with `a`'s `/.lww`, and a clean result becomes one Commit with two parents.
//! A conflict creates nothing. Publishing is overwrite-only and reports
//! the main Layers it skipped.
use super::lookup::{
    self as args, branch_node, commit_value, hex, history, invalid, layer_value, node_text, refuse,
    root, short, Answer,
};
use crate::store::{CommitError, Store};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    filesystem::FilesystemRootId,
    merge::{merge_roots, MergeInput, MergeUnit, RulesFrom, Units},
};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, BranchMergeRequest, CommitHistoryRequest,
    CommitMeta, CommitVia, ForkRequest, ForkSource, LayerId, LayerStackRecord, MergeCommit,
    MergeHead, MergeSide, NodeId, MAXIMUM_PAGE_RECORDS,
};
use layerfs_workspace::InodeSerials;
use std::sync::Arc;

/// The name of the project's line of Layers.
pub const MAIN: &str = "main";

/// Serials one merge reservation takes.
const MERGE_SERIALS: u64 = 64;

/// One merge side by name.
fn side(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
) -> Result<
    (MergeSide, NodeId, Option<layerfs_history::BranchRecord>),
    layerfs_bridge::control::SdkRefusal,
> {
    if name == MAIN {
        return Ok((MergeSide::Main, NodeId::Layer(stack.head_layer), None));
    }
    let branch = args::branch(store, stack, name)?;
    Ok((
        MergeSide::Branch(branch.id),
        branch_node(&branch),
        Some(branch),
    ))
}

/// Every Branch, and main, one page of `limit` from `pos`; with `name`,
/// that one Branch from the name index, never a listing (#352 009.a1).
pub fn list(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    limit: usize,
    pos: u64,
    name: Option<&str>,
) -> Answer {
    if let Some(name) = name {
        let found = if name == MAIN {
            Some(main_item(stack))
        } else {
            match args::branch(store, stack, name) {
                Ok(branch) => Some(item(&branch)),
                Err(refusal) if refusal.code == "NoSuchBranch" => None,
                Err(refusal) => return Err(refusal),
            }
        };
        let items: Vec<SdkValue> = found
            .into_iter()
            .filter(|_| pos == 0 && limit > 0)
            .collect();
        return Ok(SdkValue::map([
            ("items", SdkValue::List(items)),
            ("hasMore", SdkValue::Bool(false)),
            ("nextPagePos", SdkValue::Null),
        ]));
    }
    let mut page = layerfs_history::Page {
        cursor: None,
        limit: MAXIMUM_PAGE_RECORDS,
    };
    let (mut seen, mut items, mut more, mut spent) = (0u64, Vec::new(), false, 0);
    // Main participates in the same byte-name ordering as the Branches.
    let mut main = Some(main_item(stack));
    let mut add = |value: SdkValue| -> bool {
        if seen < pos {
            seen += 1;
            return true;
        }
        let cost = value.get_text("name").map_or(0, str::len) + 512;
        if items.len() == limit || spent + cost > crate::read::reads::REPLY_BUDGET {
            more = true;
            return false;
        }
        spent += cost;
        items.push(value);
        true
    };
    'pages: loop {
        let found = store.history().branches(stack.id, &page).map_err(history)?;
        for branch in found.records {
            if branch.name.as_str() == MAIN {
                continue;
            }
            if branch.name.as_str() > MAIN {
                if let Some(value) = main.take() {
                    if !add(value) {
                        break 'pages;
                    }
                }
            }
            if !add(item(&branch)) {
                break 'pages;
            }
        }
        match found.continuation {
            Some(cursor) => page.cursor = Some(cursor),
            None => {
                if let Some(value) = main.take() {
                    add(value);
                }
                break;
            }
        }
    }
    let next = args::next_page(pos, items.len(), more);
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]))
}

/// The listed form of the project's main line.
fn main_item(stack: &layerfs_history::LayerStackRecord) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(MAIN)),
        ("id", SdkValue::Null),
        ("base", SdkValue::text(hex(stack.head_layer.as_slice()))),
        ("head", SdkValue::Null),
    ])
}

/// The listed form of one Branch.
fn item(branch: &layerfs_history::BranchRecord) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(branch.name.as_str())),
        ("id", SdkValue::text(hex(branch.id.as_slice()))),
        ("base", SdkValue::text(hex(branch.base_layer.as_slice()))),
        (
            "head",
            SdkValue::opt_text(branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
        (
            "forkedFrom",
            branch
                .forked_from
                .map_or(SdkValue::Null, |node| SdkValue::text(node_text(node))),
        ),
    ])
}

/// A new Branch `name` at `from` (main when absent): a Branch, main, or a
/// Commit or Layer identity. Metadata only.
pub fn fork(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
    from: Option<&str>,
) -> Answer {
    if name == MAIN {
        return Err(refuse(
            "ReservedName",
            "\"main\" names the project's main line",
            SdkValue::Null,
        ));
    }
    let from = from.unwrap_or(MAIN);
    let source = if from == MAIN {
        ForkSource::Layer(stack.head_layer)
    } else if let Ok(branch) = args::branch(store, stack, from) {
        match branch.head_commit {
            Some(commit) => ForkSource::Commit {
                branch: branch.id,
                commit,
            },
            None => ForkSource::Layer(branch.base_layer),
        }
    } else {
        match args::node(from)? {
            NodeId::Layer(layer) => ForkSource::Layer(layer),
            NodeId::Commit(commit) => {
                let record = args::commit(store, commit)?;
                let branch = record
                    .meta
                    .branch
                    .ok_or_else(|| invalid("the Commit names no Branch to read it through"))?;
                ForkSource::Commit { branch, commit }
            }
        }
    };
    let name = args::name(name)?;
    if args::branch(store, stack, name.as_str()).is_ok() {
        return Err(refuse(
            "NameTaken",
            format!("Branch {} already exists", name.as_str()),
            SdkValue::Null,
        ));
    }
    let snapshot = store
        .history()
        .fork(&ForkRequest {
            stack: stack.id,
            branch: BranchId::named(stack.id, &name),
            name,
            source,
        })
        .map_err(history)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Forked")),
        ("branch", SdkValue::text(snapshot.branch.name.as_str())),
        (
            "base",
            SdkValue::text(hex(snapshot.branch.base_layer.as_slice())),
        ),
        (
            "head",
            SdkValue::opt_text(snapshot.branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
    ]))
}

/// A Branch's Commits, newest first: one page of `limit` from `pos`.
pub fn list_commits(
    store: &Arc<Store>,
    branch: &layerfs_history::BranchRecord,
    limit: usize,
    pos: u64,
) -> Answer {
    let (mut cursor, mut seen, mut items, mut spent, mut more) = (None, 0u64, Vec::new(), 0, false);
    'pages: loop {
        let page = store
            .history()
            .commit_history(&CommitHistoryRequest {
                branch: branch.id,
                start: branch.head_commit,
                cursor,
                limit: MAXIMUM_PAGE_RECORDS,
            })
            .map_err(history)?;
        for record in page.records {
            if seen < pos {
                seen += 1;
                continue;
            }
            let cost = record.meta.message.as_ref().map_or(0, String::len) + 1024;
            if items.len() == limit || spent + cost > crate::read::reads::REPLY_BUDGET {
                more = true;
                break 'pages;
            }
            spent += cost;
            items.push(commit_value(&record, Some(branch.name.as_str())));
        }
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    let next = args::next_page(pos, items.len(), more);
    Ok(SdkValue::map([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]))
}

/// Merges `left` and `right` (Branch names or main) into a new Branch
/// `name`; `message` names the merge Commit. A unit both sides changed is
/// taken whole from the side with the later write (#352 F-057).
pub fn merge(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    left: &str,
    right: &str,
    name: &str,
    message: Option<&str>,
    units: &[MergeUnit],
) -> Answer {
    if left == right {
        return Err(invalid("merge takes two different sources"));
    }
    let admitted = Units::new(units).map_err(invalid)?;
    if name == MAIN {
        return Err(refuse(
            "ReservedName",
            "\"main\" names the project's main line",
            SdkValue::Null,
        ));
    }
    if args::branch(store, stack, name).is_ok() {
        return Err(refuse(
            "NameTaken",
            format!("Branch {name} already exists"),
            SdkValue::Null,
        ));
    }
    let name = args::name(name)?;
    let (left_side, ln, left_branch) = side(store, stack, left)?;
    let (right_side, rn, _) = side(store, stack, right)?;
    let sources = SdkValue::List(vec![
        SdkValue::map([
            ("ref", SdkValue::text(left)),
            ("node", SdkValue::text(node_text(ln))),
        ]),
        SdkValue::map([
            ("ref", SdkValue::text(right)),
            ("node", SdkValue::text(node_text(rn))),
        ]),
    ]);
    let catalog = store.history();
    let newer = if catalog.is_ancestor(rn, ln).map_err(history)? {
        Some((ln, rn))
    } else if catalog.is_ancestor(ln, rn).map_err(history)? {
        Some((rn, ln))
    } else {
        None
    };
    let request = |head| BranchMergeRequest {
        stack: stack.id,
        branch: BranchId::named(stack.id, &name),
        name: name.clone(),
        sides: [(left_side, ln), (right_side, rn)],
        head,
    };
    if let Some((newer, older)) = newer {
        let snapshot = catalog
            .merge_branch(&request(MergeHead::Node(newer)))
            .map_err(history)?;
        return Ok(SdkValue::map([
            ("kind", SdkValue::text("Merged")),
            ("fastForward", SdkValue::Bool(true)),
            ("branch", SdkValue::text(name.as_str())),
            (
                "head",
                SdkValue::opt_text(snapshot.branch.head_commit.map(|id| hex(id.as_slice()))),
            ),
            (
                "root",
                SdkValue::text(hex(snapshot.effective_root.as_bytes())),
            ),
            ("sources", sources),
            ("mergeBase", SdkValue::text(node_text(older))),
            ("conflicts", SdkValue::List(Vec::new())),
            ("lww", SdkValue::List(Vec::new())),
            ("conflictsTruncated", SdkValue::Bool(false)),
            ("lwwTruncated", SdkValue::Bool(false)),
            ("units", SdkValue::List(Vec::new())),
            ("unitsTruncated", SdkValue::Bool(false)),
        ]));
    }
    let base_node = catalog
        .merge_base(ln, rn)
        .map_err(history)?
        .ok_or_else(|| refuse("NoMergeBase", "the sides share no history", SdkValue::Null))?;
    let labels = [
        format!("{left}@{}", short(ln)),
        format!("{right}@{}", short(rn)),
    ];
    let storage = store.producer().map_err(args::failed)?;
    let save = storage.begin_save().map_err(args::failed)?;
    let ports = store.ports(stack.scope);
    let mut range = (0u64, 0u64);
    let mut fresh = || {
        if range.0 == range.1 {
            let (start, count) = ports.reserve(MERGE_SERIALS).map_err(|_| {
                layerfs_content::ContentError::ResourceUnavailable {
                    what: "merge inode serials",
                }
            })?;
            range = (start, start.saturating_add(count));
        }
        range.0 += 1;
        Ok(range.0 - 1)
    };
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let hints = crate::store::CatalogHints::new(catalog);
    let (base_root, ours) = (
        FilesystemRootId(root(store, base_node)?),
        FilesystemRootId(root(store, ln)?),
    );
    let mut sides = super::units::Sides {
        base: base_root,
        ours,
        theirs: FilesystemRootId(root(store, rn)?),
        policy,
        capacities: &capacities,
    };
    let mut decided = Vec::new();
    if !admitted.is_empty() {
        let units = crate::telemetry::timed("branch.merge.units", |timing| {
            super::units::decide(store, &save, &hints, &admitted, &sides, &mut fresh, timing)
        })?;
        (sides.ours, sides.theirs, decided) = (units.ours, units.theirs, units.reports);
    }
    let input = MergeInput {
        base: base_root,
        ours: sides.ours,
        theirs: sides.theirs,
        labels: [&labels[0], &labels[1]],
        rules_from: RulesFrom::Ours,
        policy,
        capacities: &capacities,
        hints: Some(&hints),
        decide_from: layerfs_content::merge::Side::Ours,
        bound: None,
    };
    let merged = crate::telemetry::timed("branch.merge", |timing| {
        store.merge_records(|records| {
            merge_roots(
                &save,
                &mut save.sink(),
                &input,
                &mut fresh,
                Some(records),
                timing,
            )
        })
    });
    let (merged, scratch) = merged.map_err(args::failed)?;
    let merged = merged.map_err(|error| match (hints.failure.take(), scratch) {
        (Some(failure), _) => history(failure),
        (None, Some(failure)) => args::failed(failure),
        (None, None) => args::content(error),
    })?;
    store.note_merge(&merged);
    let lww = layerfs_bridge::sdk_values::lww(&merged.lww);
    if !merged.clean() {
        // Nothing is created: the unfinished Save is dropped.
        drop(save);
        return Err(refuse(
            "Conflict",
            format!(
                "{left} + {right} conflict in {} file(s); nothing was created",
                merged.conflicts.len()
            ),
            SdkValue::map([
                ("sources", sources),
                ("mergeBase", SdkValue::text(node_text(base_node))),
                (
                    "conflicts",
                    layerfs_bridge::sdk_values::conflicts(&merged.conflicts),
                ),
                ("lww", lww),
                (
                    "conflictsTruncated",
                    SdkValue::Bool(layerfs_bridge::sdk_values::conflicts_truncated(
                        &merged.conflicts,
                    )),
                ),
                (
                    "lwwTruncated",
                    SdkValue::Bool(layerfs_bridge::sdk_values::lww_truncated(&merged.lww)),
                ),
                ("units", layerfs_bridge::sdk_values::units(&decided)),
                (
                    "unitsTruncated",
                    SdkValue::Bool(layerfs_bridge::sdk_values::units_truncated(&decided)),
                ),
            ]),
        ));
    }
    let known = store.logical_size(ln).map_err(history)?;
    let saved = save.finish().map_err(args::failed)?;
    store.note_save(&saved);
    // Counted over finished, reference-closed objects, as a Commit counts.
    let reader = storage.reader().map_err(args::failed)?;
    // File lengths from the Store catalogue a window at a time, as a Commit
    // counts them, never from each changed file's root (#364).
    let mut refused = None;
    let logical = layerfs_content::filesystem::statistics::logical_size_after_with(
        &reader,
        ours,
        merged.root,
        known,
        &mut |roots| {
            reader.file_lengths(roots).map_err(|error| {
                refused = Some(error);
                layerfs_content::ContentError::OutputRejected
            })
        },
    )
    .map_err(|error| match refused.take() {
        Some(error) => args::failed(error),
        None => args::content(error),
    })?;
    crate::store::record_new_bindings(&reader, catalog, Some(ours), merged.root).map_err(
        |error| match error {
            CommitError::History(error) => history(error),
            CommitError::Content { error, .. } => args::content(error),
            error => args::failed(format!("{error:?}")),
        },
    )?;
    let base = match (left_side, right_side, rn) {
        (MergeSide::Main, _, _) => match ln {
            NodeId::Layer(layer) => layer,
            NodeId::Commit(_) => return Err(invalid("main is a Layer")),
        },
        (_, MergeSide::Main, NodeId::Layer(layer)) => layer,
        _ => left_branch
            .as_ref()
            .map_or(stack.head_layer, |b| b.base_layer),
    };
    let publish = request(MergeHead::Commit(Box::new(MergeCommit {
        base,
        parent: match ln {
            NodeId::Commit(id) => Some(id),
            NodeId::Layer(_) => None,
        },
        second_parent: rn,
        root: merged.root.0,
        meta: CommitMeta {
            message: Some(
                message.map_or_else(|| format!("Merge {left} and {right}"), str::to_owned),
            ),
            lww: merged.lww.clone(),
            units: units.to_vec(),
            statistics: layerfs_history::CommitStatistics {
                logical,
                new_canonical: saved.canonical_bytes,
                new_stored: saved.stored_bytes,
                new_metadata: saved.metadata_bytes,
            },
            ..CommitMeta::new(CommitVia::Merge)
        },
    })));
    // The merge's one publish write, timed on its own: its writer hold.
    let snapshot =
        crate::telemetry::timed("branch.merge.publish", |_| catalog.merge_branch(&publish))
            .map_err(history)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Merged")),
        ("fastForward", SdkValue::Bool(false)),
        ("branch", SdkValue::text(name.as_str())),
        (
            "head",
            SdkValue::opt_text(snapshot.branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
        ("root", SdkValue::text(hex(merged.root.0.as_bytes()))),
        ("sources", sources),
        ("mergeBase", SdkValue::text(node_text(base_node))),
        ("conflicts", SdkValue::List(Vec::new())),
        ("lww", lww),
        ("conflictsTruncated", SdkValue::Bool(false)),
        (
            "lwwTruncated",
            SdkValue::Bool(layerfs_bridge::sdk_values::lww_truncated(&merged.lww)),
        ),
        ("units", layerfs_bridge::sdk_values::units(&decided)),
        (
            "unitsTruncated",
            SdkValue::Bool(layerfs_bridge::sdk_values::units_truncated(&decided)),
        ),
    ]))
}

/// Publishes `commit` (the Branch's head when absent) of `branch` as main's
/// next Layer: overwrite-only.
pub fn publish(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    branch: &layerfs_history::BranchRecord,
    commit: Option<NodeId>,
) -> Answer {
    let commit = match commit {
        Some(node) => match node {
            NodeId::Commit(commit) => commit,
            NodeId::Layer(_) => return Err(invalid("publish takes a Commit")),
        },
        None => branch.head_commit.ok_or_else(|| {
            refuse(
                "NoCommit",
                format!("{} has no saved Commit to publish", branch.name.as_str()),
                SdkValue::Null,
            )
        })?,
    };
    let outcome = store
        .history()
        .add_layer(&AddLayerRequest {
            stack: stack.id,
            branch: branch.id,
            commit,
            expected_stack_head: stack.head_layer,
            expected_branch_base: branch.base_layer,
        })
        .map_err(history)?;
    Ok(match outcome {
        AddLayerOutcome::Added { layer } => {
            // The overwritten Layers are read after the publication, one
            // page; operator `store.overwritten` pages the rest.
            let (overwrote, more) = overwritten(store, layer.id, 0, 50)?;
            SdkValue::map([
                ("kind", SdkValue::text("Added")),
                (
                    "source",
                    SdkValue::map([
                        ("branch", SdkValue::text(branch.name.as_str())),
                        ("commit", SdkValue::text(hex(commit.as_slice()))),
                    ]),
                ),
                (
                    "publishedLayer",
                    layer_value(&layer, Some(branch.name.as_str())),
                ),
                ("basedOn", SdkValue::text(hex(branch.base_layer.as_slice()))),
                (
                    "overwroteCount",
                    SdkValue::Int(layer.overwrote_count as i64),
                ),
                ("overwrote", SdkValue::List(overwrote)),
                ("overwroteHasMore", SdkValue::Bool(more)),
            ])
        }
        AddLayerOutcome::UpToDate { layer } => SdkValue::map([
            ("kind", SdkValue::text("UpToDate")),
            ("layer", SdkValue::text(hex(layer.as_slice()))),
        ]),
        AddLayerOutcome::NoChanges { head } => SdkValue::map([
            ("kind", SdkValue::text("NoChanges")),
            ("headLayer", SdkValue::text(hex(head.as_slice()))),
        ]),
    })
}

/// Up to `limit` Layers `layer` overwrote after `position`, newest first, and
/// whether more follow.
pub(crate) fn overwritten(
    store: &Arc<Store>,
    layer: LayerId,
    position: u64,
    limit: usize,
) -> Result<(Vec<SdkValue>, bool), SdkRefusal> {
    let mut items = Vec::new();
    loop {
        let want = (limit + 1 - items.len()).min(usize::from(MAXIMUM_PAGE_RECORDS));
        let page = store
            .history()
            .overwritten(layer, position + items.len() as u64, want as u16)
            .map_err(history)?;
        let done = page.len() < want;
        items.extend(page.iter().map(|id| SdkValue::text(hex(id.as_slice()))));
        if done || items.len() > limit {
            break;
        }
    }
    let more = items.len() > limit;
    items.truncate(limit);
    Ok((items, more))
}
