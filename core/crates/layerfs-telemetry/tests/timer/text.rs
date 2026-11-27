//! Text rendering of completed reports.

use std::io::{self, Write};
use std::time::Duration;

use layerfs_telemetry::timer::{NodeOutcome, Timing, TimingNode, TimingReport};

struct FailingWriter {
    budget: usize,
}

impl Write for FailingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "no space left"));
        }
        let written = buffer.len().min(self.budget);
        self.budget -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn render(report: &TimingReport) -> String {
    let mut output = Vec::new();
    report.write_text(&mut output).expect("text output");
    String::from_utf8(output).expect("utf-8 text output")
}

fn sample() -> TimingReport {
    TimingReport::from_root(
        TimingNode::new("object.create", Duration::from_millis(5)).with_children(vec![
            TimingNode::new("canonical.construct", Duration::from_micros(3))
                .with_children(vec![TimingNode::new("encode", Duration::from_nanos(250))]),
            TimingNode::new("storage.save", Duration::from_micros(1_500))
                .with_outcome(NodeOutcome::Error),
        ]),
    )
}

#[test]
fn text_renders_a_readable_tree() {
    assert_eq!(
        render(&sample()),
        "object.create  5.000ms\n  canonical.construct  3.000us\n    encode  250ns\n  storage.save  1.500ms [error]\n"
    );
}

#[test]
fn markers_show_error_and_missing_detail() {
    let report = TimingReport::from_root(
        TimingNode::new("slow.call", Duration::from_millis(2_500))
            .with_incomplete(true)
            .with_children(vec![TimingNode::new("inner", Duration::from_micros(2_500))
                .with_outcome(NodeOutcome::Error)
                .with_incomplete(true)]),
    );

    assert_eq!(
        render(&report),
        "slow.call  2.500s [incomplete]\n  inner  2.500ms [error] [incomplete]\n"
    );
}

#[test]
fn labels_are_escaped_for_display() {
    let report = TimingReport::from_root(
        TimingNode::new("tab\there\nnew\\slash\u{7}\u{1b}", Duration::from_nanos(1))
            .with_children(vec![TimingNode::new("café ☕", Duration::from_nanos(2))]),
    );

    assert_eq!(
        render(&report),
        "tab\\there\\nnew\\\\slash\\u{7}\\u{1b}  1ns\n  café ☕  2ns\n"
    );
}

#[test]
fn disabled_report_is_explicit() {
    assert_eq!(
        render(&TimingReport::disabled()),
        "disabled: no timing recorded\n"
    );
}

#[test]
fn writer_failures_propagate() {
    let report = sample();

    let mut failing = FailingWriter { budget: 10 };
    let error = report
        .write_text(&mut failing)
        .expect_err("a failing writer must surface its error");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);

    let mut closed = FailingWriter { budget: 0 };
    assert!(report.write_text(&mut closed).is_err());
}

#[test]
fn a_real_recording_renders_its_structure() {
    let (result, report) = Timing::record("object.create", |root| {
        root.child("canonical.construct")
            .run(|content| content.child("encode").run(|_| Ok::<(), ()>(())))?;
        root.child("storage.save").run(|_| Err::<(), ()>(()))
    });

    assert!(result.is_err());
    let text = render(&report);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4);
    assert!(lines[0].starts_with("object.create  "));
    assert!(lines[1].starts_with("  canonical.construct  "));
    assert!(lines[2].starts_with("    encode  "));
    assert!(lines[3].starts_with("  storage.save  "));
    assert!(lines[0].ends_with(" [error]"));
    assert!(lines[3].ends_with(" [error]"));
    assert!(!lines[1].ends_with("[error]"));
    assert!(!lines[2].ends_with("[error]"));
}
