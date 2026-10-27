//! Idle exit (#373): a Sandbox's daemon ends its process once it has held no
//! Workspace and run no control call for `idle_exit_s` seconds, so an unused
//! Sandbox closes by itself and its container stops. Open connections do not
//! keep it: a client that reaches it later finds the container stopped.
//! The decision and the exit happen under the application state lock, which
//! every control call takes to begin, so no call starts after the decision.
//! Failed connections still retained are written to stderr first. Zero never
//! exits.
use super::{owner::Slot, Application};
use layerfs_bridge::control::DaemonPhase;
use std::{
    io::Write,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

/// How often the idle state is read.
const CHECK: Duration = Duration::from_secs(1);

/// One control call while it runs; its end is counted on every path.
pub(super) struct Call<'a>(&'a Application);
impl Drop for Call<'_> {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.in_flight -= 1;
    }
}

impl Application {
    pub(super) fn begin_call(&self) -> Call<'_> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.in_flight += 1;
        Call(self)
    }

    /// Starts the idle watcher when the setup selects an idle period.
    pub(super) fn watch_idle(self: &Arc<Self>) -> std::io::Result<()> {
        let seconds = self.setup.limits.idle_exit_s;
        if seconds == 0 {
            return Ok(());
        }
        let idle = Duration::from_secs(u64::from(seconds));
        let application = self.clone();
        thread::Builder::new()
            .name("layerfs-idle".into())
            .spawn(move || {
                let mut since = Instant::now();
                loop {
                    thread::sleep(CHECK);
                    let Ok(state) = application.state.lock() else {
                        return;
                    };
                    let used = state.in_flight > 0
                        || state.startup.phase != DaemonPhase::ControlReady
                        || state
                            .startup
                            .service
                            .as_ref()
                            .is_none_or(|service| service.workspaces() != Some(0));
                    if used {
                        since = Instant::now();
                        continue;
                    }
                    if since.elapsed() < idle {
                        continue;
                    }
                    let mut stderr = std::io::stderr().lock();
                    for slot in &state.slots {
                        if let Slot::Retained(failure) = slot {
                            let _ = writeln!(stderr, "{failure:?}");
                        }
                    }
                    let _ = writeln!(
                        stderr,
                        "LAYERFS_DAEMON_IDLE_EXIT after {seconds} s with no Workspace and no call"
                    );
                    drop(stderr);
                    std::process::exit(0);
                }
            })
            .map(drop)
    }
}
