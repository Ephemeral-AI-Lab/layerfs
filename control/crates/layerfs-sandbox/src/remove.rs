//! `sandbox.remove` (call surface 2.0; `sandbox.close` before it), and the
//! removal of a Sandbox whose create never recorded an id: one whose create
//! may have started a daemon (its record names one on `native`; its
//! machine's worker may keep one elsewhere) is closed as a started one is,
//! so its daemon is stopped, or the remove is refused and the record kept.
//!
//! A remove records its `op` intent (`sandbox.remove`, #536 §7.1) under the
//! home and Sandbox locks before it stops the runtime, and removes the
//! record at the end. A live intent of another operation refuses it
//! `Busy`; a gone owner's uncertain intent does not: `sandbox.remove` is the
//! call that clears it. Its progress is its op's phases: `cancel` (forced,
//! one event per command this home records), `stop` and `delete`.
//!
//! The home's retired tombstone of the Sandbox is written before its record
//! goes only when its runtime is confirmed stopped (a close answered
//! `Closed`), confirmed gone (its side answered `NoSuchSandbox` with
//! `containerGone`, or its machine is recorded stopped) or never started (a
//! create that ended claiming the name). A removal that leaves a runtime it
//! did not stop (a record this build does not read or reopen, a worker that
//! keeps its record, a machine whose record has no 2.0 name or whose VM a
//! live read found missing, which may come back) writes none: no evidence
//! is the safe outcome, a tombstone would prove a live mount dead.
use super::create::remove_folder;
use super::direct;
use super::family::named;
use super::list::handle;
use super::record::{self, SandboxRecord};
use crate::host::Call;
use crate::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{refused, shell_word, with_next};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The op's name a remove records.
pub const OP: &str = "sandbox.remove";

/// `sandbox.remove(sandbox, { force })`.
#[derive(Clone, Debug, Default)]
pub struct RemoveRequest {
    /// The Sandbox's name.
    pub name: String,
    /// Remove it even while Workspaces are mounted or commands run.
    pub force: bool,
}

/// How a remove ended.
pub enum Removed {
    /// Its runtime stopped and its record went: the Workspaces it ended
    /// (their handles), whose uncommitted changes ended with it.
    Stopped {
        /// The handles of the Workspaces the stop ended.
        unmounted: Vec<String>,
        /// Each sentence about what may be left: a Workspace whose mount
        /// intent could not be ended in its Store.
        left: Vec<String>,
    },
    /// Its runtime was gone already, it never recorded one, or it was
    /// recorded under the retired docker backend; its record went.
    AlreadyGone {
        /// Each sentence about what may be left unrecorded.
        left: Vec<String>,
    },
}

/// The phases a remove reports, in order.
const PHASES: [&str; 3] = ["cancel", "stop", "delete"];

/// Sends progress event `phase` of the remove: `done` and `total` count
/// the phases, or for `cancel` the commands.
fn phase(events: &mut Events<'_>, phase: &str, counted: Option<(usize, usize)>) {
    let count = |n: usize| i64::try_from(n).ok();
    let (done, total) = counted.unwrap_or_else(|| {
        let at = PHASES.iter().position(|name| *name == phase).unwrap_or(0);
        (at, PHASES.len())
    });
    events.phase(phase, count(done), count(total), Vec::new());
}

/// The last phase ended: all are done.
fn done(events: &mut Events<'_>) {
    phase(events, "delete", Some((PHASES.len(), PHASES.len())));
}

/// The texts of list `items`.
fn texts(items: &[SdkValue]) -> impl Iterator<Item = &str> {
    items.iter().filter_map(|item| match item {
        SdkValue::Text(text) => Some(text.as_str()),
        _ => None,
    })
}

/// Stops the Sandbox and removes its record.
pub fn remove(home: &Path, events: &mut Events<'_>, request: &RemoveRequest) -> Result<Removed> {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    let found = named(&home, name)?;
    if let Some(container) = found.retired_container() {
        let container = format!(
            "its container of the retired docker backend was not touched: {}",
            super::reopen::container_fix(container.as_deref())
        );
        return forget(&home, events, name, container, Worker::Ask);
    }
    // a Sandbox on a machine whose record has no 2.0 name (RV3 FAIL 1), or
    // whose VM a live read found missing: nothing on that machine is
    // reached, its worker included, and neither proves its runtime gone
    let machine = found.machine();
    let on = layerfs_machine::record::MachineRecord::read(&home, machine)?;
    if let Some(on) = on.filter(|record| record.gone()) {
        let first = match on.nameless() {
            true => format!(
                "machine {machine}'s record has no 2.0 name, so nothing on it was touched: the \
                 Sandbox's daemon and run folder there, if any, stay in the VM or container an \
                 earlier build made, which no call reaches"
            ),
            false => missing_vm(machine, found.project()),
        };
        return forget(&home, events, name, first, Worker::Skip);
    }
    if let Some(unnamed) = foreign(&home, &found)? {
        return forget(&home, events, name, unnamed, Worker::Ask);
    }
    // a local record this build does not read (RV3 FAIL 2, 3): its
    // identity cannot be checked, so nothing is signalled and no Store or
    // daemon is resolved. A forwarded record is not local (`unread` answers
    // `None`): it goes on to its worker below
    if let Some(daemon) = found.daemon() {
        if let Some(why) = crate::reopen::unread(daemon) {
            let first = crate::lifecycle::unread_left(daemon, &why);
            return forget(&home, events, name, first, Worker::Ask);
        }
    }
    // a machine recorded stopped runs no daemon and its worker is not
    // reached: a Sandbox whose runtime a forced `machine.stop` ended, or any
    // with `force`, goes by its records alone; without either, the remove
    // says how it can go
    if let Some(first) = stopped_on(&home, &found, request.force)? {
        return forget(&home, events, name, first, Worker::Gone);
    }
    let force = request.force;
    // the commands this home records, which a forced remove cancels first
    let running = match force {
        true => crate::exec::record::running(&named(&home, name)?.dir(&home)).unwrap_or_default(),
        false => Vec::new(),
    };
    let first = if running.is_empty() { "stop" } else { "cancel" };
    let op = Op::begin(OP, first, SdkValue::map([("force", SdkValue::Bool(force))]))?;
    // an unstarted Sandbox whose create may have started a daemon is closed
    // like a started one; its create's intent comes back if the close fails
    let (record, created) = {
        let _lock = home::lock::take(&home)?;
        let _sandbox = record::lock(&home, name)?;
        let Some(record) = SandboxRecord::read(&home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        if record.id().is_none() && !may_run(&home, &record) {
            return remove_unstarted(&home, &record);
        }
        let created = record.id().is_none().then(|| record.op()).flatten();
        let record = record.with_op(&op);
        record.write(&home)?;
        (record, created)
    };
    for (done, _) in running.iter().enumerate() {
        phase(events, "cancel", Some((done, running.len())));
    }
    // the Workspaces this home records, by kernel path, for their handles
    let known: Vec<(String, String)> = (crate::files::workspaces(&record.dir(&home))?)
        .into_iter()
        .filter_map(|(_, workspace)| {
            let path = workspace.path()?.to_owned();
            Some((path, handle(name, &workspace)?))
        })
        .collect();
    let handle_of = |path: &str| match known.iter().find(|(at, _)| at == path) {
        Some((_, handle)) => handle.clone(),
        None => format!("{name}:{path}"),
    };
    let mut held = None;
    // a Sandbox reached directly keeps its Workspace records here: its
    // worker is told which it holds (#535 G4), and its commands' records
    // are settled here (D-10: `Busy`, or cancelled first with `force`)
    if direct::forwarded(&record) {
        let settled = crate::exec::remote::settle(events, (&home, &record), None, force)
            .and_then(|()| direct::held(&home, &record));
        match settled {
            Ok(paths) => held = Some(paths),
            Err(refusal) => {
                let _lock = home::lock::take(&home)?;
                let _sandbox = record::lock(&home, name)?;
                if let Some(now) = SandboxRecord::read(&home, name)? {
                    now.without_op().write(&home)?;
                }
                return Err(shown(refusal, &handle_of));
            }
        }
    }
    if first == "cancel" {
        let _sandbox = record::lock(&home, name)?;
        if let Some(now) = SandboxRecord::read(&home, name)? {
            now.with_op(&op.clone().at("stop")).write(&home)?;
        }
    }
    phase(events, "stop", None);
    let stopped = if force && layerfs_machine::runs_here(&home, record.machine()).unwrap_or(false) {
        // a forced remove on a machine whose calls run in this process needs
        // no Store or daemon discovery, even when active mounts or a broken
        // Store would block the ordinary route
        crate::force::machine_free(&home, record.machine())
            .and_then(|()| crate::force::local_host(&home, name))
            .and_then(|host| crate::force::close(&host, held.as_deref().unwrap_or(&[])))
    } else {
        on_machine(
            &home,
            events,
            &record,
            Call::Close { force, held },
            crate::dispatch::call,
        )
    };
    let _lock = home::lock::take(&home)?;
    // on `native` the Sandbox's own side removed the folder already
    let dir = record.dir(&home);
    let _sandbox = match dir.exists() {
        true => Some(record::lock(&home, name)?),
        false => None,
    };
    match stopped {
        // a record its side does not read went by its records alone, no
        // signal sent (RV3 FAIL 2, 3): as gone, `left` saying what may
        // remain. Its daemon may still run, so no tombstone is written
        Ok(value) if value.get_text("kind") == Some("Forgotten") && value.get("left").is_some() => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            remove_folder(&dir)?;
            done(events);
            let left = match value.get("left") {
                Some(SdkValue::List(items)) => texts(items).map(str::to_owned).collect(),
                _ => Vec::new(),
            };
            Ok(Removed::AlreadyGone { left })
        }
        Ok(value) => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            // only a close that stopped the runtime proves its mounts dead;
            // a retired docker record its side forgot left its container
            let unretired = match value.get_text("kind") {
                Some("Closed") => retire_home(&home, &record, &dir),
                _ => None,
            };
            remove_folder(&dir)?;
            let unmounted = match value.get("unknown") {
                Some(SdkValue::List(paths)) => texts(paths).map(&handle_of).collect(),
                _ => Vec::new(),
            };
            // a mount intent its stop could not end says what ends it
            // where this Sandbox ran
            let left = match value.get("left") {
                Some(SdkValue::List(items)) if !items.is_empty() => {
                    let remedy = crate::mount_intents::remedy(&home, &record);
                    texts(items)
                        .map(|said| format!("{said}; {remedy}"))
                        .collect()
                }
                _ => Vec::new(),
            };
            let left = left.into_iter().chain(unretired).collect();
            done(events);
            Ok(Removed::Stopped { unmounted, left })
        }
        // a Sandbox already gone loses its record too; its side's reclaim
        // names a tombstone it could not write in its `left` detail. Only a
        // daemon its side found gone or stopped (`containerGone`) proves its
        // mounts dead: any other `NoSuchSandbox` (no started record, no
        // daemon keys) inspected no runtime, so no tombstone is written
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            let reclaimed: Vec<String> = match refusal.detail.get("left") {
                Some(SdkValue::List(items)) => texts(items).map(str::to_owned).collect(),
                _ => Vec::new(),
            };
            let unretired = match refusal.detail.get_bool("containerGone") {
                true => retire_home(&home, &record, &dir),
                false => None,
            };
            let left = reclaimed.into_iter().chain(unretired).collect();
            remove_folder(&dir)?;
            done(events);
            Ok(Removed::AlreadyGone { left })
        }
        // its outcome is not known: the record stays with the intent,
        // uncertain, until another `sandbox.remove` clears it
        Err(refusal) if refusal.code == "Unknown" => {
            let op = op.at("stop").as_uncertain();
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.with_op(&op).write(&home)?;
            }
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(shown(refusal, &handle_of))
        }
        // a certain failure leaves the Sandbox as it was, its create's
        // intent included, except a forced remove whose runtime already
        // stopped: the record says so
        Err(refusal) => {
            if let Some(now) = SandboxRecord::read(&home, name)? {
                let stopped = refusal.detail.get_bool(crate::force::RUNTIME_STOPPED);
                match (stopped, &created) {
                    (true, _) => now.without_op().with_machine_stop(true),
                    (false, Some(created)) => now.with_op(created),
                    (false, None) => now.without_op(),
                }
                .write(&home)?;
            }
            Err(shown(refusal, &handle_of))
        }
    }
}

/// A remove's refusal with the fields the caller's next call needs: a
/// `Busy` or `Unknown` names the commands `running` (their exec ids), an
/// `InUse` (`MountsActive`) the Workspaces mounted, by handle.
fn shown(refusal: SdkRefusal, handle_of: &dyn Fn(&str) -> String) -> SdkRefusal {
    let listed = |field: &str| match refusal.detail.get(field) {
        Some(SdkValue::List(items)) => Some(items.clone()),
        _ => None,
    };
    match refusal.code.as_str() {
        "Busy" | "Unknown" => match listed("execs") {
            Some(execs) => with_next(refusal, "running", SdkValue::List(execs)),
            None => refusal,
        },
        "MountsActive" => match listed("workspaces") {
            Some(paths) => {
                let handles = texts(&paths).map(handle_of).map(SdkValue::Text).collect();
                with_next(refusal, "workspaces", SdkValue::List(handles))
            }
            None => refusal,
        },
        _ => refusal,
    }
}

/// What Sandbox `record` left when its daemon's recorded name is not the
/// one this home gives it (`<prefix><name>`, the run folder an earlier
/// build named `lfs-<home id8>-<name>`): its run folder and daemon process,
/// which this build neither reopens nor deletes, and the line that removes
/// them. The process is named only while the one its record identifies
/// (id, start time and boot) still runs (RV3 FAIL 4): a reused id is never
/// offered to `kill`. `None` for a record with the name this home gives,
/// or none.
fn foreign(home: &Path, record: &SandboxRecord) -> Result<Option<String>> {
    let Some(daemon) = record.daemon() else {
        return Ok(None);
    };
    let Some(recorded) = daemon.name() else {
        return Ok(None);
    };
    let id = home::opened(home)?.id;
    let given = format!(
        "{}{}",
        layerfs_home::names::provider_prefix(&id),
        record.name()
    );
    if recorded == given {
        return Ok(None);
    }
    let folder = daemon.directory().unwrap_or(recorded);
    let word = shell_word(folder);
    let said = format!(
        "its record names {recorded}, which this build does not give it, so its run folder \
         {folder} and its daemon process were not touched"
    );
    let process = crate::reopen::recorded_process(daemon);
    Ok(Some(match process {
        Ok(process) if process.alive() => {
            let pid = process.pid;
            format!(
                "{said}: process {pid} still runs, and `kill {pid}; rm -rf {word}` removes them"
            )
        }
        Ok(_) => format!(
            "{said}: the process its record names is gone, and `rm -rf {word}` removes the folder"
        ),
        Err(_) => format!(
            "{said}: its record does not identify that process, so none is named here, and \
             `rm -rf {word}` removes the folder once nothing runs in it"
        ),
    }))
}

/// Removes the record of Sandbox `name` that this build does not reopen:
/// one recorded under the retired docker backend (D-8,
/// layerfs-project#111), one whose daemon's name this home does not give
/// ([`foreign`]), one on a machine whose record has no 2.0 name or whose VM
/// a live read found missing, or one on a machine recorded stopped. Its
/// machine's worker is asked once to drop its own record of it (a worker
/// that cannot be reached or refuses leaves that record, and `left` says
/// so), unless `worker` skips it, then the home's record goes. Nothing it
/// ran is touched; `first`, the first of `left`, says what was left. The
/// home's retired tombstone is written only for [`Worker::Gone`], whose
/// runtime is confirmed gone: a worker that keeps its record, or one that
/// forgot it without stopping it, proves nothing.
fn forget(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    first: String,
    worker: Worker,
) -> Result<Removed> {
    let record = {
        let _lock = home::lock::take(home)?;
        let _sandbox = record::lock(home, name)?;
        let Some(record) = SandboxRecord::read(home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        record
    };
    let here = layerfs_machine::runs_here(home, record.machine()).unwrap_or(false);
    let left = match (here, worker) {
        // the record of a Sandbox on a machine whose calls run in this
        // process is the folder removed below
        (true, _) | (_, Worker::Skip | Worker::Gone) => None,
        _ => on_machine(
            home,
            events,
            &record,
            Call::Close {
                force: true,
                held: None,
            },
            crate::dispatch::call,
        )
        .err()
        .filter(|refusal| refusal.code != "NoSuchSandbox")
        .map(|refusal| refusal.message),
    };
    phase(events, "delete", None);
    let _lock = home::lock::take(home)?;
    let dir = record.dir(home);
    {
        let _sandbox = record::lock(home, name)?;
        direct::release(home, &record);
    }
    let unretired = match worker {
        Worker::Gone => retire_home(home, &record, &dir),
        Worker::Ask | Worker::Skip => None,
    };
    remove_folder(&dir)?;
    done(events);
    let kept = left.map(|left| {
        format!(
            "its machine {}'s worker keeps its record: {left}",
            record.machine()
        )
    });
    Ok(Removed::AlreadyGone {
        left: std::iter::once(first)
            .chain(kept)
            .chain(unretired)
            .collect(),
    })
}

/// What a remove of Sandbox `record` leaves on its machine when that
/// machine is recorded `stopped` (no worker runs, and no daemon with it)
/// and the Sandbox may go by its records alone: its runtime a forced
/// `machine.stop` ended (`machine_stopped`), or the remove is forced. The
/// worker's own record and run folder of it stay on the machine until its
/// worker next starts, which removes them (`crate::sweep::worker_start`).
/// `None` on a machine whose calls run in this process, which this
/// process closes itself, and on a machine not recorded stopped; refused `Unavailable` on a stopped machine
/// without either, naming both ways on.
fn stopped_on(home: &Path, record: &SandboxRecord, force: bool) -> Result<Option<String>> {
    let machine = record.machine();
    let stopped = !layerfs_machine::runs_here(home, machine).unwrap_or(false)
        && layerfs_machine::record::MachineRecord::read(home, machine)?
            .is_some_and(|on| on.recorded_state() == Some("stopped"));
    if !stopped {
        return Ok(None);
    }
    let (name, word) = (record.name(), shell_word(machine));
    if !force && !record.machine_stopped() {
        let refusal = refused(
            "Unavailable",
            format!(
                "machine {machine} is stopped, so Sandbox {name}'s daemon is not reached: \
                 `layerfs machine start {word}` lets the remove stop it, or `layerfs sandbox \
                 remove {} --force` removes its records here and leaves the rest to the \
                 machine's worker when it next starts",
                shell_word(name)
            ),
        );
        return Err(layerfs_home::outcome::with_fix(
            refusal,
            format!("layerfs machine start {word}"),
        ));
    }
    Ok(Some(format!(
        "machine {machine} is stopped, so only the Sandbox's records here were removed and its \
         worker was not asked: its daemon ended with the machine, and the worker's own record \
         and run folder of it stay on the machine until its worker next starts, which removes \
         them"
    )))
}

/// What a remove of a Sandbox of project `project` on machine `machine`,
/// whose VM a live read found missing, leaves: its records went and nothing
/// on the machine was reached. The VM may come back (a provider listing can
/// miss it, and a live read mends the record then), so its daemon is not
/// proved stopped: no tombstone is written, and a mount intent it held stays
/// until the operator's repair settles it.
fn missing_vm(machine: &str, project: Option<&str>) -> String {
    let project = project.map_or_else(|| "<project>".to_owned(), shell_word);
    format!(
        "machine {machine}'s VM was found missing, so only the Sandbox's records were removed \
         and nothing on the machine was reached: its daemon and run folder were in that VM. A \
         provider that lists no such VM does not prove it gone, so no retired tombstone was \
         written and a mount intent the Sandbox held is not proven dead: `layerfs store \
         repair-mount-holders {project} <branch> list` shows it, and `settle --op <op> --force` \
         ends it once the VM is known gone"
    )
}

/// Whether [`forget`] asks the Sandbox's machine's worker to drop its own
/// record of it, and whether its runtime is confirmed gone.
#[derive(Clone, Copy)]
enum Worker {
    /// Asked once; a refusal is a sentence of `left`. Nothing was stopped.
    Ask,
    /// Not reached at all: the machine's record has no 2.0 name, so what
    /// runs on it is not known, or a live read found its VM missing, which
    /// does not prove its runtime gone (the VM may come back).
    Skip,
    /// Not reached: its runtime ended with its machine, recorded stopped
    /// ([`stopped_on`]), so the home's retired tombstone is written.
    Gone,
}

/// Refuses a remove while another operation's intent holds Sandbox
/// `record` (`Busy`); a gone owner's uncertain intent is what a remove
/// clears, and a gone owner's certain one holds nothing.
fn clearable(record: &SandboxRecord) -> Result<()> {
    match record.op() {
        Some(op) if op.standing() == Standing::Unresolved => Ok(()),
        _ => record.free(),
    }
}

/// Whether Sandbox `record`, which has no id, may have a daemon its create
/// started, which its remove then stops through the close a started one
/// takes: on a machine whose calls run in this process when its record
/// names one (`daemon`, which its own side writes before the create
/// records the id); on a machine with a worker, which keeps its own record
/// of it, once its create got past claiming the name.
fn may_run(home: &Path, record: &SandboxRecord) -> bool {
    match layerfs_machine::runs_here(home, record.machine()).unwrap_or(false) {
        true => record.daemon().is_some(),
        false => record.op().is_none_or(|op| op.phase() != "claim"),
    }
}

/// Removes Sandbox `record`, which has no id and no daemon its create may
/// have started ([`may_run`]): its create's outcome was not known, or its
/// process ended before recording one. The folder goes and the name is
/// free. The caller holds the home and Sandbox locks and has refused a
/// live create.
fn remove_unstarted(home: &Path, record: &SandboxRecord) -> Result<Removed> {
    let unresolved = (record.op()).is_some_and(|op| op.standing() == Standing::Unresolved);
    let dir = record.dir(home);
    let unretired = retire_home(home, record, &dir);
    remove_folder(&dir)?;
    let mut left = Vec::new();
    if unresolved {
        left.push("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here".to_owned());
    }
    left.extend(unretired);
    Ok(Removed::AlreadyGone { left })
}

/// Writes the home's retirement tombstone of Sandbox `record` before its home
/// record (folder `dir`) goes: best effort, its failure a sentence of `left`.
/// This is the tombstone of a worker's Sandbox, whose worker writes none. A
/// folder already gone was removed by the Sandbox's own side on `native`,
/// which wrote the tombstone first.
fn retire_home(home: &Path, record: &SandboxRecord, dir: &Path) -> Option<String> {
    match dir.exists() {
        true => record::retire_reported(home, record),
        false => None,
    }
}
