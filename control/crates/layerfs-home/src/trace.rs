//! The SDK trace route. While a traced call runs, every control exchange it
//! makes travels as a traced observation under the call's scope, and the
//! telemetry of the replies is collected for its trace line. An untraced call
//! sends exactly what it did before. Telemetry never changes a result.
//!
//! [`connection`] appends one line per connection to a machine's worker to
//! the file `LAYERFS_TRACE` names (#535 phase 8 item 6).
use layerfs_bridge::control::SdkValue;
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::time::{Duration, Instant};

/// Most telemetry values one call keeps; later ones are dropped.
const TELEMETRY: usize = 64;

thread_local! {
    static SCOPE: Cell<Option<[u8; 32]>> = const { Cell::new(None) };
    static COLLECTED: RefCell<Vec<SdkValue>> = const { RefCell::new(Vec::new()) };
    /// Time this thread's call waited for a free control connection.
    static WAITED: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// The connection wait of the call this thread runs, and resets it.
pub fn take_waited() -> Duration {
    WAITED.replace(Duration::ZERO)
}

/// Takes the trace scope a call's `observe` argument names out of its
/// arguments; an absent or malformed one leaves the call untraced.
pub fn take_scope(args: &mut SdkValue) -> Option<[u8; 32]> {
    let SdkValue::Map(fields) = args else {
        return None;
    };
    let at = fields.iter().position(|(name, _)| name == "observe")?;
    let SdkValue::Text(hex) = fields.remove(at).1 else {
        return None;
    };
    let scope: [u8; 32] = layerfs_bridge::sdk_values::unhex(&hex)?.try_into().ok()?;
    (scope != [0; 32]).then_some(scope)
}

/// Runs `work` with this thread traced under `scope` and returns the
/// telemetry its exchanges received. The scope is cleared on every exit.
pub fn run<T>(scope: Option<[u8; 32]>, work: impl FnOnce() -> T) -> (T, Vec<SdkValue>) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            SCOPE.set(None);
            COLLECTED.with_borrow_mut(Vec::clear);
        }
    }
    let _reset = Reset;
    COLLECTED.with_borrow_mut(Vec::clear);
    WAITED.set(Duration::ZERO);
    SCOPE.set(scope);
    let value = work();
    (value, COLLECTED.with_borrow_mut(std::mem::take))
}

/// True while this thread runs a traced call.
pub fn traced() -> bool {
    SCOPE.get().is_some()
}

/// Keeps one telemetry value of an in-process call for its trace line.
pub fn collect(value: SdkValue) {
    COLLECTED.with_borrow_mut(|collected| {
        if collected.len() < TELEMETRY {
            collected.push(value);
        }
    });
}

/// The scope this thread's call is traced under, if it is.
pub fn scope() -> Option<[u8; 32]> {
    SCOPE.get()
}

/// Adds `waited` to this thread's call's wait for a free control connection.
pub fn add_waited(waited: Duration) {
    WAITED.set(WAITED.get() + waited);
}

/// Appends `{at, event: "worker.connection", op, worker, sent, ms}` to the
/// file `LAYERFS_TRACE` names, for one connection to the worker at
/// `worker` that began at `started`: `op` is the request it carried
/// (`hello` for one that sent none) and `sent` whether the request went
/// out. Unset, nothing is written; a trace that cannot be written changes
/// no outcome.
pub fn connection(worker: &str, op: &str, sent: bool, started: Instant) {
    let Some(path) = std::env::var_os("LAYERFS_TRACE").filter(|path| !path.is_empty()) else {
        return;
    };
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
    let file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path);
    if let Ok(mut file) = file {
        let _ = file.write_all(text.as_bytes());
    }
}
