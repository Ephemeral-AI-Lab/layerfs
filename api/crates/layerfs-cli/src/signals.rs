//! Ctrl-C and SIGTERM during `store upgrade` (#804 §6), the only call that
//! installs a handler. SIGINT and SIGTERM are blocked on the main thread
//! before any thread starts, so every thread the call starts inherits the
//! mask and neither signal ends the process; one thread takes them with
//! `sigwait`. The first asks the call to stop (the process-wide cancel
//! request, read until the upgrade's `commit` phase); the second exits at
//! once, its outcome unknown. Once the call has answered, a signal changes
//! nothing: the answer is printed and the process ends.
use crate::output;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use nix::sys::signal::{SigSet, Signal};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once the call has answered.
static ANSWERED: AtomicBool = AtomicBool::new(false);

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
    std::thread::Builder::new()
        .name("layerfs-signals".to_owned())
        .spawn(move || wait(signals, &store, detail, text))?;
    Ok(())
}

/// Marks the call answered: a signal from now on changes nothing.
pub fn answered() {
    ANSWERED.store(true, Ordering::SeqCst);
}

fn wait(signals: SigSet, store: &str, detail: bool, text: bool) {
    let mut seen = 0;
    while signals.wait().is_ok() {
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
        let _ = output::refusal(&refusal, detail, text);
        std::process::exit(i32::from(crate::run::REFUSED));
    }
}
