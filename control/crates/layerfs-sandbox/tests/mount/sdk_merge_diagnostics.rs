//! The public SDK conversion exposes diagnostic omission without changing refusal.
use layerfs_content::merge::{Conflict, ConflictKind, LwwDecision};
use layerfs_protocol::daemon::ConflictReport;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::daemon::values;
use layerfs_sandbox::daemon::workspace::CommitReply;

#[test]
fn sdk_conflict_keeps_its_outcome_and_explicit_byte_window_flags() {
    let report = ConflictReport {
        head: None,
        base: None,
        markers: false,
        conflicts: (0..900)
            .map(|index| Conflict {
                path: format!("/source/{index}/{}", "界".repeat(64)),
                kind: ConflictKind::Binary,
                hunks: Vec::new(),
            })
            .collect(),
        lww: (0..700)
            .map(|index| LwwDecision {
                path: format!("/target/debug/{index}/{}", "generated".repeat(20)),
                kept_ours: true,
                deleted: false,
                rule: "target/".into(),
            })
            .collect(),
        written: Vec::new(),
        truncated: false,
    };
    let error = values::commit(&CommitReply::Conflict(Box::new(report))).unwrap_err();
    assert_eq!(error.code, "Conflict");
    assert!(error.detail.get_bool("conflictsTruncated"));
    assert!(error.detail.get_bool("lwwTruncated"));
    assert!(!error.detail.get_bool("markers"));
    let SdkValue::List(paths) = error.detail.get("conflicts").unwrap() else {
        panic!("conflicts are a list")
    };
    assert!(!paths.is_empty());
    assert_eq!(
        paths[0].get_text("path"),
        Some(format!("/source/0/{}", "界".repeat(64)).as_str())
    );
    assert!(!paths[0].get_bool("hunksOmitted"));
    // flat beside the code: the first 20 paths and, as the list was cut,
    // the count of all
    let next = error.detail.get("next").expect("next fields");
    let SdkValue::List(shown) = next.get("conflicts").unwrap() else {
        panic!("conflicts are a list")
    };
    assert_eq!(shown.len(), 20);
    assert_eq!(next.get_int("conflicts_count"), Some(900));
}

/// A conflict list that was not cut answers no `conflicts_count`.
#[test]
fn an_uncut_conflict_list_has_no_count() {
    let report = ConflictReport {
        head: None,
        base: None,
        markers: false,
        conflicts: vec![Conflict {
            path: "/a".to_owned(),
            kind: ConflictKind::Binary,
            hunks: Vec::new(),
        }],
        lww: Vec::new(),
        written: Vec::new(),
        truncated: false,
    };
    let error = values::commit(&CommitReply::Conflict(Box::new(report))).unwrap_err();
    let next = error.detail.get("next").expect("next fields");
    assert_eq!(
        next.get("conflicts"),
        Some(&SdkValue::List(vec![SdkValue::text("/a")]))
    );
    assert_eq!(next.get("conflicts_count"), None, "{next:?}");
}
