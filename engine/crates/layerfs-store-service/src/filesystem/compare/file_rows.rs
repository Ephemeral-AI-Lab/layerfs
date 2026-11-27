//! File comparison's compatible legacy view and numbered aligned detail rows.
use super::{
    super::{
        cache::{self, Cached},
        facts,
        paged::query,
        root::{self, Snap},
        text::attribution,
        tree::visibility::Visibility,
        versions::Batching,
    },
    diff::{self, Selector},
};
use crate::{history::saved, lookup as args, store::Store};
use layerfs_content::{
    compare::{diff_exact, line_diff, EntryFacts, MAXIMUM_TEXT_BYTES},
    object::inode_leaf::InodeKind,
};
use layerfs_history::LayerStackRecord;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::sync::Arc;

pub struct Options {
    pub include_attribution: bool,
    pub allow_missing: bool,
    pub page: query::ResultPage,
    pub extended: bool,
    /// Whether both sides' files are read together (#818 H6).
    pub batching: Batching,
}

struct Side {
    path: String,
    snap: Snap,
    entry: Option<EntryFacts>,
    /// The governing marker frontier when this side is hidden at its node.
    hidden: Option<u64>,
    text: Option<String>,
}

fn side(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    reader: &Cached<'_>,
    selector: args::Arg<Selector<'_>>,
    allow_missing: bool,
) -> Result<Side, SdkRefusal> {
    let selector = selector?;
    let snap = root::resolve_in(store, stack, reader, selector.at)?;
    // Explicit selectors stay in their project; a globally valid ID is not admission.
    let belongs = match snap.node {
        layerfs_history::NodeId::Commit(id) => args::commit(store, id)?.stack == stack.id,
        layerfs_history::NodeId::Layer(id) => args::layer(store, id)?.stack == stack.id,
    };
    if !belongs {
        return Err(args::invalid(
            "saved file selector belongs to another project",
        ));
    }
    // §4.3: a side is visible, hidden or absent; a hidden side is treated as
    // absent for the comparison but keeps its own `Removed` side object and
    // never refuses, even when `allowMissing` is false.
    let raw = reader.lookup(&snap.tree, &selector.path)?;
    let hidden = if raw.is_some() {
        Visibility::for_snap(store, &snap)?.hidden(&selector.path)
    } else {
        None
    };
    let entry = if hidden.is_some() {
        None
    } else if allow_missing {
        raw
    } else {
        // The strict lookup above is the one `existing` made (#818 H3).
        Some(root::found(&snap, &selector.path, raw)?)
    };
    if hidden.is_none() && entry.is_some_and(|e| e.value.kind == InodeKind::Directory) {
        return Err(args::refuse(
            "IsDirectory",
            format!("{} is a directory", selector.path),
            SdkValue::map([("path", SdkValue::text(&selector.path))]),
        ));
    }
    Ok(Side {
        path: selector.path,
        snap,
        entry,
        hidden,
        text: None,
    })
}

/// The side with its text read, unless hidden.
fn with_text(reader: &Cached<'_>, mut side: Side) -> Result<Side, SdkRefusal> {
    if side.hidden.is_none() {
        side.text = root::text(reader, side.entry)?;
    }
    Ok(side)
}

/// Both sides' regular files read together (#818 H6): their lengths in one
/// per-object demand, then the roots of the texts within the text bound in
/// one, kept and held by the call's caches, so each side's reads are served
/// where they were made, an object's own failure there too.
fn plan(reader: &Cached<'_>, sides: [&Side; 2]) -> Result<(), SdkRefusal> {
    let mut files = Vec::new();
    for side in sides.iter().filter(|side| side.hidden.is_none()) {
        if let Some(e) = side
            .entry
            .filter(|e| e.value.kind == InodeKind::RegularFile)
        {
            if !files.contains(&e.value.content_root) {
                files.push(e.value.content_root);
            }
        }
    }
    if files.is_empty() {
        return Ok(());
    }
    reader.lengths_each(&files)?;
    let (mut texts, mut lengths) = (Vec::new(), Vec::new());
    for file in files {
        if let Some(Ok(length)) = reader.length(file) {
            if length <= MAXIMUM_TEXT_BYTES {
                texts.push(file);
                lengths.push(length);
            }
        }
    }
    let first = cache::batches(&lengths)
        .into_iter()
        .next()
        .unwrap_or_default();
    reader.hold_each(&texts[first]).map_err(args::content)
}

fn label(side: &Side) -> SdkValue {
    if let Some(frontier) = side.hidden {
        return SdkValue::map([
            ("removed", SdkValue::Bool(true)),
            ("path", SdkValue::text(&side.path)),
            ("frontier_serial", SdkValue::Int(frontier as i64)),
        ]);
    }
    if side.entry.is_none() {
        return SdkValue::Null;
    }
    SdkValue::map([
        ("path", SdkValue::text(&side.path)),
        ("snapshot", SdkValue::text(&side.snap.label)),
        ("inode", SdkValue::Int(side.entry.unwrap().serial as i64)),
    ])
}

fn omitted(reader: &Cached<'_>, side: &Side) -> Result<Option<&'static str>, SdkRefusal> {
    if side.hidden.is_some() {
        return Ok(None);
    }
    Ok(match side.entry {
        None => None,
        Some(e) if e.value.kind != InodeKind::RegularFile => Some("unsupported_type"),
        Some(e) if root::size_in(reader, &e)? > MAXIMUM_TEXT_BYTES => Some("text_limit"),
        Some(_) if side.text.is_none() => Some("binary"),
        Some(_) => None,
    })
}

/// Both sides are resolved once; every returned fact uses those immutable roots.
pub fn diff(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    left: args::Arg<Selector<'_>>,
    right: args::Arg<Selector<'_>>,
    options: Options,
) -> args::Answer {
    let ports = root::reader(store, stack);
    // Both sides' objects go through the call's caches (#818 H3).
    let reader = &Cached::new(&ports)?;
    let left = side(store, stack, reader, left, options.allow_missing)?;
    // Planned together, the right side is located before the left side's
    // text is read; its refusal still follows that read, as before.
    let (left, right) = if options.batching.on(store, reader) {
        let right = side(store, stack, reader, right, options.allow_missing);
        if let Ok(right) = &right {
            plan(reader, [&left, right])?;
        }
        (with_text(reader, left)?, with_text(reader, right?)?)
    } else {
        let left = with_text(reader, left)?;
        let right = side(store, stack, reader, right, options.allow_missing)?;
        (left, with_text(reader, right)?)
    };
    // Only a genuinely absent side is `NoSuchPath`; a hidden side is the
    // `Removed` state, and two hidden sides are a removal with both objects.
    if left.entry.is_none()
        && right.entry.is_none()
        && left.hidden.is_none()
        && right.hidden.is_none()
    {
        return Err(args::refuse(
            "NoSuchPath",
            "both file comparison paths are absent",
            SdkValue::Null,
        ));
    }
    let mut content_budget = facts::CONTENT_QUERY_BYTES;
    let (content, metadata, identity) = if options.extended {
        facts::changes_bounded(reader, left.entry, right.entry, &mut content_budget)?
    } else {
        facts::changes(reader, left.entry, right.entry)?
    };
    let equal = left.entry.is_some() && right.entry.is_some() && !content;
    let same_inode = left
        .entry
        .zip(right.entry)
        .is_some_and(|(a, b)| a.serial == b.serial);
    let reason = omitted(reader, &left)?.or(omitted(reader, &right)?);
    let mut legacy_budget = super::super::text::window::BYTES;
    let (legacy_kind, mut legacy_hunks) = match (&left.text, &right.text) {
        (Some(a), Some(b)) if a == b => ("Equal", Some(SdkValue::List(Vec::new()))),
        (Some(a), Some(b)) => ("Text", diff::hunks(&line_diff(a, b), &mut legacy_budget)),
        _ => (
            if equal { "Equal" } else { "Binary" },
            Some(SdkValue::List(Vec::new())),
        ),
    };
    let left_text = left.text.as_deref().unwrap_or("");
    let right_text = right.text.as_deref().unwrap_or("");
    let mut aligned = if reason.is_none() && options.extended {
        let left_seen = Visibility::for_snap(store, &left.snap)?;
        let right_seen = Visibility::for_snap(store, &right.snap)?;
        attribution::align(
            store,
            reader,
            (
                (&left.snap, &left.path, &left_seen),
                (&right.snap, &right.path, &right_seen),
            ),
            (left_text, right_text),
            options.include_attribution,
        )?
    } else {
        attribution::Aligned {
            rows: Vec::new(),
            summary: attribution::AttributionSummary {
                changed: None,
                count: None,
                coverage: facts::coverage(
                    if options.include_attribution {
                        "unavailable"
                    } else {
                        "not_requested"
                    },
                    None,
                    &reason.into_iter().collect::<Vec<_>>(),
                ),
            },
        }
    };
    if options.include_attribution
        && left.hidden.is_none()
        && right.hidden.is_none()
        && left.snap.node == right.snap.node
        && left.path == right.path
    {
        aligned.summary = attribution::AttributionSummary {
            changed: Some(false),
            count: Some(0),
            coverage: facts::coverage("complete", Some("exact"), &[]),
        };
    }
    let total = aligned.rows.len() as u64;
    let rows = aligned
        .rows
        .into_iter()
        .skip(options.page.skip as usize)
        .take(options.page.limit)
        .collect::<Vec<_>>();
    let cost = rows.iter().map(facts::value_bytes).sum::<usize>() + 4096;
    if cost > super::super::text::window::BYTES {
        return Err(query::capacity(
            "diff row page exceeds reply budget; reduce page_size",
        ));
    }
    if cost + legacy_hunks.as_ref().map_or(0, facts::value_bytes)
        > super::super::text::window::BYTES
    {
        legacy_hunks = None;
    }
    let exact_content_alignment = diff_exact(
        &left_text.split_inclusive('\n').collect::<Vec<_>>(),
        &right_text.split_inclusive('\n').collect::<Vec<_>>(),
    ) && diff_exact(
        &left_text.split('\n').collect::<Vec<_>>(),
        &right_text.split('\n').collect::<Vec<_>>(),
    );
    // The visible/hidden/absent table: a hidden side is absent for the
    // change kind, and two hidden sides are a removal, never a refusal.
    let change_kind = match (left.entry, right.entry) {
        (None, Some(_)) => "Added",
        (Some(_), None) => "Removed",
        (None, None) => "Removed",
        (Some(_), Some(_)) => {
            if content || metadata || identity || aligned.summary.changed == Some(true) {
                "Modified"
            } else {
                "Equal"
            }
        }
    };
    let mut fields = vec![
        ("kind", SdkValue::text(legacy_kind)),
        ("left", label(&left)),
        ("right", label(&right)),
        ("contentEqual", SdkValue::Bool(equal)),
        ("sameInode", SdkValue::Bool(same_inode)),
        ("hunksOmitted", SdkValue::Bool(legacy_hunks.is_none())),
        ("hunks", legacy_hunks.unwrap_or_default()),
        ("leftRef", saved::reference(store, left.snap.node)?),
        ("rightRef", saved::reference(store, right.snap.node)?),
        ("before", facts::facts(reader, left.entry)?),
        ("after", facts::facts(reader, right.entry)?),
        ("changeKind", SdkValue::text(change_kind)),
        (
            "changes",
            facts::flags(content, metadata, identity, aligned.summary.changed),
        ),
        (
            "contentCoverage",
            facts::coverage(
                if reason.is_some() {
                    "unavailable"
                } else {
                    "complete"
                },
                reason.is_none().then_some(if exact_content_alignment {
                    "exact"
                } else {
                    "coarse"
                }),
                &if reason.is_some() {
                    reason.into_iter().collect::<Vec<_>>()
                } else if !exact_content_alignment {
                    vec!["alignment_coarse"]
                } else {
                    Vec::new()
                },
            ),
        ),
        ("attributionCoverage", aligned.summary.coverage),
        (
            "omissionReason",
            reason.map_or(SdkValue::Null, SdkValue::text),
        ),
        ("rows", SdkValue::List(rows)),
        ("page", SdkValue::Int(options.page.number as i64)),
        ("pageSize", SdkValue::Int(options.page.limit as i64)),
        (
            "hasMore",
            SdkValue::Bool(options.page.skip + (options.page.limit as u64) < total),
        ),
    ];
    if let Some(count) = aligned.summary.count {
        fields.push(("attributionChanges", SdkValue::Int(count as i64)));
    }
    let answer = root::answer(fields)?;
    if options.extended && facts::value_bytes(&answer) > super::super::text::window::BYTES {
        return Err(query::capacity(
            "diff page exceeds reply budget; reduce page_size",
        ));
    }
    Ok(answer)
}
