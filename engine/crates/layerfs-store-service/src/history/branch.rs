//! Branch history: list, fork, list Commits, merge and publish, typed.
//!
//! A merge of `a` and `b` into `name` creates Branch `name` and never changes `a` or
//! `b`. When one side contains the other it is a fast-forward to the newer
//! node; otherwise the sides are merged against their merge base in one Save,
//! with `a`'s `/.lww`, and a clean result becomes one Commit with two parents.
//! A conflict creates nothing. Publishing is a compare-and-swap on main's
//! head by default, and reports the main Layers a forced publication skipped.
use super::lookup::{
    self as args, branch_node, commit_value, hex, history, invalid, layer_value, node_text, refuse,
    root, short, Answer,
};
use super::resolve::{self, Named};
use crate::store::{CommitError, Store};
use layerfs_content::{
    filesystem::FilesystemRootId,
    merge::{merge_roots, MergeInput, MergeUnit, RulesFrom, Units},
};
use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, BranchMergeRequest, CommitHistoryRequest, CommitId,
    CommitMeta, CommitVia, ForkRequest, ForkSource, HistoryError, LayerId, LayerStackRecord,
    MergeCommit, MergeHead, MergeSide, NodeId, MAXIMUM_PAGE_RECORDS,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_vfs::InodeSerials;
use std::sync::Arc;

/// The name of the project's line of Layers.
pub const MAIN: &str = "main";

/// Serials one merge reservation takes.
const MERGE_SERIALS: u64 = 64;

/// The conflicting paths a merge's `Conflict` names in `paths`, in order:
/// the call surface's inline list (its first 20), whatever the byte window
/// of `conflicts` held; `total` counts them all.
const CONFLICT_PATHS: usize = 20;

/// One merge side by name.
fn side(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
) -> Result<
    (MergeSide, NodeId, Option<layerfs_history::BranchRecord>),
    layerfs_protocol::sdk::SdkRefusal,
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

/// A new Branch `name` at the head of `from` (main when absent), or at the
/// Commit `at` of its history. `from` is a Branch name first and, when no
/// Branch has it, a Commit id or a prefix; `at` is a Commit id or a prefix.
/// Metadata only.
pub fn fork(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
    from: Option<&str>,
    at: Option<&str>,
) -> Answer {
    if name == MAIN {
        return Err(refuse(
            "ReservedName",
            "\"main\" names the project's main line",
            SdkValue::Map(Vec::new()),
        ));
    }
    let from = from.unwrap_or(MAIN);
    let named = resolve::reference(store, stack, from)?;
    let node = match at {
        Some(at) => {
            let node = resolve::commit(store, stack, at)?.node();
            super::listing::held(store, from, named.node(stack), node)?
        }
        None => named.node(stack),
    };
    let source = match (node, &named) {
        (NodeId::Layer(layer), _) => ForkSource::Layer(layer),
        // a Branch's own head is read through that Branch
        (NodeId::Commit(commit), Named::Branch(branch)) if branch.head_commit == Some(commit) => {
            ForkSource::Commit {
                branch: branch.id,
                commit,
            }
        }
        (NodeId::Commit(commit), _) => {
            let record = args::commit(store, commit)?;
            let branch = record
                .meta
                .branch
                .ok_or_else(|| invalid("the Commit names no Branch to read it through"))?;
            ForkSource::Commit { branch, commit }
        }
    };
    let name = args::name(name)?;
    // A live, retired or deleted holder reaches the typed `NameTaken` from
    // the catalog's collision query instead of a Null detail pre-check.
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
        (
            "commit",
            SdkValue::text(node_text(branch_node(&snapshot.branch))),
        ),
        (
            "root",
            SdkValue::text(hex(snapshot.effective_root.as_bytes())),
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

/// What one merge is asked: `right` merged into `left` (Branch names or
/// main), the result written to `into` when it names a new Branch, else to
/// `left` itself; `expected` (a Commit id or prefix) must be `left`'s head.
pub struct Merge<'a> {
    /// The Branch merged into, whose `/.lww` rules apply.
    pub left: &'a str,
    /// The Branch, or main, merged from.
    pub right: &'a str,
    /// The new Branch the result goes to; `left` moves when absent.
    pub into: Option<&'a str>,
    /// The head `left` must still be at.
    pub expected: Option<&'a str>,
    /// The merge Commit's message.
    pub message: Option<&'a str>,
    /// The merge units (#352 F-057).
    pub units: &'a [MergeUnit],
}

/// Merges `request.right` into `request.left`: into a new Branch `into`
/// (neither side changes), or moving `left` to the result. A unit both
/// sides changed is taken whole from the side with the later write (#352
/// F-057). `left` already holding `right` answers `unchanged` and writes
/// nothing.
pub fn merge(store: &Arc<Store>, stack: &LayerStackRecord, request: &Merge<'_>) -> Answer {
    let name = request.into.unwrap_or(request.left);
    let admission = crate::history::admission::SharedIntent::begin(
        store.history(),
        layerfs_history::records::GcIntentKind::Merge,
        stack,
        Some(name),
    )?;
    match merge_inner(store, stack, request) {
        Ok(value) => {
            admission.release()?;
            Ok(value)
        }
        Err(error) => Err(error),
    }
}

fn merge_inner(store: &Arc<Store>, stack: &LayerStackRecord, merge: &Merge<'_>) -> Answer {
    let (left, right, message, units) = (merge.left, merge.right, merge.message, merge.units);
    if left == right {
        return Err(invalid("merge takes two different sources"));
    }
    let admitted = Units::new(units).map_err(invalid)?;
    let name = match merge.into {
        Some(MAIN) => {
            return Err(refuse(
                "ReservedName",
                "\"main\" names the project's main line",
                SdkValue::Map(Vec::new()),
            ))
        }
        Some(into) => into,
        None if left == MAIN => {
            return Err(invalid(
                "main moves only by branch publish: publish the Branch, or merge into another Branch or a new one",
            ))
        }
        None => left,
    };
    let existing = merge.into.is_none();
    let name = args::name(name)?;
    let (left_side, ln, left_branch) = side(store, stack, left)?;
    let (right_side, rn, _) = side(store, stack, right)?;
    resolve::expect(store, stack, merge.expected, left, ln)?;
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
    let branch = match (&left_branch, existing) {
        (Some(branch), true) => branch.id,
        _ => BranchId::named(stack.id, &name),
    };
    let request = |head| BranchMergeRequest {
        stack: stack.id,
        branch,
        name: name.clone(),
        sides: [(left_side, ln), (right_side, rn)],
        head,
    };
    let write = |request: &BranchMergeRequest| match existing {
        true => catalog.merge_into_branch(request),
        false => catalog.merge_branch(request),
    };
    if let Some((newer, older)) = newer {
        // `left` already holds `right`: nothing moves
        let unchanged = existing && newer == ln;
        let (head, root) = match unchanged {
            true => (ln, root(store, ln)?),
            false => {
                let snapshot = write(&request(MergeHead::Node(newer))).map_err(history)?;
                (branch_node(&snapshot.branch), snapshot.effective_root)
            }
        };
        return Ok(SdkValue::map([
            ("kind", SdkValue::text("Merged")),
            ("fastForward", SdkValue::Bool(!unchanged)),
            ("unchanged", SdkValue::Bool(unchanged)),
            ("branch", SdkValue::text(name.as_str())),
            (
                "head",
                SdkValue::opt_text(match head {
                    NodeId::Commit(id) => Some(hex(id.as_slice())),
                    NodeId::Layer(_) => None,
                }),
            ),
            ("commit", SdkValue::text(node_text(head))),
            ("root", SdkValue::text(hex(root.as_bytes()))),
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
    // §4.3: derive each side at the position it records, so a marker hides
    // inherited entries on a side at or below the frontier while a
    // post-frontier re-creation stays and wins the merge.
    let reader = store.ports(stack.scope);
    let derive = |node, root| {
        crate::removal::derive_root(store, stack.id, &*reader, &save, node, root)
            .map_err(crate::removal::refusal)
    };
    let base_root = derive(base_node, FilesystemRootId(root(store, base_node)?))?;
    let ours = derive(ln, FilesystemRootId(root(store, ln)?))?;
    let theirs = derive(rn, FilesystemRootId(root(store, rn)?))?;
    let mut sides = super::units::Sides {
        base: base_root,
        ours,
        theirs,
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
    let lww = layerfs_protocol::sdk::lww(&merged.lww);
    if !merged.clean() {
        // Nothing is created: the unfinished Save is dropped.
        drop(save);
        return Err(refuse(
            "Conflict",
            format!(
                "{left} + {right} conflict in {} file(s); nothing changed",
                merged.conflicts.len()
            ),
            SdkValue::map([
                ("sources", sources),
                ("mergeBase", SdkValue::text(node_text(base_node))),
                (
                    "conflicts",
                    layerfs_protocol::sdk::conflicts(&merged.conflicts),
                ),
                (
                    "paths",
                    SdkValue::List(
                        (merged.conflicts.iter().take(CONFLICT_PATHS))
                            .map(|conflict| SdkValue::text(&conflict.path))
                            .collect(),
                    ),
                ),
                (
                    "total",
                    SdkValue::Int(i64::try_from(merged.conflicts.len()).unwrap_or(i64::MAX)),
                ),
                ("lww", lww),
                (
                    "conflictsTruncated",
                    SdkValue::Bool(layerfs_protocol::sdk::conflicts_truncated(
                        &merged.conflicts,
                    )),
                ),
                (
                    "lwwTruncated",
                    SdkValue::Bool(layerfs_protocol::sdk::lww_truncated(&merged.lww)),
                ),
                ("units", layerfs_protocol::sdk::units(&decided)),
                (
                    "unitsTruncated",
                    SdkValue::Bool(layerfs_protocol::sdk::units_truncated(&decided)),
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
        crate::telemetry::timed("branch.merge.publish", |_| write(&publish)).map_err(history)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Merged")),
        ("fastForward", SdkValue::Bool(false)),
        ("unchanged", SdkValue::Bool(false)),
        ("branch", SdkValue::text(name.as_str())),
        (
            "head",
            SdkValue::opt_text(snapshot.branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
        (
            "commit",
            SdkValue::text(node_text(branch_node(&snapshot.branch))),
        ),
        ("root", SdkValue::text(hex(merged.root.0.as_bytes()))),
        ("sources", sources),
        ("mergeBase", SdkValue::text(node_text(base_node))),
        ("conflicts", SdkValue::List(Vec::new())),
        ("lww", lww),
        ("conflictsTruncated", SdkValue::Bool(false)),
        (
            "lwwTruncated",
            SdkValue::Bool(layerfs_protocol::sdk::lww_truncated(&merged.lww)),
        ),
        ("units", layerfs_protocol::sdk::units(&decided)),
        (
            "unitsTruncated",
            SdkValue::Bool(layerfs_protocol::sdk::units_truncated(&decided)),
        ),
    ]))
}

/// Publishes the Commit `at` (an id or a prefix; the Branch's head when
/// absent) of `branch` as main's next Layer.
///
/// By default the publication is refused `HeadMoved` when it would replace a
/// Layer main gained since the Branch forked; a Branch may publish again over
/// its own earlier Layers. `expected` (main's head, an id or a prefix), when
/// given, is a strict compare-and-swap expectation, and `force` deliberately
/// keeps the older overwrite-only behavior. A Commit already published
/// answers `UpToDate` whichever check would refuse it, and a tree main
/// already holds answers `NoChanges`. A refusal names main's head as
/// `head`. Nothing is written on a refusal.
pub fn publish(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    branch: &layerfs_history::BranchRecord,
    at: Option<&str>,
    expected: Option<&str>,
    force: bool,
) -> Answer {
    let commit = match at {
        Some(at) => {
            let node = resolve::commit(store, stack, at)?.node();
            let name = branch.name.as_str();
            match super::listing::held(store, name, branch_node(branch), node)? {
                NodeId::Commit(commit) => commit,
                NodeId::Layer(_) => {
                    return Err(invalid(format!(
                        "{at} is one of main's publications, which main already holds: --at takes a Commit of {name} (`layerfs commit list` lists them)"
                    )))
                }
            }
        }
        None => branch.head_commit.ok_or_else(|| {
            refuse(
                "NoCommit",
                format!("{} has no saved Commit to publish", branch.name.as_str()),
                SdkValue::Null,
            )
        })?,
    };
    let expected_base = match expected {
        None => None,
        Some(text) => match resolve::commit(store, stack, text)?.node() {
            NodeId::Layer(layer) => Some(layer),
            NodeId::Commit(_) => {
                return Err(invalid(
                    "expected names main's head, a publication of main, not a Branch's Commit",
                ))
            }
        },
    };
    // A caller-named base is a strict compare-and-swap: main must still be
    // the Layer the caller read. Without one, publishing is allowed while it
    // cannot drop a Layer main gained since the Branch forked: main is still
    // at the fork point, already holds this tree, or its head is this
    // Branch's own earlier publication.
    let refusal = match expected_base {
        Some(expected) if stack.head_layer != expected => {
            Some(moved_base(expected, stack.head_layer))
        }
        _ if !force && !safe_to_publish(store, stack, branch, commit)? => {
            Some(skipped(branch, branch.base_layer, stack.head_layer))
        }
        _ => None,
    };
    if let Some(refusal) = refusal {
        // A Commit already on main answers `UpToDate`, as `add_layer` would:
        // repeating a publication after main moved on is not a stale one.
        return match store
            .history()
            .published_layer(branch.id, commit)
            .map_err(history)?
        {
            Some(layer) => Ok(up_to_date(layer)),
            None => Err(refusal),
        };
    }
    // The stack head read here is checked in `add_layer`'s write transaction,
    // so a publication that lands between the check above and the write is
    // not overwritten: it is refused instead. `force` asks for the previous
    // overwrite-only behavior, still CAS'd against a concurrent publication.
    let outcome = store
        .history()
        .add_layer(&AddLayerRequest {
            stack: stack.id,
            branch: branch.id,
            commit,
            expected_stack_head: stack.head_layer,
            expected_branch_base: branch.base_layer,
        })
        .map_err(|error| match (error.cause(), expected_base) {
            (HistoryError::StackMoved { actual, .. }, Some(expected)) => {
                moved_base(expected, *actual)
            }
            (HistoryError::StackMoved { actual, .. }, None) if !force => {
                skipped(branch, branch.base_layer, *actual)
            }
            _ => history(error),
        })?;
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
                (
                    "previous",
                    SdkValue::opt_text(
                        (stack.head_layer != branch.base_layer)
                            .then(|| hex(stack.head_layer.as_slice())),
                    ),
                ),
            ])
        }
        AddLayerOutcome::UpToDate { layer } => up_to_date(layer),
        AddLayerOutcome::NoChanges { head } => SdkValue::map([
            ("kind", SdkValue::text("NoChanges")),
            ("headLayer", SdkValue::text(hex(head.as_slice()))),
        ]),
    })
}

/// The answer for a Commit already published as `layer`.
fn up_to_date(layer: LayerId) -> SdkValue {
    SdkValue::map([
        ("kind", SdkValue::text("UpToDate")),
        ("layer", SdkValue::text(hex(layer.as_slice()))),
    ])
}

/// Whether publishing `commit` cannot drop a Layer `main` gained since the
/// Branch forked: main is at the fork point, main's tree is already the
/// Commit's (`NoChanges`), or its head is this Branch's own publication and
/// the Commit contains that publication's source.
fn safe_to_publish(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    branch: &layerfs_history::BranchRecord,
    commit: CommitId,
) -> Result<bool, SdkRefusal> {
    if stack.head_layer == branch.base_layer {
        return Ok(true);
    }
    let head = args::layer(store, stack.head_layer)?;
    if head.root == root(store, NodeId::Commit(commit))? {
        return Ok(true);
    }
    if head.stack != stack.id || head.source_branch != Some(branch.id) {
        return Ok(false);
    }
    let Some(source) = head.source_commit else {
        return Ok(false);
    };
    if source == commit {
        return Ok(true);
    }
    store
        .history()
        .is_ancestor(NodeId::Commit(source), NodeId::Commit(commit))
        .map_err(history)
}

/// A refusal for a main that is not the Layer the caller expected.
fn moved_base(expected: LayerId, actual: LayerId) -> SdkRefusal {
    refuse(
        "HeadMoved",
        format!(
            "main is at {}, not the expected {}; read main again and publish, or pass force to overwrite",
            prefix(NodeId::Layer(actual)),
            prefix(NodeId::Layer(expected)),
        ),
        SdkValue::map([
            ("expectedBase", SdkValue::text(hex(expected.as_slice()))),
            ("actualBase", SdkValue::text(hex(actual.as_slice()))),
            ("head", SdkValue::text(hex(actual.as_slice()))),
        ]),
    )
}

/// The first 12 characters of a node's id: a prefix the calls take back.
fn prefix(node: NodeId) -> String {
    node_text(node)[..12].to_owned()
}

/// A refusal for a publication that would replace main's head with a tree
/// that does not contain it.
fn skipped(branch: &layerfs_history::BranchRecord, base: LayerId, head: LayerId) -> SdkRefusal {
    refuse(
        "HeadMoved",
        format!(
            "publishing {} would skip main's publication {}: its fork point is {}; merge main into {} and publish again, or pass force to overwrite",
            branch.name.as_str(),
            prefix(NodeId::Layer(head)),
            prefix(NodeId::Layer(base)),
            branch.name.as_str(),
        ),
        SdkValue::map([
            ("expectedBase", SdkValue::text(hex(base.as_slice()))),
            ("actualBase", SdkValue::text(hex(head.as_slice()))),
            ("head", SdkValue::text(hex(head.as_slice()))),
        ]),
    )
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
