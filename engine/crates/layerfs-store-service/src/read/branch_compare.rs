//! Pinned mixed-kind comparisons, with optional equal-content provenance discovery.
use super::{
    attribution, compare,
    query::{result_page, MAXIMUM_QUERY_WORK},
    snapshot::{self, Snap},
    walk::Walker,
};
use crate::{
    history::{lookup as args, saved},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{ChangeKind, EntryFacts, PathChange},
    object::inode_leaf::InodeKind,
};
use std::sync::Arc;
type Result<T> = std::result::Result<T, SdkRefusal>;

fn walker<'a>(
    reader: &'a StorePorts,
    snap: &'a Snap,
    path: &str,
    after: Option<&str>,
) -> Result<Option<Walker<'a>>> {
    if snapshot::entry(reader, &snap.tree, path)?.is_some() {
        Walker::after(reader, snap, path, after).map(Some)
    } else {
        Ok(None)
    }
}
fn next(walk: &mut Option<Walker<'_>>) -> Result<Option<(String, EntryFacts)>> {
    match walk {
        Some(walk) => walk.next(),
        None => Ok(None),
    }
}
fn set(item: &mut SdkValue, name: &str, value: SdkValue) {
    if let SdkValue::Map(fields) = item {
        if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == name) {
            *old = value
        } else {
            fields.push((name.to_owned(), value));
        }
    }
}

/// Every path considered in component order; equal roots cannot prune provenance.
pub fn diff(
    store: &Arc<Store>,
    reader: &StorePorts,
    left: &Snap,
    right: &Snap,
    options: &SdkValue,
) -> Result<SdkValue> {
    let page = result_page(options)?;
    let prefix = crate::write::files::path(options.get_text("pathPrefix").unwrap_or("/"))?;
    let attribution = options.get_bool("includeAttribution");
    let hunks = !matches!(options.get("hunks"), Some(SdkValue::Bool(false)));
    let key = super::comparison_pages::Key {
        left: left.node,
        right: right.node,
        prefix: prefix.clone(),
        attribution,
        hunks,
        size: page.limit,
        page: page.number,
    };
    let cached = super::comparison_pages::find(store, &key);
    let exhausted = left.node == right.node || matches!(cached, Some(None));
    let after = cached.as_ref().and_then(|v| v.as_deref());
    let skip = if cached.is_some() { 0 } else { page.skip };
    let mut lw = if exhausted {
        None
    } else {
        walker(reader, left, &prefix, after)?
    };
    let mut rw = if exhausted {
        None
    } else {
        walker(reader, right, &prefix, after)?
    };
    let mut last = None;
    let (mut l, mut r) = (next(&mut lw)?, next(&mut rw)?);
    let (mut seen, mut examined, mut bytes, mut more) = (0u64, 0u64, 0usize, false);
    let mut items = Vec::new();
    let mut incomplete = false;
    let mut attribution_budget = attribution::AttributionBudget::default();
    let mut content_bytes = 64 * 1024 * 1024u64;
    while l.is_some() || r.is_some() {
        if examined == MAXIMUM_QUERY_WORK {
            return Err(args::refuse(
                "Capacity",
                "comparison page exceeds query work bound; narrow path_prefix",
                SdkValue::Null,
            ));
        }
        examined += 1;
        let ordering = match (&l, &r) {
            (Some((lp, _)), Some((rp, _))) => lp.split('/').cmp(rp.split('/')),
            (Some(_), None) => std::cmp::Ordering::Less,
            _ => std::cmp::Ordering::Greater,
        };
        let (path, before, after) = match ordering {
            std::cmp::Ordering::Less => {
                let (p, e) = l.take().unwrap();
                l = next(&mut lw)?;
                (p, Some(e), None)
            }
            std::cmp::Ordering::Greater => {
                let (p, e) = r.take().unwrap();
                r = next(&mut rw)?;
                (p, None, Some(e))
            }
            std::cmp::Ordering::Equal => {
                let (p, a) = l.take().unwrap();
                let (_, b) = r.take().unwrap();
                l = next(&mut lw)?;
                r = next(&mut rw)?;
                (p, Some(a), Some(b))
            }
        };
        if let (Some(a), Some(b)) = (before, after) {
            if a.value.kind == InodeKind::RegularFile
                && b.value.kind == InodeKind::RegularFile
                && a.value.content_root != b.value.content_root
            {
                let cost = snapshot::size(reader, &a)?.saturating_add(snapshot::size(reader, &b)?);
                if cost > content_bytes {
                    return Err(super::query::capacity(
                        "comparison content budget exhausted; narrow path_prefix",
                    ));
                }
                content_bytes -= cost;
            }
        }
        let mut budget = 0; // mandatory summaries fit first; file.diff supplies omitted hunks
        let kind = match (before, after) {
            (None, _) => ChangeKind::Added,
            (_, None) => ChangeKind::Removed,
            _ => ChangeKind::Modified,
        };
        let change = PathChange {
            path: path.clone(),
            kind,
            left: before,
            right: after,
        };
        let Some(mut item) = compare::change(reader, &change, &mut budget, false)? else {
            continue;
        };
        let facts_changed = before.is_none()
            || after.is_none()
            || item.get_bool("content")
            || item.get_bool("metadata")
            || item.get_bool("identity");
        let regular = before.is_some_and(|e| e.value.kind == InodeKind::RegularFile)
            && after.is_some_and(|e| e.value.kind == InodeKind::RegularFile);
        let mut owner_changed = if attribution && !regular && before.is_some() && after.is_some() {
            None
        } else {
            Some(false)
        };
        if attribution && regular {
            let summary = attribution::compare_bounded(
                store,
                reader,
                left,
                right,
                &path,
                &mut attribution_budget,
            )?;
            if attribution_budget.work == 0 || attribution_budget.bytes == 0 {
                return Err(super::query::capacity(
                    "comparison attribution budget exhausted; narrow path_prefix",
                ));
            }
            owner_changed = summary.changed;
            set(&mut item, "attributionCoverage", summary.coverage);
            set(
                &mut item,
                "changedOwnerLines",
                summary
                    .count
                    .map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            );
        } else {
            set(
                &mut item,
                "attributionCoverage",
                SdkValue::map([(
                    "status",
                    SdkValue::text(if attribution {
                        "unavailable"
                    } else {
                        "not_requested"
                    }),
                )]),
            );
            set(&mut item, "changedOwnerLines", SdkValue::Null);
        }
        set(
            &mut item,
            "attribution",
            if attribution {
                owner_changed.map_or(SdkValue::Null, SdkValue::Bool)
            } else {
                SdkValue::Null
            },
        );
        if !facts_changed && (!regular || owner_changed == Some(false)) {
            continue;
        }
        if seen < skip {
            seen += 1;
            continue;
        }
        if items.len() == page.limit {
            more = true;
            break;
        }
        incomplete |= attribution && owner_changed.is_none();
        if hunks && item.get_bool("content") {
            set(&mut item, "hunksOmitted", SdkValue::Bool(true));
            set(
                &mut item,
                "hunksUnavailable",
                SdkValue::text("use file.diff for bounded content hunks"),
            );
        }
        bytes += saved::reply_cost(&item);
        if bytes > 32 * 1024 {
            return Err(args::refuse(
                "Capacity",
                "comparison page exceeds reply budget; reduce page_size",
                SdkValue::Null,
            ));
        }
        last = Some(path);
        items.push(item);
    }
    let mut next_key = key;
    next_key.page += 1;
    super::comparison_pages::keep(store, next_key, if more { last } else { None });
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("from", saved::reference(store, left.node)?),
        ("to", saved::reference(store, right.node)?),
        ("items", SdkValue::List(items)),
        ("page", SdkValue::Int(page.number as i64)),
        ("pageSize", SdkValue::Int(page.limit as i64)),
        ("hasMore", SdkValue::Bool(more)),
        (
            "coverage",
            SdkValue::map([
                ("complete", SdkValue::Bool(!incomplete)),
                (
                    "reason",
                    if incomplete {
                        SdkValue::text("some path attribution is unknown")
                    } else {
                        SdkValue::Null
                    },
                ),
            ]),
        ),
        (
            "scan",
            SdkValue::map([
                ("complete", SdkValue::Bool(true)),
                ("examined", SdkValue::Int(examined as i64)),
                ("reason", SdkValue::Null),
            ]),
        ),
    ]))
}
