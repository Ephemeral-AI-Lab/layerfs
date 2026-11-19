//! `sandbox.remove` (call surface 2.0; `sandbox.close` before it), and the
//! removal of a Sandbox whose create never recorded an id.
//!
//! A remove records its `op` intent (`sandbox.remove`, #536 §7.1) under the
//! home and Sandbox locks before it stops the runtime, and removes the
//! record at the end. A live intent of another operation refuses it
//! `Busy`; a gone owner's uncertain intent does not: `sandbox.remove` is the
//! call that clears it. Its progress is its op's phases: `cancel` (forced,
//! one event per command this home records), `stop` and `delete`.
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
    // a Sandbox on a machine whose record has no 2.0 name (RV3 FAIL 1):
    // nothing on that machine is reached, its worker included
    let machine = found.machine();
    let on = layerfs_machine::record::MachineRecord::read(&home, machine)?;
    if on.is_some_and(|record| record.nameless()) {
        let first = format!(
            "machine {machine}'s record has no 2.0 name, so nothing on it was touched: the \
             Sandbox's daemon and run folder there, if any, stay in the VM or container an \
             earlier build made, which no call reaches"
        );
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
    let force = request.force;
    // the commands this home records, which a forced remove cancels first
    let running = match force {
        true => crate::exec::record::running(&named(&home, name)?.dir(&home)).unwrap_or_default(),
        false => Vec::new(),
    };
    let first = if running.is_empty() { "stop" } else { "cancel" };
    let op = Op::begin(OP, first, SdkValue::map([("force", SdkValue::Bool(force))]))?;
    let record = {
        let _lock = home::lock::take(&home)?;
        let _sandbox = record::lock(&home, name)?;
        let Some(record) = SandboxRecord::read(&home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        if record.id().is_none() {
            return remove_unstarted(&home, &record);
        }
        let record = record.with_op(&op);
        record.write(&home)?;
        record
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
    let stopped = if force && record.machine() == "native" {
        // a forced remove on `native` needs no Store or daemon discovery,
        // even when active mounts or a broken Store would block the
        // ordinary route
        crate::force::native_free(&home)
            .and_then(|()| crate::force::native_host(&home, name))
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
        // signal sent (RV3 FAIL 2, 3): as gone, `left` saying what may remain
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
            remove_folder(&dir)?;
            let unmounted = match value.get("unknown") {
                Some(SdkValue::List(paths)) => texts(paths).map(&handle_of).collect(),
                _ => Vec::new(),
            };
            let left = match value.get("left") {
                Some(SdkValue::List(items)) => texts(items).map(str::to_owned).collect(),
                _ => Vec::new(),
            };
            done(events);
            Ok(Removed::Stopped { unmounted, left })
        }
        // a Sandbox already gone loses its record too
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            remove_folder(&dir)?;
            done(events);
            Ok(Removed::AlreadyGone { left: Vec::new() })
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
        // a certain failure leaves the Sandbox as it was, except a forced
        // remove whose runtime already stopped: the record says so
        Err(refusal) => {
            if let Some(now) = SandboxRecord::read(&home, name)? {
                let stopped = refusal.detail.get_bool(crate::force::RUNTIME_STOPPED);
                match stopped {
                    true => now.without_op().with_machine_stop(true),
                    false => now.without_op(),
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
/// ([`foreign`]), or one on a machine whose record has no 2.0 name. Its
/// machine's worker is asked once to drop its own record of it (a worker
/// that cannot be reached or refuses leaves that record, and `left` says
/// so), unless `worker` skips it, then the home's record goes. Nothing it
/// ran is touched; `first`, the first of `left`, says what was left.
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
    let left = match (record.machine(), worker) {
        // a `native` Sandbox's record is the folder removed below
        ("native", _) | (_, Worker::Skip) => None,
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
    remove_folder(&dir)?;
    done(events);
    let kept = left.map(|left| {
        format!(
            "its machine {}'s worker keeps its record: {left}",
            record.machine()
        )
    });
    Ok(Removed::AlreadyGone {
        left: std::iter::once(first).chain(kept).collect(),
    })
}

/// Whether [`forget`] asks the Sandbox's machine's worker to drop its own
/// record of it.
#[derive(Clone, Copy)]
enum Worker {
    /// Asked once; a refusal is a sentence of `left`.
    Ask,
    /// Not reached at all: the machine's record has no 2.0 name.
    Skip,
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

/// Removes Sandbox `record`, which has no id: its create's outcome was not
/// known, or its process ended before recording one. The folder goes and
/// the name is free. The caller holds the home and Sandbox locks and has
/// refused a live create.
fn remove_unstarted(home: &Path, record: &SandboxRecord) -> Result<Removed> {
    let unresolved = (record.op()).is_some_and(|op| op.standing() == Standing::Unresolved);
    remove_folder(&record.dir(home))?;
    let mut left = Vec::new();
    if unresolved {
        left.push("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here".to_owned());
    }
    Ok(Removed::AlreadyGone { left })
}
