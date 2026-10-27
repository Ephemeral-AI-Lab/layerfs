//! Cargo-sized merge diagnostics must use the normal bounded control encoder.
use layerfs_bridge::{
    control::{Answer, Reply, SdkValue},
    sdk_values::{conflicts, conflicts_truncated, lww, lww_truncated},
};
use layerfs_content::merge::{Conflict, ConflictKind, LwwDecision};

#[test]
fn cargo_lww_result_fits_the_unchanged_control_frame() {
    let decisions: Vec<_> = (0..700)
        .map(|index| LwwDecision {
            path: format!(
                "/target/debug/.fingerprint/long-package-name-{index:016x}/output-build-script-build"
            ),
            kept_ours: index % 2 == 0,
            deleted: false,
            rule: "target/".into(),
        })
        .collect();
    let answer = Answer {
        id: 2,
        reply: Reply::Sdk(SdkValue::map([
            ("kind", SdkValue::text("Merged")),
            ("lww", lww(&decisions)),
            ("lwwTruncated", SdkValue::Bool(lww_truncated(&decisions))),
        ])),
    };
    let bytes = answer
        .encode()
        .expect("Cargo-sized diagnostics must not lose a completed merge reply");
    assert_eq!(Answer::decode(&bytes).unwrap(), answer);
    assert!(lww_truncated(&decisions));
    assert!(bytes.len() < 16 * 1024);
}

#[test]
fn many_multibyte_conflict_paths_fit_beside_generated_decisions() {
    let conflicts_input: Vec<_> = (0..900)
        .map(|index| Conflict {
            path: format!("/source/{index}/{}", "界".repeat(64)),
            kind: ConflictKind::Binary,
            hunks: Vec::new(),
        })
        .collect();
    let answer = Answer {
        id: 2,
        reply: Reply::Sdk(SdkValue::map([
            ("conflicts", conflicts(&conflicts_input)),
            (
                "conflictsTruncated",
                SdkValue::Bool(conflicts_truncated(&conflicts_input)),
            ),
        ])),
    };
    let bytes = answer
        .encode()
        .expect("UTF-8 byte length, not character count, bounds the reply");
    assert_eq!(Answer::decode(&bytes).unwrap(), answer);
    assert!(conflicts_truncated(&conflicts_input));
    assert!(bytes.len() < 16 * 1024);
}

#[test]
fn oversized_hunks_are_explicit_and_small_conflicts_keep_exact_text() {
    use layerfs_content::merge::Hunk;
    let values = [Conflict {
        path: "/src/界.rs".into(),
        kind: ConflictKind::Content,
        hunks: vec![
            Hunk {
                line: 3,
                ours: vec!["left\n".into()],
                theirs: vec!["right\n".into()],
            },
            Hunk {
                line: 7,
                ours: vec!["界".repeat(16 * 1024)],
                theirs: vec!["right\n".into()],
            },
        ],
    }];
    let value = conflicts(&values);
    let SdkValue::List(items) = &value else {
        panic!("conflicts are a list")
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get_int("blocks"), Some(2));
    assert!(items[0].get_bool("hunksOmitted"));
    assert!(!conflicts_truncated(&values));
    let SdkValue::List(hunks) = items[0].get("hunks").unwrap() else {
        panic!("hunks are a list")
    };
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0].get_int("line"), Some(3));
    assert_eq!(
        hunks[0].get("ours"),
        Some(&SdkValue::List(vec![SdkValue::text("left\n")]))
    );
    let answer = Answer {
        id: 2,
        reply: Reply::Sdk(value),
    };
    assert_eq!(Answer::decode(&answer.encode().unwrap()).unwrap(), answer);
    assert!(!lww_truncated(&[]));
}
