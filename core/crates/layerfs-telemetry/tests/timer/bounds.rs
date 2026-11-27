use layerfs_telemetry::timer::{TimingNode, MAX_LABEL_BYTES};
use std::time::Duration;

#[test]
fn public_labels_are_normalized_before_attachment() {
    let node = TimingNode::new("é".repeat(200), Duration::ZERO);
    assert!(node.name().len() <= MAX_LABEL_BYTES);
    assert!(node.is_incomplete());
    assert!(node.with_incomplete(false).is_incomplete());
}

#[test]
fn owned_capacity_is_not_retained() {
    let mut text = String::with_capacity(1024 * 1024);
    text.push_str("short");
    let node = TimingNode::new(text, Duration::ZERO);
    assert_eq!(node.retained_bytes(), 5);
    assert!(!node.is_incomplete());
}

#[test]
fn recording_budget_pool_unwind_and_encoding() {
    use layerfs_telemetry::{
        operation::{Diagnostic, OperationRecorder},
        timer::{RecordingLimits, TimingReport, NODE_CHARGE},
    };
    let limits = RecordingLimits::new(8, 4, 2 * NODE_CHARGE).unwrap();
    let recorder = OperationRecorder::new(limits, 1, 4 * NODE_CHARGE).unwrap();
    let (result, first) = recorder.run(1, "operation", |root| {
        for _ in 0..20 {
            root.child("child").run(|_| Ok::<_, ()>(()))?;
        }
        Ok::<_, ()>(17)
    });
    assert_eq!(result, Ok(17));
    if let Diagnostic::Report(report) = &first {
        assert_eq!(report.timing().node_count(), 2);
        assert!(report.timing().is_incomplete());
        assert!(report.timing().root().unwrap().retained_bytes() < limits.charge());
    } else {
        panic!("missing first report")
    }
    assert!(matches!(
        recorder.run(2, "busy", |_| Ok::<_, ()>(())).1,
        Diagnostic::Omitted
    ));
    drop(first);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        recorder.run::<(), (), _>(3, "panic", |_| panic!("original"))
    }));
    assert!(matches!(
        recorder.run(4, "released", |_| Err::<(), _>(7)).1,
        Diagnostic::Report(_)
    ));
    let report = TimingReport::from_root(
        TimingNode::new("root", Duration::ZERO).with_children(
            (0..100)
                .map(|_| TimingNode::new("\n".repeat(128), Duration::ZERO))
                .collect(),
        ),
    );
    let bytes = report.encode_bounded(150).unwrap().unwrap();
    assert!(bytes.len() <= 150);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("\"incomplete\": true"));
    assert!(text.trim_end().ends_with('}'));
    assert!(report.encode_bounded(1).unwrap().is_none());
}
