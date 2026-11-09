//! Line differences past the table bound (#517): the middle is matched on the
//! lines that occur once on each side, so distant edits merge cleanly and
//! blame keeps the lines between them; a middle with no such line stays one
//! block, and the gaps share one table's budget.

use layerfs_content::compare::text::{diff_exact, diff_lines, match_lines, DiffLine, LineOp};
use layerfs_content::merge::merge3;

const LABELS: [&str; 2] = ["ours", "theirs"];

/// `line1` … `line<n>`.
fn numbered(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("line{i}")).collect()
}

/// The lines, each ended by a newline.
fn text(lines: &[String]) -> String {
    lines.join("\n") + "\n"
}

fn refs(lines: &[String]) -> Vec<&str> {
    lines.iter().map(String::as_str).collect()
}

/// A copy with line `k` (1-based) of each edit tagged with its author.
fn edited(base: &[String], edits: &[(usize, &str)]) -> Vec<String> {
    let mut lines = base.to_vec();
    for &(k, who) in edits {
        lines[k - 1] = format!("{}-by-{who}", lines[k - 1]);
    }
    lines
}

/// The side a difference rebuilds when lines of `skip` are left out.
fn side(diff: &[DiffLine], skip: LineOp) -> Vec<&str> {
    diff.iter()
        .filter(|line| line.op != skip)
        .map(|line| line.text.as_str())
        .collect()
}

#[test]
fn distant_edits_past_the_table_bound_merge_cleanly() {
    // Lines 200 to 1300 are a 1101-line middle: 1102² cells exceed one table.
    let base = numbered(2000);
    let ours = edited(&base, &[(200, "A"), (1300, "A")]);
    let theirs = edited(&base, &[(600, "B")]);
    let merged = merge3(&text(&base), &text(&ours), &text(&theirs), LABELS);
    assert!(merged.hunks.is_empty(), "{} hunks", merged.hunks.len());
    let both = edited(&base, &[(200, "A"), (600, "B"), (1300, "A")]);
    assert_eq!(merged.text, text(&both));
}

#[test]
fn an_overlap_past_the_table_bound_is_one_line_per_side() {
    let base = numbered(2000);
    let ours = edited(&base, &[(200, "A"), (1300, "A")]);
    let theirs = edited(&base, &[(1300, "B")]);
    let merged = merge3(&text(&base), &text(&ours), &text(&theirs), LABELS);
    assert_eq!(merged.hunks.len(), 1);
    assert_eq!(merged.hunks[0].ours, ["line1300-by-A"]);
    assert_eq!(merged.hunks[0].theirs, ["line1300-by-B"]);
}

#[test]
fn a_moved_or_copied_line_keeps_a_distant_edit() {
    let base = numbered(2000);
    let theirs = edited(&base, &[(900, "B")]);
    let mut moved = edited(&base, &[(200, "A"), (1300, "A")]);
    let line = moved.remove(499);
    moved.insert(1499, line);
    let mut copied = edited(&base, &[(200, "A"), (1300, "A")]);
    copied.insert(1600, "line700".to_owned());
    for (label, ours) in [("moved", moved), ("copied", copied)] {
        let merged = merge3(&text(&base), &text(&ours), &text(&theirs), LABELS);
        assert!(
            merged.hunks.is_empty(),
            "{label}: {} hunks",
            merged.hunks.len()
        );
        let mut expected = ours.clone();
        let at = expected
            .iter()
            .position(|l| l == "line900")
            .expect("line900");
        expected[at] = "line900-by-B".to_owned();
        assert_eq!(merged.text, text(&expected), "{label}");
    }
}

#[test]
fn a_middle_with_no_unique_line_stays_one_block_and_loses_no_edit() {
    let shapes = ["{", "x", "}", ""];
    let base: Vec<String> = (0..3000).map(|i| shapes[i % 4].to_owned()).collect();
    let ours = edited(&base, &[(200, "A"), (2500, "A")]);
    let theirs = edited(&base, &[(1300, "B")]);
    let merged = merge3(&text(&base), &text(&ours), &text(&theirs), LABELS);
    assert_eq!(merged.hunks.len(), 1);
    let hunk = &merged.hunks[0];
    let by = |lines: &[String], who: &str| lines.iter().filter(|l| l.ends_with(who)).count();
    assert_eq!(by(&hunk.ours, "-by-A"), 2);
    assert_eq!(by(&hunk.theirs, "-by-B"), 1);
}

#[test]
fn blame_keeps_the_lines_between_distant_edits() {
    // `fs.blame` gives an unmatched old line to the newer version.
    let base = numbered(2000);
    let new = edited(&base, &[(200, "A"), (1300, "A")]);
    let matched = match_lines(&refs(&base), &refs(&new));
    let unmatched: Vec<usize> = (0..matched.len())
        .filter(|&i| matched[i].is_none())
        .map(|i| i + 1)
        .collect();
    assert_eq!(unmatched, [200, 1300]);
}

#[test]
fn gaps_past_the_bound_share_one_table_budget() {
    // 500 gaps a table would align almost line by line, each closed by a
    // unique line: together far past one table, so only the first is
    // compared and the rest stay blocks.
    let (mut x, mut y) = (Vec::new(), Vec::new());
    for k in 0..500 {
        for i in 0..1000 {
            x.push(["a", "b"][i % 2].to_owned());
            y.push(["b", "a"][i % 2].to_owned());
        }
        x.push(format!("unique{k}"));
        y.push(format!("unique{k}"));
    }
    let diff = diff_lines(&refs(&x), &refs(&y));
    assert_eq!(side(&diff, LineOp::Added), refs(&x));
    assert_eq!(side(&diff, LineOp::Removed), refs(&y));
    let same = diff.iter().filter(|l| l.op == LineOp::Same).count();
    assert!(same >= 500 + 999, "{same}: the anchors and the first gap");
    assert!(same < 500 + 2 * 999, "{same}: more than one gap compared");
}

#[test]
fn differences_past_the_bound_rebuild_both_sides() {
    let mut seed = 0x517_u64;
    let mut below = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for round in 0..60 {
        let n = 1100 + (round % 4) * 600;
        let base: Vec<String> = (0..n)
            .map(|_| match below(10) {
                0..=2 => format!("d{}", below(6)),
                _ => format!("u{}", below(u64::MAX)),
            })
            .collect();
        let mut other = base.clone();
        for _ in 0..=below(12) {
            let at = below(other.len() as u64) as usize;
            let len = 1 + below(40) as usize;
            let end = (at + len).min(other.len());
            match below(3) {
                0 => other[at..end]
                    .iter_mut()
                    .for_each(|l| *l = format!("e{}", below(u64::MAX))),
                1 => other
                    .splice(at..at, (0..len).map(|i| format!("i{round}.{i}")))
                    .for_each(drop),
                _ => other.drain(at..end).for_each(drop),
            }
        }
        let (x, y) = (refs(&base), refs(&other));
        let diff = diff_lines(&x, &y);
        assert_eq!(side(&diff, LineOp::Added), x, "round {round}: left side");
        assert_eq!(side(&diff, LineOp::Removed), y, "round {round}: right side");
    }
}

#[test]
fn diff_exact_discloses_only_a_true_coarse_fallback() {
    // A middle of distinct numbered lines resolves on its unique lines: exact.
    let base = numbered(2000);
    let new = edited(&base, &[(200, "A"), (1300, "A")]);
    assert!(diff_exact(&refs(&base), &refs(&new)));
    // A middle with no line on both sides stays one block: coarse.
    let x: Vec<String> = (0..1100).map(|i| format!("line{i}")).collect();
    let y: Vec<String> = (0..1100).map(|i| format!("other{i}")).collect();
    assert!(!diff_exact(&refs(&x), &refs(&y)));
    // Gaps share one table budget: past it the remaining gaps are coarse.
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for k in 0..500 {
        for i in 0..1000 {
            left.push(["a", "b"][i % 2].to_owned());
            right.push(["b", "a"][i % 2].to_owned());
        }
        left.push(format!("unique{k}"));
        right.push(format!("unique{k}"));
    }
    assert!(!diff_exact(&refs(&left), &refs(&right)));
}
