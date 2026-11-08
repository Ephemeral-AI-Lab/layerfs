//! Bounded blame along persisted first parents. Comparison uses exact byte
//! prefix/suffix and bounded LCS for the changed middle. A larger middle is a
//! removed/added block, matching Content's bounded diff policy; it is disclosed.
//! A line is kept from the common suffix only where both texts start a line,
//! and from the common prefix only where both texts end it.
use super::{
    snapshot::{self, Snap},
    window,
};
use crate::{
    history::lookup::{self as args, Answer, Arg},
    operator::files::{entry, revision},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{match_lines, EntryFacts},
    object::inode_leaf::InodeKind,
};
use std::sync::Arc;
type Result<T> = std::result::Result<T, SdkRefusal>;

fn range(reader: &StorePorts, file: EntryFacts, offset: u64, count: u64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let result = crate::telemetry::timed("blame.compare", |timer| {
        layerfs_content::file::read_range(
            reader,
            file.value.content_root,
            offset..offset + count,
            &mut out,
            timer.child("read"),
        )
    });
    result.map_err(args::content)?;
    Ok(out)
}
struct Track {
    number: u64,
    text: String,
    clipped: bool,
    owner: Option<SdkValue>,
}
struct Text {
    lines: u64,
    prefix: Vec<String>,
    complete: bool,
}
fn text(reader: &StorePorts, file: EntryFacts) -> Result<Text> {
    let (mut lines, mut prefix, mut bytes, mut complete) = (0, Vec::new(), 0, true);
    window::lines(reader, &file, window::BYTES, 1, |_, line| {
        lines += 1;
        if complete
            && !line.truncated
            && prefix.len() < window::LINES
            && bytes + line.text.len() <= window::BYTES
        {
            bytes += line.text.len();
            prefix.push(line.text);
        } else {
            complete = false;
        }
        Ok(true)
    })?;
    Ok(Text {
        lines,
        prefix,
        complete,
    })
}
/// Whether a line of the file starts at byte `at`: at its start, or after a newline.
fn starts_line(reader: &StorePorts, file: EntryFacts, at: u64) -> Result<bool> {
    Ok(at == 0 || range(reader, file, at - 1, 1)?.first() == Some(&b'\n'))
}
/// The bytes the texts share at their start and at their end. A line of `newer` that starts at the
/// first byte of the common suffix is the same line of `older` only if `older` starts a line there
/// too. When it does not (a split or cut line), the suffix gives up that byte, so the line belongs to
/// the changed middle. Likewise, an unterminated last line of `newer` that ends the common prefix is
/// the same line only if `older` ends there too; otherwise the prefix gives up its last byte.
fn equal_edges(reader: &StorePorts, newer: EntryFacts, older: EntryFacts) -> Result<(u64, u64)> {
    let (a, b) = (
        snapshot::size(reader, &newer)?,
        snapshot::size(reader, &older)?,
    );
    let min = a.min(b);
    let mut prefix = 0;
    while prefix < min {
        let count = 16384.min(min - prefix);
        let (x, y) = (
            range(reader, newer, prefix, count)?,
            range(reader, older, prefix, count)?,
        );
        let same = x.iter().zip(&y).take_while(|(a, b)| a == b).count() as u64;
        prefix += same;
        if same < count {
            break;
        }
    }
    let mut suffix = 0;
    while suffix < min - prefix {
        let count = 16384.min(min - prefix - suffix);
        let (x, y) = (
            range(reader, newer, a - suffix - count, count)?,
            range(reader, older, b - suffix - count, count)?,
        );
        let same = x
            .iter()
            .rev()
            .zip(y.iter().rev())
            .take_while(|(a, b)| a == b)
            .count() as u64;
        suffix += same;
        if same < count {
            break;
        }
    }
    if suffix > 0 && !starts_line(reader, older, b - suffix)? {
        suffix -= 1;
    }
    if prefix == a && a < b && !starts_line(reader, newer, a)? {
        prefix -= 1;
    }
    Ok((prefix, suffix))
}

/// Remaining ancestry and payload admission shared across comparison windows.
pub struct Limits {
    pub versions: u64,
    pub bytes: u64,
}

/// The origin Commit of each line of the regular file at `path`, from line
/// `from` on, along persisted first parents.
pub fn blame(
    store: &Arc<Store>,
    reader: &StorePorts,
    snap: &Snap,
    path: &str,
    from: Arg<u64>,
    count: Arg<u64>,
    limits: &mut Limits,
) -> Answer {
    let tip = snapshot::existing(reader, snap, path)?;
    if tip.value.kind != InodeKind::RegularFile {
        return Err(args::refuse(
            "NotFile",
            "blame requires a regular file",
            SdkValue::Null,
        ));
    }
    let from = from?;
    let count = count?;
    if count == 0 || count > window::LINES as u64 {
        return Err(args::invalid("lineCount must be in 1..=2000"));
    }
    let (mut tracks, mut spent, mut next_line) = (Vec::new(), 0, None);
    let tip_size = snapshot::size(reader, &tip)?;
    let mut offset = 0u64;
    let scan = window::lines_bounded(reader, &tip, window::BYTES, 1, limits.bytes, |n, line| {
        offset = offset.saturating_add(line.bytes);
        if n < from {
            return Ok(true);
        }
        // Include maximum message/identity overhead for each origin on the wire.
        if tracks.len() == count as usize
            || (!tracks.is_empty() && spent + line.text.len() + 640 > window::BYTES - 8192)
        {
            next_line = Some(n);
            return Ok(false);
        }
        let mut shown = line.text;
        let mut end = shown.len().min(window::BYTES - 8192 - 640 - spent);
        while !shown.is_char_boundary(end) {
            end -= 1;
        }
        let clipped = line.truncated || end < shown.len();
        shown.truncate(end);
        spent += shown.len() + 640;
        tracks.push(Track {
            number: n,
            text: shown,
            clipped,
            owner: None,
        });
        if tracks.len() == count as usize {
            next_line = (offset < tip_size).then_some(n + 1);
            return Ok(false);
        }
        Ok(true)
    })?;
    limits.bytes = limits.bytes.saturating_sub(scan.bytes);
    if scan.exhausted && tracks.is_empty() {
        return Err(super::query::capacity(
            "blame scan budget reached before requested line",
        ));
    }
    let (mut node, mut current, mut versions, mut coarse) = (snap.node, tip, 0i64, false);

    let mut history_limited = scan.exhausted;
    loop {
        if tracks.iter().all(|t| t.owner.is_some()) {
            break;
        }
        if limits.versions == 0 {
            history_limited = true;
            break;
        }
        limits.versions -= 1;
        let (_, parent, value) = revision(store, node)?;
        versions += 1;
        let older = parent
            .map(|p| revision(store, p))
            .transpose()?
            .map(|r| entry(reader, r.0, path))
            .transpose()?
            .flatten()
            .filter(|e| e.value.kind == InodeKind::RegularFile);
        if tracks.iter().all(|t| t.owner.is_some()) {
            break;
        }
        if let Some(older) = older {
            if current.value.content_root == older.value.content_root {
                node = parent.expect("parent");
                current = older;
                continue;
            }
            let bytes = snapshot::size(reader, &current)?
                .saturating_add(snapshot::size(reader, &older)?)
                .saturating_mul(3);
            if bytes > limits.bytes {
                history_limited = true;
                break;
            }
            limits.bytes -= bytes;
            let (new, old) = (text(reader, current)?, text(reader, older)?);
            if new.complete && old.complete {
                let a = old.prefix.iter().map(String::as_str).collect::<Vec<_>>();
                let b = new.prefix.iter().map(String::as_str).collect::<Vec<_>>();
                coarse |= !super::line_alignment::exact(&a, &b);
                let mapping = match_lines(&a, &b);
                for t in tracks.iter_mut().filter(|t| t.owner.is_none()) {
                    if let Some(n) = mapping
                        .iter()
                        .position(|n| *n == Some(t.number as usize - 1))
                    {
                        t.number = n as u64 + 1;
                    } else {
                        t.owner = Some(value.clone());
                    }
                }
            } else {
                coarse = true;
                let (prefix, suffix) = equal_edges(reader, current, older)?;
                let size = snapshot::size(reader, &current)?;
                let (mut offset, mut ranges) = (0u64, Vec::new());
                window::lines(reader, &current, 0, 1, |n, line| {
                    let start = offset;
                    offset += line.bytes;
                    if tracks.iter().any(|t| t.owner.is_none() && t.number == n) {
                        ranges.push((n, start, offset));
                    }
                    Ok(true)
                })?;
                for t in tracks.iter_mut().filter(|t| t.owner.is_none()) {
                    let (_, start, end) = ranges
                        .iter()
                        .find(|(n, _, _)| *n == t.number)
                        .copied()
                        .ok_or_else(|| args::failed("blame line mapping"))?;
                    if end <= prefix {
                        continue;
                    }
                    if start >= size - suffix {
                        t.number = (t.number as i128 + old.lines as i128 - new.lines as i128)
                            .try_into()
                            .map_err(args::failed)?;
                    } else {
                        t.owner = Some(value.clone());
                    }
                }
            }

            node = parent.expect("parent");
            current = older;
        } else {
            for t in tracks.iter_mut().filter(|t| t.owner.is_none()) {
                t.owner = Some(value.clone());
            }
            break;
        }
    }
    // Admit complete line records together with their deduplicated origin facts.
    let display_clipped = tracks.iter().any(|t| t.clipped);
    let (mut lines, mut bytes) = (Vec::new(), 2048usize + path.len());
    let mut origins = std::collections::BTreeMap::new();
    for (i, t) in tracks.into_iter().enumerate() {
        let owner = t.owner.unwrap_or_default();
        let typed_owner = match owner.get_text("id") {
            Some(id) => SdkValue::map([
                (
                    "ref",
                    crate::history::saved::reference(store, args::node(id)?)?,
                ),
                ("message", owner.get("message").cloned().unwrap_or_default()),
            ]),
            None => SdkValue::Null,
        };
        let origin = owner.get_text("id").filter(|id| !origins.contains_key(*id));
        let origin_cost = origin.map_or(0, |_| super::file_facts::value_bytes(&typed_owner));
        let line = SdkValue::map([
            ("n", SdkValue::Int((from + i as u64) as i64)),
            ("owner", typed_owner.clone()),
            ("text", SdkValue::text(t.text.trim_end_matches('\n'))),
            ("lineTruncated", SdkValue::Bool(t.clipped)),
            ("clipped", SdkValue::Bool(t.clipped)),
            ("commit", owner.get("id").cloned().unwrap_or_default()),
            ("message", owner.get("message").cloned().unwrap_or_default()),
        ]);
        let cost = super::file_facts::value_bytes(&line) + origin_cost;
        if bytes + cost > window::BYTES {
            if lines.is_empty() {
                return Err(super::query::capacity(
                    "blame line and owner exceed reply budget",
                ));
            }
            next_line = Some(from + i as u64);
            break;
        }
        bytes += cost;
        if let Some(id) = origin {
            origins.insert(id.to_owned(), typed_owner);
        }
        lines.push(line);
    }
    let mut fields = snapshot::header(snap, path);
    fields.extend([
        ("at", crate::history::saved::reference(store, snap.node)?),
        (
            "coverage",
            super::file_facts::coverage(
                if history_limited || display_clipped {
                    "partial"
                } else {
                    "complete"
                },
                Some(if coarse { "coarse" } else { "exact" }),
                if history_limited {
                    &["history_budget"]
                } else if display_clipped {
                    &["line_clipped"]
                } else {
                    &[]
                },
            ),
        ),
        ("lines", SdkValue::List(lines)),
        ("origins", SdkValue::List(origins.into_values().collect())),
        ("versions", SdkValue::Int(versions)),
        ("comparisonCoarse", SdkValue::Bool(coarse)),
        (
            "nextLine",
            next_line.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
    ]);
    let answer = snapshot::answer(fields)?;
    if super::file_facts::value_bytes(&answer) > window::BYTES {
        return Err(super::query::capacity("blame reply exceeds byte budget"));
    }
    Ok(answer)
}
