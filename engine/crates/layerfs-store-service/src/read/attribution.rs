//! Server-side aligned provenance. Byte equality is never an owner shortcut.
use super::{
    blame, file_facts, query,
    snapshot::{self, Snap},
};
use crate::history::lookup as args;
use crate::store::{Store, StorePorts};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::compare::{text::diff_lines, DiffLine, EntryFacts, LineOp};
use layerfs_content::object::inode_leaf::InodeKind;
use std::{collections::BTreeMap, sync::Arc};

/// One request's ancestry/row and payload budget, shared across all paths.
pub struct AttributionBudget {
    pub work: u64,
    pub bytes: u64,
}
impl Default for AttributionBudget {
    fn default() -> Self {
        Self {
            work: query::MAXIMUM_QUERY_WORK,
            bytes: 64 * 1024 * 1024,
        }
    }
}

/// A branch comparison's complete/unknown summary at one path.
pub struct AttributionSummary {
    pub changed: Option<bool>,
    pub coverage: SdkValue,
    pub count: Option<u64>,
}

/// Bounded rows and coverage shared by file and branch comparisons.
pub struct Aligned {
    pub rows: Vec<SdkValue>,
    pub summary: AttributionSummary,
}

struct Owners {
    lines: BTreeMap<u64, SdkValue>,
    exact: bool,
}

fn owners(
    store: &Arc<Store>,
    reader: &StorePorts,
    snap: &Snap,
    path: &str,
    budget: &mut AttributionBudget,
) -> Result<Owners, SdkRefusal> {
    let (mut lines, mut from, mut exact) = (BTreeMap::new(), 1u64, true);
    loop {
        if budget.work == 0 {
            exact = false;
            break;
        }
        let mut limits = blame::Limits {
            versions: budget.work,
            bytes: budget.bytes,
        };
        let page = blame::blame(store, reader, snap, path, Ok(from), Ok(2000), &mut limits)?;
        budget.work = limits.versions;
        budget.bytes = limits.bytes;
        exact &= !page.get_bool("comparisonCoarse")
            && page.get("coverage").and_then(|c| c.get_text("status")) == Some("complete");
        if let Some(SdkValue::List(items)) = page.get("lines") {
            for item in items {
                let n = item
                    .get_int("n")
                    .ok_or_else(|| args::failed("missing blame line"))?
                    as u64;
                lines.insert(n, item.get("owner").cloned().unwrap_or_default());
                if lines.len() as u64 > query::MAXIMUM_QUERY_WORK {
                    return Err(query::capacity("attribution line budget reached"));
                }
            }
        }
        match page.get_int("nextLine") {
            Some(next) if next as u64 > from => from = next as u64,
            Some(_) => return Err(args::failed("blame window made no progress")),
            None => break,
        }
    }
    Ok(Owners { lines, exact })
}

/// LF belongs to its line, matching blame. Empty files have no invented line.
pub fn lines(left: &str, right: &str) -> Vec<DiffLine> {
    diff_lines(
        &left.split_inclusive('\n').collect::<Vec<_>>(),
        &right.split_inclusive('\n').collect::<Vec<_>>(),
    )
}

fn identity(owner: &SdkValue) -> Option<(&str, &str)> {
    let r = owner.get("ref")?;
    Some((r.get_text("kind")?, r.get_text("id")?))
}

/// Align one pair of bounded texts, optionally obtaining both saved owners.
pub fn align(
    store: &Arc<Store>,
    reader: &StorePorts,
    sides: ((&Snap, &str), (&Snap, &str)),
    texts: (&str, &str),
    include: bool,
) -> Result<Aligned, SdkRefusal> {
    align_bounded(
        store,
        reader,
        sides,
        texts,
        include,
        &mut AttributionBudget::default(),
    )
}

fn align_bounded(
    store: &Arc<Store>,
    reader: &StorePorts,
    sides: ((&Snap, &str), (&Snap, &str)),
    texts: (&str, &str),
    include: bool,
    budget: &mut AttributionBudget,
) -> Result<Aligned, SdkRefusal> {
    let ((left, left_path), (right, right_path)) = sides;
    let diff = lines(texts.0, texts.1);
    if diff.len() as u64 > query::MAXIMUM_QUERY_WORK {
        return Err(query::capacity("diff row budget reached"));
    }
    budget.work = budget.work.saturating_sub(diff.len() as u64);
    let empty = || Owners {
        lines: BTreeMap::new(),
        exact: true,
    };
    let before = if include && !texts.0.is_empty() {
        owners(store, reader, left, left_path, budget)?
    } else {
        empty()
    };
    let after = if include && !texts.1.is_empty() {
        owners(store, reader, right, right_path, budget)?
    } else {
        empty()
    };
    let old = texts.0.split_inclusive('\n').collect::<Vec<_>>();
    let new = texts.1.split_inclusive('\n').collect::<Vec<_>>();
    let alignment_exact = super::line_alignment::exact(&old, &new);
    let mut exact = before.exact && after.exact && alignment_exact;
    let (mut l, mut r, mut count) = (1u64, 1u64, 0u64);
    let mut rows = Vec::new();
    for (index, line) in diff.into_iter().enumerate() {
        let ln = (line.op != LineOp::Added).then_some(l);
        let rn = (line.op != LineOp::Removed).then_some(r);
        let a = ln
            .and_then(|n| before.lines.get(&n))
            .cloned()
            .unwrap_or_default();
        let b = rn
            .and_then(|n| after.lines.get(&n))
            .cloned()
            .unwrap_or_default();
        let changed = if line.op != LineOp::Same {
            Some(false)
        } else if include && before.exact && after.exact && alignment_exact {
            match (identity(&a), identity(&b)) {
                (Some(a), Some(b)) => Some(a != b),
                _ => {
                    exact = false;
                    None
                }
            }
        } else {
            None
        };
        if changed == Some(true) {
            count += 1;
        }
        let coverage = if !include {
            file_facts::coverage("not_requested", None, &[])
        } else if changed.is_none() || !alignment_exact {
            file_facts::coverage(
                "partial",
                Some(if before.exact && after.exact && alignment_exact {
                    "exact"
                } else {
                    "coarse"
                }),
                if !alignment_exact {
                    &["alignment_coarse"]
                } else {
                    &["unknown_owner"]
                },
            )
        } else {
            file_facts::coverage("complete", Some("exact"), &[])
        };
        rows.push(SdkValue::map([
            ("index", SdkValue::Int(index as i64)),
            (
                "op",
                SdkValue::text(match line.op {
                    LineOp::Same => " ",
                    LineOp::Removed => "-",
                    LineOp::Added => "+",
                }),
            ),
            (
                "leftLine",
                ln.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            (
                "rightLine",
                rn.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            ("text", SdkValue::text(line.text)),
            ("textClipped", SdkValue::Bool(false)),
            ("beforeOwner", a),
            ("afterOwner", b),
            (
                "ownerChanged",
                changed.map_or(SdkValue::Null, SdkValue::Bool),
            ),
            ("attributionCoverage", coverage),
        ]));
        if ln.is_some() {
            l += 1;
        }
        if rn.is_some() {
            r += 1;
        }
    }
    let coverage = if !include {
        file_facts::coverage("not_requested", None, &[])
    } else if exact {
        file_facts::coverage("complete", Some("exact"), &[])
    } else {
        file_facts::coverage("partial", Some("coarse"), &["unknown_owner"])
    };
    Ok(Aligned {
        rows,
        summary: AttributionSummary {
            changed: if count > 0 {
                Some(true)
            } else if include && exact {
                Some(false)
            } else {
                None
            },
            count: (include && exact).then_some(count),
            coverage,
        },
    })
}

fn unsupported(reason: &str) -> AttributionSummary {
    AttributionSummary {
        changed: None,
        count: None,
        coverage: file_facts::coverage("unavailable", None, &[reason]),
    }
}

/// Branch diff must call this even when both saved file roots/bytes are equal.
pub fn compare(
    store: &Arc<Store>,
    reader: &StorePorts,
    left: &Snap,
    right: &Snap,
    path: &str,
) -> Result<AttributionSummary, SdkRefusal> {
    compare_bounded(
        store,
        reader,
        left,
        right,
        path,
        &mut AttributionBudget::default(),
    )
}

/// Shared remaining provenance budget across every path of a branch query.
pub fn compare_bounded(
    store: &Arc<Store>,
    reader: &StorePorts,
    left: &Snap,
    right: &Snap,
    path: &str,
    budget: &mut AttributionBudget,
) -> Result<AttributionSummary, SdkRefusal> {
    if budget.work == 0 {
        return Ok(unsupported("history_budget"));
    }
    let regular =
        |entry: Option<EntryFacts>| entry.is_some_and(|e| e.value.kind == InodeKind::RegularFile);
    let a = snapshot::entry(reader, &left.tree, path)?;
    let b = snapshot::entry(reader, &right.tree, path)?;
    if !regular(a) || !regular(b) {
        return Ok(unsupported("unsupported_type"));
    }
    let bytes =
        snapshot::size(reader, &a.unwrap())?.saturating_add(snapshot::size(reader, &b.unwrap())?);
    if snapshot::size(reader, &a.unwrap())? > layerfs_content::compare::MAXIMUM_TEXT_BYTES
        || snapshot::size(reader, &b.unwrap())? > layerfs_content::compare::MAXIMUM_TEXT_BYTES
    {
        return Ok(unsupported("text_limit"));
    }
    if bytes > budget.bytes {
        budget.work = 0;
        return Ok(unsupported("history_budget"));
    }
    budget.bytes -= bytes;
    let (Some(a), Some(b)) = (snapshot::text(reader, a)?, snapshot::text(reader, b)?) else {
        return Ok(unsupported("binary"));
    };
    Ok(align_bounded(
        store,
        reader,
        ((left, path), (right, path)),
        (&a, &b),
        true,
        budget,
    )?
    .summary)
}
