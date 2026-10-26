//! The public SDK conversion exposes diagnostic omission without changing refusal.
use layerfs_api::sandbox::{values, CommitReply};
use layerfs_bridge::control::{ConflictReport, SdkValue};
use layerfs_content::merge::{Conflict, ConflictKind, LwwDecision};

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
}
