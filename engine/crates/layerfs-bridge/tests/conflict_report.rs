//! Reproducer and regression test for the Conflict report encoder: a report
//! too large for one control record must be truncated and say so, not refused.
//! Uses only public API (Answer::encode / Answer::decode).
use layerfs_bridge::control::{Answer, ConflictReport, Reply};
use layerfs_content::merge::{Conflict, ConflictKind, Hunk};

const LIMIT: usize = u16::MAX as usize - 16;

fn report(hunks: usize, width: usize, lines: usize) -> ConflictReport {
    let block = |fill: char, base: usize| {
        (0..lines)
            .map(|k| format!("{}{}", fill, "x".repeat(width - 1 - (base + k) % 7)))
            .collect::<Vec<_>>()
    };
    ConflictReport {
        head: None,
        base: None,
        markers: false,
        conflicts: vec![Conflict {
            path: "/project/generated.config".to_owned(),
            kind: ConflictKind::Content,
            hunks: (0..hunks)
                .map(|i| Hunk {
                    line: i * 4 + 1,
                    ours: block('a', i),
                    theirs: block('b', i),
                })
                .collect(),
        }],
        lww: Vec::new(),
        written: Vec::new(),
        truncated: false,
    }
}

fn encode(report: ConflictReport) -> Result<Vec<u8>, String> {
    Answer {
        id: 7,
        reply: Reply::Conflict(Box::new(report)),
    }
    .encode()
    .map_err(|error| error.to_string())
}

fn decoded(bytes: &[u8]) -> ConflictReport {
    match Answer::decode(bytes)
        .expect("the encoded reply decodes")
        .reply
    {
        Reply::Conflict(report) => *report,
        other => panic!("not a Conflict reply: {other:?}"),
    }
}

#[test]
fn a_small_report_is_complete_and_not_truncated() {
    let bytes = encode(report(20, 40, 3)).expect("a 20-hunk report encodes");
    let report = decoded(&bytes);
    assert!(!report.truncated);
    assert_eq!(report.conflicts[0].hunks.len(), 20);
    assert_eq!(report.conflicts[0].hunks[19].line, 77);
}

#[test]
fn a_report_over_the_record_limit_is_truncated_and_says_so() {
    for hunks in [400usize, 600, 1200] {
        let bytes = encode(report(hunks, 40, 3))
            .unwrap_or_else(|error| panic!("encoder refused a {hunks}-hunk report: {error}"));
        assert!(bytes.len() <= LIMIT);
        let report = decoded(&bytes);
        assert!(report.truncated, "{hunks} hunks did not report truncation");
        let kept = report.conflicts[0].hunks.len();
        assert!(kept > 0 && kept < hunks, "{hunks} hunks kept {kept}");
        println!(
            "{hunks:>5} hunks -> {} bytes, kept {kept}, truncated",
            bytes.len()
        );
    }
}

/// The realistic shape: many files, each with a handful of conflicts.
#[test]
fn a_large_multi_file_conflict_is_truncated_and_says_so() {
    let template = report(10, 40, 3);
    let hunks = template.conflicts[0].hunks.clone();
    let report = ConflictReport {
        conflicts: (0..60)
            .map(|file| Conflict {
                path: format!("/project/file-{file:02}.conf"),
                kind: ConflictKind::Content,
                hunks: hunks.clone(),
            })
            .collect(),
        ..template
    };
    let bytes = encode(report).expect("a 60-file, 600-hunk report encodes");
    assert!(bytes.len() <= LIMIT);
    let report = decoded(&bytes);
    assert!(report.truncated);
    let kept: usize = report.conflicts.iter().map(|c| c.hunks.len()).sum();
    assert!(kept > 0 && kept < 600, "kept {kept}");
    println!(
        "60 files x 10 hunks -> {} bytes, {} files, {kept} hunks, truncated",
        bytes.len(),
        report.conflicts.len()
    );
}
