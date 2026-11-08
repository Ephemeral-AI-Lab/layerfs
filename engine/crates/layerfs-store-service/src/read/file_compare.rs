//! File comparison's compatible legacy view and numbered aligned detail rows.
use super::{
    attribution,
    compare::{self, Selector},
    file_facts, query,
    snapshot::{self, Snap},
};
use crate::{
    history::{lookup as args, saved},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{line_diff, EntryFacts, MAXIMUM_TEXT_BYTES},
    object::inode_leaf::InodeKind,
};
use layerfs_history::LayerStackRecord;
use std::sync::Arc;

pub struct Options {
    pub include_attribution: bool,
    pub allow_missing: bool,
    pub page: query::ResultPage,
    pub extended: bool,
}

struct Side {
    path: String,
    snap: Snap,
    entry: Option<EntryFacts>,
    text: Option<String>,
}

fn side(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    reader: &StorePorts,
    selector: args::Arg<Selector<'_>>,
    allow_missing: bool,
) -> Result<Side, SdkRefusal> {
    let selector = selector?;
    let snap = snapshot::resolve(store, stack, reader, selector.at)?;
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
    let entry = if allow_missing {
        snapshot::entry(reader, &snap.tree, &selector.path)?
    } else {
        Some(snapshot::existing(reader, &snap, &selector.path)?)
    };
    if entry.is_some_and(|e| e.value.kind == InodeKind::Directory) {
        return Err(args::refuse(
            "IsDirectory",
            format!("{} is a directory", selector.path),
            SdkValue::map([("path", SdkValue::text(&selector.path))]),
        ));
    }
    let text = snapshot::text(reader, entry)?;
    Ok(Side {
        path: selector.path,
        snap,
        entry,
        text,
    })
}

fn label(side: &Side) -> SdkValue {
    if side.entry.is_none() {
        return SdkValue::Null;
    }
    SdkValue::map([
        ("path", SdkValue::text(&side.path)),
        ("snapshot", SdkValue::text(&side.snap.label)),
        ("inode", SdkValue::Int(side.entry.unwrap().serial as i64)),
    ])
}

fn omitted(reader: &StorePorts, side: &Side) -> Result<Option<&'static str>, SdkRefusal> {
    Ok(match side.entry {
        None => None,
        Some(e) if e.value.kind != InodeKind::RegularFile => Some("unsupported_type"),
        Some(e) if snapshot::size(reader, &e)? > MAXIMUM_TEXT_BYTES => Some("text_limit"),
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
    let reader = snapshot::reader(store, stack);
    let left = side(store, stack, &reader, left, options.allow_missing)?;
    let right = side(store, stack, &reader, right, options.allow_missing)?;
    if left.entry.is_none() && right.entry.is_none() {
        return Err(args::refuse(
            "NoSuchPath",
            "both file comparison paths are absent",
            SdkValue::Null,
        ));
    }
    let (content, metadata, identity) = file_facts::changes(&reader, left.entry, right.entry)?;
    let equal = left.entry.is_some() && right.entry.is_some() && !content;
    let same_inode = left
        .entry
        .zip(right.entry)
        .is_some_and(|(a, b)| a.serial == b.serial);
    let reason = omitted(&reader, &left)?.or(omitted(&reader, &right)?);
    let mut legacy_budget = super::window::BYTES;
    let (legacy_kind, mut legacy_hunks) = match (&left.text, &right.text) {
        (Some(a), Some(b)) if a == b => ("Equal", Some(SdkValue::List(Vec::new()))),
        (Some(a), Some(b)) => ("Text", compare::hunks(&line_diff(a, b), &mut legacy_budget)),
        _ => (
            if equal { "Equal" } else { "Binary" },
            Some(SdkValue::List(Vec::new())),
        ),
    };
    let left_text = left.text.as_deref().unwrap_or("");
    let right_text = right.text.as_deref().unwrap_or("");
    let mut aligned = if reason.is_none() && options.extended {
        attribution::align(
            store,
            &reader,
            ((&left.snap, &left.path), (&right.snap, &right.path)),
            (left_text, right_text),
            options.include_attribution,
        )?
    } else {
        attribution::Aligned {
            rows: Vec::new(),
            summary: attribution::AttributionSummary {
                changed: None,
                count: None,
                coverage: file_facts::coverage(
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
    if options.include_attribution && left.snap.node == right.snap.node && left.path == right.path {
        aligned.summary = attribution::AttributionSummary {
            changed: Some(false),
            count: Some(0),
            coverage: file_facts::coverage("complete", Some("exact"), &[]),
        };
    }
    let total = aligned.rows.len() as u64;
    let rows = aligned
        .rows
        .into_iter()
        .skip(options.page.skip as usize)
        .take(options.page.limit)
        .collect::<Vec<_>>();
    let cost = rows.iter().map(file_facts::value_bytes).sum::<usize>() + 4096;
    if cost > super::window::BYTES {
        return Err(query::capacity(
            "diff row page exceeds reply budget; reduce page_size",
        ));
    }
    if cost + legacy_hunks.as_ref().map_or(0, file_facts::value_bytes) > super::window::BYTES {
        legacy_hunks = None;
    }
    let exact_content_alignment = super::line_alignment::exact(
        &left_text.split_inclusive('\n').collect::<Vec<_>>(),
        &right_text.split_inclusive('\n').collect::<Vec<_>>(),
    ) && super::line_alignment::exact(
        &left_text.split('\n').collect::<Vec<_>>(),
        &right_text.split('\n').collect::<Vec<_>>(),
    );
    let change_kind = match (left.entry, right.entry) {
        (None, _) => "Added",
        (_, None) => "Removed",
        _ => {
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
        ("before", file_facts::facts(&reader, left.entry)?),
        ("after", file_facts::facts(&reader, right.entry)?),
        ("changeKind", SdkValue::text(change_kind)),
        (
            "changes",
            file_facts::flags(content, metadata, identity, aligned.summary.changed),
        ),
        (
            "contentCoverage",
            file_facts::coverage(
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
    let answer = snapshot::answer(fields)?;
    if options.extended && file_facts::value_bytes(&answer) > super::window::BYTES {
        return Err(query::capacity(
            "diff page exceeds reply budget; reduce page_size",
        ));
    }
    Ok(answer)
}
