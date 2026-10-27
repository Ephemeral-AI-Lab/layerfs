//! Literal search for `fs.grep`: the standard library's substring search over
//! a whole text part, with line numbers counted across the gaps and line
//! bounds found only around a match. A call scans each content once.
use super::window::{self, after_last_newline, newlines, Line, Open};
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::{compare::EntryFacts, file::FileView, AuthenticatedObjects, ObjectId};
use std::collections::HashMap;

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
    mut visit: impl FnMut(u64, Line) -> Result<bool, SdkRefusal>,
) -> Result<(), SdkRefusal> {
    let never = pattern.as_bytes()[..pattern.len() - 1].contains(&b'\n');
    // The open line's last bytes, for a match that crosses into the next part.
    let keep = pattern.len() - 1;
    let (mut open, mut matched, mut tail, mut stopped) = (Open::new(), false, String::new(), false);
    window::texts(reader, entry, view, |text| {
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
            matched = format!("{tail}{}", &text[..reach]).contains(pattern);
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
            let found = text[at..].find(pattern).map(|i| at + i);
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
            let part = &text[begin..];
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
    Ok(())
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
    pub fn matches(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        entry: &EntryFacts,
        view: Option<FileView>,
        prefix: usize,
        pattern: &str,
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
        let result = matches(reader, entry, view, prefix, pattern, |n, line| {
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
