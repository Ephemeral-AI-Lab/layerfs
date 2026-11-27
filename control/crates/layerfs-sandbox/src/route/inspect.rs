//! The inspect route: where a request runs ([`ask`]), the worker's side of
//! it ([`in_worker`]) and how a side finds a Sandbox's run folder
//! ([`run_folder`]). The one match on a request's kind ([`Serve`]) is
//! `layerfs-inspect`'s, passed in by the caller.
use crate::deploy::Side;
use crate::record::SandboxRecord;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::place::{self, Executor};
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

/// The worker operation every inspect request is sent as.
pub const OP: &str = "inspect";

/// One admitted request `{ kind, .. }` on a side, with its budget: the
/// match on its kind.
pub type Serve = fn(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome;

/// Runs inspect `kind` with `args` where machine `machine`'s run folders
/// are: in this process under its own admission, or in the machine's
/// worker under the worker's, waited for at most `place::READ`. A machine
/// whose worker is not recorded or does not answer in time is
/// `Unavailable`; nothing starts it.
pub fn ask(
    home: &Path,
    machine: &str,
    kind: &'static str,
    args: SdkValue,
    serve: Serve,
) -> Outcome {
    let request = layerfs_home::home::with_field(args, "kind", SdkValue::text(kind));
    match on_owner(home, machine)? {
        Executor::Local => {
            let home = home.to_owned();
            admit(move |budget| serve(&Side::Home(&home), &request, budget))
        }
        Executor::Worker(mut worker) => worker.read(OP, request, place::READ),
    }
}

/// The worker's `inspect` operation: one request under this process's
/// admission, on the worker's `root`.
pub fn in_worker(root: &Path, args: SdkValue, serve: Serve) -> Outcome {
    let root = root.to_owned();
    admit(move |budget| serve(&Side::Worker(&root), &args, budget))
}

/// The run folder of Sandbox `name` on `side`: the `directory` of the
/// daemon record in `<root>/sandbox/<name>/record.json`, used only when it
/// is a folder directly under this side's run root, so a damaged or edited
/// record never points a read elsewhere. The record is read as a file: no
/// lock, no registry, no daemon. A side with no such record or folder
/// answers `NoSuchSandbox`, as a Sandbox's own calls do there.
pub fn run_folder(side: &Side<'_>, name: &str) -> Result<PathBuf, SdkRefusal> {
    layerfs_home::names::SandboxName::parse(name)?;
    let (Side::Home(root) | Side::Worker(root)) = side;
    let lost = || {
        refused(
            "NoSuchSandbox",
            format!("no Sandbox {name} runs on this machine"),
        )
    };
    let record = SandboxRecord::read_in(&paths::sandbox_dir(root, name), name)?.ok_or_else(lost)?;
    let directory = (record.daemon())
        .and_then(|daemon| daemon.directory())
        .map(PathBuf::from)
        .ok_or_else(lost)?;
    let named = matches!(
        directory.components().next_back(),
        Some(Component::Normal(_))
    );
    if !named || directory.parent() != Some(paths::run_root(root).as_path()) {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {name}'s record names a run folder outside {}; it is not read",
                paths::run_root(root).display()
            ),
        ));
    }
    match std::fs::symlink_metadata(&directory) {
        Ok(found) if found.is_dir() => Ok(directory),
        Ok(_) => Err(lost()),
        Err(error) if error.kind() == ErrorKind::NotFound => Err(lost()),
        Err(error) => Err(failed(error)),
    }
}

// Who runs an inspect request, and how much of the process it may hold
// (layerfs-project#179 B.3 to B.5, #178 isolation rules 3 and 4).
//
// At most [`PERMITS`] requests run at once in a process, the worker's and
// this process's own alike: one more is refused `Busy` at once, which is
// admission and not a retry. Each runs on a helper thread that owns its
// permit until it ends, so at most that many threads can ever be stuck in
// a read. A request has a soft budget ([`SOFT`]), which its work checks
// between units ([`Budget::spent`]) and then answers what it has so far,
// and a hard deadline ([`HARD`]), after which its caller is answered
// `Unavailable` and the helper is left to end by itself, still holding
// its permit.
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
