//! The `step` family through the one call table (#555): a real command
//! starts asynchronously, a later call waits for it, a cancel is not a
//! timeout, a retry runs no second copy and every call checks its inputs.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A temporary home, removed at the end of the test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "layerfs-step-calls-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn home(&self) -> &Path {
        &self.0
    }

    fn receipt(&self, id: &str) -> PathBuf {
        self.0.join("step").join("launch").join(id)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, SdkRefusal> {
    let value = SdkValue::Map(
        args.iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, value, &mut |_| {})
}

fn refused(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> String {
    match call(home, name, args) {
        Err(error) => error.message,
        Ok(value) => panic!(
            "expected a refusal, got {}",
            layerfs_api::json::print(&value)
        ),
    }
}

/// Waits until `done` answers true, for at most `limit`.
fn until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

/// Whether the wrapper recorded `code` beside `id`.
fn exit_is(temp: &Temp, id: &str, code: &str) -> bool {
    fs::read_to_string(temp.receipt(id).join("exit"))
        .map(|text| text.trim() == code)
        .unwrap_or(false)
}

fn start(temp: &Temp, key: &str, command: &str, timeout_ms: Option<i64>) -> String {
    let mut args = vec![("key", text(key)), ("command", text(command))];
    if let Some(timeout) = timeout_ms {
        args.push(("timeout_ms", SdkValue::Int(timeout)));
    }
    call(temp.home(), "step.exec_async", &args)
        .unwrap()
        .get_text("exec_id")
        .unwrap()
        .to_owned()
}

#[test]
fn a_real_command_starts_waits_and_answers_its_status() {
    let temp = Temp::new("run");
    let started = call(
        temp.home(),
        "step.exec_async",
        &[
            ("key", text("k")),
            ("command", text("echo hello; exit 3")),
            ("timeout_ms", SdkValue::Int(60_000)),
        ],
    )
    .unwrap();
    let id = started.get_text("exec_id").unwrap().to_owned();
    assert!(!started.get_bool("reused"));
    assert_eq!(started.get_text("status"), Some("running"));

    // every call builds a new launcher; this one resumes from the entry
    let waited = call(
        temp.home(),
        "step.exec_wait",
        &[("exec_id", text(&id)), ("wait_ms", SdkValue::Int(10_000))],
    )
    .unwrap();
    assert_eq!(waited.get_text("status"), Some("failed"));
    assert_eq!(waited.get_int("exit_code"), Some(3));

    let again = call(temp.home(), "step.exec_status", &[("exec_id", text(&id))]).unwrap();
    assert_eq!(again.get_text("status"), Some("failed"));
    assert_eq!(again.get_int("exit_code"), Some(3));
    let out = fs::read_to_string(temp.receipt(&id).join("stdout")).unwrap();
    assert_eq!(out.trim(), "hello");
}

#[test]
fn a_second_call_observes_a_still_running_command() {
    let temp = Temp::new("restart");
    let gate = temp.home().join("gate");
    let marker = temp.home().join("marker");
    let command = format!(
        "while [ ! -f '{}' ]; do sleep 0.02; done; echo done > '{}'",
        gate.display(),
        marker.display()
    );
    let id = start(&temp, "k", &command, Some(60_000));
    let running = call(temp.home(), "step.exec_status", &[("exec_id", text(&id))]).unwrap();
    assert_eq!(running.get_text("status"), Some("running"));
    fs::write(&gate, b"go").unwrap();
    let waited = call(
        temp.home(),
        "step.exec_wait",
        &[("exec_id", text(&id)), ("wait_ms", SdkValue::Int(10_000))],
    )
    .unwrap();
    assert_eq!(waited.get_text("status"), Some("succeeded"));
    assert!(marker.is_file());
}

#[test]
fn cancel_and_timeout_are_distinct_outcomes() {
    let temp = Temp::new("cancel-timeout");

    // a cancellation recorded before the deadline is cancelled
    let cancelled = start(&temp, "k-cancel", "sleep 30", Some(60_000));
    let answer = call(temp.home(), "step.cancel", &[("exec_id", text(&cancelled))]).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("cancelled"));
    let status = call(
        temp.home(),
        "step.exec_status",
        &[("exec_id", text(&cancelled))],
    )
    .unwrap();
    assert_eq!(status.get_text("status"), Some("cancelled"));
    assert!(until(Duration::from_secs(10), || exit_is(
        &temp, &cancelled, "143"
    )));

    // a deadline that passed first is timed_out, and a later cancel keeps it
    let timed_out = start(&temp, "k-timeout", "sleep 30", Some(50));
    let waited = call(
        temp.home(),
        "step.exec_wait",
        &[
            ("exec_id", text(&timed_out)),
            ("wait_ms", SdkValue::Int(10_000)),
        ],
    )
    .unwrap();
    assert_eq!(waited.get_text("status"), Some("timed_out"));
    let again = call(temp.home(), "step.cancel", &[("exec_id", text(&timed_out))]).unwrap();
    assert_eq!(again.get_text("outcome"), Some("timed_out"));
    assert!(until(Duration::from_secs(10), || exit_is(
        &temp, &timed_out, "143"
    )));
}

#[test]
fn a_retry_starts_no_second_command() {
    let temp = Temp::new("retry");
    let counter = temp.home().join("count");
    let command = format!("echo x >> '{}'", counter.display());
    let args = [("key", text("k")), ("command", text(&command))];
    let first = call(temp.home(), "step.exec_async", &args).unwrap();
    let retry = call(temp.home(), "step.exec_async", &args).unwrap();
    assert_eq!(first.get_text("exec_id"), retry.get_text("exec_id"));
    assert!(retry.get_bool("reused"));
    let id = first.get_text("exec_id").unwrap().to_owned();
    let waited = call(
        temp.home(),
        "step.exec_wait",
        &[("exec_id", text(&id)), ("wait_ms", SdkValue::Int(10_000))],
    )
    .unwrap();
    assert_eq!(waited.get_text("status"), Some("succeeded"));
    assert_eq!(fs::read_to_string(&counter).unwrap().lines().count(), 1);
}

#[test]
fn an_unknown_id_is_unknown() {
    let temp = Temp::new("unknown");
    let id = "exec-0000000000000000";
    let status = call(temp.home(), "step.exec_status", &[("exec_id", text(id))]).unwrap();
    assert_eq!(status.get_text("status"), Some("unknown"));
    let answer = call(temp.home(), "step.cancel", &[("exec_id", text(id))]).unwrap();
    assert_eq!(answer.get_text("outcome"), Some("unknown"));
}

#[test]
fn the_calls_check_their_inputs() {
    let temp = Temp::new("inputs");
    assert!(
        refused(temp.home(), "step.exec_async", &[("key", text("k"))])
            .contains("`command` is required")
    );
    assert!(
        refused(temp.home(), "step.exec_async", &[("command", text("true"))])
            .contains("`key` is required")
    );
    assert!(refused(temp.home(), "step.exec_wait", &[]).contains("`exec_id` is required"));
    assert!(refused(
        temp.home(),
        "step.exec_async",
        &[
            ("key", text("k")),
            ("command", text("true")),
            ("timeout_ms", SdkValue::Int(-1)),
        ],
    )
    .contains("not negative"));
    assert!(refused(
        temp.home(),
        "step.exec_wait",
        &[
            ("exec_id", text("exec-0000000000000000")),
            ("wait_ms", SdkValue::Int(-1)),
        ],
    )
    .contains("not negative"));
    assert!(refused(
        temp.home(),
        "step.exec_status",
        &[("exec_id", text("nope"))],
    )
    .contains("exec-"));
    assert!(refused(
        temp.home(),
        "step.cancel",
        &[("exec_id", text("exec-000000000000000"))],
    )
    .contains("exec-"));
}
