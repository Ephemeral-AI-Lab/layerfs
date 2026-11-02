//! Line difference of two bounded texts, reused by file diffs, merge and blame.
//!
//! Lines are the `\n`-separated pieces of the text, so a trailing newline
//! yields a final empty line, exactly as the SDK simulator splits them. The
//! difference is a longest common subsequence with removals before additions
//! on ties. Its table is bounded by [`MAXIMUM_DIFF_CELLS`]: a larger input keeps
//! its common prefix and suffix and, if the middle still exceeds the bound,
//! matches the lines that occur once on each side in order (#517). The gaps
//! between them are compared alone while their tables together stay within
//! the bound; a gap past it is one removed-then-added block. No table grows
//! past the bound, and the matching holds one entry per distinct line.

use std::collections::HashMap;

/// Largest file, in bytes, that is compared or merged line by line.
pub const MAXIMUM_TEXT_BYTES: u64 = 1 << 20;
/// Largest `(lines + 1) * (lines + 1)` table one difference may hold.
pub const MAXIMUM_DIFF_CELLS: usize = 1 << 20;

/// One line of a difference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineOp {
    /// Present on both sides.
    Same,
    /// Only on the left.
    Removed,
    /// Only on the right.
    Added,
}

/// One line with its operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffLine {
    /// Operation.
    pub op: LineOp,
    /// Line text without its newline.
    pub text: String,
}

/// The bytes as text: valid UTF-8 with no NUL byte, else `None` (binary).
pub fn as_text(bytes: &[u8]) -> Option<&str> {
    if bytes.contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

/// Splits like the simulator: every `\n` ends a line, the rest is the last.
pub fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// Line difference from `left` to `right`.
pub fn line_diff(left: &str, right: &str) -> Vec<DiffLine> {
    diff_lines(&lines(left), &lines(right))
}

/// Line difference of two already split texts.
pub fn diff_lines(x: &[&str], y: &[&str]) -> Vec<DiffLine> {
    if cells(x.len(), y.len()) <= MAXIMUM_DIFF_CELLS {
        return table_diff(x, y);
    }
    let mut out = Vec::with_capacity(x.len().max(y.len()));
    let mut budget = MAXIMUM_DIFF_CELLS;
    trimmed_diff(x, y, &mut budget, true, &mut out);
    out
}

fn cells(m: usize, n: usize) -> usize {
    (m + 1).saturating_mul(n + 1)
}

fn line(op: LineOp, text: &str) -> DiffLine {
    DiffLine {
        op,
        text: text.to_owned(),
    }
}

/// Keeps the common prefix and suffix and compares the middle with one table
/// while `budget` holds its cells; past it, with `anchor`, between unique
/// lines, else as one removed-then-added block. A middle with one empty side
/// is that block exactly and spends nothing.
fn trimmed_diff(x: &[&str], y: &[&str], budget: &mut usize, anchor: bool, out: &mut Vec<DiffLine>) {
    let prefix = x.iter().zip(y).take_while(|(a, b)| a == b).count();
    let suffix = x[prefix..]
        .iter()
        .rev()
        .zip(y[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (mx, my) = (&x[prefix..x.len() - suffix], &y[prefix..y.len() - suffix]);
    out.extend(x[..prefix].iter().map(|t| line(LineOp::Same, t)));
    let both = !mx.is_empty() && !my.is_empty();
    let need = cells(mx.len(), my.len());
    if both && need <= *budget {
        *budget -= need;
        out.extend(table_diff(mx, my));
    } else if both && anchor {
        anchored_diff(mx, my, budget, out);
    } else {
        out.extend(mx.iter().map(|t| line(LineOp::Removed, t)));
        out.extend(my.iter().map(|t| line(LineOp::Added, t)));
    }
    out.extend(x[x.len() - suffix..].iter().map(|t| line(LineOp::Same, t)));
}

/// A middle too large for one table: the lines that occur once on each side
/// and keep their order are matched, and each gap between them is compared
/// alone.
fn anchored_diff(x: &[&str], y: &[&str], budget: &mut usize, out: &mut Vec<DiffLine>) {
    let (mut i, mut j) = (0, 0);
    for (ai, aj) in unique_anchors(x, y).into_iter().chain([(x.len(), y.len())]) {
        trimmed_diff(&x[i..ai], &y[j..aj], budget, false, out);
        if let Some(text) = x.get(ai) {
            out.push(line(LineOp::Same, text));
        }
        (i, j) = (ai + 1, aj + 1);
    }
}

/// Pairs `(i, j)` with `x[i] == y[j]` where the line occurs once in each
/// side: the longest run increasing on both sides (patience sorting).
fn unique_anchors(x: &[&str], y: &[&str]) -> Vec<(usize, usize)> {
    let mut seen: HashMap<&str, [(u32, usize); 2]> = HashMap::new();
    for (side, texts) in [x, y].into_iter().enumerate() {
        for (at, &text) in texts.iter().enumerate() {
            let slot = &mut seen.entry(text).or_insert([(0, 0); 2])[side];
            *slot = (slot.0.saturating_add(1), at);
        }
    }
    let mut pairs: Vec<(usize, usize)> = seen
        .into_values()
        .filter(|[a, b]| a.0 == 1 && b.0 == 1)
        .map(|[a, b]| (a.1, b.1))
        .collect();
    pairs.sort_unstable();
    // `ends[k]` is the pair ending the run of length `k + 1` with the
    // smallest right index; `before` links each pair to its predecessor.
    let mut ends: Vec<usize> = Vec::new();
    let mut before = vec![None; pairs.len()];
    for (at, &(_, j)) in pairs.iter().enumerate() {
        let k = ends.partition_point(|&e| pairs[e].1 < j);
        before[at] = k.checked_sub(1).map(|k| ends[k]);
        if k == ends.len() {
            ends.push(at);
        } else {
            ends[k] = at;
        }
    }
    let mut run = Vec::with_capacity(ends.len());
    let mut at = ends.last().copied();
    while let Some(e) = at {
        run.push(pairs[e]);
        at = before[e];
    }
    run.reverse();
    run
}

/// The simulator's longest-common-subsequence walk, line for line.
fn table_diff(x: &[&str], y: &[&str]) -> Vec<DiffLine> {
    let (m, n) = (x.len(), y.len());
    let width = n + 1;
    let mut table = vec![0u32; (m + 1) * width];
    for i in (0..m).rev() {
        for j in (0..n).rev() {
            table[i * width + j] = if x[i] == y[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let mut out = Vec::with_capacity(m + n);
    let (mut i, mut j) = (0, 0);
    let push = |out: &mut Vec<DiffLine>, op, text: &str| {
        out.push(DiffLine {
            op,
            text: text.to_owned(),
        })
    };
    while i < m && j < n {
        if x[i] == y[j] {
            push(&mut out, LineOp::Same, x[i]);
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            push(&mut out, LineOp::Removed, x[i]);
            i += 1;
        } else {
            push(&mut out, LineOp::Added, y[j]);
            j += 1;
        }
    }
    for text in &x[i..] {
        push(&mut out, LineOp::Removed, text);
    }
    for text in &y[j..] {
        push(&mut out, LineOp::Added, text);
    }
    out
}

/// For each line of `a`, the index of its matching line of `b`.
pub fn match_lines(a: &[&str], b: &[&str]) -> Vec<Option<usize>> {
    let mut matched = vec![None; a.len()];
    let (mut i, mut j) = (0, 0);
    for line in diff_lines(a, b) {
        match line.op {
            LineOp::Same => {
                matched[i] = Some(j);
                i += 1;
                j += 1;
            }
            LineOp::Removed => i += 1,
            LineOp::Added => j += 1,
        }
    }
    matched
}
