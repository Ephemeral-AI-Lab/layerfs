//! Three-way line merge and conflict markers.
//!
//! The merge is the SDK simulator's `merge3Lines`: lines of the base matched
//! on both sides split the texts into chunks; a chunk changed on one side
//! takes that side, one changed identically on both takes either, and one
//! changed differently on both becomes a marker block
//! `<<<<<<< ours` / `=======` / `>>>>>>> theirs`. Each block is reported as a
//! hunk with its first output line (1-based) and both sides' lines.

use crate::compare::text::{lines, match_lines};

/// One conflicting chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hunk {
    /// Output line (1-based) of the block's opening marker.
    pub line: usize,
    /// Our lines.
    pub ours: Vec<String>,
    /// Their lines.
    pub theirs: Vec<String>,
}

/// A merged text and the blocks that conflicted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextMerge {
    /// Merged text, with a marker block for each hunk.
    pub text: String,
    /// Conflicting chunks, in text order.
    pub hunks: Vec<Hunk>,
}

/// Opening marker prefix.
pub const OPEN: &str = "<<<<<<< ";
/// Separator line.
pub const SEPARATOR: &str = "=======";
/// Closing marker prefix.
pub const CLOSE: &str = ">>>>>>> ";

/// Merges `ours` and `theirs` against `base`, line by line.
pub fn merge3(base: &str, ours: &str, theirs: &str, labels: [&str; 2]) -> TextMerge {
    let (b, o, t) = (lines(base), lines(ours), lines(theirs));
    let (mo, mt) = (match_lines(&b, &o), match_lines(&b, &t));
    let mut out: Vec<String> = Vec::new();
    let mut hunks = Vec::new();
    let (mut bi, mut oi, mut ti) = (0, 0, 0);
    let mut chunk = |out: &mut Vec<String>, (bi, oi, ti): (usize, usize, usize), (be, oe, te)| {
        let (cb, co, ct): (&[&str], &[&str], &[&str]) = (&b[bi..be], &o[oi..oe], &t[ti..te]);
        let owned = |side: &[&str]| {
            side.iter()
                .map(|line| (*line).to_owned())
                .collect::<Vec<_>>()
        };
        if co == cb {
            out.extend(owned(ct));
        } else if ct == cb || co == ct {
            out.extend(owned(co));
        } else {
            hunks.push(Hunk {
                line: out.len() + 1,
                ours: owned(co),
                theirs: owned(ct),
            });
            out.push(format!("{OPEN}{}", labels[0]));
            out.extend(owned(co));
            out.push(SEPARATOR.to_owned());
            out.extend(owned(ct));
            out.push(format!("{CLOSE}{}", labels[1]));
        }
    };
    for k in 0..b.len() {
        let (Some(at_o), Some(at_t)) = (mo[k], mt[k]) else {
            continue;
        };
        if at_o < oi || at_t < ti {
            continue;
        }
        chunk(&mut out, (bi, oi, ti), (k, at_o, at_t));
        out.push(b[k].to_owned());
        (bi, oi, ti) = (k + 1, at_o + 1, at_t + 1);
    }
    chunk(&mut out, (bi, oi, ti), (b.len(), o.len(), t.len()));
    TextMerge {
        text: out.join("\n"),
        hunks,
    }
}

/// The marker text of a file one side deleted and the other kept.
///
/// `ours` true: our text survives and theirs is the deletion.
pub fn deletion_markers(text: &str, ours: bool, labels: [&str; 2]) -> String {
    if ours {
        format!(
            "{OPEN}{}\n{text}\n{SEPARATOR}\n{CLOSE}{} (deleted)",
            labels[0], labels[1]
        )
    } else {
        format!(
            "{OPEN}{} (deleted)\n{SEPARATOR}\n{text}\n{CLOSE}{}",
            labels[0], labels[1]
        )
    }
}

/// True when the text holds one complete marker block: an opening marker
/// line, a later separator line and a later closing marker line.
pub fn has_markers(text: &str) -> bool {
    let mut stage = 0;
    for line in text.split('\n') {
        stage = match stage {
            0 if line.starts_with(OPEN) => 1,
            1 if line == SEPARATOR => 2,
            2 if line.starts_with(CLOSE) => return true,
            other => other,
        };
    }
    false
}
