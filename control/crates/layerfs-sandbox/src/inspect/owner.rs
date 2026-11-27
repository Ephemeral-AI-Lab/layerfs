//! Who runs an inspect request, and how much of the process it may hold
//! (layerfs-project#179 B.3 to B.5, #178 isolation rules 3 and 4).
//!
//! At most [`PERMITS`] requests run at once in a process, the worker's and
//! this process's own alike: one more is refused `Busy` at once, which is
//! admission and not a retry. Each runs on a helper thread that owns its
//! permit until it ends, so at most that many threads can ever be stuck in
//! a read. A request has a soft budget ([`SOFT`]), which its work checks
//! between units ([`Budget::spent`]) and then answers what it has so far,
//! and a hard deadline ([`HARD`]), after which its caller is answered
//! `Unavailable` and the helper is left to end by itself, still holding
//! its permit.
use layerfs_protocol::provision::install::SdkRefusal;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::place::{self, Executor};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

/// How many inspect requests run at once in one process.
pub const PERMITS: usize = 2;
/// After this long a request answers what it has so far.
pub const SOFT: Duration = Duration::from_secs(4);
/// After this long a request's caller is answered `Unavailable`.
pub const HARD: Duration = Duration::from_secs(6);

/// The most of a panic's message a refusal holds.
const SHOWN: usize = 300;

/// The inspect requests running in this process, stuck ones included.
static RUNNING: AtomicUsize = AtomicUsize::new(0);

/// One running request's place; its drop frees it.
struct Permit;

impl Permit {
    fn take() -> Option<Self> {
        let admit = |running: usize| (running < PERMITS).then_some(running + 1);
        RUNNING
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, admit)
            .ok()
            .map(|_| Permit)
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How many inspect requests this process is running now, those whose
/// caller was already answered `Unavailable` included.
pub fn running() -> usize {
    RUNNING.load(Ordering::SeqCst)
}

/// One request's time, handed to its work.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    started: Instant,
}

impl Budget {
    /// Whether the soft budget is spent. Work made of many units (a folder
    /// walk's entries) asks between them and, once it is, answers the
    /// units so far with `has_more: true` and `reason: "deadline"`.
    pub fn spent(&self) -> bool {
        self.started.elapsed() >= SOFT
    }
}

/// Where machine `machine`'s inspect requests run: this process
/// (`Executor::Local`: `native`) or its worker (docker, Lima whether its
/// Sandboxes' daemons are forwarded or not, Apple Container). No lease, no
/// Store, no write, and no refusal while a machine operation holds the
/// machine (`place::on_machine_read`).
pub fn on_owner(home: &Path, machine: &str) -> Result<Executor, SdkRefusal> {
    place::on_machine_read(home, machine)
}

/// Runs `work` as one inspect request of this process: under a permit, on
/// a helper thread, within the deadlines. More than [`PERMITS`] at once is
/// `Busy`; work that has not answered by [`HARD`] is `Unavailable`, and
/// its helper keeps its permit until it ends; work that panics is
/// `Failed`, and changed nothing.
pub fn admit(work: impl FnOnce(&Budget) -> Outcome + Send + 'static) -> Outcome {
    let Some(permit) = Permit::take() else {
        return Err(refused(
            "Busy",
            format!("{PERMITS} inspect requests are running on this machine already; read again"),
        ));
    };
    let budget = Budget {
        started: Instant::now(),
    };
    let (answer, answered) = mpsc::sync_channel(1);
    let helper = move || {
        let outcome = catch_unwind(AssertUnwindSafe(|| work(&budget)));
        // freed before the answer, so its caller's next request is admitted
        drop(permit);
        let _ = answer.send(outcome);
    };
    if let Err(error) = std::thread::Builder::new()
        .name("inspect".to_owned())
        .spawn(helper)
    {
        return Err(refused(
            "Unavailable",
            format!("no thread for an inspect request ({error})"),
        ));
    }
    match answered.recv_timeout(HARD.saturating_sub(budget.started.elapsed())) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(panic)) => {
            let message = (panic.downcast_ref::<&str>().copied())
                .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("no message");
            let message: String = (message.chars().take(SHOWN))
                .map(|c| if c.is_control() { ' ' } else { c })
                .collect();
            Err(refused(
                "Failed",
                format!("an inspect request panicked and changed nothing: {message}"),
            ))
        }
        Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => Err(refused(
            "Unavailable",
            format!(
                "an inspect request did not finish in {} s and was abandoned; it changes nothing",
                HARD.as_secs()
            ),
        )),
    }
}
