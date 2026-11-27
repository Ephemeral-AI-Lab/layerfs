//! Literal search for `fs.grep`: the standard library's substring search over
//! a whole text part, with line numbers counted across the gaps and line
//! bounds found only around a match. A call scans each content once.
use super::window::{self, after_last_newline, newlines, Line, Open};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_content::merge::Rules;
use layerfs_content::{compare::EntryFacts, file::FileView, AuthenticatedObjects, ObjectId};
use std::collections::HashMap;

/// Which paths a search reads and how it matches: each glob in the rules
/// syntax matches a path or one of its folders (`node_modules` leaves out
/// everything under such a folder), and `fold` matches ASCII letters of
/// either case.
#[derive(Default)]
pub struct Filters {
    /// Only paths this matches, when given.
    pub include: Option<Rules>,
    /// No path this matches.
    pub exclude: Option<Rules>,
    /// Whether ASCII letters match in either case.
    pub fold: bool,
}

impl Filters {
    /// The filters a search method's `include`, `exclude` and `ignoreCase`
    /// arguments ask for.
    pub fn of(args: &SdkValue) -> Result<Self, SdkRefusal> {
        let rules = |name: &str| match args.get(name) {
            None | Some(SdkValue::Null) => Ok(None),
            Some(SdkValue::Text(glob)) if !glob.is_empty() => {
                Ok(Some(Rules::of(std::slice::from_ref(glob))))
            }
            Some(_) => Err(crate::history::lookup::invalid(format!(
                "{name} must be a nonempty glob"
            ))),
        };
        Ok(Self {
            include: rules("include")?,
            exclude: rules("exclude")?,
            fold: match args.get("ignoreCase") {
                None | Some(SdkValue::Null) => false,
                Some(SdkValue::Bool(fold)) => *fold,
                Some(_) => {
                    return Err(crate::history::lookup::invalid(
                        "ignoreCase must be a boolean",
                    ))
                }
            },
        })
    }

    /// Whether any filter was given.
    pub fn any(&self) -> bool {
        self.include.is_some() || self.exclude.is_some() || self.fold
    }

    /// Whether the walk leaves out `path` and, when a folder, what is under it.
    pub fn excluded(&self, path: &str) -> bool {
        self.exclude
            .as_ref()
            .is_some_and(|rules| rules.matching(path).is_some())
    }

    /// Whether the file at `path`, not excluded, is searched.
    pub fn included(&self, path: &str) -> bool {
        self.include
            .as_ref()
            .is_none_or(|rules| rules.matching(path).is_some())
    }
}

/// Every line holding `pattern`, in order, until `visit` stops. A line is its
/// bytes with its final LF, so a pattern with an LF anywhere but last matches
/// nothing. Fails `Binary` as [`window::lines`] does, after the lines before
/// the failing window.
pub fn matches(
    reader: &dyn AuthenticatedObjects,
    entry: &EntryFacts,
    view: Option<FileView>,
    prefix: usize,
    pattern: &str,
    fold: bool,
    visit: impl FnMut(u64, Line) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    matches_bounded(reader, entry, view, prefix, pattern, fold, u64::MAX, visit)
        .map(|_| ())
        .map_err(window::without_scan_detail)
}

/// Literal matching with an actual delivered-byte budget; the legacy wrapper is unchanged.
/// With `fold`, ASCII letters match in either case: the search runs over
/// a lowercased copy of each part, as long as the part, and lines are
/// taken from the part itself.
#[allow(clippy::too_many_arguments)]
pub fn matches_bounded(
    reader: &dyn AuthenticatedObjects,
    entry: &EntryFacts,
    view: Option<FileView>,
    prefix: usize,
    pattern: &str,
    fold: bool,
    budget: u64,
    mut visit: impl FnMut(u64, Line) -> Result<bool, SdkRefusal>,
) -> Result<window::Scan, SdkRefusal> {
    let folded = fold.then(|| pattern.to_ascii_lowercase());
    let pattern = folded.as_deref().unwrap_or(pattern);
    let never = pattern.as_bytes()[..pattern.len() - 1].contains(&b'\n');
    // The open line's last bytes, for a match that crosses into the next part.
    let keep = pattern.len() - 1;
    let (mut open, mut matched, mut tail, mut stopped) = (Open::new(), false, String::new(), false);
    let scan = window::texts_bounded(reader, entry, view, budget, |text| {
        let lowered = fold.then(|| text.to_ascii_lowercase());
        let hay = lowered.as_deref().unwrap_or(text);
        let bytes = text.as_bytes();
        if never {
            open.number += newlines(bytes);
            return Ok(true);
        }
        if !matched && !tail.is_empty() {
            let line = bytes
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |i| i + 1);
            let mut reach = line.min(keep);
            while !text.is_char_boundary(reach) {
                reach += 1;
            }
            matched = format!("{tail}{}", &hay[..reach]).contains(pattern);
        }
        let mut at = 0;
        loop {
            if matched {
                let Some(i) = text[at..].find('\n') else {
                    open.take(&bytes[at..], prefix);
                    return Ok(true);
                };
                open.take(&bytes[at..at + i + 1], prefix);
                if !visit(open.number, open.finish(true))? {
                    stopped = true;
                    return Ok(false);
                }
                (open.number, matched, at) = (open.number + 1, false, at + i + 1);
                tail.clear();
                continue;
            }
            let found = hay[at..].find(pattern).map(|i| at + i);
            let end = found.unwrap_or(bytes.len());
            let gap = &bytes[at..end];
            let begin = match after_last_newline(gap) {
                Some(after) => {
                    open.number += newlines(gap);
                    open.clear();
                    tail.clear();
                    at + after
                }
                None => at,
            };
            open.take(&bytes[begin..end], prefix);
            if let Some(found) = found {
                (matched, at) = (true, found);
                continue;
            }
            let part = &hay[begin..];
            let mut cut = part.len().saturating_sub(keep);
            while !part.is_char_boundary(cut) {
                cut -= 1;
            }
            tail.push_str(&part[cut..]);
            let mut cut = tail.len().saturating_sub(keep);
            while !tail.is_char_boundary(cut) {
                cut -= 1;
            }
            tail.drain(..cut);
            return Ok(true);
        }
    })?;
    if !stopped && matched {
        visit(open.number, open.finish(true))?;
    }
    Ok(scan)
}

/// What one content's complete scan reported: its lines, then whether it
/// failed `Binary`.
struct Found {
    lines: Vec<(u64, Line)>,
    binary: bool,
}

/// Bytes of scan results one call keeps for its other paths with the same
/// content: the reply budget. A result is kept whole or not at all, and
/// nothing outlives the call.
const KEPT: usize = super::reads::REPLY_BUDGET;
/// Bytes a kept result costs beyond its lines' text.
const ENTRY: usize = 64;

/// One call's reuse of scans by content identity: files with one content
/// give the same lines, so the second and later are answered from the first.
#[derive(Default)]
pub struct Reuse {
    found: HashMap<ObjectId, Found>,
    spent: usize,
}

impl Reuse {
    /// True when a scan of the content `id` is kept.
    pub fn kept(&self, id: &ObjectId) -> bool {
        self.found.contains_key(id)
    }

    /// [`matches`] for `entry`, from a kept result when its content was
    /// scanned before in this call.
    #[allow(clippy::too_many_arguments)]
    pub fn matches(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        entry: &EntryFacts,
        view: Option<FileView>,
        prefix: usize,
        pattern: &str,
        fold: bool,
        mut visit: impl FnMut(u64, Line) -> Result<bool, SdkRefusal>,
    ) -> Result<(), SdkRefusal> {
        let id = entry.value.content_root;
        if let Some(found) = self.found.get(&id) {
            for (n, line) in &found.lines {
                if !visit(*n, line.clone())? {
                    return Ok(());
                }
            }
            return if found.binary {
                Err(window::binary())
            } else {
                Ok(())
            };
        }
        let (mut lines, mut cost, mut complete) = (Vec::new(), ENTRY, true);
        let result = matches(reader, entry, view, prefix, pattern, fold, |n, line| {
            cost += line.text.len() + ENTRY;
            if self.spent + cost <= KEPT {
                lines.push((n, line.clone()));
            }
            let more = visit(n, line)?;
            complete &= more;
            Ok(more)
        });
        let binary = match &result {
            Ok(()) => false,
            Err(refusal) if refusal.code == "Binary" => true,
            Err(_) => return result,
        };
        if complete && self.spent + cost <= KEPT {
            self.spent += cost;
            self.found.insert(id, Found { lines, binary });
        }
        result
    }
}
