//! A project's doctor (call surface 2.0, `checks.project`): its record
//! (`project.record`), its record against the Store's own list
//! (`project.listed`, the comparison `project.get` makes), and with
//! `--full` its data (`project.data`): everything main's head and each
//! live Branch's head reach, read and checked where the Store's calls run,
//! in this process for a sqlite Store on this computer and a PostgreSQL
//! Store, through the machine worker's `store.diagnose` (`cut: project`)
//! for a sqlite Store on another machine. The walk writes nothing to the
//! Store; its visited set is a private temporary database removed when the
//! walk ends (the engine's project audit, through `layerfs_store::diagnostic`).
use super::frame::{CheckId, Probe};
use super::report::Node;
use super::target::Checks;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::{hex, unhex};
use layerfs_home::home::paths;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, Outcome as CallOutcome};
use layerfs_home::state::BlockedBy;
use layerfs_store::diagnostic::{
    self, LayerStackId, Observation, Options, Outcome, ProjectCoverage,
};
use layerfs_store::identity::{self, Identity};
use layerfs_store::place::{self, Executor};
use layerfs_store::project::live;
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::{StoreRecord, StoreRef};
use std::path::Path;
use std::time::{Duration, Instant};

const RECORD: &str = "project.record";
const LISTED: &str = "project.listed";
const DATA: &str = diagnostic::PROJECT_CHECK;

/// A project's checks, in the frozen order; the data walk is Full only.
pub(super) const CHECKS: &[CheckId] = &[
    CheckId::own(RECORD),
    CheckId::own(LISTED),
    CheckId::full(DATA),
];

/// The worker's project cut, the `cut` its `store.diagnose` takes for it.
pub(super) const CUT: &str = "project";

/// Problems of one walk a report names; the rest are counted.
const KEPT: usize = 64;
/// Characters of one problem's message a report keeps.
const MESSAGE: usize = 512;

/// Checks project `probe.thing` (`<store>/<project>`) into `node`.
pub(super) fn run(probe: &Probe<'_>, node: &mut Node) {
    if let Some(by) = probe.blocked {
        *node = Node::blocked(probe.thing.clone(), by.clone());
        return;
    }
    let handle = probe.thing.name.as_str();
    let mut checks = Checks::default();
    let (store, name) = handle.split_once('/').unwrap_or((handle, ""));
    let recorded = StoreRecord::read(probe.home, store).and_then(|found| {
        let found = found.ok_or_else(|| refused("NotFound", format!("no Store {store}")))?;
        Ok((found, ProjectRecord::read(probe.home, store, name)?))
    });
    match recorded {
        Ok((record, Some(project))) => {
            checks.pass(RECORD, "the project and its Store are on record");
            let stack = listed(probe, (&record, &project), handle, &mut checks);
            if let (Some(stack), true) = (stack, probe.full) {
                data(probe, &record, stack, &mut checks);
            }
        }
        Ok((_, None)) => {
            checks.fail(RECORD, format!("the home has no record of {handle}"));
        }
        Err(refusal) => checks.fail(RECORD, refusal.message),
    }
    checks.fill(node, probe.checks, probe.budget.expired());
}

/// `project.listed`: the Store's own list holds the project its record
/// names, by its stack id, or by its name for a record without one. The
/// stack the list names, when it does.
fn listed(
    probe: &Probe<'_>,
    (store, project): (&StoreRecord, &ProjectRecord),
    handle: &str,
    checks: &mut Checks,
) -> Option<LayerStackId> {
    let name = handle.split_once('/').map_or("", |(_, name)| name);
    match live::projects(probe.home, store.name(), store, false) {
        Ok(projects) => match live::find(project, name, &projects) {
            Some(found) => {
                checks.pass(LISTED, "the Store lists it");
                Some(found.stack)
            }
            None => {
                checks.notice(LISTED, "on record, but the Store does not list it");
                checks.fix(
                    LISTED,
                    format!(
                        "layerfs project delete {handle}",
                        handle = shell_word(handle)
                    ),
                );
                checks.evidence(
                    LISTED,
                    SdkValue::map([
                        ("recorded", SdkValue::Bool(true)),
                        ("listed", SdkValue::Bool(false)),
                    ]),
                );
                None
            }
        },
        Err(refusal) => {
            not_made(probe.home, store, LISTED, refusal, checks);
            None
        }
    }
}

/// A check `id` the Store's route could not make: a Store this host cannot
/// open, or whose file is gone, and a worker that does not answer block it
/// (their own doctors say why); an operation holding the Store makes it
/// `busy`; anything else gives no verdict.
fn not_made(home: &Path, store: &StoreRecord, id: &str, refusal: SdkRefusal, checks: &mut Checks) {
    let on_worker = match store.store_ref() {
        Ok(StoreRef::Sqlite { .. }) => store
            .machine()
            .ok()
            .filter(|machine| layerfs_machine::runs_here(home, machine).ok() == Some(false)),
        _ => None,
    };
    let by = |thing: &'static str, handle: String, reason: &'static str| BlockedBy {
        thing,
        handle,
        reason,
    };
    match (refusal.code.as_str(), on_worker) {
        ("NoStore", _) => checks.waits(
            id,
            refusal.message,
            by("store", store.name().to_owned(), "missing"),
        ),
        ("Unsupported", _) => checks.waits(
            id,
            refusal.message,
            by("store", store.name().to_owned(), "unsupported"),
        ),
        ("Busy", _) => checks.busy(id, refusal.message),
        ("Unavailable", Some(machine)) => {
            let message = format!(
                "machine {machine}'s worker does not answer: {}",
                refusal.message
            );
            checks.waits(id, message, by("machine", machine, "not_answering"));
        }
        // any other refusal of the Store's route (a Store whose id is not
        // the one recorded, one that cannot be opened): the Store failed
        // it, which its own doctor explains; never `lost` (R4 F3)
        _ => checks.waits(
            id,
            refusal.message,
            by("store", store.name().to_owned(), "failed"),
        ),
    }
}

/// `project.data`: everything the project's heads reach, read and checked
/// where the Store's calls run.
fn data(probe: &Probe<'_>, store: &StoreRecord, stack: LayerStackId, checks: &mut Checks) {
    let identity = match identity::identity_in(store) {
        Ok(identity) => identity,
        Err(refusal) => {
            checks.unknown(
                DATA,
                format!("its Store's id cannot be read: {}", refusal.message),
            );
            return;
        }
    };
    let options = Options {
        quick: false,
        deadline: probe.budget.deadline(),
        cancelled: None,
    };
    let mut found = Found::default();
    let mut progress = |done: u64, total: u64| (probe.progress)("data", done, total);
    let made = match store.store_ref() {
        Ok(StoreRef::Postgres { .. }) => diagnostic::postgresql_project(
            store,
            &identity,
            stack,
            options,
            &mut |o| found.observe(&o),
            &mut progress,
        ),
        Ok(StoreRef::Sqlite { .. }) => {
            let machine = store.machine().unwrap_or_default();
            match place::on_machine_read(probe.home, &machine) {
                Ok(Executor::Local) => diagnostic::sqlite_project(
                    &paths::store_file(probe.home, store.name()),
                    &identity,
                    stack,
                    options,
                    &mut |o| found.observe(&o),
                    &mut progress,
                ),
                Ok(Executor::Worker(_)) => remote(
                    probe,
                    (store.name(), &machine),
                    &identity,
                    stack,
                    &mut found,
                ),
                Err(refusal) => Err(refusal),
            }
        }
        Err(refusal) => Err(refusal),
    };
    found.fill(checks);
    match made {
        Ok(coverage) if coverage.failed == 0 && !coverage.incomplete && found.kept.is_empty() => {
            checks.pass(
                DATA,
                format!(
                    "{} objects in {} trees ({} heads: main and its Branches) read and checked",
                    coverage.objects, coverage.trees, coverage.heads
                ),
            );
        }
        Ok(_) => {}
        Err(refusal) => not_made(probe.home, store, DATA, refusal, checks),
    }
}

/// The problems one walk reported, the first [`KEPT`] of them kept.
#[derive(Default)]
struct Found {
    kept: Vec<(Outcome, String)>,
    more: u64,
}

impl Found {
    fn observe(&mut self, observation: &Observation) {
        if observation.id != DATA
            || !matches!(observation.outcome, Outcome::Fail | Outcome::Unknown)
        {
            return;
        }
        match self.kept.len() < KEPT {
            true => {
                let message = observation.message.chars().take(MESSAGE).collect();
                self.kept.push((observation.outcome, message));
            }
            false => self.more += 1,
        }
    }

    /// Each kept problem as one finding of `project.data`: an object the
    /// walk could not read (a corrupt pack) is as much a problem of the data
    /// as a wrong one, and is never also a `not_checked` entry (R4 F3).
    fn fill(&self, checks: &mut Checks) {
        for (_, message) in &self.kept {
            checks.fail(DATA, message.clone());
        }
        if self.more > 0 {
            checks.fail(
                DATA,
                format!("{} more problems were found and are not listed", self.more),
            );
        }
    }

    /// The walk's answer as a worker sends it.
    fn encode(&self, coverage: ProjectCoverage) -> SdkValue {
        let count = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
        let problems = self.kept.iter().map(|(outcome, message)| {
            let word = if *outcome == Outcome::Fail {
                "fail"
            } else {
                "unknown"
            };
            SdkValue::map([
                ("outcome", SdkValue::text(word)),
                ("message", SdkValue::text(message)),
            ])
        });
        SdkValue::map([
            ("trees", count(coverage.trees)),
            ("heads", count(coverage.heads)),
            ("objects", count(coverage.objects)),
            ("failed", count(coverage.failed)),
            ("incomplete", SdkValue::Bool(coverage.incomplete)),
            ("problems", SdkValue::List(problems.collect())),
            ("more", count(self.more)),
        ])
    }

    /// A worker's answer, read back into `self`.
    fn decode(&mut self, answer: &SdkValue) -> ProjectCoverage {
        let count = |key: &str| answer.get_int(key).and_then(|n| u64::try_from(n).ok());
        if let Some(SdkValue::List(problems)) = answer.get("problems") {
            for problem in problems {
                let outcome = match problem.get_text("outcome") {
                    Some("fail") => Outcome::Fail,
                    _ => Outcome::Unknown,
                };
                let message = problem.get_text("message").unwrap_or("").to_owned();
                self.observe(&Observation {
                    id: DATA,
                    outcome,
                    message,
                });
            }
        }
        self.more += count("more").unwrap_or(0);
        ProjectCoverage {
            trees: count("trees").unwrap_or(0),
            heads: count("heads").unwrap_or(0),
            objects: count("objects").unwrap_or(0),
            failed: count("failed").unwrap_or(0),
            // an answer without its counts covered nothing it can vouch for
            incomplete: answer.get("incomplete") != Some(&SdkValue::Bool(false)),
        }
    }
}

/// The walk through machine `machine`'s worker: one diagnostic connection,
/// its progress events passed on, nothing resent.
fn remote(
    probe: &Probe<'_>,
    (store, machine): (&str, &str),
    identity: &Identity,
    stack: LayerStackId,
    found: &mut Found,
) -> Result<ProjectCoverage, SdkRefusal> {
    use layerfs_machine::worker::{diagnostic_connect, Endpoint};
    let saved = layerfs_home::secrets::worker_secret(probe.home, machine)?
        .ok_or_else(|| refused("Unavailable", "it records no worker endpoint"))?;
    let endpoint = Endpoint {
        address: saved.address,
        secret: saved.secret,
    };
    let deadline = Instant::now() + probe.budget.limit(Duration::from_secs(5));
    let connection = diagnostic_connect(&endpoint, deadline)?;
    let timeout = probe
        .budget
        .remaining()
        .map_or(SdkValue::Null, |remaining| {
            SdkValue::Int(
                i64::try_from(remaining.as_millis())
                    .unwrap_or(i64::MAX)
                    .max(1),
            )
        });
    let args = SdkValue::map([
        ("store", SdkValue::text(store)),
        ("binding", SdkValue::Bytes(identity.binding.clone())),
        ("cursor", SdkValue::Bytes(identity.cursor_key.to_vec())),
        ("cut", SdkValue::text(CUT)),
        ("stack", SdkValue::text(hex(stack.as_slice()))),
        ("timeout_ms", timeout),
    ]);
    let answer = connection.diagnose_store(args, probe.budget.deadline(), &mut |event| {
        let count = |key: &str| event.get_int(key).and_then(|n| u64::try_from(n).ok());
        if let (Some(phase), Some(done), Some(total)) =
            (event.get_text("phase"), count("done"), count("total"))
        {
            (probe.progress)(phase, done, total);
        }
        Ok(())
    })?;
    Ok(found.decode(&answer))
}

/// The worker's `store.diagnose` with `cut: project`: project `stack`'s
/// data in Store `store` under the worker's fixed root, its progress sent
/// as events, its answer the walk's counts and first problems.
pub(super) fn in_worker(
    root: &Path,
    args: &SdkValue,
    identity: &Identity,
    events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
) -> CallOutcome {
    let name = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store required"))?;
    let stack = (args.get_text("stack").and_then(unhex))
        .and_then(|bytes| LayerStackId::from_slice(&bytes).ok())
        .ok_or_else(|| refused("InvalidInput", "the project's stack id required"))?;
    let deadline = (args.get_int("timeout_ms"))
        .and_then(|ms| u64::try_from(ms).ok())
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    let options = Options {
        quick: false,
        deadline,
        cancelled: None,
    };
    let mut found = Found::default();
    let coverage = diagnostic::sqlite_project(
        &paths::store_file(root, name),
        identity,
        stack,
        options,
        &mut |o| found.observe(&o),
        &mut |done, total| {
            let count = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
            let _ = events(SdkValue::map([
                ("phase", SdkValue::text("data")),
                ("done", count(done)),
                ("total", count(total)),
            ]));
        },
    )?;
    Ok(found.encode(coverage))
}
