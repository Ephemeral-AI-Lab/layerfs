//! Ctrl-C and SIGTERM during `store upgrade` (#804 §6), the only call that
//! installs a handler. SIGINT and SIGTERM are blocked on the main thread
//! before any thread starts, so every thread the call starts inherits the
//! mask and neither signal ends the process; one thread takes them with
//! `sigwait`. The first asks the call to stop (the process-wide cancel
//! request, read until the upgrade's `commit` phase); the second exits at
//! once, its outcome unknown. Once the call has answered, a signal changes
//! nothing: the answer is printed and the process ends.
//!
//! The second signal's refusal is written to a duplicate of descriptor 2
//! on a thread of its own, never through `stderr().lock()`: the call's
//! thread may hold that lock, blocked writing a progress line to a full
//! pipe. The exit waits at most [`WRITE_WAIT`] for that write.
//!
//! nix's `SigSet::wait` panics when `sigwait` fails (it reads `sigwait`'s
//! error number as success, then unwraps an unset signal number). On macOS
//! that happens when two signals arrive back to back, and the failed wait
//! has taken the first of them (seen: a failed wait, then the second
//! signal, then nothing). So each wait runs under `catch_unwind` and every
//! return, a panic included, counts as one signal taken. The panic hook
//! says nothing for this thread (the default hook would write to a
//! standard error that may be full) and hands every other thread's panic
//! to the hook it replaced.
//!
//! The residual window: the call's thread marks the call answered as soon
//! as the call returns, and the signal thread checks the mark again just
//! before it exits. A second signal taken in the few instructions between
//! the call's return and that mark still exits `Unknown`; the outcome was
//! settled, so the next `store upgrade` answers `current` or does the work.
use crate::output;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use nix::sys::signal::{SigSet, Signal};
use std::io::Write;
use std::os::fd::AsFd;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Set once the call has answered.
static ANSWERED: AtomicBool = AtomicBool::new(false);

/// The name of the thread that takes the signals.
const THREAD: &str = "layerfs-signals";

/// How long the second signal's exit waits for its refusal to be written.
const WRITE_WAIT: Duration = Duration::from_millis(500);

/// Blocks SIGINT and SIGTERM in this thread and starts the thread that
/// takes them; `store` names the Store the second signal's fix upgrades,
/// `detail` and `text` how its refusal is printed. Call it before any
/// other thread starts.
pub fn cancel_on_signal(store: &str, detail: bool, text: bool) -> std::io::Result<()> {
    let mut signals = SigSet::empty();
    signals.add(Signal::SIGINT);
    signals.add(Signal::SIGTERM);
    signals.thread_block().map_err(std::io::Error::from)?;
    let store = store.to_owned();
    quiet_panics_here();
    std::thread::Builder::new()
        .name(THREAD.to_owned())
        .spawn(move || wait(signals, &store, detail, text))?;
    Ok(())
}

/// Marks the call answered: a signal from now on changes nothing.
pub fn answered() {
    ANSWERED.store(true, Ordering::SeqCst);
}

/// Replaces the panic hook: silent on the signal thread, the previous
/// hook everywhere else.
fn quiet_panics_here() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() != Some(THREAD) {
            previous(info);
        }
    }));
}

/// Waits for the next signal; a failed wait has taken one too (see the
/// module's words), so every return is one signal taken.
fn next(signals: &SigSet) {
    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| signals.wait()));
}

fn wait(signals: SigSet, store: &str, detail: bool, text: bool) {
    let mut seen = 0;
    loop {
        next(&signals);
        if ANSWERED.load(Ordering::SeqCst) {
            continue;
        }
        seen += 1;
        if seen == 1 {
            layerfs_api::cancel::request();
            continue;
        }
        let refusal = SdkRefusal {
            code: "Unknown".to_owned(),
            message: "stopped by a second signal: whether the Store was upgraded is not known"
                .to_owned(),
            detail: SdkValue::map([(
                "fix",
                SdkValue::Text(format!("layerfs store upgrade {store}")),
            )]),
        };
        let line = format!("{}\n", output::refusal_line(&refusal, detail, text));
        if ANSWERED.load(Ordering::SeqCst) {
            continue;
        }
        unlocked(line.into_bytes());
        std::process::exit(i32::from(crate::run::REFUSED));
    }
}

/// Writes `bytes` to a duplicate of descriptor 2, which takes no lock the
/// call's thread may hold, on a thread of its own; waits at most
/// [`WRITE_WAIT`] for it (a full pipe may never take it).
fn unlocked(bytes: Vec<u8>) {
    let Ok(fd) = std::io::stderr().as_fd().try_clone_to_owned() else {
        return;
    };
    let (done, wait) = std::sync::mpsc::channel();
    let started = std::thread::Builder::new().spawn(move || {
        let _ = std::fs::File::from(fd).write_all(&bytes);
        let _ = done.send(());
    });
    if started.is_ok() {
        let _ = wait.recv_timeout(WRITE_WAIT);
    }
}
