//! The SDK trace route.
//!
//! [`connection`] appends one line per connection to a machine's worker to
//! the file `LAYERFS_TRACE` names (#535 phase 8 item 6). With
//! `LAYERFS_TRACE_STEP` set too, [`stepped`] times the steps of one call
//! ([`step`]) and appends one line per step to that file; a worker-routed
//! call asks its worker for the worker's own steps (`trace_step` in the
//! request, `steps` in the answer). Unset, a step mark is one thread-local
//! check.
use layerfs_bridge::control::SdkValue;
use std::cell::RefCell;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

thread_local! {
    /// The steps of the call this thread times: its last mark, and each
    /// step's name and time.
    static STEPS: RefCell<Option<Steps>> = const { RefCell::new(None) };
}

type Steps = (Instant, Vec<(String, Duration)>);

/// Runs `work`, one call `op`, with its steps timed when `LAYERFS_TRACE`
/// and `LAYERFS_TRACE_STEP` are both set, then appends one line per step
/// to that file (see [`steps_write`]). Unset, `work` just runs.
pub fn stepped<T>(op: &str, work: impl FnOnce() -> T) -> T {
    let set = |name| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if !set("LAYERFS_TRACE") || !set("LAYERFS_TRACE_STEP") {
        return work();
    }
    steps_begin();
    let value = work();
    steps_write(op, &steps_take());
    value
}

/// Starts timing this thread's steps, from now.
pub fn steps_begin() {
    STEPS.set(Some((Instant::now(), Vec::new())));
}

/// True while this thread's steps are timed.
pub fn stepping() -> bool {
    STEPS.with_borrow(Option::is_some)
}

/// Ends step `name`, the time since the last mark, and marks now; nothing
/// while this thread's steps are not timed.
pub fn step(name: &str) {
    STEPS.with_borrow_mut(|steps| {
        if let Some((mark, list)) = steps {
            let now = Instant::now();
            list.push((name.to_owned(), now - *mark));
            *mark = now;
        }
    });
}

/// Adds step `name` timed elsewhere (`took`) and leaves the mark.
pub fn step_of(name: String, took: Duration) {
    STEPS.with_borrow_mut(|steps| {
        if let Some((_, list)) = steps {
            list.push((name, took));
        }
    });
}

/// The steps timed on this thread, which stops timing them.
pub fn steps_take() -> Vec<(String, Duration)> {
    STEPS.take().map(|(_, list)| list).unwrap_or_default()
}

/// `steps` as an answer's field: `[{ step, us }]`.
pub fn steps_value(steps: Vec<(String, Duration)>) -> SdkValue {
    SdkValue::List(
        (steps.into_iter())
            .map(|(name, took)| {
                SdkValue::map([
                    ("step", SdkValue::Text(name)),
                    ("us", SdkValue::Int(micros(took))),
                ])
            })
            .collect(),
    )
}

/// Adds a worker's answered `steps` (`[{ step, us }]`) as this thread's,
/// each named `worker.<step>`; a malformed entry is skipped.
pub fn steps_from(steps: &SdkValue) {
    let SdkValue::List(list) = steps else {
        return;
    };
    for entry in list {
        if let (Some(name), Some(us)) = (entry.get_text("step"), entry.get_int("us")) {
            let took = Duration::from_micros(u64::try_from(us).unwrap_or(0));
            step_of(format!("worker.{name}"), took);
        }
    }
}

/// Appends `{at, event: "step", op, step, us, ms}` for each of `steps` to
/// the file `LAYERFS_TRACE` names: `us` the step's microseconds, `ms` its
/// whole milliseconds.
pub fn steps_write(op: &str, steps: &[(String, Duration)]) {
    let mut text = String::new();
    for (name, took) in steps {
        let line = SdkValue::map([
            ("at", SdkValue::Int(crate::home::now_ms())),
            ("event", SdkValue::text("step")),
            ("op", SdkValue::text(op)),
            ("step", SdkValue::text(name)),
            ("us", SdkValue::Int(micros(*took))),
            ("ms", SdkValue::Int(micros(*took) / 1000)),
        ]);
        text.push_str(&crate::json::print(&line));
        text.push('\n');
    }
    if !text.is_empty() {
        append(&text);
    }
}

fn micros(took: Duration) -> i64 {
    i64::try_from(took.as_micros()).unwrap_or(i64::MAX)
}

/// Appends `{at, event: "worker.connection", op, worker, sent, ms}` to the
/// file `LAYERFS_TRACE` names, for one connection to the worker at
/// `worker` that began at `started`: `op` is the request it carried
/// (`hello` for one that sent none) and `sent` whether the request went
/// out. Unset, nothing is written; a trace that cannot be written changes
/// no outcome.
pub fn connection(worker: &str, op: &str, sent: bool, started: Instant) {
    if std::env::var_os("LAYERFS_TRACE").is_none_or(|path| path.is_empty()) {
        return;
    }
    let line = SdkValue::map([
        ("at", SdkValue::Int(crate::home::now_ms())),
        ("event", SdkValue::text("worker.connection")),
        ("op", SdkValue::text(op)),
        ("worker", SdkValue::text(worker)),
        ("sent", SdkValue::Bool(sent)),
        (
            "ms",
            SdkValue::Int(i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)),
        ),
    ]);
    let mut text = crate::json::print(&line);
    text.push('\n');
    append(&text);
}

/// Appends `text` to the file `LAYERFS_TRACE` names; unset or unwritable,
/// nothing.
fn append(text: &str) {
    let Some(path) = std::env::var_os("LAYERFS_TRACE").filter(|path| !path.is_empty()) else {
        return;
    };
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path);
    if let Ok(mut file) = file {
        let _ = file.write_all(text.as_bytes());
    }
}
