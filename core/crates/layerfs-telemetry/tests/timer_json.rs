//! JSON output, independent parsing, writer failures and explicit saving.

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
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

struct TruncatingWriter<W: Write> {
    inner: W,
    budget: usize,
}

impl<W: Write> Write for TruncatingWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.budget == 0 {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "out of space"));
        }
        let allowed = buffer.len().min(self.budget);
        let written = self.inner.write(&buffer[..allowed])?;
        self.budget -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

struct FlushFailure<W: Write> {
    inner: W,
}

impl<W: Write> Write for FlushFailure<W> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "flush failed"))
    }
}

fn render_json(report: &TimingReport) -> String {
    let mut output = Vec::new();
    report.write_json(&mut output).expect("json output");
    String::from_utf8(output).expect("utf-8 json output")
}

fn run_python(script: &str, document: &str) -> Output {
    let mut child = std::process::Command::new("python3")
        .arg("-c")
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run python3 for independent JSON validation");
    {
        let mut stdin = child.stdin.take().expect("python stdin");
        stdin
            .write_all(document.as_bytes())
            .expect("write document");
    }
    child.wait_with_output().expect("python output")
}

fn python_stdout(script: &str, document: &str) -> String {
    let output = run_python(script, document);
    assert!(
        output.status.success(),
        "python3 rejected the document:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 python output")
}

fn python_accepts(document: &str) -> bool {
    run_python("import json,sys\njson.load(sys.stdin)\n", document)
        .status
        .success()
}

fn temp_path(tag: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "layerfs-telemetry-json-{}-{tag}-{id}.json",
        std::process::id()
    ))
}

fn save_json_to<W: Write>(report: &TimingReport, writer: W) -> io::Result<()> {
    let mut writer = BufWriter::new(writer);
    report.write_json(&mut writer)?;
    writer.flush()
}

fn save_json(report: &TimingReport, path: &Path) -> io::Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    save_json_to(report, file)
}

fn sample() -> TimingReport {
    TimingReport::from_root(
        TimingNode::new("object.create", Duration::from_millis(5)).with_children(vec![
            TimingNode::new("canonical.construct", Duration::from_millis(3)),
            TimingNode::new("storage.save", Duration::from_micros(1_500)),
        ]),
    )
}

#[test]
fn json_matches_the_documented_shape() {
    let json = render_json(&sample());
    assert_eq!(
        json,
        "{\n  \"name\": \"object.create\",\n  \"elapsed_ns\": 5000000,\n  \"children\": [\n    {\"name\": \"canonical.construct\", \"elapsed_ns\": 3000000, \"children\": []},\n    {\"name\": \"storage.save\", \"elapsed_ns\": 1500000, \"children\": []}\n  ]\n}\n"
    );

    let summary = python_stdout(
        "import json,sys\n\
         doc = json.load(sys.stdin)\n\
         assert doc[\"name\"] == \"object.create\"\n\
         assert isinstance(doc[\"elapsed_ns\"], int)\n\
         assert [c[\"name\"] for c in doc[\"children\"]] == [\"canonical.construct\", \"storage.save\"]\n\
         assert all(c[\"children\"] == [] for c in doc[\"children\"])\n\
         assert \"outcome\" not in doc and \"incomplete\" not in doc\n\
         print(\"ok\")\n",
        &json,
    );
    assert_eq!(summary, "ok\n");
}

#[test]
fn outcome_and_incomplete_fields_are_opt_in() {
    let report = TimingReport::from_root(
        TimingNode::new("op", Duration::from_nanos(7))
            .with_outcome(NodeOutcome::Error)
            .with_incomplete(true),
    );

    assert_eq!(
        render_json(&report),
        "{\"name\": \"op\", \"elapsed_ns\": 7, \"outcome\": \"error\", \"incomplete\": true, \"children\": []}\n"
    );
}

#[test]
fn a_disabled_report_serializes_to_null() {
    let json = render_json(&TimingReport::disabled());
    assert_eq!(json, "null\n");
    assert_eq!(
        python_stdout(
            "import json,sys\nprint(json.load(sys.stdin) is None)\n",
            &json
        ),
        "True\n"
    );
}

#[test]
fn escaping_round_trips_through_an_independent_parser() {
    let label = "quote\" back\\slash\ttab\nnewline\u{1}ctrl ☕/é";
    let report = TimingReport::from_root(TimingNode::new(label, Duration::from_nanos(9)));
    let json = render_json(&report);

    assert!(json.contains("\\\""));
    assert!(json.contains("\\\\"));
    assert!(json.contains("\\t"));
    assert!(json.contains("\\n"));
    assert!(json.contains("\\u0001"));
    assert!(json.contains("☕"));
    assert_eq!(
        python_stdout(
            "import json,sys\nsys.stdout.write(json.load(sys.stdin)[\"name\"])\n",
            &json
        ),
        label
    );
}

#[test]
fn durations_beyond_the_schema_are_reported_not_saturated() {
    let report = TimingReport::from_root(TimingNode::new("op", Duration::MAX));
    let mut output = Vec::new();
    let error = report
        .write_json(&mut output)
        .expect_err("an unrepresentable duration must be reported");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);

    let largest = TimingReport::from_root(TimingNode::new("op", Duration::from_nanos(u64::MAX)));
    let json = render_json(&largest);
    assert!(json.contains(&format!("\"elapsed_ns\": {}", u64::MAX)));
    assert!(python_accepts(&json));
}

#[test]
fn writer_failures_propagate() {
    let report = sample();

    let mut closed = FailingWriter { budget: 0 };
    let error = report
        .write_json(&mut closed)
        .expect_err("a closed writer must surface its error");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);

    let mut truncated = FailingWriter { budget: 24 };
    let error = report
        .write_json(&mut truncated)
        .expect_err("a truncated write must surface its error");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

#[test]
fn a_real_recording_round_trips_through_json() {
    let attached = TimingReport::from_root(
        TimingNode::new("remote.execute", Duration::from_micros(7)).with_children(vec![
            TimingNode::new("remote.pack", Duration::from_micros(2)),
        ]),
    );

    let (result, report) = Timing::record("object.create ☕", |root| {
        root.child("canonical.construct")
            .run(|content| content.child("encode").run(|_| Ok::<(), ()>(())))?;
        root.child("storage.save").run(|active| {
            active.attach(Some(attached));
            Err::<(), ()>(())
        })
    });
    assert!(result.is_err());

    let json = render_json(&report);
    let summary = python_stdout(
        "import json,sys\n\
         doc = json.load(sys.stdin)\n\
         assert doc[\"name\"] == \"object.create ☕\"\n\
         kids = {c[\"name\"]: c for c in doc[\"children\"]}\n\
         assert sorted(kids) == [\"canonical.construct\", \"storage.save\"]\n\
         assert kids[\"canonical.construct\"][\"children\"][0][\"name\"] == \"encode\"\n\
         assert kids[\"storage.save\"][\"outcome\"] == \"error\"\n\
         remote = kids[\"storage.save\"][\"children\"][0]\n\
         assert remote[\"name\"] == \"remote.execute\"\n\
         assert remote[\"children\"][0][\"name\"] == \"remote.pack\"\n\
         print(\"ok\")\n",
        &json,
    );
    assert_eq!(summary, "ok\n");
}

#[test]
fn saving_uses_no_clobber_creation_and_stays_separate_from_the_product_result() {
    let path = temp_path("save");
    let (result, report) = Timing::record("object.create", |root| {
        root.child("child").run(|_| Ok::<u32, ()>(3))
    });

    assert!(save_json(&report, &path).is_ok());
    let written = std::fs::read_to_string(&path).expect("written report");
    assert!(written.starts_with("{\n"));
    assert!(written.ends_with("}\n"));
    assert!(python_accepts(&written));

    let error = save_json(&report, &path).expect_err("no-clobber creation must reject a file");
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(result, Ok(3));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_failed_write_leaves_a_partial_file_and_surfaces_the_error() {
    let path = temp_path("partial");
    let report = sample();

    let outcome = (|| -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        save_json_to(
            &report,
            TruncatingWriter {
                inner: file,
                budget: 40,
            },
        )
    })();
    let error = outcome.expect_err("the truncated write must fail");
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);

    let partial = std::fs::read_to_string(&path).expect("the partial file is retained");
    assert!(!partial.is_empty());
    assert!(partial.len() <= 40);
    assert!(partial.len() < render_json(&report).len());
    assert!(!python_accepts(&partial));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_flush_failure_is_surfaced() {
    let path = temp_path("flush");
    let report = sample();

    let outcome = (|| -> io::Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        save_json_to(&report, FlushFailure { inner: file })
    })();
    let error = outcome.expect_err("a failing flush must surface its error");
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);

    let _ = std::fs::remove_file(&path);
}
